pub mod browser;
pub mod config;
pub mod flows;
pub mod middleware;
pub mod pkce;
pub mod store;

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub use config::OAuthConfig;
pub use store::{FileTokenStore, TokenStore};
#[cfg(feature = "keyring-storage")]
pub use store::KeyringTokenStore;

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

#[cfg(not(feature = "keyring-storage"))]
pub(crate) fn token_store() -> FileTokenStore {
    FileTokenStore::new(DATA_DIR)
}

#[cfg(feature = "keyring-storage")]
pub(crate) fn token_store() -> KeyringTokenStore {
    KeyringTokenStore
}

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
        }
    }
}

impl std::error::Error for OAuthError {}
