use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::http_format::{HttpFormatError, parse_request, write_request};
use crate::persistence::models::{
    ResponsePreview, ResponsePreviewDetail, ResponseSummary, SessionState,
};
use crate::state::request::RequestDraft;

#[derive(Debug)]
pub enum PersistenceError {
    Io(io::Error),
    Serde(serde_json::Error),
    HttpFormat(HttpFormatError),
    NotFound(String),
    InvalidPath(String),
    Other(String),
}

impl From<io::Error> for PersistenceError {
    fn from(error: io::Error) -> Self {
        PersistenceError::Io(error)
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(error: serde_json::Error) -> Self {
        PersistenceError::Serde(error)
    }
}

impl From<HttpFormatError> for PersistenceError {
    fn from(error: HttpFormatError) -> Self {
        PersistenceError::HttpFormat(error)
    }
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::Io(e) => write!(f, "IO error: {e}"),
            PersistenceError::Serde(e) => write!(f, "JSON error: {e}"),
            PersistenceError::HttpFormat(e) => write!(f, "http format error: {e}"),
            PersistenceError::NotFound(p) => write!(f, "not found: {p}"),
            PersistenceError::InvalidPath(p) => write!(f, "invalid path: {p}"),
            PersistenceError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for PersistenceError {}

const COLLECTIONS_DIR: &str = "collections";
const ENV_FILE: &str = "http-client.env.json";
const PRIVATE_ENV_FILE: &str = "http-client.private.env.json";
const INTERNAL_DIR: &str = ".probe";
const RESPONSES_DIR: &str = "responses";
const RESPONSE_PREVIEWS_DIR: &str = "response_previews";
const SESSION_DIR: &str = "session";
const SESSION_STATE_KEY: &str = "state";
const OAUTH_CONFIGS_DIR: &str = "oauth_configs";
const HTTP_EXT: &str = "http";

/// Map of environment name -> variable map. Matches the
/// `http-client.env.json` layout used by VS Code REST Client and the
/// JetBrains HTTP Client.
pub type EnvFile = BTreeMap<String, BTreeMap<String, String>>;

/// A single `.http` file on disk.
#[derive(Debug, Clone)]
pub struct RequestFile {
    /// Relative path from the collections root, without the `.http` extension.
    /// Example: `"auth/login"`.
    pub relative_path: String,
    pub request: RequestDraft,
}

pub struct FileStorage {
    base_dir: PathBuf,
}

impl FileStorage {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self, PersistenceError> {
        let base = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&base)?;
        Ok(Self { base_dir: base })
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    // ---- Request (.http) APIs ---------------------------------------------

    pub fn save_request(&self, file: &RequestFile) -> Result<(), PersistenceError> {
        let path = self.request_path(&file.relative_path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = write_request(&file.request);
        atomic_write(&path, text.as_bytes())
    }

    pub fn load_request(&self, relative_path: &str) -> Result<RequestFile, PersistenceError> {
        let path = self.request_path(relative_path)?;
        if !path.exists() {
            return Err(PersistenceError::NotFound(path.display().to_string()));
        }
        let text = fs::read_to_string(&path)?;
        let mut request = parse_request(&text)?;

        let normalized = relative_path.trim_matches('/');
        let (folder, stem) = match normalized.rsplit_once('/') {
            Some((folder, stem)) => (folder.to_owned(), stem.to_owned()),
            None => (String::new(), normalized.to_owned()),
        };
        request.set_folder_path(&folder);
        if request.name.trim().is_empty() {
            request.set_request_name(&stem);
        }

        Ok(RequestFile {
            relative_path: normalized.to_owned(),
            request,
        })
    }

    pub fn delete_request(&self, relative_path: &str) -> Result<(), PersistenceError> {
        let path = self.request_path(relative_path)?;
        if !path.exists() {
            return Err(PersistenceError::NotFound(path.display().to_string()));
        }
        fs::remove_file(&path)?;

        let mut cursor = path.parent();
        let collections_root = self.base_dir.join(COLLECTIONS_DIR);
        while let Some(dir) = cursor {
            if dir == collections_root.as_path() || !dir.starts_with(&collections_root) {
                break;
            }
            match fs::read_dir(dir) {
                Ok(mut entries) => {
                    if entries.next().is_none() {
                        let _ = fs::remove_dir(dir);
                        cursor = dir.parent();
                        continue;
                    }
                }
                Err(_) => break,
            }
            break;
        }
        Ok(())
    }

    pub fn list_requests(&self) -> Result<Vec<RequestFile>, PersistenceError> {
        let root = self.base_dir.join(COLLECTIONS_DIR);
        if !root.exists() {
            return Ok(Vec::new());
        }

        let mut files: Vec<RequestFile> = Vec::new();
        collect_http_files(&root, &root, &mut files)?;
        files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(files)
    }

    fn request_path(&self, relative_path: &str) -> Result<PathBuf, PersistenceError> {
        if relative_path.is_empty()
            || relative_path.starts_with('/')
            || relative_path.starts_with('\\')
        {
            return Err(PersistenceError::InvalidPath(relative_path.to_owned()));
        }
        for segment in relative_path.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." {
                return Err(PersistenceError::InvalidPath(relative_path.to_owned()));
            }
            if segment
                .chars()
                .any(|c| matches!(c, '\\' | '\n' | '\r' | '\0' | ':'))
            {
                return Err(PersistenceError::InvalidPath(relative_path.to_owned()));
            }
        }

