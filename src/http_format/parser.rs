use base64::Engine;

use crate::state::request::{RequestAuth, RequestDraft, normalize_request_name};

use super::HttpFormatError;

/// Parse the contents of a single `.http` file into a [`RequestDraft`].
///
/// Supports the REST Client / JetBrains HTTP Client subset: comments (`#`, `//`),
/// `# @name`, `# @probe-auth` directives, and the first request when
/// multiple requests are separated by `###`.
pub fn parse_request(text: &str) -> Result<RequestDraft, HttpFormatError> {
    if text.trim().is_empty() {
        return Err(HttpFormatError::Empty);
    }

    let mut name = String::new();
    let mut directive_auth: Option<RequestAuth> = None;
    let mut import_key: Option<String> = None;
    let mut method = String::new();
    let mut url = String::new();
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut body_lines: Vec<&str> = Vec::new();

    let mut state = ParseState::BeforeRequestLine;

    for line in text.lines() {
        match state {
            ParseState::BeforeRequestLine => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if is_separator(trimmed) {
                    continue;
                }
                if let Some(directive) = strip_comment_prefix(trimmed) {
                    apply_directive(directive, &mut name, &mut directive_auth, &mut import_key);
                    continue;
                }

                let (parsed_method, parsed_url) = parse_request_line(trimmed)?;
                method = parsed_method;
                url = parsed_url;
                state = ParseState::InHeaders;
            }
            ParseState::InHeaders => {
                if line.trim().is_empty() {
                    state = ParseState::InBody;
                    continue;
                }
                let trimmed = line.trim_start();
                if is_separator(trimmed) {
                    state = ParseState::AfterSeparator;
                    continue;
                }
                if let Some(directive) = strip_comment_prefix(trimmed) {
                    apply_directive(directive, &mut name, &mut directive_auth, &mut import_key);
                    continue;
                }

                let (header_name, header_value) = parse_header_line(trimmed)?;
                headers.push((header_name, header_value));
            }
            ParseState::InBody => {
                if is_separator(line.trim_start()) {
                    state = ParseState::AfterSeparator;
                    continue;
                }
                body_lines.push(line);
            }
            ParseState::AfterSeparator => break,
        }
    }

    if method.is_empty() || url.is_empty() {
        return Err(HttpFormatError::MissingRequestLine);
    }

    while body_lines.first().is_some_and(|line| line.trim().is_empty()) {
        body_lines.remove(0);
    }
    while body_lines.last().is_some_and(|line| line.trim().is_empty()) {
        body_lines.pop();
    }
    let body = if body_lines.is_empty() {
        None
    } else {
        Some(body_lines.join("\n"))
    };

    let (header_auth, remaining_headers) = extract_auth_from_headers(headers);
    let auth = directive_auth.unwrap_or(header_auth);

    let mut draft = RequestDraft {
        name: normalize_request_name(&name).unwrap_or_default(),
        folder: String::new(),
        method,
        url: String::new(),
        query_params: Vec::new(),
        auth,
        headers: remaining_headers,
        body,
        attach_oauth: true,
        import_key,
    };
    draft.adopt_url_query(&url);

    Ok(draft)
}

enum ParseState {
    BeforeRequestLine,
    InHeaders,
    InBody,
    AfterSeparator,
}

fn is_separator(line: &str) -> bool {
    line.starts_with("###")
}

fn strip_comment_prefix(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("//") {
        return Some(rest.trim_start());
    }
    if let Some(rest) = line.strip_prefix('#') {
        if rest.starts_with('#') {
            return None;
        }
        return Some(rest.trim_start());
    }
    None
}

