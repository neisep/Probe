use eframe::egui;
use std::{fs, time::Duration};

use crate::openapi::{OpenApiError, compute_merge, parse_spec};
use crate::openapi_import::PendingOpenApiImport;
use crate::openapi::source::fetch_url;
use crate::persistence::{FileStorage, persist_state, restore_workspace};
use crate::request_prep::{active_resolution_values, prepare_request_draft};
use crate::runtime::{AsyncRequest, AsyncRequestResult, Event, Runtime};
use crate::state::{AppState, View};
use crate::ui::response_viewer::ResponseViewerState;
use crate::ui::{request_preview_modal, shell};
use crate::workspace::{
    PendingWorkspaceImport, backup_workspace, preview_workspace_import,
    workspace_bundle_from_json, workspace_bundle_to_json,
};

#[derive(Debug, Clone)]
struct PendingRequestContext {
    request_id: String,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
}

struct PendingOAuthAuth {
    rx: std::sync::mpsc::Receiver<Result<Option<crate::oauth::middleware::AttachmentHeader>, crate::oauth::OAuthError>>,
    prepared_request: AsyncRequest,
    request_index: usize,
}

#[derive(Debug, Clone)]
struct PendingRequestPreview {
    preview: request_preview_modal::RequestPreviewData,
    prepared_request: Option<AsyncRequest>,
    pending_request_context: Option<PendingRequestContext>,
}

pub struct ProbeApp {
    status: String,
    state: AppState,
    runtime: Option<Runtime>,
    storage: Option<FileStorage>,
    pending_request: Option<u64>,
    pending_request_context: Option<PendingRequestContext>,
    pending_workspace_import: Option<PendingWorkspaceImport>,
    pending_request_preview: Option<PendingRequestPreview>,
    pending_openapi_import: Option<PendingOpenApiImport>,
    pending_openapi_fetch: Option<(String, std::sync::mpsc::Receiver<Result<String, OpenApiError>>)>,
    pending_oauth_auth: Option<PendingOAuthAuth>,
    openapi_url_input: String,
    openapi_url_dialog_open: bool,
    theme_installed: bool,
    response_viewer: ResponseViewerState,
    saved_requests: Vec<crate::state::RequestDraft>,
    saved_environments: Vec<crate::state::Environment>,
    pending_close: bool,
}

impl ProbeApp {
    pub fn new() -> Self {
        let runtime = Runtime::new(8);
        let storage = create_storage();

        match (runtime, AppState::bootstrap()) {
            (Ok(runtime), Ok(mut state)) => {
                if let Some(stor) = &storage {
                    restore_workspace(&mut state, stor);
                }
                let saved_requests = state.requests.clone();
                let saved_environments = state.environments.clone();
                Self {
                    status: "Ready when you are!".to_owned(),
                    state,
                    runtime: Some(runtime),
                    storage,
                    pending_request: None,
                    pending_request_context: None,
                    pending_workspace_import: None,
                    pending_request_preview: None,
                    pending_openapi_import: None,
                    pending_openapi_fetch: None,
                    pending_oauth_auth: None,
                    openapi_url_input: String::new(),
                    openapi_url_dialog_open: false,
                    theme_installed: false,
                    response_viewer: ResponseViewerState::new(),
                    saved_requests,
                    saved_environments,
                    pending_close: false,
                }
            }
            (Err(error), Ok(state)) => Self {
                status: format!("Runtime unavailable: {error}"),
                saved_requests: state.requests.clone(),
                saved_environments: state.environments.clone(),
                state,
                runtime: None,
                storage,
                pending_request: None,
                pending_request_context: None,
                pending_workspace_import: None,
                pending_request_preview: None,
                pending_openapi_import: None,
                pending_openapi_fetch: None,
                pending_oauth_auth: None,
                openapi_url_input: String::new(),
                openapi_url_dialog_open: false,
                theme_installed: false,
                response_viewer: ResponseViewerState::new(),
                pending_close: false,
            },
            (Ok(runtime), Err(error)) => Self {
                status: format!("State bootstrap fallback: {error}"),
                state: AppState::default(),
                runtime: Some(runtime),
                storage,
                pending_request: None,
                pending_request_context: None,
                pending_workspace_import: None,
                pending_request_preview: None,
                pending_openapi_import: None,
                pending_openapi_fetch: None,
                pending_oauth_auth: None,
                openapi_url_input: String::new(),
                openapi_url_dialog_open: false,
                theme_installed: false,
                response_viewer: ResponseViewerState::new(),
                saved_requests: Vec::new(),
                saved_environments: Vec::new(),
                pending_close: false,
            },
            (Err(runtime_error), Err(state_error)) => Self {
                status: format!("Startup fallback: runtime={runtime_error}; state={state_error}"),
                state: AppState::default(),
                runtime: None,
                storage,
                pending_request: None,
                pending_request_context: None,
                pending_workspace_import: None,
                pending_request_preview: None,
                pending_openapi_import: None,
                pending_openapi_fetch: None,
                pending_oauth_auth: None,
                openapi_url_input: String::new(),
                openapi_url_dialog_open: false,
                theme_installed: false,
                response_viewer: ResponseViewerState::new(),
                saved_requests: Vec::new(),
                saved_environments: Vec::new(),
                pending_close: false,
            },
        }
    }

