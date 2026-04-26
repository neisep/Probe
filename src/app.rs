use base64::Engine;
use eframe::egui;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::openapi::{ImportedOperation, MergePreview, OpenApiError, compute_merge, parse_spec};
use crate::openapi::source::fetch_url;
use crate::persistence::{EnvFile, FileStorage, RequestFile};
use crate::runtime::{
    AsyncRequest, AsyncRequestResult, Event, ResolutionError, ResolutionErrorKind,
    ResolutionValues, Runtime, UnresolvedBehavior, resolve_body_text, resolve_headers,
    resolve_text_with_behavior,
};
use crate::state::request::{
    ApiKeyLocation, RequestAuth, normalize_folder_path, normalize_request_name,
};
use crate::state::{AppState, View};
use crate::ui::response_viewer::ResponseViewerState;
use crate::ui::{request_preview_modal, shell};
use serde::{Deserialize, Serialize};

const WORKSPACE_BUNDLE_FORMAT_VERSION: u32 = 1;

#[derive(Serialize)]
struct WorkspaceBundleRef<'a> {
    format_version: u32,
    requests: &'a Vec<crate::state::RequestDraft>,
    responses: &'a Vec<crate::state::ResponseSummary>,
    environments: &'a Vec<crate::state::Environment>,
    active_environment: Option<usize>,
    ui: &'a crate::state::UIState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceBundle {
    format_version: u32,
    #[serde(default)]
    requests: Vec<crate::state::RequestDraft>,
    #[serde(default)]
    responses: Vec<crate::state::ResponseSummary>,
    #[serde(default)]
    environments: Vec<crate::state::Environment>,
    #[serde(default)]
    active_environment: Option<usize>,
    #[serde(default)]
    ui: crate::state::UIState,
}

#[derive(Debug, Clone)]
struct PendingRequestContext {
    request_id: String,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct WorkspaceImportPreview {
    request_count: usize,
    response_count: usize,
    environment_count: usize,
    selected_request_label: Option<String>,
}

struct PendingWorkspaceImport {
    path: PathBuf,
    preview: WorkspaceImportPreview,
    imported_state: AppState,
}

struct PendingOpenApiImport {
    source: String,
    preview: MergePreview,
    ops: Vec<ImportedOperation>,
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
    show_openapi_url_dialog: bool,
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
                    status: "First slice ready".to_owned(),
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
                    show_openapi_url_dialog: false,
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
                show_openapi_url_dialog: false,
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
                show_openapi_url_dialog: false,
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
                show_openapi_url_dialog: false,
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
        if self.pending_workspace_import.is_none() {
            return;
        }

        let mut confirm_import = false;
        let mut cancel_import = false;
        let preview = self
            .pending_workspace_import
            .as_ref()
            .map(|pending_import| pending_import.preview.clone())
            .expect("preview exists when pending import exists");
        let import_path = self
            .pending_workspace_import
            .as_ref()
            .map(|pending_import| pending_import.path.clone())
            .expect("path exists when pending import exists");

