use base64::Engine;
use eframe::egui;
use std::{fs, time::Duration};

use crate::persistence::FileStorage;
use crate::persistence::Snapshot;
use crate::persistence::WorkspaceSnapshot;
use crate::runtime::{
    AsyncRequest, AsyncRequestResult, Event, ResolutionError, ResolutionErrorKind,
    ResolutionValues, Runtime, UnresolvedBehavior, resolve_body_text, resolve_headers,
    resolve_text_with_behavior,
};
use crate::state::request::{
    ApiKeyLocation, RequestAuth, normalize_folder_path, normalize_request_name,
};
use crate::state::{AppState, View};
use crate::ui::shell;
use serde::{Deserialize, Serialize};
use serde_json::json;

const LEGACY_SNAPSHOT_ID: &str = "last";
const WORKSPACE_SNAPSHOT_ID: &str = "current";
const DEFAULT_WORKSPACE_ID: &str = "default";
const WORKSPACE_BUNDLE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct PersistedRequestDraft {
    #[serde(default)]
    name: String,
    #[serde(default)]
    folder: String,
    method: String,
    url: String,
    #[serde(default)]
    query_params: Vec<(String, String)>,
    #[serde(default)]
    auth: RequestAuth,
    #[serde(default)]
    headers: Vec<(String, String)>,
    body: Option<String>,
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

pub struct ProbeApp {
    status: String,
    state: AppState,
    runtime: Option<Runtime>,
    storage: Option<FileStorage>,
    pending_request: Option<u64>,
    pending_request_context: Option<PendingRequestContext>,
}

impl ProbeApp {
    pub fn new() -> Self {
        let runtime = Runtime::new(8);
        let storage = create_storage();

        match (runtime, AppState::bootstrap()) {
            (Ok(runtime), Ok(mut state)) => {
                if let Some(stor) = &storage {
                    restore_last_snapshot(&mut state, stor);
                }

                Self {
                    status: "First slice ready".to_owned(),
                    state,
                    runtime: Some(runtime),
                    storage,
                    pending_request: None,
                    pending_request_context: None,
                }
            }
            (Err(error), Ok(state)) => Self {
                status: format!("Runtime unavailable: {error}"),
                state,
                runtime: None,
                storage,
                pending_request: None,
                pending_request_context: None,
            },
            (Ok(runtime), Err(error)) => Self {
                status: format!("State bootstrap fallback: {error}"),
                state: AppState::default(),
                runtime: Some(runtime),
                storage,
                pending_request: None,
                pending_request_context: None,
            },
            (Err(runtime_error), Err(state_error)) => Self {
                status: format!("Startup fallback: runtime={runtime_error}; state={state_error}"),
                state: AppState::default(),
                runtime: None,
                storage,
                pending_request: None,
                pending_request_context: None,
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

        self.status = format!("Workspace exported to {}", path.display());
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

        self.state = imported_state;
        self.pending_request = None;
        self.pending_request_context = None;
        self.save_snapshot();
        self.status = format!("Workspace imported from {}", path.display());
    }

    fn save_snapshot(&mut self) {
        let Some(storage) = &self.storage else {
            return;
        };

        let Some(selected_request_index) = self.state.selected_request_index() else {
            return;
        };
        let Some(request) = self.state.selected_request().cloned() else {
            return;
        };
        let selected_request_id = match self.state.selected_request_id() {
            Some(selected_request_id) => selected_request_id,
            None => AppState::request_id_for_index(selected_request_index),
        };
        let active_environment_id = self
            .state
            .active_environment_index()
            .map(environment_storage_id_for_index);

        let snapshot = Snapshot {
            id: LEGACY_SNAPSHOT_ID.to_owned(),
            data: json!({
                "selected_request": Some(selected_request_index),
                "name": normalize_request_name(&request.name).unwrap_or_default(),
                "folder": normalize_folder_path(&request.folder),
                "method": request.method,
                "url": request.url,
                "query_params": request.query_params,
                "auth": request.auth,
                "headers": request.headers,
                "body": request.body,
            }),
        };

        if let Err(error) = storage.save_snapshot(&snapshot) {
            self.status = format!("Save failed: {error}");
            return;
        }

        if let Err(error) = storage.delete_draft_previews_for_workspace(DEFAULT_WORKSPACE_ID) {
            self.status = format!("Save failed: {error}");
            return;
        }

        if let Err(error) = storage.delete_drafts_for_workspace(DEFAULT_WORKSPACE_ID) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let environment_snapshot = build_environment_snapshot(&self.state);
        if let Err(error) = storage.save_environment_snapshot(&environment_snapshot) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let mut draft_ids = Vec::with_capacity(self.state.requests.len());
        for (index, request) in self.state.requests.iter().enumerate() {
            let draft_id = AppState::request_id_for_index(index);
            let draft_content = match serde_json::to_string(&PersistedRequestDraft {
                name: normalize_request_name(&request.name).unwrap_or_default(),
                folder: normalize_folder_path(&request.folder),
                method: request.method.clone(),
                url: request.url.clone(),
                query_params: request.query_params.clone(),
                auth: request.auth.clone(),
                headers: request.headers.clone(),
                body: request.body.clone(),
            }) {
                Ok(draft_content) => draft_content,
                Err(error) => {
                    self.status = format!("Save failed: {error}");
                    return;
                }
            };

            let draft = crate::persistence::models::Draft {
                id: draft_id.clone(),
                workspace_id: Some(DEFAULT_WORKSPACE_ID.to_owned()),
                path: normalized_optional_value(Some(&normalize_folder_path(&request.folder))),
                content: draft_content,
                created_at: None,
                tags: vec!["request".to_owned(), "mvp".to_owned()],
            };

            if let Err(error) = storage.save_draft(&draft) {
                self.status = format!("Save failed: {error}");
                return;
            }

            let draft_preview = crate::persistence::models::DraftPreview {
                id: draft_id.clone(),
                draft_id: draft_id.clone(),
                method: Some(request.method.clone()),
                target_url: Some(request.url.clone()),
                preview_title: Some(request.display_name()),
                preview_snippet: request.body.clone(),
                tags: vec!["request".to_owned()],
                created_at: None,
            };

            if let Err(error) = storage.save_draft_preview(&draft_preview) {
                self.status = format!("Save failed: {error}");
                return;
            }

            let request_metadata = crate::persistence::models::RequestMetadata {
                request_name: normalize_request_name(&request.name),
                folder_path: normalized_optional_value(Some(&normalize_folder_path(
                    &request.folder,
                ))),
            };

            if let Err(error) = storage.save_draft_request_metadata(&draft_id, &request_metadata) {
                self.status = format!("Save failed: {error}");
                return;
            }

            draft_ids.push(draft_id);
        }

        let mut response_ids = Vec::new();
        for (index, response) in self.state.responses.iter().enumerate() {
            let response_id = format!("response-{index}");
            let stored_response = crate::persistence::models::ResponseSummary {
                id: response_id.clone(),
                workspace_id: Some(DEFAULT_WORKSPACE_ID.to_owned()),
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
                content_type: response.content_type.clone(),
                header_count: response.header_count,
                size_bytes: response.size_bytes,
                model: None,
                tokens: None,
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

        let workspace_snapshot = WorkspaceSnapshot {
            id: WORKSPACE_SNAPSHOT_ID.to_owned(),
            name: Some("Current Workspace".to_owned()),
            workspace_root: None,
            created_at: None,
            open_files: vec![self.state.ui.view.label().to_owned()],
            drafts: draft_ids,
            responses: response_ids,
            meta: json!({
                "selected_request": self.state.ui.selected_request,
                "selected_response": self.state.ui.selected_response,
                "request_count": self.state.requests.len(),
                "response_count": self.state.responses.len(),
            }),
        };

        if let Err(error) = storage.save_workspace_snapshot(&workspace_snapshot) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let session_state = crate::persistence::models::SessionState {
            selected_response: self
                .state
                .ui
                .selected_response
                .map(|selected_response| format!("response-{selected_response}")),
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

        if let Err(error) = storage.save_selected_request(Some(&selected_request_id)) {
            self.status = format!("Save failed: {error}");
            return;
        }

        if let Err(error) = storage.save_active_environment(active_environment_id.as_deref()) {
            self.status = format!("Save failed: {error}");
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
                                self.status = format!(
                                    "Request {id} completed ({} in {} ms)",
                                    info.status, info.duration_ms
                                );
                                self.state.responses.push(summary);
                                self.state
                                    .ui
                                    .select_response(self.state.responses.len() - 1);
                                self.state.ui.set_view(View::History);
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
                                self.status = format!("Request {id} failed");
                                self.state.responses.push(summary);
                                self.state
                                    .ui
                                    .select_response(self.state.responses.len() - 1);
                                self.state.ui.set_view(View::History);
                                self.save_snapshot();
                            }
                        }
                    }
                }
            }
        }

        if self.pending_request.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        egui::Panel::bottom("bottom_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Save draft").clicked() {
                    self.save_snapshot();
                    if !self.status.starts_with("Save failed") {
                        self.status = "Draft saved".to_owned();
                    }
                }

                if ui.button("Export workspace").clicked() {
                    self.export_workspace();
                }

                if ui
                    .add_enabled(
                        self.pending_request.is_none(),
                        egui::Button::new("Import workspace"),
                    )
                    .clicked()
                {
                    self.import_workspace();
                }

                if ui.button("Send selected request").clicked() {
                    if let Some(req) = self.state.selected_request().cloned() {
                        let Some(runtime) = &self.runtime else {
                            self.status = "Runtime unavailable".to_owned();
                            return;
                        };
                        let Some(selected_request_index) = self.state.selected_request_index()
                        else {
                            self.status = "No request selected".to_owned();
                            return;
                        };
                        let resolution_values = active_resolution_values(&self.state);
                        let prepared_request = match prepare_request_draft(&req, &resolution_values)
                        {
                            Ok(prepared_request) => prepared_request,
                            Err(error) => {
                                let error_info = error.to_error_info();
                                self.status = format_error(&error_info);
                                return;
                            }
                        };
                        let pending_request_context = PendingRequestContext {
                            request_id: AppState::request_id_for_index(selected_request_index),
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
                            Err(e) => {
                                self.status = format!("Submit error: {}", e);
                            }
                        }
                    } else {
                        self.status = "No request selected".to_string();
                    }
                }

                if ui.button("Clear responses").clicked() {
                    self.state.responses.clear();
                    self.state.ui.clear_selected_response();
                    self.save_snapshot();
                }
            });
        });

        shell::show(ui, &mut self.state, &self.status);
    }
}

