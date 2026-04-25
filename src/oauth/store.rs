use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{OAuthError, Token};

const INTERNAL_DIR: &str = ".probe";
const TOKENS_DIR: &str = "oauth_tokens";

pub trait TokenStore {
    fn get(&self, env_id: &str, flow_id: &str) -> Result<Option<Token>, OAuthError>;
    fn put(&self, env_id: &str, flow_id: &str, token: &Token) -> Result<(), OAuthError>;
    fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError>;
    fn delete_env(&self, env_id: &str) -> Result<(), OAuthError>;
    fn list(&self) -> Result<Vec<(String, String)>, OAuthError>;
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
        let mut file = self.load_env(env_id)?;
        file.tokens.insert(flow_id.to_owned(), token.clone());
        self.save_env(env_id, &file)
    }

    fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError> {
        validate_key(flow_id)?;
        let mut file = self.load_env(env_id)?;
        if file.tokens.remove(flow_id).is_none() {
            return Ok(());
        }
        self.save_env(env_id, &file)
    }

    fn delete_env(&self, env_id: &str) -> Result<(), OAuthError> {
        let path = self.env_path(env_id)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<(String, String)>, OAuthError> {
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
        let mut file = Self::load_env(env_id)?;
        file.tokens.insert(flow_id.to_owned(), token.clone());
        Self::save_env(env_id, &file)
    }

    fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError> {
        validate_key(env_id)?;
        validate_key(flow_id)?;
        let mut file = Self::load_env(env_id)?;
        if file.tokens.remove(flow_id).is_none() {
            return Ok(());
        }
        Self::save_env(env_id, &file)
    }

    fn delete_env(&self, env_id: &str) -> Result<(), OAuthError> {
        validate_key(env_id)?;
        let entry = Self::entry(env_id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(OAuthError::Config(format!("keyring delete_env: {e}"))),
        }
    }

    fn list(&self) -> Result<Vec<(String, String)>, OAuthError> {
        Ok(Vec::new())
    }
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
    let tmp = path.with_extension("tmp");
    let mut f = fs::File::create(&tmp)?;
    f.write_all(data)?;
    let _ = f.sync_all();
    fs::rename(&tmp, path)?;
    Ok(())
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
