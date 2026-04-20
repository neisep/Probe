use serde::{Deserialize, Serialize};

/// Summary of a response for UI listing and quick restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSummary {
    pub id: String,
    pub request_id: Option<String>,
    pub status_code: Option<u16>,
    pub summary: Option<String>,
    pub duration_ms: Option<u64>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
}

impl From<(String, String)> for HeaderEntry {
    fn from((name, value): (String, String)) -> Self {
        Self { name, value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePreview {
    pub id: String,
    pub response_id: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub request_method: Option<String>,
    #[serde(default)]
    pub request_url: Option<String>,
    #[serde(default)]
    pub content_preview: Option<String>,
    #[serde(default)]
    pub content_body: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub header_count: Option<usize>,
    #[serde(default)]
    pub size_bytes: Option<usize>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsePreviewDetail {
    #[serde(default)]
    pub request_headers: Vec<HeaderEntry>,
    #[serde(default)]
    pub response_headers: Vec<HeaderEntry>,
}

/// Session-level UI state restored between runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub selected_request: Option<String>,
    #[serde(default)]
    pub selected_response: Option<String>,
    #[serde(default)]
    pub active_environment: Option<String>,
    #[serde(default)]
    pub active_view: Option<String>,
    #[serde(default)]
    pub open_panels: Vec<String>,
    pub updated_at: Option<String>,
}
