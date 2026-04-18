use crate::state::StateError;

#[derive(Debug, Clone)]
pub struct RequestDraft {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl RequestDraft {
    pub fn new(method: &str, url: &str) -> Result<Self, StateError> {
        let method = method.trim();
        let url = url.trim();

        if method.is_empty() {
            return Err(StateError::InvalidInput("method empty".into()));
        }
        if url.is_empty() {
            return Err(StateError::InvalidInput("url empty".into()));
        }

        Ok(Self {
            method: method.to_uppercase(),
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
        })
    }
}
