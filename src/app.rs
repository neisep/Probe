use eframe::egui;
use std::time::Duration;

use crate::persistence::FileStorage;
use crate::persistence::Snapshot;
use crate::persistence::WorkspaceSnapshot;
use crate::runtime::{AsyncRequest, AsyncRequestResult, Event, Runtime};
use crate::state::{AppState, View};
use crate::ui::shell;
use serde::{Deserialize, Serialize};
use serde_json::json;

const LEGACY_SNAPSHOT_ID: &str = "last";
const WORKSPACE_SNAPSHOT_ID: &str = "current";
const SELECTED_DRAFT_ID: &str = "selected_request";
const DEFAULT_WORKSPACE_ID: &str = "default";

#[derive(Debug, Serialize, Deserialize)]
struct PersistedRequestDraft {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    body: Option<String>,
}

pub struct ProbeApp {
    status: String,
    state: AppState,
    runtime: Option<Runtime>,
    storage: Option<FileStorage>,
    pending_request: Option<u64>,
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
                }
            }
            (Err(error), Ok(state)) => Self {
                status: format!("Runtime unavailable: {error}"),
                state,
                runtime: None,
                storage,
                pending_request: None,
            },
            (Ok(runtime), Err(error)) => Self {
                status: format!("State bootstrap fallback: {error}"),
                state: AppState::default(),
                runtime: Some(runtime),
                storage,
                pending_request: None,
            },
            (Err(runtime_error), Err(state_error)) => Self {
                status: format!("Startup fallback: runtime={runtime_error}; state={state_error}"),
                state: AppState::default(),
                runtime: None,
                storage,
                pending_request: None,
            },
        }
    }

    fn save_snapshot(&mut self) {
        let Some(storage) = &self.storage else {
            return;
        };

        let Some(request) = self.state.selected_request() else {
            return;
        };

        let snapshot = Snapshot {
            id: LEGACY_SNAPSHOT_ID.to_owned(),
            data: json!({
                "selected_request": self.state.ui.selected_request,
                "method": request.method,
                "url": request.url,
                "headers": request.headers,
                "body": request.body,
            }),
        };

        if let Err(error) = storage.save_snapshot(&snapshot) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let draft_content = match serde_json::to_string(&PersistedRequestDraft {
            method: request.method.clone(),
            url: request.url.clone(),
            headers: request.headers.clone(),
            body: request.body.clone(),
        }) {
            Ok(draft_content) => draft_content,
            Err(error) => {
                self.status = format!("Save failed: {error}");
                return;
            }
        };

        let selected_draft = crate::persistence::models::Draft {
            id: SELECTED_DRAFT_ID.to_owned(),
            workspace_id: Some(DEFAULT_WORKSPACE_ID.to_owned()),
            path: Some("selected-request".to_owned()),
            content: draft_content,
            created_at: None,
            tags: vec!["selected".to_owned(), "mvp".to_owned()],
        };

        if let Err(error) = storage.save_draft(&selected_draft) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let draft_preview = crate::persistence::models::DraftPreview {
            id: SELECTED_DRAFT_ID.to_owned(),
            draft_id: SELECTED_DRAFT_ID.to_owned(),
            method: Some(request.method.clone()),
            target_url: Some(request.url.clone()),
            preview_title: Some(format!("{} {}", request.method, request.url)),
            preview_snippet: request.body.clone(),
            tags: vec!["selected".to_owned()],
            created_at: None,
        };

        if let Err(error) = storage.save_draft_preview(&draft_preview) {
            self.status = format!("Save failed: {error}");
            return;
        }

        let mut response_ids = Vec::new();
        for (index, response) in self.state.responses.iter().enumerate() {
            let response_id = format!("response-{index}");
            let stored_response = crate::persistence::models::ResponseSummary {
                id: response_id.clone(),
                workspace_id: Some(DEFAULT_WORKSPACE_ID.to_owned()),
                request_id: Some(SELECTED_DRAFT_ID.to_owned()),
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

            response_ids.push(response_id);
        }

        let workspace_snapshot = WorkspaceSnapshot {
            id: WORKSPACE_SNAPSHOT_ID.to_owned(),
            name: Some("Current Workspace".to_owned()),
            workspace_root: None,
            created_at: None,
            open_files: vec![self.state.ui.view.label().to_owned()],
            drafts: vec![SELECTED_DRAFT_ID.to_owned()],
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

                        match result {
                            AsyncRequestResult::Ok(info) => {
                                let mut summary = crate::state::ResponseSummary::default();
                                summary.status = Some(info.status);
                                summary.timing_ms = Some(info.duration_ms);
                                summary.size_bytes = Some(info.body.len());
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

                if ui.button("Send selected request").clicked() {
                    if let Some(req) = self.state.selected_request().cloned() {
                        let Some(runtime) = &self.runtime else {
                            self.status = "Runtime unavailable".to_owned();
                            return;
                        };

                        let ar = AsyncRequest {
                            url: req.url.clone(),
                            method: req.method.clone(),
                            body: req.body.as_ref().map(|b| b.as_bytes().to_vec()),
                        };
                        match runtime.submit_blocking(ar) {
                            Ok(id) => {
                                self.pending_request = Some(id);
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
                    self.state.ui.selected_response = None;
                }
            });
        });

        shell::show(ui, &mut self.state, &self.status);
    }
}

fn restore_last_snapshot(state: &mut AppState, storage: &FileStorage) {
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
        if let Some(method) = snapshot
            .data
            .get("method")
            .and_then(|method| method.as_str())
        {
            request.method = method.to_owned();
        }

        if let Some(url) = snapshot.data.get("url").and_then(|url| url.as_str()) {
            request.url = url.to_owned();
        }

        if let Some(headers) = snapshot
            .data
            .get("headers")
            .and_then(|headers| headers.as_array())
        {
            request.headers = headers
                .iter()
                .filter_map(|header| {
                    let key = header.get(0).and_then(|key| key.as_str())?;
                    let value = header.get(1).and_then(|value| value.as_str())?;
                    Some((key.to_owned(), value.to_owned()))
                })
                .collect();
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

    if let Some(selected_request) = snapshot
        .meta
        .get("selected_request")
        .and_then(|selected_request| selected_request.as_u64())
        .map(|selected_request| selected_request as usize)
    {
        if selected_request < state.requests.len() {
            state.ui.select_request(selected_request);
        }
    }

    let Some(draft_id) = snapshot.drafts.first() else {
        return true;
    };

    let Ok(draft) = storage.load_draft(draft_id) else {
        return true;
    };

    let Ok(persisted_request) = serde_json::from_str::<PersistedRequestDraft>(&draft.content)
    else {
        return true;
    };

    if let Some(request) = state.selected_request_mut() {
        request.method = persisted_request.method;
        request.url = persisted_request.url;
        request.headers = persisted_request.headers;
        request.body = persisted_request.body;
    }

    state.responses.clear();
    for response_id in &snapshot.responses {
        let Ok(stored_response) = storage.load_response_summary(response_id) else {
            continue;
        };

        let response_preview = storage.load_response_preview(response_id).ok();
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

        state.responses.push(crate::state::ResponseSummary {
            status: stored_response.status_code,
            timing_ms: stored_response
                .duration_ms
                .map(|duration_ms| duration_ms as u128),
            size_bytes,
            content_type,
            header_count,
            preview_text,
            error: stored_response.summary.clone(),
        });
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
                if selected_response < state.requests.len() {
                    state.ui.select_request(selected_response);
                }
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
        }
    }

    true
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