fn apply_directive(
    directive: &str,
    name: &mut String,
    auth: &mut Option<RequestAuth>,
    import_key: &mut Option<String>,
) {
    if let Some(value) = directive.strip_prefix("@name ") {
        *name = value.trim().to_owned();
        return;
    }
    if let Some(value) = directive.strip_prefix("@probe-auth ") {
        if let Some(parsed) = parse_probe_auth_directive(value.trim()) {
            *auth = Some(parsed);
        }
        return;
    }
    if let Some(value) = directive.strip_prefix("@probe-import-key ") {
        let key = value.trim();
        if !key.is_empty() {
            *import_key = Some(key.to_owned());
        }
    }
}

fn parse_probe_auth_directive(value: &str) -> Option<RequestAuth> {
    let (kind, rest) = value.split_once(' ')?;
    match kind {
        "basic" => {
            let (username, password) = rest.split_once(':')?;
            Some(RequestAuth::Basic {
                username: username.to_owned(),
                password: password.to_owned(),
            })
        }
        _ => None,
    }
}

fn parse_request_line(line: &str) -> Result<(String, String), HttpFormatError> {
    let mut tokens = line.split_whitespace();
    let method = tokens
        .next()
        .ok_or_else(|| HttpFormatError::MalformedRequestLine(line.to_owned()))?;
    let url = tokens
        .next()
        .ok_or_else(|| HttpFormatError::MalformedRequestLine(line.to_owned()))?;
    Ok((method.to_uppercase(), url.to_owned()))
}

fn parse_header_line(line: &str) -> Result<(String, String), HttpFormatError> {
    let (name, value) = line
        .split_once(':')
        .ok_or_else(|| HttpFormatError::MalformedHeader(line.to_owned()))?;
    Ok((name.trim().to_owned(), value.trim().to_owned()))
}

fn extract_auth_from_headers(
    headers: Vec<(String, String)>,
) -> (RequestAuth, Vec<(String, String)>) {
    let mut auth = RequestAuth::None;
    let mut remaining = Vec::with_capacity(headers.len());
    let mut consumed = false;

    for (name, value) in headers {
        if !consumed && name.eq_ignore_ascii_case("authorization") {
            if let Some(rest) = strip_case_insensitive_prefix(&value, "Bearer ") {
                auth = RequestAuth::Bearer {
                    token: rest.trim().to_owned(),
                };
                consumed = true;
                continue;
            }
            if let Some(rest) = strip_case_insensitive_prefix(&value, "Basic ") {
                if let Some(decoded) = decode_basic(rest.trim()) {
                    auth = decoded;
                    consumed = true;
                    continue;
                }
            }
        }
        remaining.push((name, value));
    }

    (auth, remaining)
}

fn strip_case_insensitive_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    if value.len() < prefix.len() {
        return None;
    }
    let (head, rest) = value.split_at(prefix.len());
    head.eq_ignore_ascii_case(prefix).then_some(rest)
}

