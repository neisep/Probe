use crate::state::StateError;

#[derive(Debug, Clone)]
pub struct RequestDraft {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl RequestDraft {
    pub fn default_request() -> Self {
        Self {
            method: "GET".to_owned(),
            url: "https://example.com".to_owned(),
            headers: Vec::new(),
            body: None,
        }
    }

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

    pub fn duplicate(&self) -> Self {
        self.clone()
    }

    pub fn display_name(&self) -> String {
        let method = self.method.trim();
        let url = self.url.trim();

        match (method.is_empty(), url.is_empty()) {
            (true, true) => "Untitled request".to_owned(),
            (false, true) => method.to_owned(),
            (true, false) => url.to_owned(),
            (false, false) => format!("{method} {url}"),
        }
    }
}

impl Default for RequestDraft {
    fn default() -> Self {
        Self::default_request()
    }
}

#[cfg(test)]
mod tests {
    use super::RequestDraft;

    #[test]
    fn default_request_is_editable_and_valid() {
        let draft = RequestDraft::default_request();

        assert_eq!(draft.method, "GET");
        assert_eq!(draft.url, "https://example.com");
        assert!(draft.headers.is_empty());
        assert!(draft.body.is_none());
    }

    #[test]
    fn duplicate_clones_all_fields() {
        let draft = RequestDraft {
            method: "POST".to_owned(),
            url: "https://example.com/items".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: Some("{\"ok\":true}".to_owned()),
        };

        let clone = draft.duplicate();

        assert_eq!(clone.method, draft.method);
        assert_eq!(clone.url, draft.url);
        assert_eq!(clone.headers, draft.headers);
        assert_eq!(clone.body, draft.body);
    }

    #[test]
    fn display_name_uses_method_and_url() {
        let draft = match RequestDraft::new("post", "https://example.com/items") {
            Ok(draft) => draft,
            Err(error) => panic!("expected valid request draft, got {error}"),
        };
        assert_eq!(draft.display_name(), "POST https://example.com/items");
    }
}