        let mut path = self.base_dir.join(COLLECTIONS_DIR);
        for segment in relative_path.split('/') {
            path.push(segment);
        }
        path.set_extension(HTTP_EXT);
        Ok(path)
    }

    // ---- Environment file APIs --------------------------------------------

    pub fn save_env_file(&self, envs: &EnvFile) -> Result<(), PersistenceError> {
        let path = self.base_dir.join(ENV_FILE);
        let text = serde_json::to_string_pretty(envs)?;
        atomic_write(&path, text.as_bytes())
    }

    pub fn load_env_file(&self) -> Result<EnvFile, PersistenceError> {
        self.load_env_file_at(&self.base_dir.join(ENV_FILE))
    }

    pub fn load_private_env_file(&self) -> Result<Option<EnvFile>, PersistenceError> {
        let path = self.base_dir.join(PRIVATE_ENV_FILE);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(self.load_env_file_at(&path)?))
    }

    fn load_env_file_at(&self, path: &Path) -> Result<EnvFile, PersistenceError> {
        if !path.exists() {
            return Err(PersistenceError::NotFound(path.display().to_string()));
        }
        let text = fs::read_to_string(path)?;
        let raw: serde_json::Value = serde_json::from_str(&text)?;
        let object = raw.as_object().ok_or_else(|| {
            PersistenceError::Other(format!("{} must be a JSON object", path.display()))
        })?;

        let mut result = EnvFile::new();
        for (env_name, env_value) in object {
            let Some(env_object) = env_value.as_object() else {
                continue;
            };
            let mut vars = BTreeMap::new();
            for (key, value) in env_object {
                if let Some(text) = value.as_str() {
                    vars.insert(key.clone(), text.to_owned());
                } else {
                    vars.insert(key.clone(), value.to_string());
                }
            }
            result.insert(env_name.clone(), vars);
        }
        Ok(result)
    }

    // ---- Response APIs (JSON sidecar under .probe/) -----------------------

    pub fn save_response_summary(&self, summary: &ResponseSummary) -> Result<(), PersistenceError> {
        self.write_internal_json(RESPONSES_DIR, &summary.id, summary)
    }

    pub fn load_response_summary(&self, id: &str) -> Result<ResponseSummary, PersistenceError> {
        self.read_internal_json(RESPONSES_DIR, id)
    }

    pub fn list_response_ids(&self) -> Result<Vec<String>, PersistenceError> {
        self.list_internal_keys(RESPONSES_DIR)
    }

    #[allow(dead_code)]
    pub fn delete_response(&self, id: &str) -> Result<(), PersistenceError> {
        let _ = self.delete_internal(RESPONSE_PREVIEWS_DIR, id);
        self.delete_internal(RESPONSES_DIR, id)
    }

    pub fn save_response_preview(&self, preview: &ResponsePreview) -> Result<(), PersistenceError> {
        let existing = self
            .read_internal_json::<StoredResponsePreview>(RESPONSE_PREVIEWS_DIR, &preview.id)
            .ok();
        let stored = StoredResponsePreview {
            preview: preview.clone(),
            detail: existing.map(|e| e.detail).unwrap_or_default(),
        };
        self.write_internal_json(RESPONSE_PREVIEWS_DIR, &preview.id, &stored)
    }

    pub fn load_response_preview(&self, id: &str) -> Result<ResponsePreview, PersistenceError> {
        let stored: StoredResponsePreview = self.read_internal_json(RESPONSE_PREVIEWS_DIR, id)?;
        Ok(stored.preview)
    }

