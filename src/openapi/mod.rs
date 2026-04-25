pub mod merge;
pub mod parser;
pub mod source;

pub use merge::{MergePreview, compute_merge};
pub use parser::parse_spec;

use crate::state::request::RequestAuth;

#[derive(Debug)]
pub enum OpenApiError {
    Json(serde_json::Error),
    Yaml(serde_yaml::Error),
    UnsupportedVersion(String),
    Http(String),
}

impl std::fmt::Display for OpenApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenApiError::Json(e) => write!(f, "JSON parse error: {e}"),
            OpenApiError::Yaml(e) => write!(f, "YAML parse error: {e}"),
            OpenApiError::UnsupportedVersion(s) => write!(f, "unsupported spec version: {s}"),
            OpenApiError::Http(s) => write!(f, "fetch error: {s}"),
        }
    }
}

impl std::error::Error for OpenApiError {}

/// A single endpoint extracted from an OpenAPI spec, ready to be merged into
/// the collection. Does not leak openapiv3 types to the rest of the app.
#[derive(Debug, Clone)]
pub struct ImportedOperation {
    /// Stable merge key: `"METHOD:/path/template"` — e.g. `"GET:/pets/{petId}"`.
    pub import_key: String,
    pub name: String,
    pub folder: String,
    pub method: String,
    /// Fully resolved URL including base (e.g. `"https://api.example.com/pets/{petId}"`).
    pub url: String,
    /// Spec-defined query parameters (names only, values left blank).
    pub query_params: Vec<(String, String)>,
    /// Auth type hint derived from the spec's security schemes — no credentials set.
    pub auth_hint: Option<RequestAuth>,
    /// Best-effort example body from requestBody, if present.
    pub body_example: Option<String>,
}
