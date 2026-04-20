use base64::Engine;

use crate::state::request::{ApiKeyLocation, RequestAuth, RequestDraft};

/// Render a [`RequestDraft`] to the `.http` file format used by the REST Client
/// and JetBrains HTTP Client.
pub fn write_request(draft: &RequestDraft) -> String {
    let mut out = String::new();

    let trimmed_name = draft.name.trim();
    if !trimmed_name.is_empty() {
        out.push_str("# @name ");
        out.push_str(trimmed_name);
        out.push('\n');
    }

    let mut directive_basic_auth: Option<(String, String)> = None;
    if let RequestAuth::Basic { username, password } = &draft.auth {
        if username.contains("{{") || password.contains("{{") {
            directive_basic_auth = Some((username.clone(), password.clone()));
        }
    }
    if let Some((username, password)) = &directive_basic_auth {
        out.push_str("# @probe-auth basic ");
        out.push_str(username);
        out.push(':');
        out.push_str(password);
        out.push('\n');
    }

    out.push_str(draft.method.trim());
    out.push(' ');
    out.push_str(&merged_url(&draft.url, &draft.query_params, &draft.auth));
    out.push('\n');

    match &draft.auth {
        RequestAuth::None => {}
        RequestAuth::Bearer { token } => {
            let token = token.trim();
            if !token.is_empty() {
                out.push_str("Authorization: Bearer ");
                out.push_str(token);
                out.push('\n');
            }
        }
        RequestAuth::Basic { username, password } => {
            if directive_basic_auth.is_none() {
                let encoded = base64::prelude::BASE64_STANDARD
                    .encode(format!("{username}:{password}").as_bytes());
                out.push_str("Authorization: Basic ");
                out.push_str(&encoded);
                out.push('\n');
            }
        }
        RequestAuth::ApiKey {
            location,
            name,
            value,
        } => {
            if matches!(location, ApiKeyLocation::Header) {
                let name = name.trim();
                if !name.is_empty() {
                    out.push_str(name);
                    out.push_str(": ");
                    out.push_str(value);
                    out.push('\n');
                }
            }
        }
    }

    for (name, value) in &draft.headers {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push('\n');
    }

    if let Some(body) = draft.body.as_deref() {
        if !body.is_empty() {
            out.push('\n');
            out.push_str(body);
            if !body.ends_with('\n') {
                out.push('\n');
            }
        }
    }

    out
}