    fn export_workspace(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Probe workspace", &["json"])
            .set_file_name("workspace.probe.json")
            .save_file()
        else {
            return;
        };

        let json = match workspace_bundle_to_json(&self.state) {
            Ok(json) => json,
            Err(error) => {
                self.status = format!("Export failed: {error}");
                return;
            }
        };

        if let Err(error) = fs::write(&path, json) {
            self.status = format!("Export failed: {error}");
            return;
        }

        self.status = format!(
            "Exported {} requests, {} responses, {} environments to {}",
            self.state.requests.len(),
            self.state.responses.len(),
            self.state.environments.len(),
            path.display()
        );
    }

    fn import_workspace(&mut self) {
        if self.pending_request.is_some() {
            self.status = "Import unavailable while a request is running".to_owned();
            return;
        }

        let Some(path) = rfd::FileDialog::new()
            .add_filter("Probe workspace", &["json"])
            .pick_file()
        else {
            return;
        };

        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) => {
                self.status = format!("Import failed reading {}: {error}", path.display());
                return;
            }
        };

        let imported_state = match workspace_bundle_from_json(&contents) {
            Ok(imported_state) => imported_state,
            Err(error) => {
                self.status = format!("Import failed: {error}");
                return;
            }
        };

        let preview = preview_workspace_import(&imported_state);
        self.pending_workspace_import = Some(PendingWorkspaceImport {
            path: path.clone(),
            preview: preview.clone(),
            imported_state,
        });
        self.status = format!(
            "Review import from {} ({} requests, {} responses, {} environments)",
            path.display(),
            preview.request_count,
            preview.response_count,
            preview.environment_count
        );
    }

    fn confirm_workspace_import(&mut self) {
        let Some(pending_import) = self.pending_workspace_import.take() else {
            return;
        };

        let backup_path = match backup_workspace(&self.state) {
            Ok(path) => path,
            Err(error) => {
                self.pending_workspace_import = Some(pending_import);
                self.status = format!("Import failed before applying: {error}");
                return;
            }
        };

        let preview = pending_import.preview.clone();
        self.state = pending_import.imported_state;
        self.pending_request = None;
        self.pending_request_context = None;
        self.save_snapshot();

        if self.status.starts_with("Save failed") {
            self.status = format!(
                "Import applied but persistence failed. Backup saved to {}",
                backup_path.display()
            );
            return;
        }

        self.status = format!(
            "Imported {} requests, {} responses, {} environments from {}. Backup saved to {}",
            preview.request_count,
            preview.response_count,
            preview.environment_count,
            pending_import.path.display(),
            backup_path.display()
        );
    }

    fn show_import_confirmation(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_workspace_import.as_ref() else {
            return;
        };
        match crate::ui::dialogs::workspace_import::show(ctx, pending) {
            crate::ui::dialogs::workspace_import::WorkspaceImportDialogAction::None => {}
            crate::ui::dialogs::workspace_import::WorkspaceImportDialogAction::Cancel => {
                self.pending_workspace_import = None;
                self.status = "Import cancelled".to_owned();
            }
            crate::ui::dialogs::workspace_import::WorkspaceImportDialogAction::Confirm => {
                self.confirm_workspace_import();
            }
        }
    }

    fn import_openapi_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("OpenAPI spec", &["json", "yaml", "yml"])
            .pick_file()
        else {
            return;
        };

        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("OpenAPI read failed: {e}");
                return;
            }
        };

        self.apply_openapi_text(&text, path.display().to_string());
    }

    fn import_openapi_from_url(&mut self) {
        let url = self.openapi_url_input.trim().to_owned();
        if url.is_empty() {
            return;
        }
        self.openapi_url_dialog_open = false;
        self.status = format!("Fetching {url}…");
        self.pending_openapi_fetch = Some((url.clone(), fetch_url(&url)));
    }

    fn apply_openapi_text(&mut self, text: &str, source: String) {
        let ops = match parse_spec(text) {
            Ok(ops) => ops,
            Err(e) => {
                self.status = format!("OpenAPI parse failed: {e}");
                return;
            }
        };

        let (_, preview) = compute_merge(&self.state.requests, &ops);
        self.status = format!(
            "OpenAPI preview: {} new, {} updated, {} unchanged — confirm to apply",
            preview.new_count, preview.updated_count, preview.unchanged_count
        );
        self.pending_openapi_import = Some(PendingOpenApiImport {
            source,
            preview,
            ops,
        });
    }

    fn confirm_openapi_import(&mut self) {
        let Some(pending) = self.pending_openapi_import.take() else {
            return;
        };
        let (merged, _) = compute_merge(&self.state.requests, &pending.ops);
        self.state.requests = merged;
        self.state.ensure_valid_selection();
        self.save_snapshot();
        self.status = format!(
            "OpenAPI import applied from {} ({} new, {} updated, {} unchanged)",
            pending.source, pending.preview.new_count,
            pending.preview.updated_count, pending.preview.unchanged_count
        );
    }

    fn show_openapi_import_confirmation(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_openapi_import.as_ref() else {
            return;
        };
        match crate::ui::dialogs::openapi_import::show(ctx, pending) {
            crate::ui::dialogs::openapi_import::OpenApiImportDialogAction::None => {}
            crate::ui::dialogs::openapi_import::OpenApiImportDialogAction::Cancel => {
                self.pending_openapi_import = None;
                self.status = "OpenAPI import cancelled".to_owned();
            }
            crate::ui::dialogs::openapi_import::OpenApiImportDialogAction::Confirm => {
                self.confirm_openapi_import();
            }
        }
    }

    fn show_openapi_url_dialog(&mut self, ctx: &egui::Context) {
        if !self.openapi_url_dialog_open {
            return;
        }
        match crate::ui::dialogs::openapi_url::show(ctx, &mut self.openapi_url_input) {
            crate::ui::dialogs::openapi_url::OpenApiUrlDialogAction::None => {}
            crate::ui::dialogs::openapi_url::OpenApiUrlDialogAction::Close => {
                self.openapi_url_dialog_open = false;
            }
            crate::ui::dialogs::openapi_url::OpenApiUrlDialogAction::Fetch => {
                self.import_openapi_from_url();
            }
        }
    }

    fn can_start_request_preview(&self) -> bool {
        self.pending_request.is_none()
            && self.pending_workspace_import.is_none()
            && self.pending_request_preview.is_none()
            && self.pending_oauth_auth.is_none()
    }

    fn preview_selected_request(&mut self) {
        let Some(selected_request_index) = self.state.selected_request_index() else {
            self.status = "No request selected".to_owned();
            return;
        };
        self.preview_request_at_index(selected_request_index);
    }

    fn preview_request_at_index(&mut self, request_index: usize) {
        if self.pending_request.is_some() {
            self.status = "Wait for the current request to finish".to_owned();
            return;
        }
        if self.pending_workspace_import.is_some() {
            self.status = "Finish the workspace import flow before sending".to_owned();
            return;
        }
        if self.pending_request_preview.is_some() {
            self.status = "Finish or close the current request preview first".to_owned();
            return;
        }
        let Some(_runtime) = &self.runtime else {
            self.status = "Runtime unavailable".to_owned();
            return;
        };
        if request_index >= self.state.requests.len() {
            self.status = "Selected request is unavailable".to_owned();
            return;
        }

        self.state.ui.select_request(request_index);
        self.state.ui.set_view(View::Editor);

        let Some(request) = self.state.requests.get(request_index).cloned() else {
            self.status = "Selected request is unavailable".to_owned();
            return;
        };

        let resolution_values = active_resolution_values(&self.state);
        match prepare_request_draft(&request, &resolution_values) {
            Ok(mut prepared_request) => {
                let Some(runtime) = &self.runtime else {
                    self.status = "Runtime unavailable".to_owned();
                    return;
                };
                if request.attach_oauth {
                    if let Some(env_name) = self.state.active_environment_name() {
                        use crate::oauth::middleware::AuthResolution;
                        match crate::oauth::middleware::resolve_authorization(env_name) {
                            AuthResolution::Ready(Ok(Some(header_value))) => {
                                let already_set = prepared_request
                                    .headers
                                    .iter()
                                    .any(|(name, _)| name.eq_ignore_ascii_case(&header_value.name));
                                if !already_set {
                                    prepared_request
                                        .headers
                                        .push((header_value.name, header_value.value));
                                }
                            }
                            AuthResolution::Ready(Ok(None)) => {}
                            AuthResolution::Ready(Err(error)) => {
                                self.status = format!("OAuth middleware: {error}");
                                return;
                            }
                            AuthResolution::Refreshing(rx) => {
                                self.status = "OAuth: refreshing token…".to_owned();
                                self.pending_oauth_auth = Some(PendingOAuthAuth {
                                    rx,
                                    prepared_request,
                                    request_index,
                                });
                                return;
                            }
                        }
                    }
                }
                let pending_request_context = PendingRequestContext {
                    request_id: AppState::request_id_for_index(request_index),
                    method: prepared_request.method.clone(),
                    url: prepared_request.url.clone(),
                    headers: prepared_request.headers.clone(),
                };
                match runtime.submit_blocking(prepared_request) {
                    Ok(id) => {
                        self.pending_request = Some(id);
                        self.pending_request_context = Some(pending_request_context);
                        self.status = format!("Submitted request {id}");
                        self.save_snapshot();
                    }
                    Err(error) => {
                        self.status = format!("Submit error: {error}");
                    }
                }
            }
            Err(error) => {
                let error_info = error.to_error_info();
                self.status = error_info.format_display();
            }
        }
    }

    fn handle_pending_ui_actions(&mut self) {
        let Some(action) = self.state.ui.take_pending_request_action() else {
            return;
        };

        match action {
            crate::state::ui_state::RequestUiAction::PreviewRequest(request_index) => {
                self.preview_request_at_index(request_index);
            }
        }
    }

    fn submit_previewed_request(&mut self) {
        let Some(runtime) = &self.runtime else {
            self.status = "Runtime unavailable".to_owned();
            return;
        };
        let Some(pending_preview) = self.pending_request_preview.clone() else {
            return;
        };
        let Some(prepared_request) = pending_preview.prepared_request.clone() else {
            self.status = "Fix the request preview issues before sending".to_owned();
            return;
        };
        let Some(pending_request_context) = pending_preview.pending_request_context.clone() else {
            self.status = "Preview is missing request context".to_owned();
            return;
        };

        match runtime.submit_blocking(prepared_request) {
            Ok(id) => {
                self.pending_request = Some(id);
                self.pending_request_context = Some(pending_request_context);
                self.pending_request_preview = None;
                self.status = format!("Submitted request {id}");
                self.save_snapshot();
            }
            Err(error) => {
                self.status = format!("Submit error: {error}");
            }
        }
    }

    fn show_request_preview(&mut self, ctx: &egui::Context) {
        let Some(pending_preview) = self.pending_request_preview.as_ref() else {
            return;
        };

        match request_preview_modal::show_request_preview(ctx, &pending_preview.preview) {
            request_preview_modal::RequestPreviewAction::None => {}
            request_preview_modal::RequestPreviewAction::Close => {
                self.pending_request_preview = None;
            }
            request_preview_modal::RequestPreviewAction::Send => {
                self.submit_previewed_request();
            }
        }
    }

    fn save_snapshot(&mut self) {
        let Some(storage) = &self.storage else {
            return;
        };
        match persist_state(&self.state, storage) {
            Ok(()) => {
                self.saved_requests = self.state.requests.clone();
                self.saved_environments = self.state.environments.clone();
            }
            Err(error) => self.status = format!("Save failed: {error}"),
        }
    }

    fn has_unsaved_changes(&self) -> bool {
        self.state.requests != self.saved_requests
            || self.state.environments != self.saved_environments
            || self.pending_openapi_import.is_some()
            || self.pending_workspace_import.is_some()
    }

    fn show_unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        if !self.pending_close {
            return;
        }
        let has_pending_import =
            self.pending_openapi_import.is_some() || self.pending_workspace_import.is_some();
        match crate::ui::dialogs::unsaved_changes::show(ctx, has_pending_import) {
            crate::ui::dialogs::unsaved_changes::UnsavedChangesAction::None => {}
            crate::ui::dialogs::unsaved_changes::UnsavedChangesAction::Cancel => {
                self.pending_close = false;
            }
            crate::ui::dialogs::unsaved_changes::UnsavedChangesAction::CloseWithoutSaving => {
                self.saved_requests = self.state.requests.clone();
                self.saved_environments = self.state.environments.clone();
                self.pending_openapi_import = None;
                self.pending_workspace_import = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            crate::ui::dialogs::unsaved_changes::UnsavedChangesAction::SaveAndClose => {
                self.save_snapshot();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn poll_pending_openapi_fetch(&mut self) {
        let Some((url, rx)) = self.pending_openapi_fetch.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(text)) => {
                self.apply_openapi_text(&text, url);
            }
            Ok(Err(e)) => {
                self.status = format!("OpenAPI fetch failed: {e}");
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.pending_openapi_fetch = Some((url, rx));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.status = "OpenAPI fetch: internal error (channel closed)".to_owned();
            }
        }
    }

    fn poll_pending_oauth_auth(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_oauth_auth.take() else {
            return;
        };
        match pending.rx.try_recv() {
            Ok(result) => {
                let mut prepared_request = pending.prepared_request;
                match result {
                    Ok(Some(header_value)) => {
                        let already_set = prepared_request
                            .headers
                            .iter()
                            .any(|(name, _)| name.eq_ignore_ascii_case(&header_value.name));
                        if !already_set {
                            prepared_request
                                .headers
                                .push((header_value.name, header_value.value));
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.status = format!("OAuth refresh failed: {error}");
                        return;
                    }
                }
                let Some(runtime) = &self.runtime else {
                    self.status = "Runtime unavailable".to_owned();
                    return;
                };
                let pending_request_context = PendingRequestContext {
                    request_id: AppState::request_id_for_index(pending.request_index),
                    method: prepared_request.method.clone(),
                    url: prepared_request.url.clone(),
                    headers: prepared_request.headers.clone(),
                };
                match runtime.submit_blocking(prepared_request) {
                    Ok(id) => {
                        self.pending_request = Some(id);
                        self.pending_request_context = Some(pending_request_context);
                        self.status = format!("Submitted request {id}");
                        self.save_snapshot();
                    }
                    Err(error) => {
                        self.status = format!("Submit error: {error}");
                    }
                }
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.pending_oauth_auth = Some(pending);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.status = "OAuth refresh: internal error (channel closed)".to_owned();
            }
        }
    }
}

impl Default for ProbeApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for ProbeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.theme_installed {
            crate::ui::theme::install(ui.ctx());
            self.theme_installed = true;
        }

        if let Some(runtime) = &self.runtime {
            let events = runtime.poll_events_blocking();
            for ev in events {
                match ev {
                    Event::StatusChanged { id, status } => {
                        self.pending_request = Some(id);
                        self.status = format!("Request {id}: {status:?}");
                    }
                    Event::Completed { id, result } => {
                        self.pending_request = None;
                        let pending_context = self.pending_request_context.take();

                        match result {
                            AsyncRequestResult::Ok(info) => {
                                let mut summary = crate::state::ResponseSummary::default();
                                apply_pending_request_context(
                                    &mut summary,
                                    pending_context.as_ref(),
                                );
                                summary.status = Some(info.status);
                                summary.timing_ms = Some(info.duration_ms);
                                summary.size_bytes = Some(info.body.len());
                                summary.response_headers = info.headers.clone();
                                summary.content_type =
                                    info.header("content-type").or_else(|| info.media_hint());
                                summary.header_count = Some(info.header_count());
                                summary.preview_text = info.text_preview(400);
                                summary.body_text = info.text_preview(usize::MAX);
                                self.status = format!(
                                    "Request {id} completed ({} in {} ms)",
                                    info.status, info.duration_ms
                                );
                                self.state.responses.push(summary);
                                self.state
                                    .ui
                                    .select_response(self.state.responses.len() - 1);
                                self.save_snapshot();
                            }
                            AsyncRequestResult::Err(err) => {
                                let mut summary = crate::state::ResponseSummary::default();
                                apply_pending_request_context(
                                    &mut summary,
                                    pending_context.as_ref(),
                                );
                                summary.error = Some(err.format_display());
                                summary.preview_text = err.details.clone();
                                summary.body_text = err.details.clone();
                                self.status = format!("Request {id} failed");
                                self.state.responses.push(summary);
                                self.state
                                    .ui
                                    .select_response(self.state.responses.len() - 1);
                                self.save_snapshot();
                            }
                        }
                    }
                }
            }
        }

        self.poll_pending_openapi_fetch();
        self.poll_pending_oauth_auth(ui.ctx());

        if self.pending_request.is_some()
            || self.pending_openapi_fetch.is_some()
            || self.pending_oauth_auth.is_some()
        {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        if self.can_start_request_preview()
            && ui
                .ctx()
                .input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.command)
        {
            self.preview_selected_request();
        }

        egui::Panel::bottom("bottom_bar").show_inside(ui, |ui| {
            egui::Frame::NONE
                .fill(crate::ui::theme::PANEL)
                .inner_margin(egui::Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("Save").clicked() {
                            self.save_snapshot();
                            if !self.status.starts_with("Save failed") {
                                self.status = "Draft saved".to_owned();
                            }
                        }
                        if ui.small_button("Export").clicked() {
                            self.export_workspace();
                        }
                        if ui
                            .add_enabled(
                                self.can_start_request_preview(),
                                egui::Button::new("Import").small(),
                            )
                            .clicked()
                        {
                            self.import_workspace();
                        }
                        let openapi_busy = self.pending_openapi_import.is_some()
                            || self.openapi_url_dialog_open;
                        if ui
                            .add_enabled(
                                !openapi_busy,
                                egui::Button::new("OpenAPI").small(),
                            )
                            .on_hover_text("Import from OpenAPI / Swagger file")
                            .clicked()
                        {
                            self.import_openapi_file();
                        }
                        if ui
                            .add_enabled(
                                !openapi_busy,
                                egui::Button::new("OA URL").small(),
                            )
                            .on_hover_text("Import from OpenAPI / Swagger URL")
                            .clicked()
                        {
                            self.openapi_url_dialog_open = true;
                        }
                        if ui.small_button("Clear").clicked() {
                            self.state.responses.clear();
                            self.state.ui.clear_selected_response();
                            self.save_snapshot();
                        }

                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if let Some(resp) = self.state.latest_response() {
                                    if let Some(code) = resp.status {
                                        let timing = resp
                                            .timing_ms
                                            .map(|t| format!(" · {t}ms"))
                                            .unwrap_or_default();
                                        ui.label(
                                            egui::RichText::new(format!("Last {code}{timing}"))
                                                .monospace()
                                                .color(crate::ui::theme::status_color(Some(code)))
                                                .small(),
                                        );
                                    }
                                }
                                ui.add_space(12.0);
                                ui.label(
                                    egui::RichText::new(&self.status)
                                        .color(crate::ui::theme::TEXT_MUTED)
                                        .small(),
                                );
                            },
                        );
                    });
                });
        });

        shell::show(
            ui,
            &mut self.state,
            &mut self.response_viewer,
            self.pending_request.is_some(),
        );
        self.handle_pending_ui_actions();
        self.show_import_confirmation(ui.ctx());
        self.show_openapi_import_confirmation(ui.ctx());
        self.show_openapi_url_dialog(ui.ctx());
        self.show_request_preview(ui.ctx());

        if ui.ctx().input(|i| i.viewport().close_requested()) && self.has_unsaved_changes() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending_close = true;
        }
        self.show_unsaved_changes_dialog(ui.ctx());
    }
}

fn apply_pending_request_context(
    summary: &mut crate::state::ResponseSummary,
    pending_context: Option<&PendingRequestContext>,
) {
    let Some(pending_context) = pending_context else {
        return;
    };

    summary.request_id = Some(pending_context.request_id.clone());
    summary.request_method = Some(pending_context.method.clone());
    summary.request_url = Some(pending_context.url.clone());
    summary.request_headers = pending_context.headers.clone();
}


fn create_storage() -> Option<FileStorage> {
    match FileStorage::new("./data") {
        Ok(storage) => Some(storage),
        Err(error) => {
            eprintln!("storage init failed: {error}");
            None
        }
    }
}

