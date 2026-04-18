/// Small, explicit types for the async send flow.
/// Extended with light-weight metadata for UI integration.
pub type RequestId = u64;
pub type RequestHeaders = Vec<(String, String)>;

#[derive(Clone, Debug)]
pub struct AsyncRequest {
    pub url: String,
    /// "GET", "POST", etc. Keep simple for now.
    pub method: String,
    /// Request headers captured from the editor.
    pub headers: RequestHeaders,
    /// Optional body bytes for methods that support a payload.
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ResponseInfo {
    /// HTTP status code
    pub status: u16,
    /// Raw response body bytes
    pub body: Vec<u8>,
    /// Response headers captured as (name, value) pairs
    pub headers: RequestHeaders,
    /// Optional content-type/media hint extracted from headers
    pub content_type: Option<String>,
    /// Duration of the request round-trip in milliseconds
    pub duration_ms: u128,
}

#[derive(Clone, Debug)]
pub struct ErrorInfo {
    /// Human message describing the error
    pub message: String,
    /// Optional HTTP status code when applicable (e.g., non-2xx)
    pub code: Option<u16>,
    /// Optional details or source error string
    pub details: Option<String>,
    /// Optional machine-friendly kind ("timeout", "dns", "http", etc.)
    pub kind: Option<String>,
}

#[derive(Clone, Debug)]
pub enum AsyncRequestResult {
    Ok(ResponseInfo),
    Err(ErrorInfo),
}

#[derive(Clone, Debug)]
pub enum Event {
    StatusChanged {
        id: RequestId,
        status: RequestStatus,
    },
    Completed {
        id: RequestId,
        result: AsyncRequestResult,
    },
}

impl Default for RequestStatus {
    fn default() -> Self {
        RequestStatus::Pending
    }
}

impl Default for AsyncRequest {
    fn default() -> Self {
        AsyncRequest {
            url: String::new(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
        }
    }
}

impl ResponseInfo {
    /// Number of headers captured
    pub fn header_count(&self) -> usize {
        self.headers.len()
    }

    /// Case-insensitive header lookup. Returns the first match if present.
    pub fn header(&self, name: &str) -> Option<String> {
        let name_lc = name.to_ascii_lowercase();
        for (k, v) in &self.headers {
            if k.to_ascii_lowercase() == name_lc {
                return Some(v.clone());
            }
        }
        None
    }

    /// Media/content-type hint if present
    pub fn media_hint(&self) -> Option<String> {
        self.content_type.clone()
    }

    /// Heuristic whether the response should be treated as textual for previewing.
    pub fn is_textual(&self) -> bool {
        if let Some(ct) = &self.content_type {
            let ct_lc = ct.to_ascii_lowercase();
            return ct_lc.starts_with("text/")
                || ct_lc.contains("json")
                || ct_lc.contains("xml")
                || ct_lc.contains("html")
                || ct_lc.contains("javascript");
        }
        std::str::from_utf8(&self.body).is_ok()
    }

    /// Return a safe textual preview when available. Truncates to `max_chars` characters.
    pub fn text_preview(&self, max_chars: usize) -> Option<String> {
        if !self.is_textual() {
            return None;
        }
        match std::str::from_utf8(&self.body) {
            Ok(s) => {
                if s.chars().count() <= max_chars {
                    Some(s.to_string())
                } else {
                    Some(s.chars().take(max_chars).collect())
                }
            }
            Err(_) => None,
        }
    }
}

impl ErrorInfo {
    /// Create a new ErrorInfo with optional kind
    pub fn new(
        message: String,
        code: Option<u16>,
        details: Option<String>,
        kind: Option<String>,
    ) -> Self {
        ErrorInfo {
            message,
            code,
            details,
            kind,
        }
    }
}