fn restore_last_snapshot(state: &mut AppState, storage: &FileStorage) {
    restore_environment_snapshot(state, storage);

    if restore_workspace_snapshot(state, storage) {
        return;
    }

    let Ok(snapshot) = storage.load_snapshot(LEGACY_SNAPSHOT_ID) else {
        return;
    };

    if let Some(selected_request) = snapshot
        .data
        .get("selected_request")
        .and_then(|selected_request| selected_request.as_u64())
        .map(|selected_request| selected_request as usize)
    {
        if selected_request < state.requests.len() {
            state.ui.select_request(selected_request);
        }
    }

    if let Some(request) = state.selected_request_mut() {
        if let Some(name) = snapshot.data.get("name").and_then(|name| name.as_str()) {
            request.set_request_name(name);
        }

        if let Some(folder) = snapshot
            .data
            .get("folder")
            .and_then(|folder| folder.as_str())
        {
            request.set_folder_path(folder);
        }

        if let Some(method) = snapshot
            .data
            .get("method")
            .and_then(|method| method.as_str())
        {
            request.method = method.to_owned();
        }

        if let Some(url) = snapshot.data.get("url").and_then(|url| url.as_str()) {
            request.adopt_url_query(url);
        }

        if let Some(query_params) = snapshot
            .data
            .get("query_params")
            .and_then(parse_string_pairs)
        {
            request.query_params = query_params;
        }

        if let Some(auth) = snapshot
            .data
            .get("auth")
            .and_then(|auth| serde_json::from_value::<RequestAuth>(auth.clone()).ok())
        {
            request.auth = auth;
        }

        if let Some(headers) = snapshot.data.get("headers").and_then(parse_string_pairs) {
            request.headers = headers;
        }

        request.body = snapshot
            .data
            .get("body")
            .and_then(|body| body.as_str())
            .map(|body| body.to_owned());
    }
}

