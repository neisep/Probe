use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, mpsc};

use crate::oauth::config::slugify_env_id;
use crate::oauth::flows::refresh::{self, RefreshConfig};
use crate::oauth::{now_unix, FileTokenStore, FlowKind, OAuthConfig, OAuthError, Token, TokenStore};
use crate::persistence::FileStorage;

const REFRESH_BUFFER_SECONDS: i64 = 60;

struct CachedAuth {
    header: AttachmentHeader,
    valid_until: i64,
}

static AUTH_CACHE: OnceLock<Mutex<HashMap<String, CachedAuth>>> = OnceLock::new();

fn auth_cache() -> &'static Mutex<HashMap<String, CachedAuth>> {
    AUTH_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_auth(key: &str, header: AttachmentHeader, expires_at: i64) {
    if let Ok(mut guard) = auth_cache().lock() {
        guard.insert(
            key.to_owned(),
            CachedAuth {
                header,
                valid_until: expires_at - REFRESH_BUFFER_SECONDS,
            },
        );
    }
}

pub fn invalidate(env_id: &str) {
    invalidate_at(env_id, crate::oauth::DATA_DIR);
}

pub(crate) fn invalidate_at(env_id: &str, base_dir: &str) {
    let slug = slugify_env_id(env_id);
    let cache_key = format!("{base_dir}:{slug}");
    if let Ok(mut guard) = auth_cache().lock() {
        guard.remove(&cache_key);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentHeader {
    pub name: String,
    pub value: String,
}

pub enum AuthResolution {
    Ready(Result<Option<AttachmentHeader>, OAuthError>),
    Refreshing(mpsc::Receiver<Result<Option<AttachmentHeader>, OAuthError>>),
}

#[cfg(test)]
impl AuthResolution {
    fn into_ready(self) -> Result<Option<AttachmentHeader>, OAuthError> {
        match self {
            Self::Ready(r) => r,
            Self::Refreshing(_) => panic!("expected Ready, got Refreshing"),
        }
    }
}

pub fn resolve_authorization(env_name: &str) -> AuthResolution {
    resolve_authorization_at(env_name, crate::oauth::DATA_DIR)
}

pub(crate) fn resolve_authorization_at(
    env_name: &str,
    base_dir: &str,
) -> AuthResolution {
    let env_id = slugify_env_id(env_name);
    let cache_key = format!("{base_dir}:{env_id}");
    let now = now_unix();

    if let Ok(guard) = auth_cache().lock() {
        if let Some(cached) = guard.get(&cache_key) {
            if now < cached.valid_until {
                return AuthResolution::Ready(Ok(Some(cached.header.clone())));
            }
        }
    }

    let Ok(storage) = FileStorage::new(base_dir) else {
        return AuthResolution::Ready(Ok(None));
    };

    let Ok(config) = storage.load_oauth_config(&env_id) else {
        return AuthResolution::Ready(Ok(None));
    };
    if !config.injection.enabled {
        return AuthResolution::Ready(Ok(None));
    }
    let Some(flow) = config.active_flow else {
        return AuthResolution::Ready(Ok(None));
    };

    let token_store = FileTokenStore::new(base_dir);
    let Some(token) = (match token_store.get(&env_id, flow.as_str()) {
        Ok(t) => t,
        Err(e) => return AuthResolution::Ready(Err(e)),
    }) else {
        return AuthResolution::Ready(Ok(None));
    };

    if !token.expires_within(now, REFRESH_BUFFER_SECONDS) {
        let header = attachment_for(&config, &token);
        cache_auth(&cache_key, header.clone(), token.expires_at);
        return AuthResolution::Ready(Ok(Some(header)));
    }

    if let Some(refresh_token) = token.refresh_token.clone() {
        let Some(endpoint) = config.token_endpoint(flow) else {
            return AuthResolution::Ready(Err(OAuthError::Config(
                "token endpoint missing for refresh".into(),
            )));
        };
        let refresh_config = RefreshConfig {
            token_url: endpoint.token_url,
            client_id: endpoint.client_id,
            client_secret: endpoint.client_secret,
            refresh_token,
        };
        let base_dir_owned = base_dir.to_owned();
        let env_id_owned = env_id.clone();
        let scopes = token.scopes.clone();

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let refreshed = block_on_refresh(refresh_config, flow, &scopes)?;
                let store = FileTokenStore::new(&base_dir_owned);
                store.put(&env_id_owned, flow.as_str(), &refreshed)?;
                let header = attachment_for(&config, &refreshed);
                cache_auth(&cache_key, header.clone(), refreshed.expires_at);
                Ok(Some(header))
            })();
            let _ = tx.send(result);
        });
        return AuthResolution::Refreshing(rx);
    }

    if token.is_expired(now) {
        AuthResolution::Ready(Err(OAuthError::AuthDenied(
            "token expired and no refresh token available".into(),
        )))
    } else {
        let header = attachment_for(&config, &token);
        cache_auth(&cache_key, header.clone(), token.expires_at);
        AuthResolution::Ready(Ok(Some(header)))
    }
}