fn merged_url(base: &str, query_params: &[(String, String)], auth: &RequestAuth) -> String {
    let mut pairs: Vec<(String, String)> = query_params.to_vec();
    if let RequestAuth::ApiKey {
        location: ApiKeyLocation::Query,
        name,
        value,
    } = auth
    {
        let trimmed_name = name.trim();
        if !trimmed_name.is_empty() {
            pairs.push((trimmed_name.to_owned(), value.clone()));
        }
    }

    if pairs.is_empty() {
        return base.to_owned();
    }

    let (head, fragment) = match base.split_once('#') {
        Some((head, fragment)) => (head.to_owned(), Some(fragment)),
        None => (base.to_owned(), None),
    };

    let separator = if head.contains('?') { '&' } else { '?' };
    let encoded: Vec<String> = pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    let mut result = format!("{head}{separator}{}", encoded.join("&"));
    if let Some(fragment) = fragment {
        result.push('#');
        result.push_str(fragment);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse_request;
    use super::*;
    use crate::state::request::{ApiKeyLocation, RequestAuth};

    #[test]
    fn writes_minimal_get() {
        let draft = RequestDraft {
            name: String::new(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://example.com/ping".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::None,
            headers: Vec::new(),
            body: None,
        };
        assert_eq!(write_request(&draft), "GET https://example.com/ping\n");
    }

    #[test]
    fn writes_name_directive_before_request_line() {
        let mut draft = RequestDraft::default_request();
        draft.name = "Fetch health".to_owned();
        draft.method = "GET".to_owned();
        draft.url = "https://example.com/health".to_owned();
        let text = write_request(&draft);
        assert!(text.starts_with("# @name Fetch health\nGET https://example.com/health\n"));
    }

    #[test]
    fn writes_bearer_token_as_authorization_header() {
        let draft = RequestDraft {
            name: String::new(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://example.com/me".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::Bearer {
                token: "{{API_TOKEN}}".to_owned(),
            },
            headers: Vec::new(),
            body: None,
        };
        let text = write_request(&draft);
        assert!(text.contains("Authorization: Bearer {{API_TOKEN}}\n"));
    }

    #[test]
    fn writes_basic_auth_as_base64_when_no_placeholders() {
        let draft = RequestDraft {
            name: String::new(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://example.com/".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::Basic {
                username: "john".to_owned(),
                password: "s3cr3t".to_owned(),
            },
            headers: Vec::new(),
            body: None,
        };
        let text = write_request(&draft);
        assert!(text.contains("Authorization: Basic am9objpzM2NyM3Q=\n"));
        assert!(!text.contains("# @probe-auth"));
    }

    #[test]
    fn writes_basic_auth_as_directive_when_placeholders_present() {
        let draft = RequestDraft {
            name: String::new(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://example.com/".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::Basic {
                username: "{{USER}}".to_owned(),
                password: "{{PASS}}".to_owned(),
            },
            headers: Vec::new(),
            body: None,
        };
        let text = write_request(&draft);
        assert!(text.contains("# @probe-auth basic {{USER}}:{{PASS}}\n"));
        assert!(!text.contains("Authorization: Basic"));
    }

    #[test]
    fn writes_api_key_header_as_custom_header() {
        let draft = RequestDraft {
            name: String::new(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://example.com/".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::ApiKey {
                location: ApiKeyLocation::Header,
                name: "X-API-Key".to_owned(),
                value: "s3cret".to_owned(),
            },
            headers: Vec::new(),
            body: None,
        };
        let text = write_request(&draft);
        assert!(text.contains("X-API-Key: s3cret\n"));
    }

    #[test]
    fn writes_api_key_query_into_url() {
        let draft = RequestDraft {
            name: String::new(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://example.com/items".to_owned(),
            query_params: vec![("page".to_owned(), "1".to_owned())],
            auth: RequestAuth::ApiKey {
                location: ApiKeyLocation::Query,
                name: "api_key".to_owned(),
                value: "s3cret".to_owned(),
            },
            headers: Vec::new(),
            body: None,
        };
        let text = write_request(&draft);
        assert!(
            text.contains("GET https://example.com/items?page=1&api_key=s3cret\n"),
            "unexpected output: {text}"
        );
    }

    #[test]
    fn round_trip_preserves_fields() {
        let draft = RequestDraft {
            name: "Create user".to_owned(),
            folder: String::new(),
            method: "POST".to_owned(),
            url: "https://api.example.com/users".to_owned(),
            query_params: vec![("dry_run".to_owned(), "true".to_owned())],
            auth: RequestAuth::Bearer {
                token: "{{API_TOKEN}}".to_owned(),
            },
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: Some("{\"name\":\"jane\"}".to_owned()),
        };

        let text = write_request(&draft);
        let parsed = parse_request(&text).expect("parse back");

        assert_eq!(parsed.name, draft.name);
        assert_eq!(parsed.method, draft.method);
        assert_eq!(parsed.url, draft.url);
        assert_eq!(parsed.query_params, draft.query_params);
        assert_eq!(parsed.auth, draft.auth);
        assert_eq!(parsed.headers, draft.headers);
        assert_eq!(parsed.body, draft.body);
    }

    #[test]
    fn round_trip_preserves_basic_auth_with_placeholders() {
        let draft = RequestDraft {
            name: String::new(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://example.com/".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::Basic {
                username: "{{USER}}".to_owned(),
                password: "{{PASS}}".to_owned(),
            },
            headers: Vec::new(),
            body: None,
        };
        let text = write_request(&draft);
        let parsed = parse_request(&text).expect("parse back");
        assert_eq!(parsed.auth, draft.auth);
    }
}