fn decode_basic(encoded: &str) -> Option<RequestAuth> {
    let decoded_bytes = base64::prelude::BASE64_STANDARD.decode(encoded).ok()?;
    let decoded = String::from_utf8(decoded_bytes).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some(RequestAuth::Basic {
        username: username.to_owned(),
        password: password.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::request::{ApiKeyLocation, RequestAuth};

    #[test]
    fn parses_minimal_get() {
        let text = "GET https://example.com/ping\n";
        let draft = parse_request(text).expect("parse");
        assert_eq!(draft.method, "GET");
        assert_eq!(draft.url, "https://example.com/ping");
        assert!(draft.headers.is_empty());
        assert!(draft.body.is_none());
        assert_eq!(draft.auth, RequestAuth::None);
    }

    #[test]
    fn parses_headers_and_body() {
        let text = "\
POST https://api.example.com/users
Content-Type: application/json
X-Trace: abc

{\"name\":\"jane\"}
";
        let draft = parse_request(text).expect("parse");
        assert_eq!(draft.method, "POST");
        assert_eq!(
            draft.headers,
            vec![
                ("Content-Type".to_owned(), "application/json".to_owned()),
                ("X-Trace".to_owned(), "abc".to_owned()),
            ]
        );
        assert_eq!(draft.body.as_deref(), Some("{\"name\":\"jane\"}"));
    }

    #[test]
    fn parses_name_directive() {
        let text = "\
# @name Fetch health
GET https://example.com/health
";
        let draft = parse_request(text).expect("parse");
        assert_eq!(draft.name, "Fetch health");
    }

    #[test]
    fn parses_query_params_into_draft() {
        let text = "GET https://example.com/items?page=1&size=20\n";
        let draft = parse_request(text).expect("parse");
        assert_eq!(draft.url, "https://example.com/items");
        assert_eq!(
            draft.query_params,
            vec![
                ("page".to_owned(), "1".to_owned()),
                ("size".to_owned(), "20".to_owned()),
            ]
        );
    }

    #[test]
    fn parses_bearer_auth_from_authorization_header() {
        let text = "\
GET https://example.com/me
Authorization: Bearer {{API_TOKEN}}
";
        let draft = parse_request(text).expect("parse");
        assert_eq!(
            draft.auth,
            RequestAuth::Bearer {
                token: "{{API_TOKEN}}".to_owned(),
            }
        );
        assert!(draft.headers.is_empty());
    }

    #[test]
    fn parses_basic_auth_from_authorization_header() {
        let text = "\
GET https://example.com/
Authorization: Basic am9objpzM2NyM3Q=
";
        let draft = parse_request(text).expect("parse");
        assert_eq!(
            draft.auth,
            RequestAuth::Basic {
                username: "john".to_owned(),
                password: "s3cr3t".to_owned(),
            }
        );
    }

    #[test]
    fn parses_basic_auth_from_probe_directive_preserving_placeholders() {
        let text = "\
# @probe-auth basic {{USER}}:{{PASS}}
GET https://example.com/
";
        let draft = parse_request(text).expect("parse");
        assert_eq!(
            draft.auth,
            RequestAuth::Basic {
                username: "{{USER}}".to_owned(),
                password: "{{PASS}}".to_owned(),
            }
        );
    }

    #[test]
    fn parses_stops_at_separator_and_ignores_further_requests() {
        let text = "\
GET https://example.com/first

###

GET https://example.com/second
";
        let draft = parse_request(text).expect("parse");
        assert_eq!(draft.url, "https://example.com/first");
    }

    #[test]
    fn missing_request_line_is_an_error() {
        let text = "# just a comment\n";
        assert!(matches!(
            parse_request(text),
            Err(HttpFormatError::MissingRequestLine)
        ));
    }

    #[test]
    fn empty_text_is_an_error() {
        assert!(matches!(parse_request(""), Err(HttpFormatError::Empty)));
    }

    #[test]
    fn non_authorization_api_key_header_stays_as_header() {
        let text = "\
GET https://example.com/
X-API-Key: s3cret
";
        let draft = parse_request(text).expect("parse");
        assert_eq!(draft.auth, RequestAuth::None);
        assert_eq!(
            draft.headers,
            vec![("X-API-Key".to_owned(), "s3cret".to_owned())]
        );
        let _ = ApiKeyLocation::Header;
    }

    #[test]
    fn import_key_directive_round_trips() {
        use crate::http_format::writer::write_request;
        let mut draft = RequestDraft::default_request();
        draft.import_key = Some("GET:/pets/{petId}".to_owned());
        let text = write_request(&draft);
        assert!(text.contains("# @probe-import-key GET:/pets/{petId}\n"), "missing directive in: {text}");
        let parsed = parse_request(&text).expect("parse");
        assert_eq!(parsed.import_key, Some("GET:/pets/{petId}".to_owned()));
    }

    #[test]
    fn missing_import_key_directive_leaves_field_none() {
        let text = "GET https://example.com/health\n";
        let draft = parse_request(text).expect("parse");
        assert_eq!(draft.import_key, None);
    }
}
