use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResponseSummary {
    pub request_id: Option<String>,
    pub request_method: Option<String>,
    pub request_url: Option<String>,
    #[serde(default)]
    pub request_headers: Vec<(String, String)>,
    #[serde(default)]
    pub response_headers: Vec<(String, String)>,
    pub status: Option<u16>,
    pub timing_ms: Option<u128>,
    pub size_bytes: Option<usize>,
    pub content_type: Option<String>,
    pub header_count: Option<usize>,
    pub preview_text: Option<String>,
    pub error: Option<String>,
}

impl ResponseSummary {
    pub fn pending() -> Self {
        Self {
            error: Some("No response yet".to_owned()),
            ..Self::default()
        }
    }
}
