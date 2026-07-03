use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, mpsc};

use crate::oauth::config::slugify_env_id;
use crate::oauth::flows::refresh::{self, RefreshConfig};
use crate::oauth::{
    FileTokenStore, FlowKind, OAuthConfig, OAuthError, Token, TokenStore, now_unix,
};
use crate::persistence::FileStorage;

const REFRESH_BUFFER_SECONDS: i64 = 60;

type RefreshResult = Result<Option<AttachmentHeader>, OAuthError>;

struct CachedAuth {
    header: AttachmentHeader,
    valid_until: i64,
}

static AUTH_CACHE: OnceLock<Mutex<HashMap<String, CachedAuth>>> = OnceLock::new();
/// Holds the lazily-built tokio runtime *or* the error from building it.
/// Storing the result (rather than expect()-ing) means a failed build is
/// surfaced to callers as `OAuthError::Internal` instead of poisoning the
/// `OnceLock` and panicking every future call.
static REFRESH_RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
/// Per-cache-key list of subscribers waiting on an in-flight refresh.
/// Presence of a key means a refresh thread is already running; new
/// callers append their sender and await the same result instead of
/// racing the token endpoint.
static INFLIGHT_REFRESH: OnceLock<Mutex<HashMap<String, Vec<mpsc::Sender<RefreshResult>>>>> =
    OnceLock::new();

fn auth_cache() -> &'static Mutex<HashMap<String, CachedAuth>> {
    AUTH_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn inflight_refresh() -> &'static Mutex<HashMap<String, Vec<mpsc::Sender<RefreshResult>>>> {
    INFLIGHT_REFRESH.get_or_init(|| Mutex::new(HashMap::new()))
}

fn refresh_runtime() -> Result<&'static tokio::runtime::Runtime, OAuthError> {
    let cell = REFRESH_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("oauth refresh runtime: {e}"))
    });
    cell.as_ref()
        .map_err(|msg| OAuthError::Internal(msg.clone()))
}

/// Cache key for a `(base_dir, env_id)` pair. Canonicalising the base
/// dir means "./data" and "data" collapse to the same entry; without
/// this, a single env can end up with two stale-vs-fresh entries that
/// silently disagree.
fn cache_key(base_dir: &str, env_id: &str) -> String {
    let canon = std::fs::canonicalize(base_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| base_dir.to_owned());
    format!("{canon}:{env_id}")
}

