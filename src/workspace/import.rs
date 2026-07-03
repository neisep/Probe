use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::bundle::{WorkspaceImportPreview, workspace_bundle_to_json};
use crate::state::AppState;

/// Maximum byte length we'll accept for an imported workspace file.
/// Beyond this we refuse to even read the file into memory — both as a
/// DoS guard and because a "workspace" much bigger than this is almost
/// certainly the wrong file.
pub const MAX_WORKSPACE_BUNDLE_BYTES: u64 = 50 * 1024 * 1024;

/// Stat the file before reading and refuse anything beyond
/// `MAX_WORKSPACE_BUNDLE_BYTES`. `fs::read_to_string` allocates the
/// whole file up-front, so without this guard a multi-GB JSON dropped
/// on the import dialog would OOM the process before we even reach
/// parsing.
pub fn read_workspace_bundle_file(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not stat {}: {error}", path.display()))?;
    if metadata.len() > MAX_WORKSPACE_BUNDLE_BYTES {
        return Err(format!(
            "workspace file {} is {} bytes (max {})",
            path.display(),
            metadata.len(),
            MAX_WORKSPACE_BUNDLE_BYTES
        ));
    }
    fs::read_to_string(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_path(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("probe-workspace-import-{nanos}-{suffix}"))
    }

    #[test]
    fn rejects_files_above_size_cap_before_reading() {
        let path = temp_path("oversized.json");
        // Write one byte past the cap so we exercise the guard cleanly
        // without spending memory on the actual data.
        let f = fs::File::create(&path).expect("create");
        f.set_len(MAX_WORKSPACE_BUNDLE_BYTES + 1).expect("set_len");
        drop(f);

        let error = read_workspace_bundle_file(&path).expect_err("oversized file must be rejected");
        assert!(
            error.contains("max") && error.contains("bytes"),
            "error should reference the cap: {error}"
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn reads_files_within_size_cap() {
        let path = temp_path("small.json");
        let mut f = fs::File::create(&path).expect("create");
        f.write_all(b"{\"format_version\":1}").expect("write");
        drop(f);

        let contents = read_workspace_bundle_file(&path).expect("read should succeed");
        assert_eq!(contents, "{\"format_version\":1}");

        let _ = fs::remove_file(&path);
    }
}
