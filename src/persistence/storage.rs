use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::persistence::models::{
    Draft, DraftPreview, Environment, EnvironmentSnapshot, RequestMetadata, ResponsePreview,
    ResponsePreviewDetail, ResponseSummary, SessionState, Snapshot, UIState, WorkspaceSnapshot,
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
    "environments",
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

fn merge_missing_request_metadata(
    mut request_metadata: RequestMetadata,
    fallback: &RequestMetadata,
) -> RequestMetadata {
    if request_metadata.request_name.is_none() {
        request_metadata.request_name = fallback.request_name.clone();
    }
    if request_metadata.folder_path.is_none() {
        request_metadata.folder_path = fallback.folder_path.clone();
    }

    request_metadata
}

/// Simple file-backed storage rooted at a directory.
/// Data is stored under <base_dir>/<category>/<key>.json
pub struct FileStorage {
    base_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDraft {
    #[serde(flatten)]
    draft: Draft,
    #[serde(default)]
    request_metadata: RequestMetadata,
}

impl StoredDraft {
    fn effective_request_metadata(&self) -> RequestMetadata {
        let mut request_metadata = self.request_metadata.clone();
        if request_metadata.folder_path.is_none() {
            request_metadata.folder_path = self.draft.path.clone();
        }

        request_metadata
    }
}

impl From<&Draft> for StoredDraft {
    fn from(draft: &Draft) -> Self {
        Self {
            draft: draft.clone(),
            request_metadata: RequestMetadata {
                request_name: None,
                folder_path: draft.path.clone(),
            },
        }
    }
}

impl From<StoredDraft> for Draft {
    fn from(stored_draft: StoredDraft) -> Self {
        let mut draft = stored_draft.draft;
        if draft.path.is_none() {
            draft.path = stored_draft.request_metadata.folder_path;
        }

        draft
    }
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
    request_metadata: RequestMetadata,
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
            request_metadata: RequestMetadata::default(),
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
    active_environment: Option<String>,
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
            active_environment: None,
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
            active_environment: None,
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

    fn load_stored_draft(&self, id: &str) -> Result<StoredDraft, PersistenceError> {
        self.read_json("drafts", id)
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

    fn load_first_stored_draft_preview_for_draft(
        &self,
        draft_id: &str,
    ) -> Result<Option<StoredDraftPreview>, PersistenceError> {
        for preview in self.load_all_stored_draft_previews()? {
            if preview.draft_id == draft_id {
                return Ok(Some(preview));
            }
        }

        Ok(None)
    }

    fn load_stored_session_state(&self) -> Result<StoredSessionState, PersistenceError> {
        self.read_json("session", "state")
    }

    fn load_all_environments(&self) -> Result<Vec<Environment>, PersistenceError> {
        let environment_ids = self.list_environments()?;
        let mut environments = Vec::with_capacity(environment_ids.len());

        for environment_id in environment_ids {
            environments.push(self.load_environment(&environment_id)?);
        }

        Ok(environments)
    }

    /* Draft APIs */
    pub fn save_draft(&self, draft: &Draft) -> Result<(), PersistenceError> {
        let existing_request_metadata = match self.load_stored_draft(&draft.id) {
            Ok(existing_draft) => existing_draft.effective_request_metadata(),
            Err(PersistenceError::NotFound(_)) => RequestMetadata::default(),
            Err(error) => return Err(error),
        };
        let mut stored_draft = StoredDraft::from(draft);
        stored_draft.request_metadata = merge_missing_request_metadata(
            stored_draft.request_metadata,
            &existing_request_metadata,
        );

        self.write_json("drafts", &draft.id, &stored_draft)
    }
    pub fn load_draft(&self, id: &str) -> Result<Draft, PersistenceError> {
        Ok(self.load_stored_draft(id)?.into())
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
        let stored_draft = match self.load_stored_draft(&stored_preview.draft_id) {
            Ok(stored_draft) => Some(stored_draft),
            Err(PersistenceError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let existing_preview = match self.load_stored_draft_preview(&stored_preview.id) {
            Ok(existing_preview) => Some(existing_preview),
            Err(PersistenceError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };

        stored_preview.workspace_id = stored_draft
            .as_ref()
            .and_then(|stored_draft| stored_draft.draft.workspace_id.clone())
            .or_else(|| {
                existing_preview
                    .as_ref()
                    .and_then(|existing_preview| existing_preview.workspace_id.clone())
            });
        let preview_request_metadata = if let Some(stored_draft) = stored_draft.as_ref() {
            let draft_request_metadata = stored_draft.effective_request_metadata();
            if let Some(existing_preview) = existing_preview.as_ref() {
                merge_missing_request_metadata(
                    draft_request_metadata,
                    &existing_preview.request_metadata,
                )
            } else {
                draft_request_metadata
            }
        } else if let Some(existing_preview) = existing_preview.as_ref() {
            existing_preview.request_metadata.clone()
        } else {
            RequestMetadata::default()
        };
        stored_preview.request_metadata = merge_missing_request_metadata(
            stored_preview.request_metadata,
            &preview_request_metadata,
        );

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
    pub fn save_draft_request_metadata(
        &self,
        draft_id: &str,
        request_metadata: &RequestMetadata,
    ) -> Result<(), PersistenceError> {
        let mut stored_draft = self.load_stored_draft(draft_id)?;
        stored_draft.request_metadata = request_metadata.clone();
        self.write_json("drafts", draft_id, &stored_draft)?;

        for preview_id in self.list_draft_previews()? {
            let mut stored_preview = self.load_stored_draft_preview(&preview_id)?;
            if stored_preview.draft_id == draft_id {
                stored_preview.request_metadata = request_metadata.clone();
                self.write_json("draft_previews", &preview_id, &stored_preview)?;
            }
        }

        Ok(())
    }
    pub fn load_draft_request_metadata(
        &self,
        draft_id: &str,
    ) -> Result<RequestMetadata, PersistenceError> {
        let stored_draft = match self.load_stored_draft(draft_id) {
            Ok(stored_draft) => Some(stored_draft),
            Err(PersistenceError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let stored_preview = self.load_first_stored_draft_preview_for_draft(draft_id)?;

        if stored_draft.is_none() && stored_preview.is_none() {
            return Err(PersistenceError::NotFound(format!(
                "draft metadata not found for {draft_id}"
            )));
        }

        let mut request_metadata = if let Some(stored_draft) = stored_draft.as_ref() {
            stored_draft.effective_request_metadata()
        } else {
            RequestMetadata::default()
        };

        if let Some(preview) = stored_preview {
            request_metadata = merge_missing_request_metadata(
                request_metadata,
                &merge_missing_request_metadata(
                    preview.request_metadata,
                    &RequestMetadata {
                        request_name: preview.preview_title,
                        folder_path: None,
                    },
                ),
            );
        }

        Ok(request_metadata)
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

    /* Environment APIs */
    pub fn save_environment(&self, environment: &Environment) -> Result<(), PersistenceError> {
        self.write_json("environments", &environment.id, environment)
    }
    pub fn load_environment(&self, id: &str) -> Result<Environment, PersistenceError> {
        self.read_json("environments", id)
    }
    pub fn delete_environment(&self, id: &str) -> Result<(), PersistenceError> {
        self.delete_json("environments", id)
    }
    pub fn list_environments(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_keys_in_category("environments")
    }
    pub fn save_environment_snapshot(
        &self,
        snapshot: &EnvironmentSnapshot,
    ) -> Result<(), PersistenceError> {
        let desired_environment_ids: HashSet<&str> = snapshot
            .environments
            .iter()
            .map(|environment| environment.id.as_str())
            .collect();

        for environment in &snapshot.environments {
            self.save_environment(environment)?;
        }

        for existing_environment_id in self.list_environments()? {
            if !desired_environment_ids.contains(existing_environment_id.as_str()) {
                self.delete_environment(&existing_environment_id)?;
            }
        }

        self.save_active_environment(snapshot.active_environment.as_deref())
    }
    pub fn load_environment_snapshot(&self) -> Result<EnvironmentSnapshot, PersistenceError> {
        Ok(EnvironmentSnapshot {
            active_environment: self.load_active_environment()?,
            environments: self.load_all_environments()?,
        })
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
        match self.load_stored_session_state() {
            Ok(existing_state) => {
                stored_state.selected_request = existing_state.selected_request;
                stored_state.active_environment = existing_state.active_environment;
            }
            Err(PersistenceError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }

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
    pub fn save_active_environment(
        &self,
        active_environment: Option<&str>,
    ) -> Result<(), PersistenceError> {
        let mut stored_state = match self.load_stored_session_state() {
            Ok(existing_state) => existing_state,
            Err(PersistenceError::NotFound(_)) => StoredSessionState::empty(),
            Err(error) => return Err(error),
        };
        stored_state.active_environment =
            active_environment.map(|environment| environment.to_owned());

        self.write_json("session", "state", &stored_state)
    }
    pub fn load_active_environment(&self) -> Result<Option<String>, PersistenceError> {
        match self.load_stored_session_state() {
            Ok(state) => Ok(state.active_environment),
            Err(PersistenceError::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
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
        Draft, DraftPreview, Environment, EnvironmentEntry, EnvironmentSnapshot, HeaderEntry,
        RequestMetadata, ResponsePreview, ResponsePreviewDetail, SessionState,
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
    fn draft_request_metadata_round_trips_and_survives_draft_updates() {
        let storage_path = unique_storage_path("draft-request-metadata");
        let storage = match FileStorage::new(&storage_path) {
            Ok(storage) => storage,
            Err(error) => panic!("failed to create file storage: {error}"),
        };

        let draft = Draft {
            id: "draft-one".to_owned(),
            workspace_id: Some("workspace-a".to_owned()),
            path: None,
            content: "GET https://example.com/items".to_owned(),
            created_at: None,
            tags: vec!["request".to_owned()],
        };
        let preview = DraftPreview {
            id: "preview-one".to_owned(),
            draft_id: draft.id.clone(),
            method: Some("GET".to_owned()),
            target_url: Some("https://example.com/items".to_owned()),
            preview_title: Some("temporary title".to_owned()),
            preview_snippet: None,
            tags: vec!["request".to_owned()],
            created_at: None,
        };

        if let Err(error) = storage.save_draft(&draft) {
            panic!("failed to save draft: {error}");
        }
        if let Err(error) = storage.save_draft_preview(&preview) {
            panic!("failed to save draft preview: {error}");
        }

        let request_metadata = RequestMetadata {
            request_name: Some("List items".to_owned()),
            folder_path: Some("Collections/API".to_owned()),
        };
        if let Err(error) = storage.save_draft_request_metadata(&draft.id, &request_metadata) {
            panic!("failed to save draft request metadata: {error}");
        }

        let loaded_request_metadata = match storage.load_draft_request_metadata(&draft.id) {
            Ok(request_metadata) => request_metadata,
            Err(error) => panic!("failed to load draft request metadata: {error}"),
        };
        assert_eq!(loaded_request_metadata, request_metadata);

        let loaded_draft = match storage.load_draft(&draft.id) {
            Ok(draft) => draft,
            Err(error) => panic!("failed to load draft: {error}"),
        };
        assert_eq!(loaded_draft.path.as_deref(), Some("Collections/API"));

        let updated_draft = Draft {
            content: "POST https://example.com/items".to_owned(),
            ..draft.clone()
        };
        if let Err(error) = storage.save_draft(&updated_draft) {
            panic!("failed to update draft: {error}");
        }

        let updated_preview = DraftPreview {
            preview_title: Some("changed title".to_owned()),
            ..preview.clone()
        };
        if let Err(error) = storage.save_draft_preview(&updated_preview) {
            panic!("failed to update draft preview: {error}");
        }

        let preserved_request_metadata = match storage.load_draft_request_metadata(&draft.id) {
            Ok(request_metadata) => request_metadata,
            Err(error) => panic!("failed to reload draft request metadata: {error}"),
        };
        assert_eq!(preserved_request_metadata, request_metadata);

        if let Err(error) = fs::remove_dir_all(&storage_path) {
            panic!("failed to clean up test storage directory: {error}");
        }
    }

    #[test]
    fn legacy_draft_fields_backfill_request_metadata() {
        let storage_path = unique_storage_path("legacy-draft-request-metadata");
        let storage = match FileStorage::new(&storage_path) {
            Ok(storage) => storage,
            Err(error) => panic!("failed to create file storage: {error}"),
        };

        let draft = Draft {
            id: "draft-legacy".to_owned(),
            workspace_id: Some("workspace-a".to_owned()),
            path: Some("Collections/Legacy".to_owned()),
            content: "GET https://example.com/legacy".to_owned(),
            created_at: None,
            tags: vec!["request".to_owned()],
        };
        let preview = DraftPreview {
            id: "preview-legacy".to_owned(),
            draft_id: draft.id.clone(),
            method: Some("GET".to_owned()),
            target_url: Some("https://example.com/legacy".to_owned()),
            preview_title: Some("Legacy request".to_owned()),
            preview_snippet: None,
            tags: vec!["request".to_owned()],
            created_at: None,
        };

        if let Err(error) = storage.write_json("drafts", &draft.id, &draft) {
            panic!("failed to write legacy draft: {error}");
        }
        if let Err(error) = storage.write_json("draft_previews", &preview.id, &preview) {
            panic!("failed to write legacy draft preview: {error}");
        }

        let loaded_request_metadata = match storage.load_draft_request_metadata(&draft.id) {
            Ok(request_metadata) => request_metadata,
            Err(error) => panic!("failed to load legacy draft request metadata: {error}"),
        };
        assert_eq!(
            loaded_request_metadata,
            RequestMetadata {
                request_name: Some("Legacy request".to_owned()),
                folder_path: Some("Collections/Legacy".to_owned()),
            }
        );

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

    #[test]
    fn environment_helpers_round_trip_records_and_active_selection() {
        let storage_path = unique_storage_path("environment-helpers");
        let storage = match FileStorage::new(&storage_path) {
            Ok(storage) => storage,
            Err(error) => panic!("failed to create file storage: {error}"),
        };

        let first_environment = Environment {
            id: "local".to_owned(),
            name: "Local".to_owned(),
            entries: vec![
                EnvironmentEntry {
                    key: "base_url".to_owned(),
                    value: "http://localhost:3000".to_owned(),
                },
                EnvironmentEntry {
                    key: "token".to_owned(),
                    value: "dev-token".to_owned(),
                },
            ],
        };
        let second_environment = Environment {
            id: "staging".to_owned(),
            name: "Staging".to_owned(),
            entries: vec![EnvironmentEntry {
                key: "base_url".to_owned(),
                value: "https://staging.example.com".to_owned(),
            }],
        };

        if let Err(error) = storage.save_environment(&first_environment) {
            panic!("failed to save first environment: {error}");
        }
        if let Err(error) = storage.save_environment(&second_environment) {
            panic!("failed to save second environment: {error}");
        }
        if let Err(error) = storage.save_active_environment(Some("staging")) {
            panic!("failed to save active environment: {error}");
        }

        let environment_ids = match storage.list_environments() {
            Ok(environment_ids) => environment_ids,
            Err(error) => panic!("failed to list environments: {error}"),
        };
        assert_eq!(
            environment_ids,
            vec!["local".to_owned(), "staging".to_owned()]
        );

        let loaded_environment = match storage.load_environment("local") {
            Ok(environment) => environment,
            Err(error) => panic!("failed to load environment: {error}"),
        };
        assert_eq!(loaded_environment, first_environment);

        let active_environment = match storage.load_active_environment() {
            Ok(active_environment) => active_environment,
            Err(error) => panic!("failed to load active environment: {error}"),
        };
        assert_eq!(active_environment.as_deref(), Some("staging"));

        if let Err(error) = storage.delete_environment("local") {
            panic!("failed to delete environment: {error}");
        }

        let remaining_environment_ids = match storage.list_environments() {
            Ok(environment_ids) => environment_ids,
            Err(error) => panic!("failed to list environments after delete: {error}"),
        };
        assert_eq!(remaining_environment_ids, vec!["staging".to_owned()]);

        if let Err(error) = fs::remove_dir_all(&storage_path) {
            panic!("failed to clean up test storage directory: {error}");
        }
    }

    #[test]
    fn environment_snapshot_replaces_records_and_preserves_session_state() {
        let storage_path = unique_storage_path("environment-snapshot");
        let storage = match FileStorage::new(&storage_path) {
            Ok(storage) => storage,
            Err(error) => panic!("failed to create file storage: {error}"),
        };

        let session_state = SessionState {
            selected_response: Some("response-5".to_owned()),
            active_view: Some("Request".to_owned()),
            open_panels: vec!["sidebar".to_owned()],
            updated_at: Some("2026-02-01T00:00:00Z".to_owned()),
        };
        if let Err(error) = storage.save_session_state(&session_state) {
            panic!("failed to save session state: {error}");
        }

        let legacy_environment = Environment {
            id: "legacy".to_owned(),
            name: "Legacy".to_owned(),
            entries: vec![EnvironmentEntry {
                key: "base_url".to_owned(),
                value: "https://legacy.example.com".to_owned(),
            }],
        };
        if let Err(error) = storage.save_environment(&legacy_environment) {
            panic!("failed to save legacy environment: {error}");
        }

        let snapshot = EnvironmentSnapshot {
            active_environment: Some("prod".to_owned()),
            environments: vec![
                Environment {
                    id: "dev".to_owned(),
                    name: "Development".to_owned(),
                    entries: vec![EnvironmentEntry {
                        key: "base_url".to_owned(),
                        value: "http://localhost:4000".to_owned(),
                    }],
                },
                Environment {
                    id: "prod".to_owned(),
                    name: "Production".to_owned(),
                    entries: vec![EnvironmentEntry {
                        key: "base_url".to_owned(),
                        value: "https://api.example.com".to_owned(),
                    }],
                },
            ],
        };

        if let Err(error) = storage.save_environment_snapshot(&snapshot) {
            panic!("failed to save environment snapshot: {error}");
        }

        let loaded_snapshot = match storage.load_environment_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("failed to load environment snapshot: {error}"),
        };
        assert_eq!(loaded_snapshot, snapshot);

        let environment_ids = match storage.list_environments() {
            Ok(environment_ids) => environment_ids,
            Err(error) => panic!("failed to list environments after snapshot save: {error}"),
        };
        assert_eq!(environment_ids, vec!["dev".to_owned(), "prod".to_owned()]);

        let loaded_session_state = match storage.load_session_state() {
            Ok(state) => state,
            Err(error) => panic!("failed to reload session state: {error}"),
        };
        assert_eq!(
            loaded_session_state.selected_response.as_deref(),
            Some("response-5")
        );
        assert_eq!(loaded_session_state.active_view.as_deref(), Some("Request"));

        if let Err(error) = fs::remove_dir_all(&storage_path) {
            panic!("failed to clean up test storage directory: {error}");
        }
    }
}
