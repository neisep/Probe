use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, mpsc};

use crate::oauth::config::slugify_env_id;
use crate::oauth::flows::client_credentials::{self, ClientCredentialsConfig};
use crate::oauth::flows::refresh::{self, RefreshConfig};
use crate::oauth::{
    FlowKind, OAuthConfig, OAuthError, Token, TokenStore, now_unix, reacquire, token_store_at,
};
use crate::persistence::FileStorage;

/// Renew this many seconds ahead of expiry so a token cannot die in flight.
const REFRESH_BUFFER_SECONDS: i64 = 60;

/// The renewal lead time for one token: [`REFRESH_BUFFER_SECONDS`], but never
/// more than half the token's own lifetime. Without the cap, a short-lived
/// token (say 30 s) would count as stale the moment it is issued and be
/// renewed on every single send.
fn renewal_lead_seconds(token: &Token) -> i64 {
    let lifetime = token.expires_at.saturating_sub(token.obtained_at).max(0);
    REFRESH_BUFFER_SECONDS.min(lifetime / 2)
}

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

fn cache_auth(key: &str, header: AttachmentHeader, token: &Token) {
    if let Ok(mut guard) = auth_cache().lock() {
        guard.insert(
            key.to_owned(),
            CachedAuth {
                header,
                valid_until: token.expires_at - renewal_lead_seconds(token),
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

    let token_store = token_store_at(base_dir);
    let Some(token) = (match token_store.get(&env_id, flow.as_str()) {
        Ok(t) => t,
        Err(e) => return AuthResolution::Ready(Err(e)),
    }) else {
        return AuthResolution::Ready(Ok(None));
    };

    match plan_for_token(&token, flow, &config, now) {
        TokenPlan::Attach => {
            let header = attachment_for(&config, &token);
            cache_auth(&key, header.clone(), &token);
            AuthResolution::Ready(Ok(Some(header)))
        }
        TokenPlan::Refresh { refresh_token } => {
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
            let scopes = token.scopes.clone();
            start_renewal(
                RenewalContext {
                    key,
                    base_dir: base_dir.to_owned(),
                    env_id,
                    flow,
                    config,
                },
                move || block_on_refresh(refresh_config, flow, &scopes),
            )
        }
        TokenPlan::ReacquireClientCredentials(request) => start_renewal(
            RenewalContext {
                key,
                base_dir: base_dir.to_owned(),
                env_id,
                flow,
                config,
            },
            move || block_on_client_credentials(&request),
        ),
        TokenPlan::NeedsUserAuthorization(reason) => {
            AuthResolution::Ready(Err(OAuthError::AuthDenied(reason)))
        }
    }
}

/// What the next send will do about OAuth for one environment. Read by the UI
/// so the request editor can state what will happen instead of guessing from a
/// raw token lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenOutlook {
    /// No OAuth config, no active flow, or injection switched off — nothing
    /// will be attached.
    NotInjecting,
    /// Injection is on but no token is stored for the active flow yet.
    Missing,
    /// A usable token, with its unix expiry.
    Valid { expires_at: i64 },
    /// Due for renewal, and renewal needs no user interaction — the next send
    /// renews first, then goes out.
    RenewsOnSend,
    /// Due for renewal and only the user can fix it.
    NeedsUserAuthorization,
}

pub fn token_outlook(env_name: &str) -> TokenOutlook {
    token_outlook_at(env_name, crate::oauth::DATA_DIR)
}

pub(crate) fn token_outlook_at(env_name: &str, base_dir: &str) -> TokenOutlook {
    let env_id = slugify_env_id(env_name);
    let Ok(storage) = FileStorage::new(base_dir) else {
        return TokenOutlook::NotInjecting;
    };
    let Ok(config) = storage.load_oauth_config(&env_id) else {
        return TokenOutlook::NotInjecting;
    };
    if !config.injection.enabled {
        return TokenOutlook::NotInjecting;
    }
    let Some(flow) = config.active_flow else {
        return TokenOutlook::NotInjecting;
    };

    let store = token_store_at(base_dir);
    let Ok(Some(token)) = store.get(&env_id, flow.as_str()) else {
        return TokenOutlook::Missing;
    };

    // Same decision the send path takes, so the label cannot disagree with
    // what actually happens.
    match plan_for_token(&token, flow, &config, now_unix()) {
        TokenPlan::Attach => TokenOutlook::Valid {
            expires_at: token.expires_at,
        },
        TokenPlan::Refresh { .. } | TokenPlan::ReacquireClientCredentials(_) => {
            TokenOutlook::RenewsOnSend
        }
        TokenPlan::NeedsUserAuthorization(_) => TokenOutlook::NeedsUserAuthorization,
    }
}

/// What to do with the token found in the store. Kept pure and separate from
/// the IO around it so every branch is unit-testable without a token endpoint,
/// a filesystem, or a background thread.
#[derive(Debug)]
enum TokenPlan {
    /// Good for now — attach as-is.
    Attach,
    /// Trade the refresh token for a new access token.
    Refresh { refresh_token: String },
    /// Replay the client-credentials grant. No refresh token exists, but this
    /// grant needs no human step, so renewal stays silent.
    ReacquireClientCredentials(Box<ClientCredentialsConfig>),
    /// Renewal is due and impossible without the user re-authorizing.
    NeedsUserAuthorization(String),
}

fn plan_for_token(token: &Token, flow: FlowKind, config: &OAuthConfig, now: i64) -> TokenPlan {
    if !token.expires_within(now, renewal_lead_seconds(token)) {
        return TokenPlan::Attach;
    }

    if let Some(refresh_token) = token.refresh_token.clone() {
        return TokenPlan::Refresh { refresh_token };
    }

    if flow == FlowKind::ClientCredentials
        && let Some(request) = reacquire::client_credentials_config(config)
    {
        return TokenPlan::ReacquireClientCredentials(Box::new(request));
    }

    if token.is_expired(now) {
        return TokenPlan::NeedsUserAuthorization(match flow {
            FlowKind::ClientCredentials => "client-credentials token expired and cannot be \
                 renewed: token URL, client id, or client secret is missing"
                .to_owned(),
            _ => "token expired and no refresh token available".to_owned(),
        });
    }

    // Renewal was due but nothing can be done about it, and the token has not
    // actually expired yet — use it rather than failing a request that would
    // have worked.
    TokenPlan::Attach
}

/// Everything a background renewal needs to store its result and publish it.
struct RenewalContext {
    key: String,
    base_dir: String,
    env_id: String,
    flow: FlowKind,
    config: OAuthConfig,
}

/// Run `acquire` on a background thread under single-flight, returning a
/// receiver the caller polls.
///
/// At most one renewal runs per `(base_dir, env_id)`: the first caller becomes
/// the leader and every later caller subscribes to the same result instead of
/// racing the token endpoint. Shared by the refresh-token and
/// client-credentials paths so both get identical concurrency behaviour.
fn start_renewal<F>(ctx: RenewalContext, acquire: F) -> AuthResolution
where
    F: FnOnce() -> Result<Token, OAuthError> + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<RefreshResult>();
    let became_leader = {
        let Ok(mut inflight) = inflight_refresh().lock() else {
            // Lock poisoning is unexpected but recoverable — report it rather
            // than panicking.
            return AuthResolution::Ready(Err(OAuthError::Internal(
                "inflight refresh lock poisoned".into(),
            )));
        };
        match inflight.get_mut(&ctx.key) {
            Some(subscribers) => {
                subscribers.push(tx);
                false
            }
            None => {
                inflight.insert(ctx.key.clone(), vec![tx]);
                true
            }
        }
    };

    if !became_leader {
        return AuthResolution::Refreshing(rx);
    }

    std::thread::spawn(move || {
        let result: RefreshResult = (|| {
            let renewed = acquire()?;
            let store = token_store_at(&ctx.base_dir);
            store.put(&ctx.env_id, ctx.flow.as_str(), &renewed)?;
            let header = attachment_for(&ctx.config, &renewed);
            cache_auth(&ctx.key, header.clone(), &renewed);
            Ok(Some(header))
        })();

        // Drain the subscriber list under the lock so a late arriver (between
        // the result completing and the slot being removed) becomes the next
        // leader rather than waiting on a closed sender.
        let subscribers = match inflight_refresh().lock() {
            Ok(mut guard) => guard.remove(&ctx.key).unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        for tx in subscribers {
            let _ = tx.send(clone_result_for_fanout(&result));
        }
    });

    AuthResolution::Refreshing(rx)
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

fn block_on_client_credentials(config: &ClientCredentialsConfig) -> Result<Token, OAuthError> {
    refresh_runtime()?.block_on(async { client_credentials::run(config).await })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests marked `#[cfg(not(feature = "keyring-storage"))]` seed a token
    // through `FileTokenStore` and then assert on what the middleware reads
    // back. Under `--features keyring-storage` the middleware reads the OS
    // keychain instead, so those assertions could not hold — and a test suite
    // must never read or write the developer's real keychain. The
    // store-independent tests below (planning, cache keys, lead times) run in
    // both builds.
    #[cfg(not(feature = "keyring-storage"))]
    use crate::oauth::FileTokenStore;
    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
    #[test]
    fn returns_none_when_no_token_stored() {
        let base = TempDir::new();
        configured_env(&base, FlowKind::ClientCredentials);
        let result = resolve_authorization_at("dev", base.to_str().unwrap())
            .into_ready()
            .unwrap();
        assert!(result.is_none());
    }

    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
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

    fn token_with(flow: FlowKind, obtained_ago: i64, expires_in: i64) -> Token {
        let now = now_unix();
        Token {
            flow,
            access_token: "atk".into(),
            refresh_token: None,
            expires_at: now + expires_in,
            obtained_at: now - obtained_ago,
            scopes: vec![],
        }
    }

    fn complete_client_credentials() -> OAuthConfig {
        OAuthConfig {
            active_flow: Some(FlowKind::ClientCredentials),
            client_credentials: crate::oauth::config::ClientCredentialsFields {
                token_url: "https://example.com/token".into(),
                client_id: "svc".into(),
                client_secret: "s3cret".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn renewal_lead_is_capped_at_half_a_short_lifetime() {
        // 1 h token: full 60 s lead.
        let hour = token_with(FlowKind::ClientCredentials, 0, 3600);
        assert_eq!(renewal_lead_seconds(&hour), 60);
        // 30 s token: 15 s, so it is not stale the instant it is issued.
        let brief = token_with(FlowKind::ClientCredentials, 0, 30);
        assert_eq!(renewal_lead_seconds(&brief), 15);
        // Degenerate lifetime cannot produce a negative lead.
        let zero = token_with(FlowKind::ClientCredentials, 0, 0);
        assert_eq!(renewal_lead_seconds(&zero), 0);
    }

    #[test]
    fn a_freshly_issued_short_lived_token_is_attached_not_renewed() {
        // Regression: with a flat 60 s lead this 30 s token was "expiring
        // soon" from birth and renewed on every send.
        let token = token_with(FlowKind::ClientCredentials, 0, 30);
        let plan = plan_for_token(
            &token,
            FlowKind::ClientCredentials,
            &complete_client_credentials(),
            now_unix(),
        );
        assert!(matches!(plan, TokenPlan::Attach), "got {plan:?}");
    }

    #[test]
    fn refresh_token_wins_over_reacquisition() {
        let mut token = token_with(FlowKind::ClientCredentials, 3600, -10);
        token.refresh_token = Some("rtk".into());
        let plan = plan_for_token(
            &token,
            FlowKind::ClientCredentials,
            &complete_client_credentials(),
            now_unix(),
        );
        match plan {
            TokenPlan::Refresh { refresh_token } => assert_eq!(refresh_token, "rtk"),
            other => panic!("expected Refresh, got {other:?}"),
        }
    }

    #[test]
    fn expired_client_credentials_token_is_reacquired_silently() {
        let token = token_with(FlowKind::ClientCredentials, 3600, -10);
        let plan = plan_for_token(
            &token,
            FlowKind::ClientCredentials,
            &complete_client_credentials(),
            now_unix(),
        );
        match plan {
            TokenPlan::ReacquireClientCredentials(request) => {
                assert_eq!(request.token_url, "https://example.com/token");
                assert_eq!(request.client_id, "svc");
                assert_eq!(request.client_secret, "s3cret");
            }
            other => panic!("expected ReacquireClientCredentials, got {other:?}"),
        }
    }

    #[test]
    fn client_credentials_token_nearing_expiry_is_reacquired_before_it_dies() {
        let token = token_with(FlowKind::ClientCredentials, 3600, 30);
        let plan = plan_for_token(
            &token,
            FlowKind::ClientCredentials,
            &complete_client_credentials(),
            now_unix(),
        );
        assert!(
            matches!(plan, TokenPlan::ReacquireClientCredentials(_)),
            "got {plan:?}"
        );
    }

    #[test]
    fn incomplete_client_credentials_config_still_needs_the_user() {
        let mut config = complete_client_credentials();
        config.client_credentials.client_secret = String::new();
        let token = token_with(FlowKind::ClientCredentials, 3600, -10);
        match plan_for_token(&token, FlowKind::ClientCredentials, &config, now_unix()) {
            TokenPlan::NeedsUserAuthorization(reason) => {
                assert!(reason.contains("client-credentials"), "reason: {reason}");
            }
            other => panic!("expected NeedsUserAuthorization, got {other:?}"),
        }
    }

    #[test]
    fn interactive_flows_are_never_reacquired_silently() {
        // Auth code and device code both need a human; an expired token with
        // no refresh token must surface an error rather than a token call.
        for flow in [FlowKind::AuthCodePkce, FlowKind::DeviceCode] {
            let token = token_with(flow, 3600, -10);
            let mut config = complete_client_credentials();
            config.active_flow = Some(flow);
            match plan_for_token(&token, flow, &config, now_unix()) {
                TokenPlan::NeedsUserAuthorization(reason) => {
                    assert_eq!(reason, "token expired and no refresh token available");
                }
                other => panic!("expected NeedsUserAuthorization for {flow:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn expiring_soon_without_any_renewal_path_is_still_attached() {
        // Nothing can be renewed, but the token has seconds left — use it
        // rather than failing a request that would have worked.
        let mut config = complete_client_credentials();
        config.active_flow = Some(FlowKind::AuthCodePkce);
        let token = token_with(FlowKind::AuthCodePkce, 3600, 30);
        let plan = plan_for_token(&token, FlowKind::AuthCodePkce, &config, now_unix());
        assert!(matches!(plan, TokenPlan::Attach), "got {plan:?}");
    }

    #[cfg(not(feature = "keyring-storage"))]
    #[test]
    fn outlook_reports_injection_off_and_missing_tokens() {
        let base = TempDir::new();
        let dir = base.to_str().unwrap();

        // No config at all.
        assert_eq!(token_outlook_at("dev", dir), TokenOutlook::NotInjecting);

        // Config present, no token yet.
        configured_env(&base, FlowKind::ClientCredentials);
        assert_eq!(token_outlook_at("dev", dir), TokenOutlook::Missing);

        // Injection explicitly switched off.
        let mut config = configured_env(&base, FlowKind::ClientCredentials);
        config.injection.enabled = false;
        FileStorage::new(&base)
            .unwrap()
            .save_oauth_config("dev", &config)
            .unwrap();
        assert_eq!(token_outlook_at("dev", dir), TokenOutlook::NotInjecting);
    }

    #[cfg(not(feature = "keyring-storage"))]
    #[test]
    fn outlook_reports_a_valid_token_with_its_expiry() {
        let base = TempDir::new();
        configured_env(&base, FlowKind::ClientCredentials);
        let token = valid_token(FlowKind::ClientCredentials);
        FileTokenStore::new(&base)
            .put("dev", "client_credentials", &token)
            .unwrap();

        assert_eq!(
            token_outlook_at("dev", base.to_str().unwrap()),
            TokenOutlook::Valid {
                expires_at: token.expires_at
            }
        );
    }

    #[cfg(not(feature = "keyring-storage"))]
    #[test]
    fn outlook_reports_silent_renewal_for_an_expired_client_credentials_token() {
        let base = TempDir::new();
        let mut config = configured_env(&base, FlowKind::ClientCredentials);
        config.client_credentials.client_secret = "s3cret".into();
        FileStorage::new(&base)
            .unwrap()
            .save_oauth_config("dev", &config)
            .unwrap();
        FileTokenStore::new(&base)
            .put(
                "dev",
                "client_credentials",
                &token_with(FlowKind::ClientCredentials, 3600, -10),
            )
            .unwrap();

        assert_eq!(
            token_outlook_at("dev", base.to_str().unwrap()),
            TokenOutlook::RenewsOnSend
        );
    }

    #[cfg(not(feature = "keyring-storage"))]
    #[test]
    fn outlook_reports_user_action_when_nothing_can_renew() {
        let base = TempDir::new();
        // configured_env leaves client_secret empty, so nothing can renew.
        configured_env(&base, FlowKind::ClientCredentials);
        FileTokenStore::new(&base)
            .put(
                "dev",
                "client_credentials",
                &token_with(FlowKind::ClientCredentials, 3600, -10),
            )
            .unwrap();

        assert_eq!(
            token_outlook_at("dev", base.to_str().unwrap()),
            TokenOutlook::NeedsUserAuthorization
        );
    }

    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
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

    #[cfg(not(feature = "keyring-storage"))]
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
