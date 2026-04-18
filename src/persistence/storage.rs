use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::persistence::models::{
    Draft, DraftPreview, ResponsePreview, ResponseSummary, SessionState, Snapshot, UIState,
    WorkspaceSnapshot,
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
        self.write_json("draft_previews", &preview.id, preview)
    }
    pub fn load_draft_preview(&self, id: &str) -> Result<DraftPreview, PersistenceError> {
        self.read_json("draft_previews", id)
    }
    pub fn delete_draft_preview(&self, id: &str) -> Result<(), PersistenceError> {
        self.delete_json("draft_previews", id)
    }
    pub fn list_draft_previews(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_keys_in_category("draft_previews")
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
        self.write_json("response_previews", &preview.id, preview)
    }
    pub fn load_response_preview(&self, id: &str) -> Result<ResponsePreview, PersistenceError> {
        self.read_json("response_previews", id)
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
        self.write_json("session", "state", state)
    }
    pub fn load_session_state(&self) -> Result<SessionState, PersistenceError> {
        self.read_json("session", "state")
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
