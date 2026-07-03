use eframe::egui;
use std::{fs, time::Duration};

use crate::openapi::{OpenApiError, compute_merge, parse_spec};
use crate::openapi_import::PendingOpenApiImport;
use crate::openapi::source::fetch_url;
use crate::persistence::{FileStorage, persist_state, restore_workspace};
use crate::request_prep::{active_resolution_values, prepare_request_draft};
use crate::runtime::{AsyncRequest, AsyncRequestResult, Event, Runtime};
use crate::state::{AppState, View};
use crate::ui::intent::PanelIntent;
use crate::ui::response_viewer::ResponseViewerState;
use crate::ui::{request_preview_modal, shell};
use crate::workspace::{
    PendingWorkspaceImport, backup_workspace, preview_workspace_import,
    read_workspace_bundle_file, workspace_bundle_from_json, workspace_bundle_to_json,
};

/// Snapshot of a submitted request kept until its response arrives.
///
/// The `headers` field stores the request headers **with sensitive
/// values already redacted** (Authorization, Cookie, X-API-Key, …).
/// Redacting at capture time means the on-disk response history and any
/// `Debug` rendering of this struct never echo the live credentials.
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
    /// Data-mutation intents queued by UI panels during the current frame.
    /// Drained and applied by `apply_pending_intents` after the egui frame
    /// completes — this is the single funnel for panel-driven state changes.
    pending_intents: Vec<PanelIntent>,
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
                    pending_intents: Vec::new(),
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
                pending_intents: Vec::new(),
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
                pending_intents: Vec::new(),
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
                pending_intents: Vec::new(),
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

        let contents = match read_workspace_bundle_file(&path) {
            Ok(contents) => contents,
            Err(error) => {
                self.status = format!("Import failed: {error}");
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
                    headers: crate::runtime::types::redact_sensitive_headers(
                        &prepared_request.headers,
                    ),
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

    /// Drain `pending_intents` and apply each one. This is the *single*
    /// funnel for panel-driven data mutations — keeps validation, dirty
    /// tracking, and OAuth-cache invalidation in one place rather than
    /// scattered across UI panels.
    fn apply_pending_intents(&mut self) {
        let intents = std::mem::take(&mut self.pending_intents);
        for intent in intents {
            self.apply_intent(intent);
        }
    }

    fn apply_intent(&mut self, intent: PanelIntent) {
        let was_clear = matches!(intent, PanelIntent::ClearResponses);
        apply_intent_to_state(&mut self.state, intent);
        if was_clear {
            // Persist immediately — Clear is a destructive op the user
            // expects to survive an unexpected close.
            self.save_snapshot();
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
                    headers: crate::runtime::types::redact_sensitive_headers(
                        &prepared_request.headers,
                    ),
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
                                summary.response_headers =
                                    crate::runtime::types::redact_sensitive_headers(
                                        &info.headers,
                                    );
                                summary.content_type =
                                    info.header("content-type").or_else(|| info.media_hint());
                                summary.header_count = Some(info.header_count());
                                summary.preview_text = info.text_preview(400);
                                summary.body_text = info.text_preview(usize::MAX);
                                self.status = if info.truncated {
                                    format!(
                                        "Request {id} completed ({} in {} ms, body truncated)",
                                        info.status, info.duration_ms
                                    )
                                } else {
                                    format!(
                                        "Request {id} completed ({} in {} ms)",
                                        info.status, info.duration_ms
                                    )
                                };
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
                            self.pending_intents.push(PanelIntent::ClearResponses);
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
            &mut self.pending_intents,
            self.pending_request.is_some(),
        );
        // Drain panel-emitted intents through the single apply_intent
        // dispatcher so all data mutations land in one place.
        self.apply_pending_intents();
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

/// Pure state mutator for `PanelIntent`. Kept separate from
/// `ProbeApp::apply_intent` so unit tests can exercise every variant
/// without standing up a runtime, storage, or eframe context.
fn apply_intent_to_state(state: &mut AppState, intent: PanelIntent) {
    match intent {
        // ---- Collection-level request operations -------------------------
        PanelIntent::AddDefaultRequest => {
            let new_index = state.add_default_request();
            state.ui.select_request(new_index);
            state.ui.set_view(View::Editor);
        }
        PanelIntent::DuplicateSelectedRequest => {
            if let Some(new_index) = state.duplicate_selected_request() {
                state.ui.select_request(new_index);
                state.ui.set_view(View::Editor);
            }
        }
        PanelIntent::RemoveSelectedRequest => {
            let _ = state.remove_selected_request();
            state.ui.set_view(View::Editor);
        }

        // ---- Single-request edits ----------------------------------------
        PanelIntent::SetRequestMethod { index, method } => {
            if let Some(req) = state.requests.get_mut(index) {
                req.method = method;
            }
        }
        PanelIntent::SetRequestUrl { index, url, commit } => {
            if let Some(req) = state.requests.get_mut(index) {
                if commit {
                    if url.contains('?') {
                        req.adopt_url_query(&url);
                    } else {
                        req.set_url(&url);
                    }
                } else {
                    req.url = url;
                }
            }
        }
        PanelIntent::SetRequestName {
            index,
            name,
            commit,
        } => {
            if let Some(req) = state.requests.get_mut(index) {
                if commit {
                    req.set_request_name(&name);
                } else {
                    req.name = name;
                }
            }
        }
        PanelIntent::SetRequestFolder {
            index,
            folder,
            commit,
        } => {
            if let Some(req) = state.requests.get_mut(index) {
                if commit {
                    req.set_folder_path(&folder);
                } else {
                    req.folder = folder;
                }
            }
        }
        PanelIntent::SetRequestAuth { index, auth } => {
            if let Some(req) = state.requests.get_mut(index) {
                req.auth = auth;
            }
        }
        PanelIntent::SetRequestBody { index, body } => {
            if let Some(req) = state.requests.get_mut(index) {
                req.body = body;
            }
        }
        PanelIntent::SetAttachOAuth { index, attach } => {
            if let Some(req) = state.requests.get_mut(index) {
                req.attach_oauth = attach;
            }
        }
        PanelIntent::SetRequestQueryParams { index, params } => {
            if let Some(req) = state.requests.get_mut(index) {
                req.query_params = params;
            }
        }
        PanelIntent::SetRequestHeaders { index, headers } => {
            if let Some(req) = state.requests.get_mut(index) {
                req.headers = headers;
            }
        }

        // ---- Environment ops ---------------------------------------------
        PanelIntent::AddAutoNamedEnvironment => {
            let name = next_auto_environment_name(state);
            if state.add_environment(&name).is_ok() {
                let _ = state.select_environment(&name);
            }
        }
        PanelIntent::RemoveEnvironment { name } => {
            let _ = state.remove_environment(&name);
        }
        PanelIntent::SelectEnvironment { name } => {
            let _ = state.select_environment(&name);
        }
        PanelIntent::RenameActiveEnvironment { new_name } => {
            let trimmed = new_name.trim();
            if trimmed.is_empty() {
                return;
            }
            let active_index = state.active_environment_index();
            let name_in_use = active_index.is_some_and(|active| {
                state.environments.iter().enumerate().any(
                    |(index, environment)| {
                        index != active && environment.name == trimmed
                    },
                )
            });
            if name_in_use {
                return;
            }
            if let Some(environment) = state.active_environment_mut() {
                environment.name = trimmed.to_owned();
            }
        }
        PanelIntent::SetEnvironmentVars { name, vars } => {
            if let Some(index) = state.find_environment_index(&name)
                && let Some(environment) = state.environments.get_mut(index)
            {
                environment.vars = vars;
            }
        }

        // ---- Response history --------------------------------------------
        PanelIntent::ClearResponses => {
            state.responses.clear();
            state.ui.clear_selected_response();
        }
    }
}

fn next_auto_environment_name(state: &AppState) -> String {
    let mut next_index = state.environments.len().saturating_add(1);
    loop {
        let candidate = format!("Env {next_index}");
        if state.find_environment_index(&candidate).is_none() {
            return candidate;
        }
        next_index = next_index.saturating_add(1);
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

#[cfg(test)]
mod apply_intent_tests {
    use super::*;
    use crate::state::request::RequestAuth;
    use std::collections::BTreeMap;

    fn state_with_one_request() -> AppState {
        let mut state = AppState::new();
        let _ = state.try_add_request("GET", "https://example.com/").unwrap();
        state.ui.select_request(0);
        state
    }

    #[test]
    fn set_request_method_updates_only_method() {
        let mut state = state_with_one_request();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestMethod {
                index: 0,
                method: "POST".into(),
            },
        );
        assert_eq!(state.requests[0].method, "POST");
        assert_eq!(state.requests[0].url, "https://example.com/");
    }

    #[test]
    fn set_request_url_raw_does_not_split_query() {
        let mut state = state_with_one_request();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestUrl {
                index: 0,
                url: "https://example.com/?q=hi".into(),
                commit: false,
            },
        );
        assert_eq!(state.requests[0].url, "https://example.com/?q=hi");
        assert!(
            state.requests[0].query_params.is_empty(),
            "raw set must not split query into params"
        );
    }

    #[test]
    fn set_request_url_commit_splits_query_into_params() {
        let mut state = state_with_one_request();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestUrl {
                index: 0,
                url: "https://example.com/path?foo=bar&baz=qux".into(),
                commit: true,
            },
        );
        assert_eq!(state.requests[0].url, "https://example.com/path");
        assert_eq!(
            state.requests[0].query_params,
            vec![
                ("foo".to_owned(), "bar".to_owned()),
                ("baz".to_owned(), "qux".to_owned()),
            ]
        );
    }

    #[test]
    fn set_request_name_raw_keeps_trailing_whitespace() {
        let mut state = state_with_one_request();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestName {
                index: 0,
                name: "Login ".into(),
                commit: false,
            },
        );
        assert_eq!(state.requests[0].name, "Login ");
    }

    #[test]
    fn set_request_name_commit_normalises() {
        let mut state = state_with_one_request();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestName {
                index: 0,
                name: "  Login  ".into(),
                commit: true,
            },
        );
        assert_eq!(state.requests[0].name, "Login");
    }

    #[test]
    fn set_request_folder_commit_collapses_path() {
        let mut state = state_with_one_request();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestFolder {
                index: 0,
                folder: "  Collections // Auth  ".into(),
                commit: true,
            },
        );
        assert_eq!(state.requests[0].folder, "Collections/Auth");
    }

    #[test]
    fn add_and_remove_default_request_updates_selection() {
        let mut state = AppState::new();
        assert!(state.requests.is_empty());

        apply_intent_to_state(&mut state, PanelIntent::AddDefaultRequest);
        assert_eq!(state.requests.len(), 1);
        assert_eq!(state.ui.selected_request, Some(0));

        apply_intent_to_state(&mut state, PanelIntent::AddDefaultRequest);
        assert_eq!(state.requests.len(), 2);
        assert_eq!(state.ui.selected_request, Some(1));

        apply_intent_to_state(&mut state, PanelIntent::RemoveSelectedRequest);
        assert_eq!(state.requests.len(), 1);
    }

    #[test]
    fn duplicate_selected_request_clones_into_new_slot() {
        let mut state = state_with_one_request();
        state.requests[0].name = "Original".into();

        apply_intent_to_state(&mut state, PanelIntent::DuplicateSelectedRequest);

        assert_eq!(state.requests.len(), 2);
        assert_eq!(state.ui.selected_request, Some(1));
        assert_eq!(state.requests[1].method, "GET");
    }

    #[test]
    fn set_request_auth_replaces_auth() {
        let mut state = state_with_one_request();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestAuth {
                index: 0,
                auth: RequestAuth::Bearer {
                    token: "{{API_TOKEN}}".into(),
                },
            },
        );
        assert!(matches!(state.requests[0].auth, RequestAuth::Bearer { .. }));
    }

    #[test]
    fn set_request_body_none_clears_body() {
        let mut state = state_with_one_request();
        state.requests[0].body = Some("payload".into());
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestBody {
                index: 0,
                body: None,
            },
        );
        assert!(state.requests[0].body.is_none());
    }

    #[test]
    fn set_request_headers_replaces_collection() {
        let mut state = state_with_one_request();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestHeaders {
                index: 0,
                headers: vec![("Accept".into(), "application/json".into())],
            },
        );
        assert_eq!(state.requests[0].headers.len(), 1);
    }

    #[test]
    fn set_attach_oauth_toggle() {
        let mut state = state_with_one_request();
        let initial = state.requests[0].attach_oauth;

        apply_intent_to_state(
            &mut state,
            PanelIntent::SetAttachOAuth {
                index: 0,
                attach: !initial,
            },
        );
        assert_eq!(state.requests[0].attach_oauth, !initial);

        apply_intent_to_state(
            &mut state,
            PanelIntent::SetAttachOAuth {
                index: 0,
                attach: initial,
            },
        );
        assert_eq!(state.requests[0].attach_oauth, initial);
    }

    #[test]
    fn out_of_bounds_request_index_is_a_no_op() {
        let mut state = state_with_one_request();
        let before = state.requests.clone();
        apply_intent_to_state(
            &mut state,
            PanelIntent::SetRequestMethod {
                index: 999,
                method: "POST".into(),
            },
        );
        assert_eq!(state.requests, before, "OOB index must not panic or mutate");
    }

    #[test]
    fn add_auto_named_environment_picks_next_free_name() {
        let mut state = AppState::new();
        let initial = state.environments.len();
        apply_intent_to_state(&mut state, PanelIntent::AddAutoNamedEnvironment);
        assert_eq!(state.environments.len(), initial + 1);
    }

    #[test]
    fn rename_active_environment_rejects_empty_and_duplicate() {
        let mut state = AppState::new();
        let _ = state.add_environment("Staging").unwrap();
        let _ = state.select_environment("Staging");

        // Empty name → no-op.
        apply_intent_to_state(
            &mut state,
            PanelIntent::RenameActiveEnvironment {
                new_name: "   ".into(),
            },
        );
        assert_eq!(state.active_environment_name(), Some("Staging"));

        // Duplicate name → no-op.
        apply_intent_to_state(
            &mut state,
            PanelIntent::RenameActiveEnvironment {
                new_name: "Default".into(),
            },
        );
        assert_eq!(state.active_environment_name(), Some("Staging"));

        // Unique name → applied.
        apply_intent_to_state(
            &mut state,
            PanelIntent::RenameActiveEnvironment {
                new_name: "Prod".into(),
            },
        );
        assert_eq!(state.active_environment_name(), Some("Prod"));
    }

    #[test]
    fn set_environment_vars_replaces_vars() {
        let mut state = AppState::new();
        let name = state.active_environment_name().unwrap().to_owned();
        let mut vars = BTreeMap::new();
        vars.insert("base_url".into(), "https://api.example.com".into());

        apply_intent_to_state(
            &mut state,
            PanelIntent::SetEnvironmentVars {
                name: name.clone(),
                vars: vars.clone(),
            },
        );
        assert_eq!(state.active_variables(), Some(&vars));
    }

    #[test]
    fn clear_responses_drops_history_and_selection() {
        let mut state = state_with_one_request();
        state.responses.push(crate::state::ResponseSummary::default());
        state.ui.select_response(0);

        apply_intent_to_state(&mut state, PanelIntent::ClearResponses);
        assert!(state.responses.is_empty());
        assert_eq!(state.ui.selected_response, None);
    }
}

