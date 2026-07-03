use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde::{Deserialize, Serialize};

use super::{OAuthError, Token};

const INTERNAL_DIR: &str = ".probe";
const TOKENS_DIR: &str = "oauth_tokens";

pub trait TokenStore {
    fn get(&self, env_id: &str, flow_id: &str) -> Result<Option<Token>, OAuthError>;
    fn put(&self, env_id: &str, flow_id: &str, token: &Token) -> Result<(), OAuthError>;
    fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError>;
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct EnvTokenFile {
    #[serde(flatten)]
    tokens: BTreeMap<String, Token>,
}

pub struct FileTokenStore {
    base_dir: PathBuf,
}

impl FileTokenStore {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    fn tokens_dir(&self) -> PathBuf {
        self.base_dir.join(INTERNAL_DIR).join(TOKENS_DIR)
    }

    fn env_path(&self, env_id: &str) -> Result<PathBuf, OAuthError> {
        validate_key(env_id)?;
        Ok(self.tokens_dir().join(format!("{env_id}.json")))
    }

    fn load_env(&self, env_id: &str) -> Result<EnvTokenFile, OAuthError> {
        let path = self.env_path(env_id)?;
        if !path.exists() {
            return Ok(EnvTokenFile::default());
        }
        let text = fs::read_to_string(&path)?;
        if text.trim().is_empty() {
            return Ok(EnvTokenFile::default());
        }
        Ok(serde_json::from_str(&text)?)
    }

    fn save_env(&self, env_id: &str, file: &EnvTokenFile) -> Result<(), OAuthError> {
        let path = self.env_path(env_id)?;
        if file.tokens.is_empty() {
            if path.exists() {
                fs::remove_file(&path)?;
            }
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            // Tighten the tokens dir to owner-only on Unix. This is
            // best-effort and runs on every save — that keeps the floor
            // in place if someone later widens permissions by mistake.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }
        let text = serde_json::to_string_pretty(file)?;
        atomic_write(&path, text.as_bytes())
    }
}

impl TokenStore for FileTokenStore {
    fn get(&self, env_id: &str, flow_id: &str) -> Result<Option<Token>, OAuthError> {
        validate_key(flow_id)?;
        let file = self.load_env(env_id)?;
        Ok(file.tokens.get(flow_id).cloned())
    }

    fn put(&self, env_id: &str, flow_id: &str, token: &Token) -> Result<(), OAuthError> {
        validate_key(flow_id)?;
        let path = self.env_path(env_id)?;
        let lock = write_lock(&env_lock_key(&path));
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut file = self.load_env(env_id)?;
        file.tokens.insert(flow_id.to_owned(), token.clone());
        self.save_env(env_id, &file)
    }

    fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError> {
        validate_key(flow_id)?;
        let path = self.env_path(env_id)?;
        let lock = write_lock(&env_lock_key(&path));
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut file = self.load_env(env_id)?;
        if file.tokens.remove(flow_id).is_none() {
            return Ok(());
        }
        self.save_env(env_id, &file)
    }
}

#[cfg(test)]
impl FileTokenStore {
    pub fn delete_env(&self, env_id: &str) -> Result<(), OAuthError> {
        let path = self.env_path(env_id)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<(String, String)>, OAuthError> {
        let dir = self.tokens_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some(env_id) = name.strip_suffix(".json") else {
                continue;
            };
            if validate_key(env_id).is_err() {
                continue;
            }
            let file = self.load_env(env_id)?;
            for flow_id in file.tokens.keys() {
                out.push((env_id.to_owned(), flow_id.clone()));
            }
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(feature = "keyring-storage")]
pub struct KeyringTokenStore;

#[cfg(feature = "keyring-storage")]
impl KeyringTokenStore {
    const SERVICE: &'static str = "probe-oauth";

    fn entry(env_id: &str) -> Result<keyring::Entry, OAuthError> {
        keyring::Entry::new(Self::SERVICE, env_id)
            .map_err(|e| OAuthError::Config(format!("keyring entry: {e}")))
    }

    fn load_env(env_id: &str) -> Result<EnvTokenFile, OAuthError> {
        let entry = Self::entry(env_id)?;
        match entry.get_password() {
            Ok(json) => Ok(serde_json::from_str(&json)?),
            Err(keyring::Error::NoEntry) => Ok(EnvTokenFile::default()),
            Err(e) => Err(OAuthError::Config(format!("keyring read: {e}"))),
        }
    }

    fn save_env(env_id: &str, file: &EnvTokenFile) -> Result<(), OAuthError> {
        let entry = Self::entry(env_id)?;
        if file.tokens.is_empty() {
            return match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(OAuthError::Config(format!("keyring delete: {e}"))),
            };
        }
        let json = serde_json::to_string(file)?;
        entry
            .set_password(&json)
            .map_err(|e| OAuthError::Config(format!("keyring write: {e}")))
    }
}

#[cfg(feature = "keyring-storage")]
impl TokenStore for KeyringTokenStore {
    fn get(&self, env_id: &str, flow_id: &str) -> Result<Option<Token>, OAuthError> {
        validate_key(env_id)?;
        validate_key(flow_id)?;
        let file = Self::load_env(env_id)?;
        Ok(file.tokens.get(flow_id).cloned())
    }