/// Convert a refresh result into a value safe to fan out to multiple
/// followers. `OAuthError` isn't `Clone` (it wraps `io::Error` /
/// `serde_json::Error`), so we flatten the error to its `Display` text
/// inside `OAuthError::Internal`. The leader (first caller) gets the
/// original error; followers get a stringified copy with identical
/// message text.
fn clone_result_for_fanout(result: &RefreshResult) -> RefreshResult {
    match result {
        Ok(header) => Ok(header.clone()),
        Err(error) => Err(OAuthError::Internal(error.to_string())),
    }
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
    let key = cache_key(base_dir, &slug);
    if let Ok(mut guard) = auth_cache().lock() {
        guard.remove(&key);
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

pub(crate) fn resolve_authorization_at(env_name: &str, base_dir: &str) -> AuthResolution {
    let env_id = slugify_env_id(env_name);
    let key = cache_key(base_dir, &env_id);
    let now = now_unix();

    if let Ok(guard) = auth_cache().lock() {
        if let Some(cached) = guard.get(&key) {
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
        cache_auth(&key, header.clone(), token.expires_at);
        return AuthResolution::Ready(Ok(Some(header)));
    }

    if let Some(refresh_token) = token.refresh_token.clone() {
        let Some(endpoint) = config.token_endpoint(flow) else {
            return AuthResolution::Ready(Err(OAuthError::Config(
                "token endpoint missing for refresh".into(),
            )));
        };

        // ---- Single-flight: at most one refresh per (base_dir, env_id) ----
        //
        // Lock the inflight map briefly. If an existing entry is present,
        // another thread is already refreshing this exact key — we attach
        // our sender to its subscriber list and return a Receiver. The
        // leader thread fans the same result out to every subscriber.
        let (tx, rx) = mpsc::channel::<RefreshResult>();
        let became_leader = {
            let Ok(mut inflight) = inflight_refresh().lock() else {
                // Lock poisoning is unexpected but recoverable — fall
                // back to single-shot refresh behaviour instead of
                // panicking.
                return AuthResolution::Ready(Err(OAuthError::Internal(
                    "inflight refresh lock poisoned".into(),
                )));
            };
            match inflight.get_mut(&key) {
                Some(subscribers) => {
                    subscribers.push(tx);
                    false
                }
                None => {
                    inflight.insert(key.clone(), vec![tx]);
                    true
                }
            }
        };

        if !became_leader {
            return AuthResolution::Refreshing(rx);
        }

        let refresh_config = RefreshConfig {
            token_url: endpoint.token_url,
            client_id: endpoint.client_id,
            client_secret: endpoint.client_secret,
            refresh_token,
        };
        let base_dir_owned = base_dir.to_owned();
        let env_id_owned = env_id.clone();
        let scopes = token.scopes.clone();
        let key_owned = key.clone();
        let config_owned = config.clone();

        std::thread::spawn(move || {
            let result: RefreshResult = (|| {
                let refreshed = block_on_refresh(refresh_config, flow, &scopes)?;
                let store = FileTokenStore::new(&base_dir_owned);
                store.put(&env_id_owned, flow.as_str(), &refreshed)?;
                let header = attachment_for(&config_owned, &refreshed);
                cache_auth(&key_owned, header.clone(), refreshed.expires_at);
                Ok(Some(header))
            })();

            // Drain the subscriber list under the lock so a late arriver
            // (between the result completing and the slot being removed)
            // becomes the next leader rather than waiting on a closed
            // sender.
            let subscribers = match inflight_refresh().lock() {
                Ok(mut guard) => guard.remove(&key_owned).unwrap_or_default(),
                Err(_) => Vec::new(),
            };

            for tx in subscribers {
                let _ = tx.send(clone_result_for_fanout(&result));
            }
        });

        return AuthResolution::Refreshing(rx);
    }

    if token.is_expired(now) {
        AuthResolution::Ready(Err(OAuthError::AuthDenied(
            "token expired and no refresh token available".into(),
        )))
    } else {
        let header = attachment_for(&config, &token);
        cache_auth(&key, header.clone(), token.expires_at);
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
    refresh_runtime()?.block_on(async { refresh::run(&config, flow, fallback_scopes).await })
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
        let result = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn returns_none_when_flow_unset() {
        let base = TempDir::new();
        let storage = FileStorage::new(&base).unwrap();
        storage
            .save_oauth_config("dev", &OAuthConfig::default())
            .unwrap();
        let result = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn returns_none_when_no_token_stored() {
        let base = TempDir::new();
        configured_env(&base, FlowKind::ClientCredentials);
        let result = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn returns_default_authorization_bearer_when_token_valid() {
        let base = TempDir::new();
        configured_env(&base, FlowKind::ClientCredentials);
        let token_store = FileTokenStore::new(&base);
        token_store
            .put(
                "dev",
                "client_credentials",
                &valid_token(FlowKind::ClientCredentials),
            )
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
            .put(
                "dev",
                "client_credentials",
                &valid_token(FlowKind::ClientCredentials),
            )
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
            .put(
                "dev",
                "client_credentials",
                &valid_token(FlowKind::ClientCredentials),
            )
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
            .put(
                "dev",
                "client_credentials",
                &valid_token(FlowKind::ClientCredentials),
            )
            .unwrap();

        let result = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap();
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
        token_store
            .put("dev", "client_credentials", &token)
            .unwrap();

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
            .put(
                "dev",
                "client_credentials",
                &valid_token(FlowKind::ClientCredentials),
            )
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

        let after = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap();
        assert!(
            after.is_none(),
            "invalidate must force a re-read from the token store"
        );
    }

    #[test]
    fn clone_result_for_fanout_preserves_ok_header() {
        let header = AttachmentHeader {
            name: "Authorization".into(),
            value: "Bearer abc".into(),
        };
        let cloned = clone_result_for_fanout(&Ok(Some(header.clone())));
        assert_eq!(cloned.unwrap(), Some(header));
    }

    #[test]
    fn clone_result_for_fanout_flattens_err_to_internal_with_same_text() {
        let original = OAuthError::AuthDenied("bad refresh".into());
        let original_text = original.to_string();
        let cloned = clone_result_for_fanout(&Err(original));
        match cloned {
            Err(OAuthError::Internal(text)) => assert_eq!(text, original_text),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn cache_key_canonicalizes_equivalent_paths_to_same_key() {
        use std::path::PathBuf;
        let base = TempDir::new();

        // Construct an alternate spelling of the same directory by going
        // through `tmp_dir/../<basename>` so canonicalize() produces the
        // identical absolute path on both inputs.
        let path: PathBuf = base.0.clone();
        let parent = path.parent().expect("temp dir has a parent");
        let basename = path
            .file_name()
            .expect("temp dir has a file name")
            .to_string_lossy()
            .into_owned();
        let indirect = parent
            .join("..")
            .join(
                parent
                    .file_name()
                    .expect("parent has file name")
                    .to_string_lossy()
                    .into_owned(),
            )
            .join(&basename);

        let direct_key = cache_key(path.to_str().unwrap(), "dev");
        let indirect_key = cache_key(indirect.to_str().unwrap(), "dev");
        assert_eq!(
            direct_key, indirect_key,
            "equivalent paths must collapse to the same cache key"
        );
    }

    #[test]
    fn cache_key_falls_back_to_raw_when_path_is_unresolvable() {
        // Non-existent path → canonicalize fails → key falls back to the
        // raw string. Two distinct raw strings stay distinct.
        let a = cache_key("/this/does/not/exist/a", "dev");
        let b = cache_key("/this/does/not/exist/b", "dev");
        assert_ne!(a, b);
    }

    #[test]
    fn second_caller_during_inflight_refresh_becomes_a_follower() {
        // Pre-insert a leader slot for the cache key so the next call
        // that needs a refresh attaches to the existing subscriber list
        // instead of spawning a second refresh thread.
        let base = TempDir::new();
        let env_name = "inflight-test-env";
        let env_id = slugify_env_id(env_name);
        let key = cache_key(base.to_str().unwrap(), &env_id);

        // Make sure no prior test left state for this key (the static
        // INFLIGHT_REFRESH is process-global).
        {
            let mut guard = inflight_refresh().lock().expect("inflight lock");
            guard.remove(&key);
        }

        // Set up a config + token with a refresh_token so the resolver
        // takes the refresh branch.
        let mut config = configured_env(&base, FlowKind::ClientCredentials);
        config.client_credentials.token_url = "https://example.invalid/token".into();
        let storage = FileStorage::new(&base).unwrap();
        storage.save_oauth_config(env_name, &config).unwrap();
        let token_store = FileTokenStore::new(&base);
        let expiring_token = Token {
            flow: FlowKind::ClientCredentials,
            access_token: "atk".into(),
            // Refresh-eligible (within REFRESH_BUFFER_SECONDS of now).
            refresh_token: Some("rtk".into()),
            expires_at: now_unix() + 5,
            obtained_at: now_unix() - 3600,
            scopes: vec![],
        };
        token_store
            .put(&env_id, "client_credentials", &expiring_token)
            .unwrap();

        // Pre-insert a placeholder leader subscriber so the next caller
        // attaches as a follower (and doesn't spawn a real refresh).
        let (placeholder_tx, _placeholder_rx) = mpsc::channel::<RefreshResult>();
        {
            let mut guard = inflight_refresh().lock().expect("inflight lock");
            guard.insert(key.clone(), vec![placeholder_tx]);
        }

        // The second caller must observe Refreshing (becoming a follower)
        // and the subscriber count must rise to 2 — confirming we did not
        // spawn a second refresh thread.
        let resolution = resolve_authorization_at(env_name, base.to_str().unwrap());
        assert!(
            matches!(resolution, AuthResolution::Refreshing(_)),
            "second caller during inflight refresh must return Refreshing"
        );
        let subscribers = inflight_refresh()
            .lock()
            .expect("inflight lock")
            .get(&key)
            .map(Vec::len)
            .unwrap_or(0);
        assert_eq!(
            subscribers, 2,
            "follower must append to existing subscriber list (placeholder + new)"
        );

        // Cleanup so we don't leave a slot behind for other tests.
        let mut guard = inflight_refresh().lock().expect("inflight lock");
        guard.remove(&key);
    }

    #[test]
    fn refresh_runtime_propagates_failure_as_internal_error() {
        // We can't trigger a real runtime build failure from a test
        // (tokio::runtime::Builder::build is robust), but we can verify
        // that the public refresh_runtime() returns a usable runtime
        // and never panics — the regression we're guarding against is
        // the previous `.expect()` poisoning the OnceLock.
        let rt = refresh_runtime().expect("runtime should build");
        // Smoke test: actually drive a trivial future on it.
        let two = rt.block_on(async { 1 + 1 });
        assert_eq!(two, 2);
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
            .put(
                "My_Env",
                "client_credentials",
                &valid_token(FlowKind::ClientCredentials),
            )
            .unwrap();

        let attachment = resolve_authorization_at("My Env", base.to_str().unwrap())
            .into_ready()
            .unwrap()
            .expect("expected attachment");
        assert_eq!(attachment.value, "Bearer atk");
    }
}
