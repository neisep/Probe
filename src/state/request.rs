use crate::state::StateError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RequestAuthKind {
    #[default]
    None,
    Bearer,
    Basic,
    ApiKey,
}

impl RequestAuthKind {
    pub const ALL: [Self; 4] = [Self::None, Self::Bearer, Self::Basic, Self::ApiKey];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Bearer => "Bearer token",
            Self::Basic => "Basic auth",
            Self::ApiKey => "API key",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

impl ApiKeyLocation {
    pub const ALL: [Self; 2] = [Self::Header, Self::Query];

    pub fn label(self) -> &'static str {
        match self {
            Self::Header => "Header",
            Self::Query => "Query param",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RequestAuth {
    #[default]
    None,
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    ApiKey {
        location: ApiKeyLocation,
        name: String,
        value: String,
    },
}

impl RequestAuth {
    pub fn kind(&self) -> RequestAuthKind {
        match self {
            Self::None => RequestAuthKind::None,
            Self::Bearer { .. } => RequestAuthKind::Bearer,
            Self::Basic { .. } => RequestAuthKind::Basic,
            Self::ApiKey { .. } => RequestAuthKind::ApiKey,
        }
    }

    pub fn from_kind(kind: RequestAuthKind) -> Self {
        match kind {
            RequestAuthKind::None => Self::None,
            RequestAuthKind::Bearer => Self::Bearer {
                token: String::new(),
            },
            RequestAuthKind::Basic => Self::Basic {
                username: String::new(),
                password: String::new(),
            },
            RequestAuthKind::ApiKey => Self::ApiKey {
                location: ApiKeyLocation::Header,
                name: String::new(),
                value: String::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestDraft {
    pub name: String,
    pub folder: String,
    pub method: String,
    pub url: String,
    pub query_params: Vec<(String, String)>,
    pub auth: RequestAuth,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    #[serde(default = "default_true")]
    pub attach_oauth: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_key: Option<String>,
}

fn default_true() -> bool {
    true
}

impl RequestDraft {
    pub fn default_request() -> Self {
        Self {
            name: String::new(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://example.com".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::None,
            headers: Vec::new(),
            body: None,
            attach_oauth: true,
            import_key: None,
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
            query_params: Vec::new(),
            auth: RequestAuth::None,
            headers: Vec::new(),
            body: None,
            attach_oauth: true,
            import_key: None,
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

    pub fn set_url(&mut self, url: &str) {
        self.url = url.trim().to_owned();
    }

    pub fn adopt_url_query(&mut self, url: &str) {
        let (base_url, query_params) = split_url_query(url);
        self.url = base_url;
        self.query_params = query_params;
    }

    #[allow(dead_code)]
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

fn split_url_query(url: &str) -> (String, Vec<(String, String)>) {
    let normalized_url = url.trim();
    let (before_fragment, fragment) = match normalized_url.split_once('#') {
        Some((before_fragment, fragment)) => (before_fragment, Some(fragment)),
        None => (normalized_url, None),
    };
    let Some((base_url, query)) = before_fragment.split_once('?') else {
        return (normalized_url.to_owned(), Vec::new());
    };

    let base_url = match fragment {
        Some(fragment) if !fragment.is_empty() => format!("{base_url}#{fragment}"),
        Some(_fragment) => base_url.to_owned(),
        None => base_url.to_owned(),
    };

    let query_params = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (pair.to_owned(), String::new()),
        })
        .collect();

    (base_url, query_params)
}

#[cfg(test)]
mod tests {
    use super::{ApiKeyLocation, RequestAuth, RequestAuthKind, RequestDraft};

    #[test]
    fn default_request_is_editable_and_valid() {
        let draft = RequestDraft::default_request();

        assert!(draft.name.is_empty());
        assert!(draft.folder.is_empty());
        assert_eq!(draft.method, "GET");
        assert_eq!(draft.url, "https://example.com");
        assert!(draft.query_params.is_empty());
        assert_eq!(draft.auth, RequestAuth::None);
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
            query_params: vec![("page".to_owned(), "1".to_owned())],
            auth: RequestAuth::Bearer {
                token: "{{TOKEN}}".to_owned(),
            },
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: Some("{\"ok\":true}".to_owned()),
            attach_oauth: true,
            import_key: None,
        };

        let clone = draft.duplicate();

        assert_eq!(clone.name, draft.name);
        assert_eq!(clone.folder, draft.folder);
        assert_eq!(clone.method, draft.method);
        assert_eq!(clone.url, draft.url);
        assert_eq!(clone.query_params, draft.query_params);
        assert_eq!(clone.auth, draft.auth);
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

    #[test]
    fn adopt_url_query_extracts_query_rows_and_preserves_fragment() {
        let mut draft = RequestDraft::default_request();
        draft.adopt_url_query(" https://example.com/items?limit=10&offset=20#details ");

        assert_eq!(draft.url, "https://example.com/items#details");
        assert_eq!(
            draft.query_params,
            vec![
                ("limit".to_owned(), "10".to_owned()),
                ("offset".to_owned(), "20".to_owned()),
            ]
        );
    }

    #[test]
    fn auth_kind_round_trip_builds_expected_variants() {
        assert_eq!(
            RequestAuth::from_kind(RequestAuthKind::None),
            RequestAuth::None
        );
        assert_eq!(
            RequestAuth::from_kind(RequestAuthKind::Bearer).kind(),
            RequestAuthKind::Bearer
        );
        assert_eq!(
            RequestAuth::from_kind(RequestAuthKind::Basic).kind(),
            RequestAuthKind::Basic
        );
        assert_eq!(
            RequestAuth::from_kind(RequestAuthKind::ApiKey),
            RequestAuth::ApiKey {
                location: ApiKeyLocation::Header,
                name: String::new(),
                value: String::new(),
            }
        );
    }
}