    pub fn save_response_preview_detail(
        &self,
        id: &str,
        detail: &ResponsePreviewDetail,
    ) -> Result<(), PersistenceError> {
        let mut stored: StoredResponsePreview = self.read_internal_json(RESPONSE_PREVIEWS_DIR, id)?;
        stored.detail = detail.clone();
        self.write_internal_json(RESPONSE_PREVIEWS_DIR, id, &stored)
    }

    pub fn load_response_preview_detail(
        &self,
        id: &str,
    ) -> Result<ResponsePreviewDetail, PersistenceError> {
        let stored: StoredResponsePreview = self.read_internal_json(RESPONSE_PREVIEWS_DIR, id)?;
        Ok(stored.detail)
    }

    // ---- Session state ----------------------------------------------------

    pub fn save_session_state(&self, state: &SessionState) -> Result<(), PersistenceError> {
        self.write_internal_json(SESSION_DIR, SESSION_STATE_KEY, state)
    }

    pub fn load_session_state(&self) -> Result<SessionState, PersistenceError> {
        self.read_internal_json(SESSION_DIR, SESSION_STATE_KEY)
    }

    // ---- OAuth2 config ----------------------------------------------------

    pub fn save_oauth_config(
        &self,
        env_id: &str,
        config: &crate::oauth::OAuthConfig,
    ) -> Result<(), PersistenceError> {
        self.write_internal_json(OAUTH_CONFIGS_DIR, env_id, config)
    }

    pub fn load_oauth_config(
        &self,
        env_id: &str,
    ) -> Result<crate::oauth::OAuthConfig, PersistenceError> {
        self.read_internal_json(OAUTH_CONFIGS_DIR, env_id)
    }

    #[allow(dead_code)]
    pub fn delete_oauth_config(&self, env_id: &str) -> Result<(), PersistenceError> {
        self.delete_internal(OAUTH_CONFIGS_DIR, env_id)
    }

    // ---- Internal helpers -------------------------------------------------

    fn internal_dir(&self, category: &str) -> Result<PathBuf, PersistenceError> {
        if !is_valid_simple_key(category) {
            return Err(PersistenceError::InvalidPath(category.to_owned()));
        }
        let dir = self.base_dir.join(INTERNAL_DIR).join(category);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn internal_path(&self, category: &str, key: &str) -> Result<PathBuf, PersistenceError> {
        if !is_valid_simple_key(key) {
            return Err(PersistenceError::InvalidPath(key.to_owned()));
        }
        Ok(self.internal_dir(category)?.join(format!("{key}.json")))
    }

    fn write_internal_json<T: Serialize>(
        &self,
        category: &str,
        key: &str,
        value: &T,
    ) -> Result<(), PersistenceError> {
        let path = self.internal_path(category, key)?;
        let text = serde_json::to_string_pretty(value)?;
        atomic_write(&path, text.as_bytes())
    }

    fn read_internal_json<T: DeserializeOwned>(
        &self,
        category: &str,
        key: &str,
    ) -> Result<T, PersistenceError> {
        let path = self.internal_path(category, key)?;
        if !path.exists() {
            return Err(PersistenceError::NotFound(path.display().to_string()));
        }
        let text = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&text)?)
    }

    #[allow(dead_code)]
    fn delete_internal(&self, category: &str, key: &str) -> Result<(), PersistenceError> {
        let path = self.internal_path(category, key)?;
        if !path.exists() {
            return Err(PersistenceError::NotFound(path.display().to_string()));
        }
        fs::remove_file(&path)?;
        Ok(())
    }

    fn list_internal_keys(&self, category: &str) -> Result<Vec<String>, PersistenceError> {
        let dir = self.base_dir.join(INTERNAL_DIR).join(category);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut keys = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if let Some(key) = name.strip_suffix(".json") {
                if is_valid_simple_key(key) {
                    keys.push(key.to_owned());
                }
            }
        }
        keys.sort();
        Ok(keys)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredResponsePreview {
    #[serde(flatten)]
    preview: ResponsePreview,
    #[serde(default)]
    detail: ResponsePreviewDetail,
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<(), PersistenceError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = fs::File::create(&tmp)?;
    f.write_all(data)?;
    let _ = f.sync_all();
    fs::rename(&tmp, path)?;
    Ok(())
}

