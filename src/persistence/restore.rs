use crate::persistence::FileStorage;
use crate::state::{AppState, View};

pub fn restore_workspace(state: &mut AppState, storage: &FileStorage) {
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
