use crate::state::StateError;

#[derive(Debug, Clone)]
pub struct RequestDraft {
    pub name: String,
    pub folder: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl RequestDraft {
    pub fn default_request() -> Self {
        Self {
            name: String::new(),
            folder: String::new(),
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
            name: String::new(),
            folder: String::new(),
            method: method.to_uppercase(),
            url: url.to_owned(),
            headers: Vec::new(),
            body: None,
        })
    }

    pub fn duplicate(&self) -> Self {
        self.clone()
    }

    pub fn display_name(&self) -> String {
        if let Some(name) = self.request_name() {
            return name.to_owned();
        }

        let method = self.method.trim();
        let url = self.url.trim();

        match (method.is_empty(), url.is_empty()) {
            (true, true) => "Untitled request".to_owned(),
            (false, true) => method.to_owned(),
            (true, false) => url.to_owned(),
            (false, false) => format!("{method} {url}"),
        }
    }

    pub fn request_name(&self) -> Option<&str> {
        let name = self.name.trim();
        (!name.is_empty()).then_some(name)
    }

    pub fn folder_path(&self) -> Option<&str> {
        let folder = self.folder.trim();
        (!folder.is_empty()).then_some(folder)
    }

    pub fn set_request_name(&mut self, name: &str) {
        self.name = normalize_request_name(name).unwrap_or_default();
    }

    pub fn set_folder_path(&mut self, folder_path: &str) {
        self.folder = normalize_folder_path(folder_path);
    }

    pub fn set_organization(&mut self, name: &str, folder_path: &str) {
        self.name = normalize_request_name(name).unwrap_or_default();
        self.folder = normalize_folder_path(folder_path);
    }
}

impl Default for RequestDraft {
    fn default() -> Self {
        Self::default_request()
    }
}

pub fn normalize_request_name(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

pub fn normalize_folder_path(value: &str) -> String {
    value
        .split('/')
        .flat_map(|segment| segment.split('\\'))
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::RequestDraft;

    #[test]
    fn default_request_is_editable_and_valid() {
        let draft = RequestDraft::default_request();

        assert!(draft.name.is_empty());
        assert!(draft.folder.is_empty());
        assert_eq!(draft.method, "GET");
        assert_eq!(draft.url, "https://example.com");
        assert!(draft.headers.is_empty());
        assert!(draft.body.is_none());
    }

    #[test]
    fn duplicate_clones_all_fields() {
        let draft = RequestDraft {
            name: "Create item".to_owned(),
            folder: "Items".to_owned(),
            method: "POST".to_owned(),
            url: "https://example.com/items".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: Some("{\"ok\":true}".to_owned()),
        };

        let clone = draft.duplicate();

        assert_eq!(clone.name, draft.name);
        assert_eq!(clone.folder, draft.folder);
        assert_eq!(clone.method, draft.method);
        assert_eq!(clone.url, draft.url);
        assert_eq!(clone.headers, draft.headers);
        assert_eq!(clone.body, draft.body);
    }

    #[test]
    fn display_name_uses_method_and_url_when_name_is_blank() {
        let draft = match RequestDraft::new("post", "https://example.com/items") {
            Ok(draft) => draft,
            Err(error) => panic!("expected valid request draft, got {error}"),
        };
        assert_eq!(draft.display_name(), "POST https://example.com/items");
    }

    #[test]
    fn display_name_prefers_request_name_when_present() {
        let mut draft = RequestDraft::default_request();
        draft.set_request_name("List items");

        assert_eq!(draft.display_name(), "List items");
    }

    #[test]
    fn blank_folder_path_is_treated_as_empty() {
        let mut draft = RequestDraft::default_request();
        draft.set_folder_path("   ");

        assert_eq!(draft.folder_path(), None);
    }

    #[test]
    fn folder_path_normalization_collapses_separators_and_whitespace() {
        let mut draft = RequestDraft::default_request();
        draft.set_folder_path("  Collections / API// v1\\ Health  ");

        assert_eq!(draft.folder, "Collections/API/v1/Health");
        assert_eq!(draft.folder_path(), Some("Collections/API/v1/Health"));
    }
}