fn attachment_for(config: &OAuthConfig, token: &Token) -> AttachmentHeader {
    AttachmentHeader {
        name: config.injection.effective_header_name().to_owned(),
        value: config.injection.format_header_value(&token.access_token),
    }
}

fn block_on_refresh(
    config: RefreshConfig,
    flow: FlowKind,
    fallback_scopes: &[String],
) -> Result<Token, OAuthError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OAuthError::Http(format!("refresh runtime: {e}")))?;
    runtime.block_on(async { refresh::run(&config, flow, fallback_scopes).await })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::config::InjectionConfig;
    use crate::oauth::{FlowKind, OAuthConfig, Token};

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            use std::time::SystemTime;
            let nanos = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!("probe-oauth-mw-{nanos}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl std::ops::Deref for TempDir {
        type Target = std::path::Path;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl AsRef<std::path::Path> for TempDir {
        fn as_ref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn configured_env(base: &std::path::Path, flow: FlowKind) -> OAuthConfig {
        let storage = FileStorage::new(base).unwrap();
        let mut config = OAuthConfig::default();
        config.active_flow = Some(flow);
        match flow {
            FlowKind::ClientCredentials => {
                config.client_credentials.token_url = "https://example.com/token".into();
                config.client_credentials.client_id = "svc".into();
            }
            FlowKind::AuthCodePkce => {
                config.auth_code.token_url = "https://example.com/token".into();
                config.auth_code.client_id = "app".into();
                config.auth_code.auth_url = "https://example.com/authorize".into();
            }
            FlowKind::DeviceCode => {
                config.device_code.token_url = "https://example.com/token".into();
                config.device_code.client_id = "device".into();
                config.device_code.device_auth_url = "https://example.com/device".into();
            }
        }
        storage.save_oauth_config("dev", &config).unwrap();
        config
    }

    fn valid_token(flow: FlowKind) -> Token {
        Token {
            flow,
            access_token: "atk".into(),
            refresh_token: None,
            expires_at: now_unix() + 3600,
            obtained_at: now_unix(),
            scopes: vec![],
        }
    }

    #[test]
    fn returns_none_when_no_config() {
        let base = TempDir::new();
        let result = resolve_authorization_at("dev", base.to_str().unwrap()).into_ready().unwrap();
        assert!(result.is_none());

    }

    #[test]
    fn returns_none_when_flow_unset() {
        let base = TempDir::new();
        let storage = FileStorage::new(&base).unwrap();
        storage
            .save_oauth_config("dev", &OAuthConfig::default())
            .unwrap();
        let result = resolve_authorization_at("dev", base.to_str().unwrap()).into_ready().unwrap();
        assert!(result.is_none());

    }

    #[test]
    fn returns_none_when_no_token_stored() {
        let base = TempDir::new();
        configured_env(&base, FlowKind::ClientCredentials);
        let result = resolve_authorization_at("dev", base.to_str().unwrap()).into_ready().unwrap();
        assert!(result.is_none());

    }

    #[test]
    fn returns_default_authorization_bearer_when_token_valid() {
        let base = TempDir::new();
        configured_env(&base, FlowKind::ClientCredentials);
        let token_store = FileTokenStore::new(&base);
        token_store
            .put("dev", "client_credentials", &valid_token(FlowKind::ClientCredentials))
            .unwrap();

        let attachment = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap()
            .expect("expected attachment");
        assert_eq!(attachment.name, "Authorization");
        assert_eq!(attachment.value, "Bearer atk");

    }

    #[test]
    fn honors_custom_header_name() {
        let base = TempDir::new();
        let mut config = configured_env(&base, FlowKind::ClientCredentials);
        config.injection.header_name = "X-Custom-Auth".into();
        let storage = FileStorage::new(&base).unwrap();
        storage.save_oauth_config("dev", &config).unwrap();

        let token_store = FileTokenStore::new(&base);
        token_store
            .put("dev", "client_credentials", &valid_token(FlowKind::ClientCredentials))
            .unwrap();

        let attachment = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap()
            .expect("expected attachment");
        assert_eq!(attachment.name, "X-Custom-Auth");
        assert_eq!(attachment.value, "Bearer atk");

    }

    #[test]
    fn empty_prefix_produces_raw_token_value() {
        let base = TempDir::new();
        let mut config = configured_env(&base, FlowKind::ClientCredentials);
        config.injection.header_name = "X-API-Key".into();
        config.injection.header_prefix = "".into();
        let storage = FileStorage::new(&base).unwrap();
        storage.save_oauth_config("dev", &config).unwrap();

        let token_store = FileTokenStore::new(&base);
        token_store
            .put("dev", "client_credentials", &valid_token(FlowKind::ClientCredentials))
            .unwrap();

        let attachment = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap()
            .expect("expected attachment");
        assert_eq!(attachment.name, "X-API-Key");
        assert_eq!(attachment.value, "atk");

    }

    #[test]
    fn disabled_injection_returns_none() {
        let base = TempDir::new();
        let mut config = configured_env(&base, FlowKind::ClientCredentials);
        config.injection = InjectionConfig {
            enabled: false,
            header_name: "Authorization".into(),
            header_prefix: "Bearer".into(),
        };
        let storage = FileStorage::new(&base).unwrap();
        storage.save_oauth_config("dev", &config).unwrap();

        let token_store = FileTokenStore::new(&base);
        token_store
            .put("dev", "client_credentials", &valid_token(FlowKind::ClientCredentials))
            .unwrap();

        let result = resolve_authorization_at("dev", base.to_str().unwrap()).into_ready().unwrap();
        assert!(result.is_none());

    }

    #[test]
    fn errors_when_expired_without_refresh() {
        let base = TempDir::new();
        configured_env(&base, FlowKind::ClientCredentials);
        let token_store = FileTokenStore::new(&base);
        let token = Token {
            flow: FlowKind::ClientCredentials,
            access_token: "atk".into(),
            refresh_token: None,
            expires_at: now_unix() - 10,
            obtained_at: now_unix() - 3600,
            scopes: vec![],
        };
        token_store.put("dev", "client_credentials", &token).unwrap();

        let error = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .expect_err("expected error");
        assert!(matches!(error, OAuthError::AuthDenied(_)));

    }

    #[test]
    fn invalidate_drops_cached_header_so_next_call_rereads_token_store() {
        let base = TempDir::new();
        configured_env(&base, FlowKind::ClientCredentials);
        let token_store = FileTokenStore::new(&base);
        token_store
            .put("dev", "client_credentials", &valid_token(FlowKind::ClientCredentials))
            .unwrap();

        let first = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap()
            .expect("expected attachment");
        assert_eq!(first.value, "Bearer atk");

        token_store.delete("dev", "client_credentials").unwrap();

        let cached = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap()
            .expect("cache should still serve the previous token");
        assert_eq!(cached.value, "Bearer atk");

        invalidate_at("dev", base.to_str().unwrap());

        let after = resolve_authorization_at("dev", base.to_str().unwrap()).into_ready().unwrap();
        assert!(after.is_none(), "invalidate must force a re-read from the token store");
    }

    #[test]
    fn slugifies_env_name_consistently() {
        let base = TempDir::new();
        let storage = FileStorage::new(&base).unwrap();
        let mut config = OAuthConfig::default();
        config.active_flow = Some(FlowKind::ClientCredentials);
        config.client_credentials.token_url = "https://example.com/token".into();
        config.client_credentials.client_id = "svc".into();
        storage.save_oauth_config("My_Env", &config).unwrap();

        let token_store = FileTokenStore::new(&base);
        token_store
            .put("My_Env", "client_credentials", &valid_token(FlowKind::ClientCredentials))
            .unwrap();

        let attachment = resolve_authorization_at("My Env", base.to_str().unwrap())
            .into_ready()
            .unwrap()
            .expect("expected attachment");
        assert_eq!(attachment.value, "Bearer atk");

    }
}
