pub mod browser;
pub mod config;
pub mod flows;
pub mod jwt_exp;
pub mod middleware;
pub mod pkce;
pub mod reacquire;
pub mod store;

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub use config::OAuthConfig;
#[cfg(feature = "keyring-storage")]
pub use store::KeyringTokenStore;
pub use store::TokenStore;
// Named directly only by the file-store build and its tests; under
// `keyring-storage` the keychain store takes its place.
#[cfg_attr(feature = "keyring-storage", allow(unused_imports))]
pub use store::FileTokenStore;

use crate::persistence::FileStorage;

pub(crate) const DATA_DIR: &str = "./data";

static STORAGE: OnceLock<Option<FileStorage>> = OnceLock::new();

pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn storage() -> Option<&'static FileStorage> {
    STORAGE
        .get_or_init(|| FileStorage::new(DATA_DIR).ok())
        .as_ref()
}

/// The token store this build uses, rooted at the default data directory.
///
/// Everything that reads or writes OAuth tokens must go through this (or
/// [`token_store_at`]) rather than naming a concrete store. Picking
/// `FileTokenStore` directly is how the panel and the send path ended up
/// disagreeing under `--features keyring-storage`: tokens were written to the
/// OS keychain and then looked for on disk, so requests went out unauthorized.
pub(crate) fn token_store() -> TokenStoreImpl {
    token_store_at(DATA_DIR)
}

/// Same choice as [`token_store`], but rooted at `base_dir`.
///
/// `base_dir` only means something for the file store; the keychain is a
/// process-wide OS facility with no directory of its own, so the argument is
/// ignored there. The middleware is parameterised by base dir for its tests,
/// which is why this variant exists.
#[cfg(not(feature = "keyring-storage"))]
pub(crate) fn token_store_at(base_dir: &str) -> TokenStoreImpl {
    FileTokenStore::new(base_dir)
}

#[cfg(feature = "keyring-storage")]
pub(crate) fn token_store_at(_base_dir: &str) -> TokenStoreImpl {
    KeyringTokenStore
}

/// The concrete store `token_store`/`token_store_at` hand back, so call sites
/// can name the type without caring which build they are in.
#[cfg(not(feature = "keyring-storage"))]
pub(crate) type TokenStoreImpl = FileTokenStore;

#[cfg(feature = "keyring-storage")]
pub(crate) type TokenStoreImpl = KeyringTokenStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowKind {
    AuthCodePkce,
    ClientCredentials,
    DeviceCode,
}

impl FlowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FlowKind::AuthCodePkce => "auth_code_pkce",
            FlowKind::ClientCredentials => "client_credentials",
            FlowKind::DeviceCode => "device_code",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub flow: FlowKind,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    pub expires_at: i64,
    pub obtained_at: i64,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl Token {
    pub fn is_expired(&self, now_unix: i64) -> bool {
        now_unix >= self.expires_at
    }

    pub fn expires_within(&self, now_unix: i64, seconds: i64) -> bool {
        now_unix.saturating_add(seconds) >= self.expires_at
    }
}

#[derive(Debug)]
pub enum OAuthError {
    Io(std::io::Error),
    Serde(serde_json::Error),
    InvalidKey(String),
    Http(String),
    Browser(String),
    StateMismatch,
    AuthDenied(String),
    Parse(String),
    Config(String),
    /// System-level failures that aren't the user's fault and aren't an
    /// HTTP/protocol issue — e.g. the OAuth refresh tokio runtime failed
    /// to build, or a result was fanned out to a follower in single-flight
    /// refresh and we only have the error's message text.
    Internal(String),
}

impl From<std::io::Error> for OAuthError {
    fn from(error: std::io::Error) -> Self {
        OAuthError::Io(error)
    }
}

impl From<serde_json::Error> for OAuthError {
    fn from(error: serde_json::Error) -> Self {
        OAuthError::Serde(error)
    }
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OAuthError::Io(error) => write!(f, "io error: {error}"),
            OAuthError::Serde(error) => write!(f, "json error: {error}"),
            OAuthError::InvalidKey(key) => write!(f, "invalid key: {key}"),
            OAuthError::Http(details) => write!(f, "http error: {details}"),
            OAuthError::Browser(details) => write!(f, "browser error: {details}"),
            OAuthError::StateMismatch => write!(f, "state parameter mismatch"),
            OAuthError::AuthDenied(details) => write!(f, "authorization denied: {details}"),
            OAuthError::Parse(details) => write!(f, "parse error: {details}"),
            OAuthError::Config(details) => write!(f, "config error: {details}"),
            OAuthError::Internal(details) => write!(f, "internal error: {details}"),
        }
    }
}

impl std::error::Error for OAuthError {}
