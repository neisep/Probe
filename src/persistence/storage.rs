use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::persistence::models::{
    Draft, DraftPreview, ResponsePreview, ResponsePreviewDetail, ResponseSummary, SessionState,
    Snapshot, UIState, WorkspaceSnapshot,
};

/// Errors surfaced by the persistence layer.
#[derive(Debug)]
pub enum PersistenceError {
    Io(io::Error),
    Serde(serde_json::Error),
    NotFound(String),
    InvalidKey(String),
    InvalidCategory(String),
    Other(String),
}

impl From<io::Error> for PersistenceError {
    fn from(e: io::Error) -> Self {
        PersistenceError::Io(e)
    }
}
impl From<serde_json::Error> for PersistenceError {
    fn from(e: serde_json::Error) -> Self {
        PersistenceError::Serde(e)
    }
}

const ALLOWED_CATEGORIES: &[&str] = &[
    "drafts",
    "snapshots",
    "responses",
    "ui",
    "workspace_snapshots",
    // new preview and session categories
    "draft_previews",
    "response_previews",
    "session",
];

fn is_valid_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 255 {
        return false;
    }
    // Allow only simple filename-safe characters: alnum, '-', '_'
    key.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Simple file-backed storage rooted at a directory.
/// Data is stored under <base_dir>/<category>/<key>.json
pub struct FileStorage {
    base_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDraftPreview {
    id: String,
    draft_id: String,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    target_url: Option<String>,
    #[serde(default)]
    preview_title: Option<String>,
    #[serde(default)]
    preview_snippet: Option<String>,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    created_at: Option<String>,
}

impl From<&DraftPreview> for StoredDraftPreview {
    fn from(preview: &DraftPreview) -> Self {
        Self {
            id: preview.id.clone(),
            draft_id: preview.draft_id.clone(),
            method: preview.method.clone(),
            target_url: preview.target_url.clone(),
            preview_title: preview.preview_title.clone(),
            preview_snippet: preview.preview_snippet.clone(),
            workspace_id: None,
            tags: preview.tags.clone(),
            created_at: preview.created_at.clone(),
        }
    }
}

impl From<StoredDraftPreview> for DraftPreview {
    fn from(preview: StoredDraftPreview) -> Self {
        Self {
            id: preview.id,
            draft_id: preview.draft_id,
            method: preview.method,
            target_url: preview.target_url,
            preview_title: preview.preview_title,
            preview_snippet: preview.preview_snippet,
            tags: preview.tags,
            created_at: preview.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredResponsePreview {
    #[serde(flatten)]
    preview: ResponsePreview,
    #[serde(default)]
    detail: ResponsePreviewDetail,
}

impl From<&ResponsePreview> for StoredResponsePreview {
    fn from(preview: &ResponsePreview) -> Self {
        Self {
            preview: preview.clone(),
            detail: ResponsePreviewDetail::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSessionState {
    #[serde(default)]
    selected_response: Option<String>,
    #[serde(default)]
    selected_request: Option<String>,
    #[serde(default)]
    active_view: Option<String>,
    #[serde(default)]
    open_panels: Vec<String>,
    updated_at: Option<String>,
}

impl StoredSessionState {
    fn empty() -> Self {
        Self {
            selected_response: None,
            selected_request: None,
            active_view: None,
            open_panels: Vec::new(),
            updated_at: None,
        }
    }
}

impl From<&SessionState> for StoredSessionState {
    fn from(state: &SessionState) -> Self {
        Self {
            selected_response: state.selected_response.clone(),
            selected_request: None,
            active_view: state.active_view.clone(),
            open_panels: state.open_panels.clone(),
            updated_at: state.updated_at.clone(),
        }
    }
}

impl From<StoredSessionState> for SessionState {
    fn from(state: StoredSessionState) -> Self {
        Self {
            selected_response: state.selected_response,
            active_view: state.active_view,
            open_panels: state.open_panels,
            updated_at: state.updated_at,
        }
    }
}

impl FileStorage {
    /// Create or open a storage rooted at `base_dir`.
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self, PersistenceError> {
        let base = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&base)?;
        Ok(FileStorage { base_dir: base })
    }

    fn ensure_category(&self, category: &str) -> Result<PathBuf, PersistenceError> {
        if !ALLOWED_CATEGORIES.contains(&category) {
            return Err(PersistenceError::InvalidCategory(category.to_string()));
        }
        let dir = self.base_dir.join(category);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn path_for(&self, category: &str, key: &str) -> Result<PathBuf, PersistenceError> {
        if !is_valid_key(key) {
            return Err(PersistenceError::InvalidKey(key.to_string()));
        }
        Ok(self.base_dir.join(category).join(format!("{}.json", key)))
    }

    fn atomic_write<P: AsRef<Path>>(&self, path: P, data: &str) -> Result<(), PersistenceError> {
        let path = path.as_ref();
        let tmp = path.with_extension("json.tmp");
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data.as_bytes())?;
        // Try to flush to disk; ignore sync errors but attempt.
        f.sync_all().ok();
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn write_json<T: Serialize>(
        &self,
        category: &str,
        key: &str,
        value: &T,
    ) -> Result<(), PersistenceError> {
        let dir = self.ensure_category(category)?;
        let path = dir.join(format!("{}.json", key));
        if !is_valid_key(key) {
            return Err(PersistenceError::InvalidKey(key.to_string()));
        }
        let s = serde_json::to_string_pretty(value)?;
        self.atomic_write(path, &s)
    }

    fn read_json<T: DeserializeOwned>(
        &self,
        category: &str,
        key: &str,
    ) -> Result<T, PersistenceError> {
        let path = self.path_for(category, key)?;
        if !path.exists() {
            return Err(PersistenceError::NotFound(
                path.to_string_lossy().to_string(),
            ));
        }
        let s = fs::read_to_string(&path)?;
        let v = serde_json::from_str(&s)?;
        Ok(v)
    }

    fn delete_json(&self, category: &str, key: &str) -> Result<(), PersistenceError> {
        let path = self.path_for(category, key)?;
        if !path.exists() {
            return Err(PersistenceError::NotFound(
                path.to_string_lossy().to_string(),
            ));
        }
        fs::remove_file(path)?;
        Ok(())
    }

    fn list_keys_in_category(&self, category: &str) -> Result<Vec<String>, PersistenceError> {
        let dir = self.base_dir.join(category);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut keys = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let ent = entry?;
            if ent.file_type()?.is_file() {
                if let Some(name) = ent.file_name().to_str() {
                    if let Some(stripped) = name.strip_suffix(".json") {
                        // Only return keys that validate
                        if is_valid_key(stripped) {
                            keys.push(stripped.to_string());
                        }
                    }
                }
            }
        }
        keys.sort();
        Ok(keys)
    }

    fn load_stored_draft_preview(&self, id: &str) -> Result<StoredDraftPreview, PersistenceError> {
        self.read_json("draft_previews", id)
    }

    fn load_stored_response_preview(
        &self,
        id: &str,
    ) -> Result<StoredResponsePreview, PersistenceError> {
        self.read_json("response_previews", id)
    }

    fn load_all_drafts(&self) -> Result<Vec<Draft>, PersistenceError> {
        let draft_ids = self.list_drafts()?;
        let mut drafts = Vec::with_capacity(draft_ids.len());

        for draft_id in draft_ids {
            drafts.push(self.load_draft(&draft_id)?);
        }

        Ok(drafts)
    }

    fn load_all_stored_draft_previews(&self) -> Result<Vec<StoredDraftPreview>, PersistenceError> {
        let preview_ids = self.list_draft_previews()?;
        let mut previews = Vec::with_capacity(preview_ids.len());

        for preview_id in preview_ids {
            previews.push(self.load_stored_draft_preview(&preview_id)?);
        }

        Ok(previews)
    }

    fn load_stored_session_state(&self) -> Result<StoredSessionState, PersistenceError> {
        self.read_json("session", "state")
    }

    /* Draft APIs */
    pub fn save_draft(&self, draft: &Draft) -> Result<(), PersistenceError> {
        self.write_json("drafts", &draft.id, draft)
    }
    pub fn load_draft(&self, id: &str) -> Result<Draft, PersistenceError> {
        self.read_json("drafts", id)
    }
    pub fn delete_draft(&self, id: &str) -> Result<(), PersistenceError> {
        self.delete_json("drafts", id)
    }
    pub fn list_drafts(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_keys_in_category("drafts")
    }

    /* Draft preview APIs */
    pub fn save_draft_preview(&self, preview: &DraftPreview) -> Result<(), PersistenceError> {
        let mut stored_preview = StoredDraftPreview::from(preview);
        stored_preview.workspace_id = match self.load_draft(&stored_preview.draft_id) {
            Ok(draft) => draft.workspace_id,
            Err(PersistenceError::NotFound(_)) => {
                match self.load_stored_draft_preview(&stored_preview.id) {
                    Ok(existing_preview) => existing_preview.workspace_id,
                    Err(PersistenceError::NotFound(_)) => None,
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };

        self.write_json("draft_previews", &preview.id, &stored_preview)
    }
    pub fn load_draft_preview(&self, id: &str) -> Result<DraftPreview, PersistenceError> {
        Ok(self.load_stored_draft_preview(id)?.into())
    }
    pub fn delete_draft_preview(&self, id: &str) -> Result<(), PersistenceError> {
        self.delete_json("draft_previews", id)
    }
    pub fn list_draft_previews(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_keys_in_category("draft_previews")
    }
    pub fn load_drafts_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<Draft>, PersistenceError> {
        let drafts = self.load_all_drafts()?;
        Ok(drafts
            .into_iter()
            .filter(|draft| draft.workspace_id.as_deref() == Some(workspace_id))
            .collect())
    }
    pub fn load_draft_previews_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<DraftPreview>, PersistenceError> {
        let previews = self.load_all_stored_draft_previews()?;
        let mut matching_previews = Vec::new();

        for preview in previews {
            let matches_workspace = match preview.workspace_id.as_deref() {
                Some(preview_workspace_id) => preview_workspace_id == workspace_id,
                None => {
                    self.load_draft(&preview.draft_id)?.workspace_id.as_deref()
                        == Some(workspace_id)
                }
            };

            if matches_workspace {
                matching_previews.push(preview.into());
            }
        }

        Ok(matching_previews)
    }
    pub fn delete_drafts_for_workspace(&self, workspace_id: &str) -> Result<(), PersistenceError> {
        for draft in self.load_drafts_for_workspace(workspace_id)? {
            self.delete_draft(&draft.id)?;
        }

        Ok(())
    }
    pub fn delete_draft_previews_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<(), PersistenceError> {
        for preview in self.load_draft_previews_for_workspace(workspace_id)? {
            self.delete_draft_preview(&preview.id)?;
        }

        Ok(())
    }

    /* Snapshot APIs */
    pub fn save_snapshot(&self, snapshot: &Snapshot) -> Result<(), PersistenceError> {
        self.write_json("snapshots", &snapshot.id, snapshot)
    }
    pub fn load_snapshot(&self, id: &str) -> Result<Snapshot, PersistenceError> {
        self.read_json("snapshots", id)
    }
    pub fn delete_snapshot(&self, id: &str) -> Result<(), PersistenceError> {
        self.delete_json("snapshots", id)
    }
    pub fn list_snapshots(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_keys_in_category("snapshots")
    }

    /* Workspace snapshot (richer) APIs */
    pub fn save_workspace_snapshot(
        &self,
        snapshot: &WorkspaceSnapshot,
    ) -> Result<(), PersistenceError> {
        self.write_json("workspace_snapshots", &snapshot.id, snapshot)
    }
    pub fn load_workspace_snapshot(&self, id: &str) -> Result<WorkspaceSnapshot, PersistenceError> {
        self.read_json("workspace_snapshots", id)
    }
    pub fn delete_workspace_snapshot(&self, id: &str) -> Result<(), PersistenceError> {
        self.delete_json("workspace_snapshots", id)
    }
    pub fn list_workspace_snapshots(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_keys_in_category("workspace_snapshots")
    }

    /* Response summary APIs */
    pub fn save_response_summary(&self, summary: &ResponseSummary) -> Result<(), PersistenceError> {
        self.write_json("responses", &summary.id, summary)
    }
    pub fn load_response_summary(&self, id: &str) -> Result<ResponseSummary, PersistenceError> {
        self.read_json("responses", id)
    }
    pub fn delete_response_summary(&self, id: &str) -> Result<(), PersistenceError> {
        self.delete_json("responses", id)
    }
    pub fn list_response_summaries(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_keys_in_category("responses")
    }

    /* Response preview APIs */
    pub fn save_response_preview(&self, preview: &ResponsePreview) -> Result<(), PersistenceError> {
        let detail = match self.load_stored_response_preview(&preview.id) {
            Ok(existing_preview) => existing_preview.detail,
            Err(PersistenceError::NotFound(_)) => ResponsePreviewDetail::default(),
            Err(error) => return Err(error),
        };
        let mut stored_preview = StoredResponsePreview::from(preview);
        stored_preview.detail = detail;

        self.write_json("response_previews", &preview.id, &stored_preview)
    }
    pub fn load_response_preview(&self, id: &str) -> Result<ResponsePreview, PersistenceError> {
        Ok(self.load_stored_response_preview(id)?.preview)
    }
    pub fn save_response_preview_detail(
        &self,
        id: &str,
        detail: &ResponsePreviewDetail,
    ) -> Result<(), PersistenceError> {
        let mut stored_preview = self.load_stored_response_preview(id)?;
        stored_preview.detail = detail.clone();

        self.write_json("response_previews", id, &stored_preview)
    }
    pub fn load_response_preview_detail(
        &self,
        id: &str,
    ) -> Result<ResponsePreviewDetail, PersistenceError> {
        Ok(self.load_stored_response_preview(id)?.detail)
    }
    pub fn delete_response_preview(&self, id: &str) -> Result<(), PersistenceError> {
        self.delete_json("response_previews", id)
    }
    pub fn list_response_previews(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_keys_in_category("response_previews")
    }

    /* UI state APIs */
    pub fn save_ui_state(&self, state: &UIState) -> Result<(), PersistenceError> {
        // single UI state blob; use fixed key
        self.write_json("ui", "state", state)
    }
    pub fn load_ui_state(&self) -> Result<UIState, PersistenceError> {
        self.read_json("ui", "state")
    }

    /* Session-level UI state (richer) */
    pub fn save_session_state(&self, state: &SessionState) -> Result<(), PersistenceError> {
        let mut stored_state = StoredSessionState::from(state);
        stored_state.selected_request = match self.load_stored_session_state() {
            Ok(existing_state) => existing_state.selected_request,
            Err(PersistenceError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };

        self.write_json("session", "state", &stored_state)
    }
    pub fn load_session_state(&self) -> Result<SessionState, PersistenceError> {
        Ok(self.load_stored_session_state()?.into())
    }
    pub fn save_selected_request(
        &self,
        selected_request: Option<&str>,
    ) -> Result<(), PersistenceError> {
        let mut stored_state = match self.load_stored_session_state() {
            Ok(existing_state) => existing_state,
            Err(PersistenceError::NotFound(_)) => StoredSessionState::empty(),
            Err(error) => return Err(error),
        };
        stored_state.selected_request = selected_request.map(|request_id| request_id.to_owned());

        self.write_json("session", "state", &stored_state)
    }
    pub fn load_selected_request(&self) -> Result<Option<String>, PersistenceError> {
        Ok(self.load_stored_session_state()?.selected_request)
    }
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::Io(e) => write!(f, "IO error: {}", e),
            PersistenceError::Serde(e) => write!(f, "Serde error: {}", e),
            PersistenceError::NotFound(p) => write!(f, "Not found: {}", p),
            PersistenceError::InvalidKey(k) => write!(f, "Invalid key: {}", k),
            PersistenceError::InvalidCategory(c) => write!(f, "Invalid category: {}", c),
            PersistenceError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for PersistenceError {}

#[cfg(test)]
mod tests {
    use super::FileStorage;
    use crate::persistence::models::{
        Draft, DraftPreview, HeaderEntry, ResponsePreview, ResponsePreviewDetail, SessionState,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_storage_path(test_name: &str) -> PathBuf {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };

        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("storage-tests")
            .join(format!("{test_name}-{nanos}-{}", std::process::id()))
    }

    #[test]
    fn workspace_helpers_filter_and_delete_workspace_records() {
        let storage_path = unique_storage_path("workspace-helpers");
        let storage = match FileStorage::new(&storage_path) {
            Ok(storage) => storage,
            Err(error) => panic!("failed to create file storage: {error}"),
        };

        let drafts = vec![
            Draft {
                id: "draft-one".to_owned(),
                workspace_id: Some("workspace-a".to_owned()),
                path: Some("requests/one".to_owned()),
                content: "GET https://example.com/a".to_owned(),
                created_at: None,
                tags: vec!["workspace-a".to_owned()],
            },
            Draft {
                id: "draft-two".to_owned(),
                workspace_id: Some("workspace-b".to_owned()),
                path: Some("requests/two".to_owned()),
                content: "GET https://example.com/b".to_owned(),
                created_at: None,
                tags: vec!["workspace-b".to_owned()],
            },
        ];

        for draft in &drafts {
            if let Err(error) = storage.save_draft(draft) {
                panic!("failed to save draft {}: {error}", draft.id);
            }
        }

        let previews = vec![
            DraftPreview {
                id: "preview-one".to_owned(),
                draft_id: "draft-one".to_owned(),
                method: Some("GET".to_owned()),
                target_url: Some("https://example.com/a".to_owned()),
                preview_title: Some("workspace a".to_owned()),
                preview_snippet: Some("body a".to_owned()),
                tags: vec!["workspace-a".to_owned()],
                created_at: None,
            },
            DraftPreview {
                id: "preview-two".to_owned(),
                draft_id: "draft-two".to_owned(),
                method: Some("GET".to_owned()),
                target_url: Some("https://example.com/b".to_owned()),
                preview_title: Some("workspace b".to_owned()),
                preview_snippet: Some("body b".to_owned()),
                tags: vec!["workspace-b".to_owned()],
                created_at: None,
            },
        ];

        for preview in &previews {
            if let Err(error) = storage.save_draft_preview(preview) {
                panic!("failed to save draft preview {}: {error}", preview.id);
            }
        }

        let workspace_a_drafts = match storage.load_drafts_for_workspace("workspace-a") {
            Ok(drafts) => drafts,
            Err(error) => panic!("failed to load workspace drafts: {error}"),
        };
        assert_eq!(workspace_a_drafts.len(), 1);
        assert_eq!(workspace_a_drafts[0].id, "draft-one");

        let workspace_a_previews = match storage.load_draft_previews_for_workspace("workspace-a") {
            Ok(previews) => previews,
            Err(error) => panic!("failed to load workspace previews: {error}"),
        };
        assert_eq!(workspace_a_previews.len(), 1);
        assert_eq!(workspace_a_previews[0].id, "preview-one");

        if let Err(error) = storage.delete_draft_previews_for_workspace("workspace-a") {
            panic!("failed to delete workspace previews: {error}");
        }
        if let Err(error) = storage.delete_drafts_for_workspace("workspace-a") {
            panic!("failed to delete workspace drafts: {error}");
        }

        let remaining_drafts = match storage.list_drafts() {
            Ok(drafts) => drafts,
            Err(error) => panic!("failed to list remaining drafts: {error}"),
        };
        assert_eq!(remaining_drafts, vec!["draft-two".to_owned()]);

        let remaining_previews = match storage.list_draft_previews() {
            Ok(previews) => previews,
            Err(error) => panic!("failed to list remaining previews: {error}"),
        };
        assert_eq!(remaining_previews, vec!["preview-two".to_owned()]);

        if let Err(error) = fs::remove_dir_all(&storage_path) {
            panic!("failed to clean up test storage directory: {error}");
        }
    }

    #[test]
    fn selected_request_helpers_preserve_session_state() {
        let storage_path = unique_storage_path("selected-request");
        let storage = match FileStorage::new(&storage_path) {
            Ok(storage) => storage,
            Err(error) => panic!("failed to create file storage: {error}"),
        };

        let session_state = SessionState {
            selected_response: Some("response-1".to_owned()),
            active_view: Some("Workspace".to_owned()),
            open_panels: vec!["sidebar".to_owned(), "status".to_owned()],
            updated_at: Some("2026-01-01T00:00:00Z".to_owned()),
        };

        if let Err(error) = storage.save_session_state(&session_state) {
            panic!("failed to save session state: {error}");
        }
        if let Err(error) = storage.save_selected_request(Some("request-7")) {
            panic!("failed to save selected request: {error}");
        }

        let loaded_request = match storage.load_selected_request() {
            Ok(request) => request,
            Err(error) => panic!("failed to load selected request: {error}"),
        };
        assert_eq!(loaded_request.as_deref(), Some("request-7"));

        let loaded_state = match storage.load_session_state() {
            Ok(state) => state,
            Err(error) => panic!("failed to load session state: {error}"),
        };
        assert_eq!(
            loaded_state.selected_response.as_deref(),
            Some("response-1")
        );
        assert_eq!(loaded_state.active_view.as_deref(), Some("Workspace"));

        let updated_session_state = SessionState {
            selected_response: Some("response-2".to_owned()),
            active_view: Some("History".to_owned()),
            open_panels: vec!["sidebar".to_owned()],
            updated_at: Some("2026-01-02T00:00:00Z".to_owned()),
        };

        if let Err(error) = storage.save_session_state(&updated_session_state) {
            panic!("failed to update session state: {error}");
        }

        let preserved_request = match storage.load_selected_request() {
            Ok(request) => request,
            Err(error) => panic!("failed to reload selected request: {error}"),
        };
        assert_eq!(preserved_request.as_deref(), Some("request-7"));

        if let Err(error) = storage.save_selected_request(None) {
            panic!("failed to clear selected request: {error}");
        }

        let cleared_request = match storage.load_selected_request() {
            Ok(request) => request,
            Err(error) => panic!("failed to load cleared selected request: {error}"),
        };
        assert_eq!(cleared_request, None);

        if let Err(error) = fs::remove_dir_all(&storage_path) {
            panic!("failed to clean up test storage directory: {error}");
        }
    }

    #[test]
    fn response_preview_detail_round_trips_and_survives_preview_updates() {
        let storage_path = unique_storage_path("response-preview-detail");
        let storage = match FileStorage::new(&storage_path) {
            Ok(storage) => storage,
            Err(error) => panic!("failed to create file storage: {error}"),
        };

        let preview = ResponsePreview {
            id: "response-one".to_owned(),
            response_id: "response-one".to_owned(),
            summary: Some("HTTP 200".to_owned()),
            request_method: Some("GET".to_owned()),
            request_url: Some("https://example.com/items".to_owned()),
            content_preview: Some("{\"ok\":true}".to_owned()),
            content_type: Some("application/json".to_owned()),
            header_count: Some(2),
            size_bytes: Some(32),
            model: None,
            tokens: None,
            tags: vec!["history".to_owned()],
            created_at: None,
        };

        if let Err(error) = storage.save_response_preview(&preview) {
            panic!("failed to save response preview: {error}");
        }

        let detail = ResponsePreviewDetail {
            request_headers: vec![HeaderEntry {
                name: "Accept".to_owned(),
                value: "application/json".to_owned(),
            }],
            response_headers: vec![
                HeaderEntry {
                    name: "Content-Type".to_owned(),
                    value: "application/json".to_owned(),
                },
                HeaderEntry {
                    name: "X-Trace-Id".to_owned(),
                    value: "trace-123".to_owned(),
                },
            ],
        };

        if let Err(error) = storage.save_response_preview_detail(&preview.id, &detail) {
            panic!("failed to save response preview detail: {error}");
        }

        let loaded_preview = match storage.load_response_preview(&preview.id) {
            Ok(preview) => preview,
            Err(error) => panic!("failed to load response preview: {error}"),
        };
        assert_eq!(loaded_preview.request_method.as_deref(), Some("GET"));
        assert_eq!(
            loaded_preview.request_url.as_deref(),
            Some("https://example.com/items")
        );

        let loaded_detail = match storage.load_response_preview_detail(&preview.id) {
            Ok(detail) => detail,
            Err(error) => panic!("failed to load response preview detail: {error}"),
        };
        assert_eq!(loaded_detail, detail);

        let updated_preview = ResponsePreview {
            summary: Some("HTTP 201".to_owned()),
            size_bytes: Some(64),
            ..preview.clone()
        };

        if let Err(error) = storage.save_response_preview(&updated_preview) {
            panic!("failed to update response preview: {error}");
        }

        let preserved_detail = match storage.load_response_preview_detail(&preview.id) {
            Ok(detail) => detail,
            Err(error) => panic!("failed to reload response preview detail: {error}"),
        };
        assert_eq!(preserved_detail, detail);

        if let Err(error) = fs::remove_dir_all(&storage_path) {
            panic!("failed to clean up test storage directory: {error}");
        }
    }

    #[test]
    fn legacy_response_preview_files_load_with_empty_detail() {
        let storage_path = unique_storage_path("legacy-response-preview");
        let storage = match FileStorage::new(&storage_path) {
            Ok(storage) => storage,
            Err(error) => panic!("failed to create file storage: {error}"),
        };

        let legacy_preview = ResponsePreview {
            id: "response-legacy".to_owned(),
            response_id: "response-legacy".to_owned(),
            summary: Some("HTTP 204".to_owned()),
            request_method: Some("DELETE".to_owned()),
            request_url: Some("https://example.com/items/1".to_owned()),
            content_preview: None,
            content_type: None,
            header_count: Some(0),
            size_bytes: Some(0),
            model: None,
            tokens: None,
            tags: Vec::new(),
            created_at: None,
        };

        if let Err(error) =
            storage.write_json("response_previews", &legacy_preview.id, &legacy_preview)
        {
            panic!("failed to write legacy response preview: {error}");
        }

        let loaded_preview = match storage.load_response_preview(&legacy_preview.id) {
            Ok(preview) => preview,
            Err(error) => panic!("failed to load legacy response preview: {error}"),
        };
        assert_eq!(
            loaded_preview.request_method.as_deref(),
            legacy_preview.request_method.as_deref()
        );

        let loaded_detail = match storage.load_response_preview_detail(&legacy_preview.id) {
            Ok(detail) => detail,
            Err(error) => panic!("failed to load defaulted response preview detail: {error}"),
        };
        assert!(loaded_detail.request_headers.is_empty());
        assert!(loaded_detail.response_headers.is_empty());

        if let Err(error) = fs::remove_dir_all(&storage_path) {
            panic!("failed to clean up test storage directory: {error}");
        }
    }
}