    fn put(&self, env_id: &str, flow_id: &str, token: &Token) -> Result<(), OAuthError> {
        validate_key(env_id)?;
        validate_key(flow_id)?;
        let lock = write_lock(&format!("keyring:{env_id}"));
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut file = Self::load_env(env_id)?;
        file.tokens.insert(flow_id.to_owned(), token.clone());
        Self::save_env(env_id, &file)
    }

    fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError> {
        validate_key(env_id)?;
        validate_key(flow_id)?;
        let lock = write_lock(&format!("keyring:{env_id}"));
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut file = Self::load_env(env_id)?;
        if file.tokens.remove(flow_id).is_none() {
            return Ok(());
        }
        Self::save_env(env_id, &file)
    }

}

/// Serialises the read-modify-write of a single token-store entry.
///
/// `put`/`delete` load the whole env file (it holds every flow's token),
/// mutate one flow, and write it back. Without serialisation two concurrent
/// writers to the same file — e.g. the OAuth refresh thread rotating one
/// flow's `refresh_token` while the UI saves another flow — both read the old
/// file, each applies its own change, and the last writer wins, silently
/// dropping the other's update (including a freshly rotated refresh token).
/// Locking is per-entry so unrelated envs never contend. The registry holds a
/// small, bounded number of locks (one per env file ever touched this run).
fn write_lock(key: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    let registry = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = registry.lock().unwrap_or_else(PoisonError::into_inner);
    guard.entry(key.to_owned()).or_default().clone()
}

/// Lock key for an env file. The file itself may not exist yet (first write),
/// so we canonicalise its parent directory and rejoin the file name; this
/// collapses "./data/..." and "data/..." to a single lock. Falls back to the
/// raw path when the parent can't be resolved.
fn env_lock_key(path: &Path) -> String {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => fs::canonicalize(parent)
            .map(|p| p.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
    .to_string_lossy()
    .into_owned()
}

fn validate_key(key: &str) -> Result<(), OAuthError> {
    if key.is_empty()
        || key.len() > 255
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(OAuthError::InvalidKey(key.to_owned()));
    }
    Ok(())
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<(), OAuthError> {
    // Per-call unique suffix so concurrent writes to the same token file
    // don't stomp each other's in-flight temp file.
    let tmp = unique_tmp_path(path);

    let write_result = (|| -> io::Result<()> {
        let mut f = create_token_tmp_file(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    write_result?;

    #[cfg(unix)]
    if let Some(parent) = path.parent()
        && let Ok(dir) = fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }

    Ok(())
}

/// Create the temp file used for atomic write of a token file. On Unix
/// the file is born with mode `0o600` (owner read/write only) via
/// `OpenOptions::mode` — closing the window where a default-umask
/// `File::create` produces a 0o644 file we'd later have to chmod down.
#[cfg(unix)]
fn create_token_tmp_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_token_tmp_file(path: &Path) -> io::Result<fs::File> {
    // On Windows / WASI the Unix permission model doesn't apply; ACLs
    // / NTFS permissions are inherited from the parent directory. Keep
    // the simple `File::create` behaviour and rely on the caller's
    // directory ACL for confidentiality.
    fs::File::create(path)
}

fn unique_tmp_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut tmp = path.to_path_buf();
    tmp.set_extension(format!("tmp.{nanos}.{n}"));
    tmp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::FlowKind;
    use std::time::SystemTime;

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("probe-oauth-{nanos}"));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn sample(flow: FlowKind) -> Token {
        Token {
            flow,
            access_token: "at".into(),
            refresh_token: Some("rt".into()),
            expires_at: 1_000_000,
            obtained_at: 999_000,
            scopes: vec!["openid".into(), "profile".into()],
        }
    }

    #[test]
    fn put_get_roundtrip() {
        let base = temp_dir();
        let store = FileTokenStore::new(&base);
        let token = sample(FlowKind::AuthCodePkce);

        store.put("dev", "auth_code_pkce", &token).unwrap();
        let loaded = store.get("dev", "auth_code_pkce").unwrap().unwrap();
        assert_eq!(loaded, token);

        let missing = store.get("dev", "client_credentials").unwrap();
        assert!(missing.is_none());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn multiple_flows_per_env() {
        let base = temp_dir();
        let store = FileTokenStore::new(&base);

        store
            .put("dev", "auth_code_pkce", &sample(FlowKind::AuthCodePkce))
            .unwrap();
        store
            .put(
                "dev",
                "client_credentials",
                &sample(FlowKind::ClientCredentials),
            )
            .unwrap();

        let mut listed = store.list().unwrap();
        listed.sort();
        assert_eq!(
            listed,
            vec![
                ("dev".to_owned(), "auth_code_pkce".to_owned()),
                ("dev".to_owned(), "client_credentials".to_owned()),
            ]
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn delete_flow_leaves_others_intact() {
        let base = temp_dir();
        let store = FileTokenStore::new(&base);

        store
            .put("dev", "auth_code_pkce", &sample(FlowKind::AuthCodePkce))
            .unwrap();
        store
            .put(
                "dev",
                "client_credentials",
                &sample(FlowKind::ClientCredentials),
            )
            .unwrap();

        store.delete("dev", "auth_code_pkce").unwrap();

        assert!(store.get("dev", "auth_code_pkce").unwrap().is_none());
        assert!(store.get("dev", "client_credentials").unwrap().is_some());

        store.delete("dev", "auth_code_pkce").unwrap();

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn delete_env_wipes_file() {
        let base = temp_dir();
        let store = FileTokenStore::new(&base);

        store
            .put("dev", "auth_code_pkce", &sample(FlowKind::AuthCodePkce))
            .unwrap();
        store.delete_env("dev").unwrap();

        assert!(store.list().unwrap().is_empty());
        assert!(!store.env_path("dev").unwrap().exists());

        store.delete_env("dev").unwrap();

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn invalid_keys_rejected() {
        let base = temp_dir();
        let store = FileTokenStore::new(&base);
        let token = sample(FlowKind::AuthCodePkce);

        for bad in ["", "a/b", "a.b", "a b", ".."] {
            assert!(matches!(
                store.put(bad, "auth_code_pkce", &token),
                Err(OAuthError::InvalidKey(_))
            ));
            assert!(matches!(
                store.put("dev", bad, &token),
                Err(OAuthError::InvalidKey(_))
            ));
        }

        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn token_file_has_owner_only_mode_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let base = temp_dir();
        let store = FileTokenStore::new(&base);
        store
            .put("dev", "auth_code_pkce", &sample(FlowKind::AuthCodePkce))
            .expect("put");

        let path = store.env_path("dev").unwrap();
        let metadata = fs::metadata(&path).expect("token file exists");
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "token file must be readable only by its owner (got {mode:o})"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn tokens_directory_has_owner_only_mode_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let base = temp_dir();
        let store = FileTokenStore::new(&base);
        store
            .put("dev", "auth_code_pkce", &sample(FlowKind::AuthCodePkce))
            .expect("put");

        let dir = store.tokens_dir();
        let metadata = fs::metadata(&dir).expect("tokens dir exists");
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o700,
            "tokens directory must be traversable only by its owner (got {mode:o})"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn concurrent_puts_to_same_env_do_not_clobber_a_rotated_refresh_token() {
        // C4 regression: two writers hit the same env file at once — one
        // rotates the auth_code flow's refresh token, the other writes a
        // second flow. Without the per-file write lock the unguarded
        // read-modify-write races and the last writer drops the other's
        // update. Repeat enough rounds to surface the race reliably.
        let base = temp_dir();
        let store = Arc::new(FileTokenStore::new(&base));

        for round in 0..50 {
            store.delete_env("dev").unwrap();

            let mut rotated = sample(FlowKind::AuthCodePkce);
            let expected_rt = format!("rotated-{round}");
            rotated.refresh_token = Some(expected_rt.clone());

            let writer_a = Arc::clone(&store);
            let writer_b = Arc::clone(&store);
            let t1 = std::thread::spawn(move || writer_a.put("dev", "auth_code_pkce", &rotated));
            let t2 = std::thread::spawn(move || {
                writer_b.put(
                    "dev",
                    "client_credentials",
                    &sample(FlowKind::ClientCredentials),
                )
            });
            t1.join().unwrap().unwrap();
            t2.join().unwrap().unwrap();

            let kept = store
                .get("dev", "auth_code_pkce")
                .unwrap()
                .expect("auth_code flow must survive the concurrent write");
            assert_eq!(
                kept.refresh_token.as_deref(),
                Some(expected_rt.as_str()),
                "rotated refresh token must not be clobbered (round {round})"
            );
            assert!(
                store.get("dev", "client_credentials").unwrap().is_some(),
                "client_credentials flow must survive the concurrent write (round {round})"
            );
        }

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn empty_env_file_is_cleaned_up() {
        let base = temp_dir();
        let store = FileTokenStore::new(&base);

        store
            .put("dev", "auth_code_pkce", &sample(FlowKind::AuthCodePkce))
            .unwrap();
        store.delete("dev", "auth_code_pkce").unwrap();

        assert!(!store.env_path("dev").unwrap().exists());

        let _ = fs::remove_dir_all(&base);
    }
}
