use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::state::AppState;
use super::bundle::{WorkspaceImportPreview, workspace_bundle_to_json};

pub fn preview_workspace_import(state: &AppState) -> WorkspaceImportPreview {
    WorkspaceImportPreview {
        request_count: state.requests.len(),
        response_count: state.responses.len(),
        environment_count: state.environments.len(),
        selected_request_label: state
            .selected_request()
            .map(|request| request.display_name()),
    }
}

pub fn backup_workspace(state: &AppState) -> Result<PathBuf, String> {
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