fn collect_http_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<RequestFile>,
) -> Result<(), PersistenceError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_http_files(root, &entry_path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if entry_path.extension().and_then(|ext| ext.to_str()) != Some(HTTP_EXT) {
            continue;
        }

        let relative = entry_path
            .strip_prefix(root)
            .map_err(|_| PersistenceError::Other("path strip failed".to_owned()))?;
        let relative_without_ext = relative.with_extension("");
        let relative_path = relative_without_ext
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");

        let text = fs::read_to_string(&entry_path)?;
        match parse_request(&text) {
            Ok(mut request) => {
                let folder = relative_without_ext
                    .parent()
                    .map(|path| {
                        path.components()
                            .map(|component| component.as_os_str().to_string_lossy().to_string())
                            .collect::<Vec<_>>()
                            .join("/")
                    })
                    .unwrap_or_default();
                if request.name.trim().is_empty() {
                    let file_stem = relative_without_ext
                        .file_name()
                        .and_then(|os| os.to_str())
                        .unwrap_or_default();
                    request.set_request_name(file_stem);
                }
                request.set_folder_path(&folder);
                out.push(RequestFile {
                    relative_path,
                    request,
                });
            }
            Err(error) => {
                tracing::warn!(
                    path = %entry_path.display(),
                    error = %error,
                    "skipping malformed .http file"
                );
            }
        }
    }
    Ok(())
}

fn is_valid_simple_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 255
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::request::{RequestAuth, RequestDraft};
    use std::collections::BTreeMap;

    fn temp_dir() -> PathBuf {
        let base = std::env::temp_dir().join(format!("probe-storage-{}", rand_suffix()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn rand_suffix() -> String {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{nanos}-{:p}", &nanos as *const _)
    }

    #[test]
    fn save_and_load_request_roundtrip() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();

        let request = RequestDraft {
            name: "Get user".to_owned(),
            folder: "auth".to_owned(),
            method: "GET".to_owned(),
            url: "https://api.example.com/me".to_owned(),
            query_params: vec![],
            auth: RequestAuth::Bearer {
                token: "{{API_TOKEN}}".to_owned(),
            },
            headers: vec![],
            body: None,
            attach_oauth: true,
            import_key: None,
        };
        let file = RequestFile {
            relative_path: "auth/me".to_owned(),
            request,
        };
        storage.save_request(&file).unwrap();

        let loaded = storage.load_request("auth/me").unwrap();
        assert_eq!(loaded.relative_path, "auth/me");
        assert_eq!(loaded.request.name, "Get user");
        assert_eq!(loaded.request.folder, "auth");
        assert_eq!(
            loaded.request.auth,
            RequestAuth::Bearer {
                token: "{{API_TOKEN}}".to_owned()
            }
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn list_requests_walks_directory_tree() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();

        for relative in ["auth/login", "auth/logout", "users/list"] {
            let mut request = RequestDraft::default_request();
            request.method = "GET".to_owned();
            request.url = format!("https://example.com/{relative}");
            storage
                .save_request(&RequestFile {
                    relative_path: relative.to_owned(),
                    request,
                })
                .unwrap();
        }

        let mut listed: Vec<String> = storage
            .list_requests()
            .unwrap()
            .into_iter()
            .map(|file| file.relative_path)
            .collect();
        listed.sort();
        assert_eq!(
            listed,
            vec![
                "auth/login".to_owned(),
                "auth/logout".to_owned(),
                "users/list".to_owned(),
            ]
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn delete_request_removes_empty_parent_dirs() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();

        let mut request = RequestDraft::default_request();
        request.method = "GET".to_owned();
        request.url = "https://example.com/x".to_owned();
        storage
            .save_request(&RequestFile {
                relative_path: "deep/nested/folder/one".to_owned(),
                request,
            })
            .unwrap();

        storage.delete_request("deep/nested/folder/one").unwrap();
        let deep = base.join(COLLECTIONS_DIR).join("deep");
        assert!(!deep.exists(), "empty parent dirs should be cleaned up");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn env_file_roundtrip() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();

        let mut envs: EnvFile = BTreeMap::new();
        envs.insert(
            "default".to_owned(),
            BTreeMap::from([
                ("baseUrl".to_owned(), "https://api.dev".to_owned()),
                ("API_TOKEN".to_owned(), "dev-token".to_owned()),
            ]),
        );
        envs.insert(
            "production".to_owned(),
            BTreeMap::from([("baseUrl".to_owned(), "https://api.prod".to_owned())]),
        );
        storage.save_env_file(&envs).unwrap();

        let loaded = storage.load_env_file().unwrap();
        assert_eq!(loaded, envs);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn invalid_relative_paths_are_rejected() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();

        for bad in ["", "/abs", "..", "a/..", "a/./b", "a\\b", "a:b"] {
            assert!(
                matches!(
                    storage.load_request(bad),
                    Err(PersistenceError::InvalidPath(_))
                ),
                "should reject {bad}"
            );
        }

        let _ = fs::remove_dir_all(&base);
    }
}
