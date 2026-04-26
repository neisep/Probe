use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::state::request::normalize_request_name;
use crate::state::AppState;

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
pub struct WorkspaceImportPreview {
    pub request_count: usize,
    pub response_count: usize,
    pub environment_count: usize,
    pub selected_request_label: Option<String>,
}

pub struct PendingWorkspaceImport {
    pub path: PathBuf,
    pub preview: WorkspaceImportPreview,
    pub imported_state: AppState,
}

pub fn workspace_bundle_to_json(state: &AppState) -> Result<String, String> {
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

pub fn workspace_bundle_from_json(json: &str) -> Result<AppState, String> {
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

#[cfg(test)]
mod tests {
    use super::{workspace_bundle_from_json, workspace_bundle_to_json};
    use crate::state::{Environment, RequestDraft, View};

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
        assert_eq!(restored_state.requests[0].folder, "Collections/API");
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