        egui::Window::new("Confirm workspace import")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Replace the current workspace with {}?",
                    import_path.display()
                ));
                ui.add_space(6.0);
                ui.label(format!("Requests: {}", preview.request_count));
                ui.label(format!("Responses: {}", preview.response_count));
                ui.label(format!("Environments: {}", preview.environment_count));
                if let Some(label) = preview.selected_request_label.as_deref() {
                    ui.label(format!("Selected request: {label}"));
                }
                ui.add_space(6.0);
                ui.small(
                    "Probe will create an automatic backup of the current workspace before applying the import.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Import and replace").clicked() {
                        confirm_import = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel_import = true;
                    }
                });
            });

        if cancel_import {
            self.pending_workspace_import = None;
            self.status = "Import cancelled".to_owned();
        }

        if confirm_import {
            self.confirm_workspace_import();
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
        self.show_openapi_url_dialog = false;
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

        let mut confirm = false;
        let mut cancel = false;

        let (source, new_count, updated_count, unchanged_count) = (
            pending.source.clone(),
            pending.preview.new_count,
            pending.preview.updated_count,
            pending.preview.unchanged_count,
        );

        egui::Window::new("Confirm OpenAPI import")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Source: {source}"));
                ui.add_space(6.0);
                ui.label(format!("New requests:       {new_count}"));
                ui.label(format!("Updated requests:   {updated_count}"));
                ui.label(format!("Unchanged requests: {unchanged_count}"));
                ui.add_space(4.0);
                ui.small("Auth, headers, and body you have set on existing requests will be preserved.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Import").clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if cancel {
            self.pending_openapi_import = None;
            self.status = "OpenAPI import cancelled".to_owned();
        }
        if confirm {
            self.confirm_openapi_import();
        }
    }

    fn show_openapi_url_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_openapi_url_dialog {
            return;
        }

        let mut fetch = false;
        let mut close = false;

        egui::Window::new("Import OpenAPI from URL")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Spec URL:");
                let response = ui.text_edit_singleline(&mut self.openapi_url_input);
                if response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    fetch = true;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Fetch").clicked() {
                        fetch = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if close {
            self.show_openapi_url_dialog = false;
        }
        if fetch {
            self.import_openapi_from_url();
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
                self.status = format_error(&error_info);
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

        let mut used_paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (index, request) in self.state.requests.iter().enumerate() {
            let relative_path = reserve_request_relative_path(request, index, &mut used_paths);
            let file = RequestFile {
                relative_path,
                request: request.clone(),
            };
            if let Err(error) = storage.save_request(&file) {
                self.status = format!("Save failed: {error}");
                return;
            }
        }
        if let Err(error) = storage.delete_stale_requests(&used_paths) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let env_file = build_env_file(&self.state);
        if let Err(error) = storage.save_env_file(&env_file) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let mut response_ids = Vec::new();
        for (index, response) in self.state.responses.iter().enumerate() {
            let response_id = format!("response-{index}");
            let stored_response = crate::persistence::models::ResponseSummary {
                id: response_id.clone(),
                request_id: response.request_id.clone(),
                status_code: response.status,
                summary: response.error.clone(),
                duration_ms: response.timing_ms.map(|timing_ms| timing_ms as u64),
                created_at: None,
            };

            if let Err(error) = storage.save_response_summary(&stored_response) {
                self.status = format!("Save failed: {error}");
                return;
            }

            let response_preview = crate::persistence::models::ResponsePreview {
                id: response_id.clone(),
                response_id: response_id.clone(),
                summary: response
                    .error
                    .clone()
                    .or_else(|| response.status.map(|status| format!("HTTP {status}"))),
                request_method: response.request_method.clone(),
                request_url: response.request_url.clone(),
                content_preview: response.preview_text.clone(),
                content_body: response.body_text.clone(),
                content_type: response.content_type.clone(),
                header_count: response.header_count,
                size_bytes: response.size_bytes,
                tags: vec![],
                created_at: None,
            };

            if let Err(error) = storage.save_response_preview(&response_preview) {
                self.status = format!("Save failed: {error}");
                return;
            }

            let response_preview_detail = crate::persistence::models::ResponsePreviewDetail {
                request_headers: response
                    .request_headers
                    .iter()
                    .cloned()
                    .map(crate::persistence::models::HeaderEntry::from)
                    .collect(),
                response_headers: response
                    .response_headers
                    .iter()
                    .cloned()
                    .map(crate::persistence::models::HeaderEntry::from)
                    .collect(),
            };

            if let Err(error) =
                storage.save_response_preview_detail(&response_id, &response_preview_detail)
            {
                self.status = format!("Save failed: {error}");
                return;
            }

            response_ids.push(response_id);
        }
        if let Err(error) = storage.delete_stale_response_ids(&response_ids) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let selected_request_id = self
            .state
            .selected_request_index()
            .map(AppState::request_id_for_index);
        let selected_response_id = self
            .state
            .ui
            .selected_response
            .map(|index| format!("response-{index}"));
        let active_environment_name = self
            .state
            .active_environment()
            .map(|environment| environment.name.clone());

        let session_state = crate::persistence::models::SessionState {
            selected_request: selected_request_id,
            selected_response: selected_response_id,
            active_environment: active_environment_name,
            active_view: Some(self.state.ui.view.label().to_owned()),
            open_panels: vec![
                "sidebar".to_owned(),
                "inspector".to_owned(),
                "status_bar".to_owned(),
                "bottom_bar".to_owned(),
            ],
            updated_at: None,
        };

        if let Err(error) = storage.save_session_state(&session_state) {
            self.status = format!("Save failed: {error}");
            return;
        }

        self.saved_requests = self.state.requests.clone();
        self.saved_environments = self.state.environments.clone();
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

        let mut save_and_close = false;
        let mut close_without_saving = false;
        let mut cancel = false;

        let has_pending_import =
            self.pending_openapi_import.is_some() || self.pending_workspace_import.is_some();
        let message = if has_pending_import {
            "You have unsaved changes or a pending import in progress. Would you like to save before closing?"
        } else {
            "You have unsaved changes. Would you like to save before closing?"
        };

        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(message);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save and close").clicked() {
                        save_and_close = true;
                    }
                    if ui.button("Close without saving").clicked() {
                        close_without_saving = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if cancel {
            self.pending_close = false;
        }
        if close_without_saving {
            self.saved_requests = self.state.requests.clone();
            self.saved_environments = self.state.environments.clone();
            self.pending_openapi_import = None;
            self.pending_workspace_import = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if save_and_close {
            self.save_snapshot();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
                                summary.error = Some(format_error(&err));
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
                            || self.show_openapi_url_dialog;
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
                            self.show_openapi_url_dialog = true;
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

fn restore_workspace(state: &mut AppState, storage: &FileStorage) {
    restore_environments_from_file(state, storage);
    restore_requests_from_files(state, storage);
    restore_responses_from_sidecars(state, storage);
    apply_session_state(state, storage);
    state.ensure_valid_selection();
}

fn restore_requests_from_files(state: &mut AppState, storage: &FileStorage) {
    let Ok(files) = storage.list_requests() else {
        return;
    };
    if files.is_empty() {
        return;
    }

    let restored: Vec<crate::state::RequestDraft> =
        files.into_iter().map(|file| file.request).collect();

    state.requests = restored;
    state.ui.selected_request = None;
}

fn restore_environments_from_file(state: &mut AppState, storage: &FileStorage) {
    let env_file = match storage.load_env_file() {
        Ok(envs) if !envs.is_empty() => envs,
        _ => {
            state.ensure_valid_environment_selection();
            return;
        }
    };
    let private = storage.load_private_env_file().ok().flatten();

    let mut restored = Vec::with_capacity(env_file.len());
    for (name, vars) in env_file {
        let mut merged = vars;
        if let Some(private) = private.as_ref() {
            if let Some(private_vars) = private.get(&name) {
                for (k, v) in private_vars {
                    merged.insert(k.clone(), v.clone());
                }
            }
        }
        restored.push(crate::state::Environment {
            name,
            vars: merged,
        });
    }

    if restored.is_empty() {
        state.ensure_valid_environment_selection();
        return;
    }

    state.environments = restored;
    state.active_environment = None;
    state.ensure_valid_environment_selection();
}

fn restore_responses_from_sidecars(state: &mut AppState, storage: &FileStorage) {
    let Ok(ids) = storage.list_response_ids() else {
        return;
    };
    if ids.is_empty() {
        return;
    }

    state.responses.clear();
    for response_id in ids {
        let Ok(stored_response) = storage.load_response_summary(&response_id) else {
            continue;
        };

        let preview = storage.load_response_preview(&response_id).ok();
        let detail = storage.load_response_preview_detail(&response_id).ok();

        let mut restored = crate::state::ResponseSummary {
            request_id: stored_response.request_id.clone(),
            request_method: preview
                .as_ref()
                .and_then(|preview| preview.request_method.clone()),
            request_url: preview
                .as_ref()
                .and_then(|preview| preview.request_url.clone()),
            request_headers: detail
                .as_ref()
                .map(|detail| {
                    detail
                        .request_headers
                        .iter()
                        .map(|header| (header.name.clone(), header.value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            response_headers: detail
                .as_ref()
                .map(|detail| {
                    detail
                        .response_headers
                        .iter()
                        .map(|header| (header.name.clone(), header.value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            status: stored_response.status_code,
            timing_ms: stored_response
                .duration_ms
                .map(|duration_ms| duration_ms as u128),
            size_bytes: preview.as_ref().and_then(|preview| preview.size_bytes),
            content_type: preview.as_ref().and_then(|preview| preview.content_type.clone()),
            header_count: preview.as_ref().and_then(|preview| preview.header_count),
            preview_text: preview
                .as_ref()
                .and_then(|preview| preview.content_preview.clone()),
            body_text: preview.as_ref().and_then(|preview| preview.content_body.clone()),
            error: stored_response.summary.clone(),
        };

        if let Some(request_id) = restored.request_id.as_deref()
            && let Some(request_index) = state.find_request_index_by_id(request_id)
            && let Some(request) = state.requests.get(request_index)
        {
            if restored.request_method.is_none() {
                restored.request_method = Some(request.method.clone());
            }
            if restored.request_url.is_none() {
                restored.request_url = Some(request.url.clone());
            }
        }

        state.responses.push(restored);
    }
}

fn apply_session_state(state: &mut AppState, storage: &FileStorage) {
    let Ok(session) = storage.load_session_state() else {
        return;
    };

    if let Some(active_view) = session.active_view.as_deref().and_then(View::from_label) {
        state.ui.set_view(active_view);
    }

    if let Some(selected_request_id) = session.selected_request.as_deref() {
        if let Some(index) = state.find_request_index_by_id(selected_request_id) {
            state.ui.select_request(index);
        }
    }

    if let Some(selected_response_id) = session.selected_response.as_deref() {
        if let Some(stripped) = selected_response_id.strip_prefix("response-") {
            if let Ok(index) = stripped.parse::<usize>() {
                if index < state.responses.len() {
                    state.ui.select_response(index);
                    select_request_for_response(state, index);
                }
            }
        }
    }

    if let Some(active_environment_name) = session.active_environment.as_deref() {
        state.select_environment(active_environment_name);
    }
}

fn build_env_file(state: &AppState) -> EnvFile {
    let mut env_file = EnvFile::new();
    for environment in &state.environments {
        let name = environment.name.trim();
        if name.is_empty() {
            continue;
        }
        let mut vars: BTreeMap<String, String> = BTreeMap::new();
        for (key, value) in &environment.vars {
            vars.insert(key.clone(), value.clone());
        }
        env_file.insert(name.to_owned(), vars);
    }
    env_file
}

fn reserve_request_relative_path(
    request: &crate::state::RequestDraft,
    fallback_index: usize,
    used: &mut std::collections::BTreeSet<String>,
) -> String {
    let folder = normalize_folder_path(&request.folder);
    let raw_name = normalize_request_name(&request.name)
        .unwrap_or_else(|| format!("untitled-{fallback_index}"));
    let slug = slugify_path_segment(&raw_name);
    let slug = if slug.is_empty() {
        format!("untitled-{fallback_index}")
    } else {
        slug
    };
    let base = if folder.is_empty() {
        slug.clone()
    } else {
        format!("{folder}/{slug}")
    };

    let mut candidate = base.clone();
    let mut suffix = 2;
    while used.contains(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    used.insert(candidate.clone());
    candidate
}

fn slugify_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_was_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            out.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn active_resolution_values(state: &AppState) -> ResolutionValues {
    state.active_variables().cloned().unwrap_or_default()
}


fn preview_workspace_import(state: &AppState) -> WorkspaceImportPreview {
    WorkspaceImportPreview {
        request_count: state.requests.len(),
        response_count: state.responses.len(),
        environment_count: state.environments.len(),
        selected_request_label: state
            .selected_request()
            .map(|request| request.display_name()),
    }
}

fn backup_workspace(state: &AppState) -> Result<PathBuf, String> {
    let json = workspace_bundle_to_json(state)?;
    let backup_dir = PathBuf::from(crate::oauth::DATA_DIR).join("backups");
    fs::create_dir_all(&backup_dir).map_err(|error| {
        format!(
            "could not create backup directory {}: {error}",
            backup_dir.display()
        )
    })?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("could not compute backup timestamp: {error}"))?
        .as_millis();
    let backup_path = backup_dir.join(format!("pre-import-{timestamp}.probe.json"));
    fs::write(&backup_path, json)
        .map_err(|error| format!("could not write backup {}: {error}", backup_path.display()))?;
    Ok(backup_path)
}

fn workspace_bundle_to_json(state: &AppState) -> Result<String, String> {
    let bundle = WorkspaceBundleRef {
        format_version: WORKSPACE_BUNDLE_FORMAT_VERSION,
        requests: &state.requests,
        responses: &state.responses,
        environments: &state.environments,
        active_environment: state.active_environment,
        ui: &state.ui,
    };
    serde_json::to_string_pretty(&bundle).map_err(|error| error.to_string())
}

fn workspace_bundle_from_json(json: &str) -> Result<AppState, String> {
    let bundle: WorkspaceBundle = serde_json::from_str(json)
        .map_err(|error| format!("invalid workspace bundle JSON: {error}"))?;
    state_from_workspace_bundle(bundle)
}


fn state_from_workspace_bundle(bundle: WorkspaceBundle) -> Result<AppState, String> {
    if bundle.format_version != WORKSPACE_BUNDLE_FORMAT_VERSION {
        return Err(format!(
            "unsupported workspace format version {} (expected {})",
            bundle.format_version, WORKSPACE_BUNDLE_FORMAT_VERSION
        ));
    }

    let mut state = AppState {
        ui: bundle.ui,
        requests: bundle.requests,
        responses: bundle.responses,
        environments: bundle.environments,
        active_environment: bundle.active_environment,
    };

    normalize_imported_state(&mut state)?;
    hydrate_response_request_metadata(&mut state);
    state.ensure_valid_selection();
    Ok(state)
}

fn normalize_imported_state(state: &mut AppState) -> Result<(), String> {
    for (index, request) in state.requests.iter_mut().enumerate() {
        let request_label = describe_imported_request(index, request);
        let method = request.method.trim().to_uppercase();
        if method.is_empty() {
            return Err(format!("{request_label} has an empty method"));
        }
        let url = request.url.trim().to_owned();
        if url.is_empty() {
            return Err(format!("{request_label} has an empty URL"));
        }

        let name = request.name.clone();
        let folder = request.folder.clone();
        request.method = method;
        request.set_request_name(&name);
        request.set_folder_path(&folder);
        request.set_url(&url);
    }

    let mut environment_names = std::collections::BTreeSet::new();
    for (index, environment) in state.environments.iter_mut().enumerate() {
        let name = environment.name.trim().to_owned();
        if name.is_empty() {
            return Err(format!(
                "imported environment {} has an empty name",
                index + 1
            ));
        }
        if !environment_names.insert(name.clone()) {
            return Err(format!("duplicate imported environment '{name}'"));
        }
        environment.name = name;
    }

    Ok(())
}

fn describe_imported_request(index: usize, request: &crate::state::RequestDraft) -> String {
    if let Some(name) = normalize_request_name(&request.name) {
        return format!("Imported request {} ('{}')", index + 1, name);
    }

    let method = request.method.trim();
    let url = request.url.trim();
    if !method.is_empty() || !url.is_empty() {
        return format!(
            "Imported request {} ('{}')",
            index + 1,
            format!("{method} {url}").trim()
        );
    }

    format!("Imported request {}", index + 1)
}

fn hydrate_response_request_metadata(state: &mut AppState) {
    let request_lookup: std::collections::BTreeMap<String, (String, String)> = state
        .requests
        .iter()
        .enumerate()
        .map(|(index, request)| {
            (
                AppState::request_id_for_index(index),
                (request.method.clone(), request.url.clone()),
            )
        })
        .collect();

    for response in &mut state.responses {
        let Some(request_id) = response.request_id.clone() else {
            continue;
        };

        let Some((method, url)) = request_lookup.get(&request_id) else {
            response.request_id = None;
            continue;
        };

        if response.request_method.is_none() {
            response.request_method = Some(method.clone());
        }
        if response.request_url.is_none() {
            response.request_url = Some(url.clone());
        }
    }
}

fn prepare_request_draft(
    request: &crate::state::RequestDraft,
    resolution_values: &ResolutionValues,
) -> Result<AsyncRequest, ResolutionError> {
    let resolved_url = resolve_text_with_behavior(
        "url",
        &request.url,
        resolution_values,
        UnresolvedBehavior::Error,
    )?;
    let mut resolved_headers = resolve_headers(
        &request.headers,
        resolution_values,
        UnresolvedBehavior::Error,
    )?;
    let resolved_body = resolve_body_text(
        request.body.as_ref().map(|body| body.as_bytes()),
        resolution_values,
        UnresolvedBehavior::Error,
    )?;
    let mut resolved_query_params = Vec::with_capacity(request.query_params.len());

    for (index, (name, value)) in request.query_params.iter().enumerate() {
        let resolved_name = resolve_text_with_behavior(
            &format!("query[{index}].name"),
            name,
            resolution_values,
            UnresolvedBehavior::Error,
        )?;
        if resolved_name.trim().is_empty() {
            continue;
        }

        let resolved_value = resolve_text_with_behavior(
            &format!("query[{index}].value"),
            value,
            resolution_values,
            UnresolvedBehavior::Error,
        )?;
        resolved_query_params.push((resolved_name, resolved_value));
    }
    let resolved_auth = resolve_request_auth(&request.auth, resolution_values)?;
    apply_auth_headers(&mut resolved_headers, resolved_auth.headers)?;
    resolved_query_params.extend(resolved_auth.query_params);

    Ok(AsyncRequest {
        url: build_request_url(&resolved_url, &resolved_query_params)?,
        method: request.method.clone(),
        headers: resolved_headers,
        body: resolved_body,
    })
}

#[derive(Default)]
struct ResolvedAuth {
    headers: Vec<(String, String)>,
    query_params: Vec<(String, String)>,
}

fn resolve_request_auth(
    auth: &RequestAuth,
    resolution_values: &ResolutionValues,
) -> Result<ResolvedAuth, ResolutionError> {
    match auth {
        RequestAuth::None => Ok(ResolvedAuth::default()),
        RequestAuth::Bearer { token } => {
            let token = resolve_text_with_behavior(
                "auth.bearer.token",
                token,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            if token.trim().is_empty() {
                return Err(invalid_request_error(
                    "auth",
                    "bearer token cannot be empty",
                ));
            }

            Ok(ResolvedAuth {
                headers: vec![("Authorization".to_owned(), format!("Bearer {token}"))],
                query_params: Vec::new(),
            })
        }
        RequestAuth::Basic { username, password } => {
            let username = resolve_text_with_behavior(
                "auth.basic.username",
                username,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            let password = resolve_text_with_behavior(
                "auth.basic.password",
                password,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            if username.is_empty() && password.is_empty() {
                return Err(invalid_request_error(
                    "auth",
                    "basic auth requires a username or password",
                ));
            }

            let encoded = base64::prelude::BASE64_STANDARD.encode(format!("{username}:{password}"));
            Ok(ResolvedAuth {
                headers: vec![("Authorization".to_owned(), format!("Basic {encoded}"))],
                query_params: Vec::new(),
            })
        }
        RequestAuth::ApiKey {
            location,
            name,
            value,
        } => {
            let name = resolve_text_with_behavior(
                "auth.api_key.name",
                name,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            let value = resolve_text_with_behavior(
                "auth.api_key.value",
                value,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            if name.trim().is_empty() {
                return Err(invalid_request_error(
                    "auth",
                    "api key name cannot be empty",
                ));
            }
            if value.trim().is_empty() {
                return Err(invalid_request_error(
                    "auth",
                    "api key value cannot be empty",
                ));
            }

            match location {
                ApiKeyLocation::Header => Ok(ResolvedAuth {
                    headers: vec![(name, value)],
                    query_params: Vec::new(),
                }),
                ApiKeyLocation::Query => Ok(ResolvedAuth {
                    headers: Vec::new(),
                    query_params: vec![(name, value)],
                }),
            }
        }
    }
}

fn apply_auth_headers(
    existing_headers: &mut Vec<(String, String)>,
    auth_headers: Vec<(String, String)>,
) -> Result<(), ResolutionError> {
    for (auth_name, _) in &auth_headers {
        if existing_headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(auth_name))
        {
            return Err(invalid_request_error(
                "auth",
                &format!("auth header '{auth_name}' conflicts with an existing header"),
            ));
        }
    }

    existing_headers.extend(auth_headers);
    Ok(())
}

fn invalid_request_error(target: &str, details: &str) -> ResolutionError {
    ResolutionError {
        kind: ResolutionErrorKind::InvalidPlaceholder,
        target: target.to_owned(),
        placeholder: None,
        details: Some(details.to_owned()),
    }
}

fn build_request_url(
    base_url: &str,
    query_params: &[(String, String)],
) -> Result<String, ResolutionError> {
    if query_params.is_empty() {
        return Ok(base_url.to_owned());
    }

    let mut url = reqwest::Url::parse(base_url).map_err(|error| ResolutionError {
        kind: ResolutionErrorKind::InvalidPlaceholder,
        target: "url".to_owned(),
        placeholder: None,
        details: Some(format!("invalid url: {error}")),
    })?;
    {
        let mut serializer = url.query_pairs_mut();
        for (name, value) in query_params {
            serializer.append_pair(name, value);
        }
    }

    Ok(url.to_string())
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

fn select_request_for_response(state: &mut AppState, response_index: usize) {
    if let Some(request_id) = state
        .responses
        .get(response_index)
        .and_then(|response| response.request_id.as_deref())
        && let Some(request_index) = state.find_request_index_by_id(request_id)
    {
        state.ui.select_request(request_index);
    };
}

fn format_error(error: &crate::runtime::ErrorInfo) -> String {
    match (&error.kind, &error.code, &error.details) {
        (Some(kind), Some(code), Some(details)) => {
            format!("{} [{kind}] ({code}): {details}", error.message)
        }
        (Some(kind), Some(code), None) => format!("{} [{kind}] ({code})", error.message),
        (Some(kind), None, Some(details)) => format!("{} [{kind}]: {details}", error.message),
        (Some(kind), None, None) => format!("{} [{kind}]", error.message),
        (None, Some(code), Some(details)) => format!("{} ({code}): {details}", error.message),
        (None, Some(code), None) => format!("{} ({code})", error.message),
        (None, None, Some(details)) => format!("{}: {details}", error.message),
        (None, None, None) => error.message.clone(),
    }
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
mod tests {
    use super::{
        build_request_url, prepare_request_draft, workspace_bundle_from_json,
        workspace_bundle_to_json,
    };
    use crate::state::request::{ApiKeyLocation, RequestAuth};
    use crate::state::{Environment, RequestDraft, View};
    use std::collections::BTreeMap;

    #[test]
    fn build_request_url_appends_encoded_query_params() {
        let request_url = build_request_url(
            "https://example.com/items#details",
            &[
                ("page".to_owned(), "1".to_owned()),
                ("search".to_owned(), "hello world".to_owned()),
            ],
        )
        .expect("query params should build a valid url");
        let url = reqwest::Url::parse(&request_url).expect("built url should parse");
        let query_pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();

        assert_eq!(url.fragment(), Some("details"));
        assert_eq!(
            query_pairs,
            vec![
                ("page".to_owned(), "1".to_owned()),
                ("search".to_owned(), "hello world".to_owned()),
            ]
        );
    }

    #[test]
    fn prepare_request_draft_resolves_query_placeholders() {
        let mut request = RequestDraft::default_request();
        request.set_url("https://example.com/items");
        request.query_params = vec![("search".to_owned(), "{{term}}".to_owned())];

        let mut values = BTreeMap::new();
        values.insert("term".to_owned(), "hello world".to_owned());

        let prepared = prepare_request_draft(&request, &values)
            .expect("request draft should resolve placeholders into query params");
        let url = reqwest::Url::parse(&prepared.url).expect("prepared url should parse");
        let query_pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();

        assert_eq!(
            query_pairs,
            vec![("search".to_owned(), "hello world".to_owned())]
        );
    }

    #[test]
    fn prepare_request_draft_injects_bearer_auth_header() {
        let mut request = RequestDraft::default_request();
        request.auth = RequestAuth::Bearer {
            token: "{{TOKEN}}".to_owned(),
        };
        let mut values = BTreeMap::new();
        values.insert("TOKEN".to_owned(), "secret".to_owned());

        let prepared =
            prepare_request_draft(&request, &values).expect("bearer auth should resolve");

        assert!(
            prepared
                .headers
                .iter()
                .any(|(name, value)| name == "Authorization" && value == "Bearer secret")
        );
    }

    #[test]
    fn prepare_request_draft_injects_basic_auth_header() {
        let mut request = RequestDraft::default_request();
        request.auth = RequestAuth::Basic {
            username: "aladdin".to_owned(),
            password: "open sesame".to_owned(),
        };

        let prepared =
            prepare_request_draft(&request, &BTreeMap::new()).expect("basic auth should encode");

        assert!(prepared.headers.iter().any(|(name, value)| {
            name == "Authorization" && value == "Basic YWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        }));
    }

    #[test]
    fn prepare_request_draft_injects_query_api_key() {
        let mut request = RequestDraft::default_request();
        request.auth = RequestAuth::ApiKey {
            location: ApiKeyLocation::Query,
            name: "api_key".to_owned(),
            value: "{{KEY}}".to_owned(),
        };
        let mut values = BTreeMap::new();
        values.insert("KEY".to_owned(), "secret".to_owned());

        let prepared =
            prepare_request_draft(&request, &values).expect("query api key should resolve");
        let url = reqwest::Url::parse(&prepared.url).expect("prepared url should parse");
        let query_pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();

        assert_eq!(
            query_pairs,
            vec![("api_key".to_owned(), "secret".to_owned())]
        );
    }

    #[test]
    fn prepare_request_draft_rejects_auth_header_conflicts() {
        let mut request = RequestDraft::default_request();
        request.headers = vec![("Authorization".to_owned(), "Bearer manual".to_owned())];
        request.auth = RequestAuth::Bearer {
            token: "generated".to_owned(),
        };

        let error = prepare_request_draft(&request, &BTreeMap::new())
            .expect_err("conflicting authorization header should fail");

        assert_eq!(error.target, "auth");
        assert!(
            error
                .details
                .as_deref()
                .unwrap_or_default()
                .contains("conflicts with an existing header")
        );
    }

    #[test]
    fn workspace_bundle_round_trips_state() {
        let mut state = crate::state::AppState::new();
        let mut request = RequestDraft::default_request();
        request.set_request_name("List users");
        request.set_folder_path("Collections/API");
        request.query_params = vec![("page".to_owned(), "1".to_owned())];
        state.requests = vec![request];
        state.responses = vec![crate::state::ResponseSummary {
            request_id: Some("request-0".to_owned()),
            status: Some(200),
            ..crate::state::ResponseSummary::default()
        }];
        state.environments = vec![Environment::default()];
        state.active_environment = Some(0);
        state.ui.select_request(0);
        state.ui.select_response(0);
        state.ui.set_view(View::History);

        let json = workspace_bundle_to_json(&state).expect("workspace should serialize");
        let restored_state =
            workspace_bundle_from_json(&json).expect("workspace should deserialize");

        assert_eq!(restored_state.requests.len(), 1);
        assert_eq!(restored_state.responses.len(), 1);
        assert_eq!(restored_state.ui.selected_request, Some(0));
        assert_eq!(restored_state.ui.selected_response, Some(0));
        assert_eq!(restored_state.ui.view, View::History);
        assert_eq!(
            restored_state.requests[0].folder_path(),
            Some("Collections/API")
        );
    }

    #[test]
    fn workspace_bundle_rejects_unknown_format_version() {
        let json = r#"{"format_version":99,"requests":[],"responses":[],"environments":[],"active_environment":null,"ui":{"selected_request":null,"selected_response":null,"view":"Editor"}}"#;

        let error = workspace_bundle_from_json(json)
            .expect_err("unsupported workspace bundle version should fail");

        assert!(error.contains("unsupported workspace format version"));
    }

    #[test]
    fn workspace_bundle_reports_request_context_for_invalid_requests() {
        let json = r#"{
            "format_version":1,
            "requests":[{"name":"Broken request","folder":"","method":"","url":"https://example.com","query_params":[],"auth":"None","headers":[],"body":null}],
            "responses":[],
            "environments":[],
            "active_environment":null,
            "ui":{"selected_request":null,"selected_response":null,"view":"Editor"}
        }"#;

        let error =
            workspace_bundle_from_json(json).expect_err("invalid request should be rejected");

        assert!(error.contains("Broken request"));
        assert!(error.contains("empty method"));
    }

    #[test]
    fn workspace_bundle_reports_invalid_json_context() {
        let error =
            workspace_bundle_from_json("{").expect_err("invalid workspace json should fail");

        assert!(error.contains("invalid workspace bundle JSON"));
    }
}
