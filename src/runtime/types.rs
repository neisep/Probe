use std::collections::BTreeMap;
use std::fmt;

/// Small, explicit types for the async send flow.
/// Extended with light-weight metadata for UI integration.
pub type RequestId = u64;
pub type RequestHeaders = Vec<(String, String)>;
pub type ResolutionValues = BTreeMap<String, String>;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UnresolvedBehavior {
    Error,
    Preserve,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolutionErrorKind {
    MissingValue,
    InvalidPlaceholder,
    NonTextBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionError {
    pub kind: ResolutionErrorKind,
    pub target: String,
    pub placeholder: Option<String>,
    pub details: Option<String>,
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

impl AsyncRequest {
    #[allow(dead_code)]
    pub fn resolve(&self, values: &ResolutionValues) -> Result<Self, ResolutionError> {
        self.resolve_with_behavior(values, UnresolvedBehavior::Error)
    }

    #[allow(dead_code)]
    pub fn resolve_with_behavior(
        &self,
        values: &ResolutionValues,
        behavior: UnresolvedBehavior,
    ) -> Result<Self, ResolutionError> {
        Ok(AsyncRequest {
            url: resolve_text_with_behavior("url", &self.url, values, behavior)?,
            method: self.method.clone(),
            headers: resolve_headers(&self.headers, values, behavior)?,
            body: resolve_body_text(self.body.as_deref(), values, behavior)?,
        })
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

impl ResolutionError {
    fn missing(target: impl Into<String>, placeholder: impl Into<String>) -> Self {
        ResolutionError {
            kind: ResolutionErrorKind::MissingValue,
            target: target.into(),
            placeholder: Some(placeholder.into()),
            details: None,
        }
    }

    fn invalid(target: impl Into<String>, details: impl Into<String>) -> Self {
        ResolutionError {
            kind: ResolutionErrorKind::InvalidPlaceholder,
            target: target.into(),
            placeholder: None,
            details: Some(details.into()),
        }
    }

    fn non_text_body(details: impl Into<String>) -> Self {
        ResolutionError {
            kind: ResolutionErrorKind::NonTextBody,
            target: "body".to_string(),
            placeholder: None,
            details: Some(details.into()),
        }
    }

    pub fn to_error_info(&self) -> ErrorInfo {
        let kind = Some(match self.kind {
            ResolutionErrorKind::MissingValue => "resolve-missing".to_string(),
            ResolutionErrorKind::InvalidPlaceholder => "resolve-invalid".to_string(),
            ResolutionErrorKind::NonTextBody => "resolve-body".to_string(),
        });

        ErrorInfo::new(self.to_string(), None, self.details.clone(), kind)
    }
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ResolutionErrorKind::MissingValue => match &self.placeholder {
                Some(placeholder) => {
                    write!(
                        f,
                        "unresolved placeholder `{placeholder}` in {}",
                        self.target
                    )
                }
                None => write!(f, "unresolved placeholder in {}", self.target),
            },
            ResolutionErrorKind::InvalidPlaceholder => {
                write!(f, "invalid placeholder in {}", self.target)
            }
            ResolutionErrorKind::NonTextBody => {
                write!(f, "request body is not valid UTF-8 text")
            }
        }
    }
}

impl std::error::Error for ResolutionError {}

#[allow(dead_code)]
pub fn resolve_text(
    target: &str,
    input: &str,
    values: &ResolutionValues,
) -> Result<String, ResolutionError> {
    resolve_text_with_behavior(target, input, values, UnresolvedBehavior::Error)
}

pub fn resolve_text_with_behavior(
    target: &str,
    input: &str,
    values: &ResolutionValues,
    behavior: UnresolvedBehavior,
) -> Result<String, ResolutionError> {
    let mut resolved = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(start_offset) = input[cursor..].find("{{") {
        let start = cursor + start_offset;
        resolved.push_str(&input[cursor..start]);

        let content_start = start + 2;
        let Some(end_offset) = input[content_start..].find("}}") else {
            return Err(ResolutionError::invalid(
                target,
                "placeholder is missing closing `}}`",
            ));
        };
        let end = content_start + end_offset;
        let raw_name = &input[content_start..end];
        let name = raw_name.trim();

        if name.is_empty() {
            return Err(ResolutionError::invalid(
                target,
                "placeholder name is empty",
            ));
        }

        match values.get(name) {
            Some(value) => resolved.push_str(value),
            None => match behavior {
                UnresolvedBehavior::Error => {
                    return Err(ResolutionError::missing(target, name));
                }
                UnresolvedBehavior::Preserve => {
                    resolved.push_str("{{");
                    resolved.push_str(raw_name);
                    resolved.push_str("}}");
                }
            },
        }

        cursor = end + 2;
    }

    resolved.push_str(&input[cursor..]);
    Ok(resolved)
}

pub fn resolve_headers(
    headers: &RequestHeaders,
    values: &ResolutionValues,
    behavior: UnresolvedBehavior,
) -> Result<RequestHeaders, ResolutionError> {
    let mut resolved = Vec::with_capacity(headers.len());

    for (index, (name, value)) in headers.iter().enumerate() {
        let resolved_name =
            resolve_text_with_behavior(&format!("header[{index}].name"), name, values, behavior)?;
        let resolved_value =
            resolve_text_with_behavior(&format!("header[{index}].value"), value, values, behavior)?;
        resolved.push((resolved_name, resolved_value));
    }

    Ok(resolved)
}

pub fn resolve_body_text(
    body: Option<&[u8]>,
    values: &ResolutionValues,
    behavior: UnresolvedBehavior,
) -> Result<Option<Vec<u8>>, ResolutionError> {
    let Some(body) = body else {
        return Ok(None);
    };

    if !body.windows(2).any(|window| window == b"{{") {
        return Ok(Some(body.to_vec()));
    }

    let text =
        std::str::from_utf8(body).map_err(|e| ResolutionError::non_text_body(e.to_string()))?;
    let resolved = resolve_text_with_behavior("body", text, values, behavior)?;
    Ok(Some(resolved.into_bytes()))
}
