#[derive(Debug, Clone, Default)]
pub struct ResponseSummary {
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
