use std::collections::BTreeMap;

use crate::persistence::{EnvFile, FileStorage, RequestFile};
use crate::state::request::{normalize_folder_path, normalize_request_name};
use crate::state::{AppState, RequestDraft};

pub fn persist_state(state: &AppState, storage: &FileStorage) -> Result<(), String> {
    let mut used_paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (index, request) in state.requests.iter().enumerate() {
        let relative_path = reserve_request_relative_path(request, index, &mut used_paths);
        let file = RequestFile {
            relative_path,
            request: request.clone(),
        };
        storage.save_request(&file).map_err(|e| e.to_string())?;
    }
    storage
        .delete_stale_requests(&used_paths)
        .map_err(|e| e.to_string())?;

    let env_file = build_env_file(state);
    storage
        .save_env_file(&env_file)
        .map_err(|e| e.to_string())?;

    let mut response_ids = Vec::new();
    for (index, response) in state.responses.iter().enumerate() {
        let response_id = format!("response-{index}");
        let stored_response = crate::persistence::models::ResponseSummary {
            id: response_id.clone(),
            request_id: response.request_id.clone(),
            status_code: response.status,
            summary: response.error.clone(),
            duration_ms: response.timing_ms.map(|timing_ms| timing_ms as u64),
            created_at: None,
        };
        storage
            .save_response_summary(&stored_response)
            .map_err(|e| e.to_string())?;

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
        storage
            .save_response_preview(&response_preview)
            .map_err(|e| e.to_string())?;

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
        storage
            .save_response_preview_detail(&response_id, &response_preview_detail)
            .map_err(|e| e.to_string())?;

        response_ids.push(response_id);
    }
    storage
        .delete_stale_response_ids(&response_ids)
        .map_err(|e| e.to_string())?;

    let selected_request_id = state
        .selected_request_index()
        .map(AppState::request_id_for_index);
    let selected_response_id = state
        .ui
        .selected_response
        .map(|index| format!("response-{index}"));
    let active_environment_name = state
        .active_environment()
        .map(|environment| environment.name.clone());

    let session_state = crate::persistence::models::SessionState {
        selected_request: selected_request_id,
        selected_response: selected_response_id,
        active_environment: active_environment_name,
        active_view: Some(state.ui.view.label().to_owned()),
        open_panels: vec![
            "sidebar".to_owned(),
            "inspector".to_owned(),
            "status_bar".to_owned(),
            "bottom_bar".to_owned(),
        ],
        updated_at: None,
    };
    storage
        .save_session_state(&session_state)
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub(crate) fn build_env_file(state: &AppState) -> EnvFile {
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

pub(crate) fn reserve_request_relative_path(
    request: &RequestDraft,
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