fn restore_workspace_snapshot(state: &mut AppState, storage: &FileStorage) -> bool {
    let Ok(snapshot) = storage.load_workspace_snapshot(WORKSPACE_SNAPSHOT_ID) else {
        return false;
    };

    let mut restored_requests = Vec::new();
    for draft_id in &snapshot.drafts {
        let Ok(draft) = storage.load_draft(draft_id) else {
            continue;
        };
        let request_metadata = storage.load_draft_request_metadata(draft_id).ok();
        let Ok(persisted_request) = serde_json::from_str::<PersistedRequestDraft>(&draft.content)
        else {
            continue;
        };

        let mut restored_request = crate::state::RequestDraft {
            name: String::new(),
            folder: String::new(),
            method: persisted_request.method,
            url: String::new(),
            query_params: Vec::new(),
            auth: persisted_request.auth,
            headers: persisted_request.headers,
            body: persisted_request.body,
        };
        restored_request.adopt_url_query(persisted_request.url.as_str());
        if !persisted_request.query_params.is_empty() {
            restored_request.query_params = persisted_request.query_params;
        }
        restored_request.set_request_name(
            normalized_optional_value(Some(persisted_request.name.as_str()))
                .or_else(|| {
                    request_metadata.as_ref().and_then(|request_metadata| {
                        normalized_optional_value(request_metadata.request_name.as_deref())
                    })
                })
                .as_deref()
                .unwrap_or_default(),
        );
        restored_request.set_folder_path(
            normalized_optional_value(Some(persisted_request.folder.as_str()))
                .or_else(|| {
                    request_metadata.as_ref().and_then(|request_metadata| {
                        normalized_optional_value(request_metadata.folder_path.as_deref())
                    })
                })
                .as_deref()
                .unwrap_or_default(),
        );
        restored_requests.push(restored_request);
    }

    if restored_requests.is_empty() {
        return false;
    }

    state.requests = restored_requests;
    state.ui.selected_request = None;
    state.ensure_valid_selection();

    if let Ok(Some(selected_request_id)) = storage.load_selected_request() {
        if let Some(selected_request_index) = state.find_request_index_by_id(&selected_request_id) {
            state.ui.select_request(selected_request_index);
        }
    } else if let Some(selected_request) = snapshot
        .meta
        .get("selected_request")
        .and_then(|selected_request| selected_request.as_u64())
        .map(|selected_request| selected_request as usize)
    {
        if selected_request < state.requests.len() {
            state.ui.select_request(selected_request);
        }
    }

    state.responses.clear();
    for response_id in &snapshot.responses {
        let Ok(stored_response) = storage.load_response_summary(response_id) else {
            continue;
        };

        let response_preview = storage.load_response_preview(response_id).ok();
        let response_preview_detail = storage.load_response_preview_detail(response_id).ok();
        let preview_text = response_preview
            .as_ref()
            .and_then(|response_preview| response_preview.content_preview.clone());
        let content_type = response_preview
            .as_ref()
            .and_then(|response_preview| response_preview.content_type.clone());
        let header_count = response_preview
            .as_ref()
            .and_then(|response_preview| response_preview.header_count);
        let size_bytes = response_preview
            .as_ref()
            .and_then(|response_preview| response_preview.size_bytes);

        let mut restored_response = crate::state::ResponseSummary {
            request_id: stored_response.request_id.clone(),
            request_method: response_preview
                .as_ref()
                .and_then(|response_preview| response_preview.request_method.clone()),
            request_url: response_preview
                .as_ref()
                .and_then(|response_preview| response_preview.request_url.clone()),
            request_headers: response_preview_detail
                .as_ref()
                .map(|detail| {
                    detail
                        .request_headers
                        .iter()
                        .map(|header| (header.name.clone(), header.value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            response_headers: response_preview_detail
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
            size_bytes,
            content_type,
            header_count,
            preview_text,
            error: stored_response.summary.clone(),
        };

        if let Some(request_id) = restored_response.request_id.as_deref()
            && let Some(request_index) = state.find_request_index_by_id(request_id)
            && let Some(request) = state.requests.get(request_index)
        {
            if restored_response.request_method.is_none() {
                restored_response.request_method = Some(request.method.clone());
            }
            if restored_response.request_url.is_none() {
                restored_response.request_url = Some(request.url.clone());
            }
        }

        state.responses.push(restored_response);
    }

    if let Ok(session_state) = storage.load_session_state() {
        if let Some(active_view) = session_state
            .active_view
            .as_deref()
            .and_then(View::from_label)
        {
            state.ui.set_view(active_view);
        }

        if let Some(selected_response_id) = session_state.selected_response {
            if let Some(selected_response) = snapshot
                .responses
                .iter()
                .position(|response_id| response_id == &selected_response_id)
            {
                state.ui.select_response(selected_response);
                select_request_for_response(state, selected_response);
            }
        }
    } else if let Some(selected_response) = snapshot
        .meta
        .get("selected_response")
        .and_then(|selected_response| selected_response.as_u64())
        .map(|selected_response| selected_response as usize)
    {
        if selected_response < state.responses.len() {
            state.ui.select_response(selected_response);
            select_request_for_response(state, selected_response);
        }
    }

    state.ensure_valid_selection();
    true
}

fn environment_storage_id_for_index(index: usize) -> String {
    format!("env-{index}")
}

fn build_environment_snapshot(state: &AppState) -> crate::persistence::models::EnvironmentSnapshot {
    crate::persistence::models::EnvironmentSnapshot {
        active_environment: state
            .active_environment_index()
            .map(environment_storage_id_for_index),
        environments: state
            .environments
            .iter()
            .enumerate()
            .map(
                |(index, environment)| crate::persistence::models::Environment {
                    id: environment_storage_id_for_index(index),
                    name: environment.name.clone(),
                    entries: environment
                        .vars
                        .iter()
                        .map(
                            |(key, value)| crate::persistence::models::EnvironmentEntry {
                                key: key.clone(),
                                value: value.clone(),
                            },
                        )
                        .collect(),
                },
            )
            .collect(),
    }
}

fn restore_environment_snapshot(state: &mut AppState, storage: &FileStorage) {
    let Ok(snapshot) = storage.load_environment_snapshot() else {
        state.ensure_valid_environment_selection();
        return;
    };

    if snapshot.environments.is_empty() {
        state.ensure_valid_environment_selection();
        return;
    }

    let mut active_environment = None;
    let mut restored_environments = Vec::with_capacity(snapshot.environments.len());

    for (index, environment) in snapshot.environments.into_iter().enumerate() {
        if snapshot.active_environment.as_deref() == Some(environment.id.as_str()) {
            active_environment = Some(index);
        }

        let vars = environment
            .entries
            .into_iter()
            .filter_map(|entry| {
                let key = entry.key.trim().to_owned();
                if key.is_empty() {
                    return None;
                }

                Some((key, entry.value))
            })
            .collect();

        restored_environments.push(crate::state::Environment {
            name: environment.name,
            vars,
        });
    }

    state.environments = restored_environments;
    state.active_environment = active_environment;
    state.ensure_valid_environment_selection();
}

fn active_resolution_values(state: &AppState) -> ResolutionValues {
    state.active_variables().cloned().unwrap_or_default()
}

fn workspace_bundle_to_json(state: &AppState) -> Result<String, String> {
    serde_json::to_string_pretty(&build_workspace_bundle(state)).map_err(|error| error.to_string())
}

fn workspace_bundle_from_json(json: &str) -> Result<AppState, String> {
    let bundle: WorkspaceBundle = serde_json::from_str(json).map_err(|error| error.to_string())?;
    state_from_workspace_bundle(bundle)
}

fn build_workspace_bundle(state: &AppState) -> WorkspaceBundle {
    WorkspaceBundle {
        format_version: WORKSPACE_BUNDLE_FORMAT_VERSION,
        requests: state.requests.clone(),
        responses: state.responses.clone(),
        environments: state.environments.clone(),
        active_environment: state.active_environment,
        ui: state.ui.clone(),
    }
}

fn state_from_workspace_bundle(bundle: WorkspaceBundle) -> Result<AppState, String> {
    if bundle.format_version != WORKSPACE_BUNDLE_FORMAT_VERSION {
        return Err(format!(
            "unsupported workspace format version {}",
            bundle.format_version
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
    for request in &mut state.requests {
        let method = request.method.trim().to_uppercase();
        if method.is_empty() {
            return Err("imported request method cannot be empty".to_owned());
        }
        let url = request.url.trim().to_owned();
        if url.is_empty() {
            return Err("imported request url cannot be empty".to_owned());
        }

        let name = request.name.clone();
        let folder = request.folder.clone();
        request.method = method;
        request.set_request_name(&name);
        request.set_folder_path(&folder);
        request.set_url(&url);
    }

    let mut environment_names = std::collections::BTreeSet::new();
    for environment in &mut state.environments {
        let name = environment.name.trim().to_owned();
        if name.is_empty() {
            return Err("imported environment name cannot be empty".to_owned());
        }
        if !environment_names.insert(name.clone()) {
            return Err(format!("duplicate imported environment '{name}'"));
        }
        environment.name = name;
    }

    Ok(())
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
    apply_auth_headers(&mut resolved_headers, &resolved_auth.headers)?;
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
    auth_headers: &[(String, String)],
) -> Result<(), ResolutionError> {
    for (auth_name, _auth_value) in auth_headers {
        if existing_headers
            .iter()
            .any(|(name, _value)| name.eq_ignore_ascii_case(auth_name))
        {
            return Err(invalid_request_error(
                "auth",
                &format!("auth header '{auth_name}' conflicts with an existing header"),
            ));
        }
    }

    existing_headers.extend(auth_headers.iter().cloned());
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

fn normalized_optional_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_string_pairs(value: &serde_json::Value) -> Option<Vec<(String, String)>> {
    let pairs = value.as_array()?;
    Some(
        pairs
            .iter()
            .filter_map(|pair| {
                let key = pair.get(0).and_then(|key| key.as_str())?;
                let value = pair.get(1).and_then(|value| value.as_str())?;
                Some((key.to_owned(), value.to_owned()))
            })
            .collect(),
    )
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
        PersistedRequestDraft, build_request_url, prepare_request_draft,
        workspace_bundle_from_json, workspace_bundle_to_json,
    };
    use crate::state::request::{ApiKeyLocation, RequestAuth};
    use crate::state::{Environment, RequestDraft, View};
    use std::collections::BTreeMap;

    #[test]
    fn persisted_request_defaults_query_params_for_legacy_data() {
        let legacy =
            r#"{"method":"GET","url":"https://example.com/items","headers":[],"body":null}"#;

        let persisted: PersistedRequestDraft =
            serde_json::from_str(legacy).expect("legacy request draft should deserialize");

        assert!(persisted.query_params.is_empty());
        assert_eq!(persisted.auth, RequestAuth::None);
    }

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
}
