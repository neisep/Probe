//! Map a tokenized cURL command onto a [`RequestDraft`].
//!
//! Recognizes the flags that appear in real copy-pasted commands (browser
//! dev-tools, API docs, terminals) and folds them onto the request model. It
//! is best-effort by design: unknown flags are skipped, and constructs the
//! model can't represent (multipart `-F`, `@file` bodies) are preserved as
//! literal text rather than dropped. Anything genuinely malformed surfaces as
//! a [`CurlParseError`] for the UI to display.

use base64::Engine;

use super::CurlParseError;
use super::tokenizer::tokenize;
use crate::state::request::{RequestAuth, RequestDraft};

/// Parse a full `curl …` command into a request draft. Never panics; a
/// malformed command returns an error the caller surfaces to the user.
pub fn parse_curl(input: &str) -> Result<RequestDraft, CurlParseError> {
    let tokens = tokenize(input)?;
    let mut iter = tokens.into_iter().peekable();

    match iter.next() {
        None => return Err(CurlParseError::Empty),
        Some(first) if first.eq_ignore_ascii_case("curl") => {}
        Some(_) => return Err(CurlParseError::NotCurl),
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut body_parts: Vec<String> = Vec::new();
    let mut user_pass: Option<String> = None;
    let mut is_json = false;

    while let Some(token) = iter.next() {
        if let Some(long) = token.strip_prefix("--") {
            // `--opt=value` carries its value inline; otherwise it's the next token.
            let (name, attached) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (long, None),
            };
            let mut value = || attached.clone().or_else(|| iter.next());
            match name {
                "request" => {
                    if let Some(v) = value() {
                        method = Some(v.to_uppercase());
                    }
                }
                "url" => {
                    if let Some(v) = value()
                        && url.is_none()
                    {
                        url = Some(v);
                    }
                }
                "header" => {
                    if let Some(v) = value() {
                        push_header(&mut headers, &v);
                    }
                }
                "user" => {
                    if let Some(v) = value() {
                        user_pass = Some(v);
                    }
                }
                "oauth2-bearer" => {
                    if let Some(v) = value() {
                        headers.push(("Authorization".to_owned(), format!("Bearer {v}")));
                    }
                }
                "data" | "data-raw" | "data-ascii" | "data-binary" | "data-urlencode" => {
                    if let Some(v) = value() {
                        body_parts.push(v);
                    }
                }
                "json" => {
                    if let Some(v) = value() {
                        body_parts.push(v);
                        is_json = true;
                    }
                }
                "form" | "form-string" => {
                    // Multipart can't be modelled faithfully (body is raw text);
                    // keep the field literally so nothing is silently lost.
                    if let Some(v) = value() {
                        body_parts.push(v);
                    }
                }
                "user-agent" => {
                    if let Some(v) = value() {
                        headers.push(("User-Agent".to_owned(), v));
                    }
                }
                "referer" => {
                    if let Some(v) = value() {
                        headers.push(("Referer".to_owned(), v));
                    }
                }
                "cookie" => {
                    if let Some(v) = value() {
                        headers.push(("Cookie".to_owned(), v));
                    }
                }
                "head" => method = Some("HEAD".to_owned()),
                // Known value-taking flags we don't model: consume the value so
                // it isn't mistaken for the URL.
                "connect-timeout" | "max-time" | "retry" | "retry-delay" | "proxy"
                | "proxy-user" | "cacert" | "cert" | "cert-type" | "key" | "resolve" | "output"
                | "cookie-jar" | "interface" | "limit-rate" | "max-redirs" => {
                    let _ = value();
                }
                // Everything else (valueless flags like --compressed/--location,
                // or unknown flags): ignore without consuming a token.
                _ => {}
            }
            continue;
        }

        if token.len() > 1 && token.starts_with('-') {
            parse_short_flags(
                &token[1..],
                &mut iter,
                &mut method,
                &mut headers,
                &mut body_parts,
                &mut user_pass,
            );
            continue;
        }

        // A bare argument is the URL; the first one wins.
        if url.is_none() {
            url = Some(token);
        }
    }

    let url = url.ok_or(CurlParseError::MissingUrl)?;

    if is_json {
        ensure_header(&mut headers, "Content-Type", "application/json");
        ensure_header(&mut headers, "Accept", "application/json");
    }

    // `-u` takes precedence over any Authorization header; otherwise fall back
    // to extracting Bearer/Basic from the headers (mirrors the .http importer).
    let (header_auth, remaining_headers) = extract_auth_from_headers(headers);
    let auth = match user_pass {
        Some(user_pass) => basic_from_user(&user_pass),
        None => header_auth,
    };

    let has_body = !body_parts.is_empty();
    let method = method.unwrap_or_else(|| if has_body { "POST" } else { "GET" }.to_owned());
    let body = has_body.then(|| body_parts.join("&"));

    let mut draft = RequestDraft {
        name: String::new(),
        folder: String::new(),
        method,
        url: String::new(),
        query_params: Vec::new(),
        auth,
        headers: remaining_headers,
        body,
        attach_oauth: true,
        import_key: None,
    };
    draft.adopt_url_query(&url);
    Ok(draft)
}

/// Cheap heuristic for the URL bar: does this text look like a curl command?
/// True when the first whitespace-delimited token is `curl` and at least one
/// argument follows — enough to route the input to [`parse_curl`] without
/// tripping on a user typing a plain URL.
pub fn looks_like_curl(text: &str) -> bool {
    let mut tokens = text.split_whitespace();
    matches!(tokens.next(), Some(first) if first.eq_ignore_ascii_case("curl"))
        && tokens.next().is_some()
}

/// Parse one bundled short-flag token (the part after the leading `-`), e.g.
/// `sSL`, `X`, or `d{"a":1}`. Value-taking flags consume the rest of the token
/// or the next argument.
fn parse_short_flags<I: Iterator<Item = String>>(
    flags: &str,
    iter: &mut std::iter::Peekable<I>,
    method: &mut Option<String>,
    headers: &mut Vec<(String, String)>,
    body_parts: &mut Vec<String>,
    user_pass: &mut Option<String>,
) {
    let chars: Vec<char> = flags.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let flag = chars[i];
        let takes_value = matches!(flag, 'X' | 'H' | 'd' | 'u' | 'A' | 'e' | 'b' | 'F');
        if takes_value {
            let rest: String = chars[i + 1..].iter().collect();
            let value = if rest.is_empty() {
                iter.next().unwrap_or_default()
            } else {
                rest
            };
            match flag {
                'X' => *method = Some(value.to_uppercase()),
                'H' => push_header(headers, &value),
                'd' | 'F' => body_parts.push(value),
                'u' => *user_pass = Some(value),
                'A' => headers.push(("User-Agent".to_owned(), value)),
                'e' => headers.push(("Referer".to_owned(), value)),
                'b' => headers.push(("Cookie".to_owned(), value)),
                _ => {}
            }
            return; // the value consumed the remainder of the token
        }
        if flag == 'I' {
            *method = Some("HEAD".to_owned());
        }
        // Other valueless flags (-s, -S, -L, -k, -i, -v, -G, -f, …) are ignored.
        i += 1;
    }
}

/// Push a `Name: value` header, tolerating curl's `Name;` form (send an empty
/// header). Malformed entries are dropped.
fn push_header(headers: &mut Vec<(String, String)>, raw: &str) {
    if let Some((name, value)) = raw.split_once(':') {
        let name = name.trim();
        if !name.is_empty() {
            headers.push((name.to_owned(), value.trim().to_owned()));
        }
    } else if let Some(name) = raw.strip_suffix(';') {
        let name = name.trim();
        if !name.is_empty() {
            headers.push((name.to_owned(), String::new()));
        }
    }
}

/// Add a header only if one with that name isn't already present (case-insensitive).
fn ensure_header(headers: &mut Vec<(String, String)>, name: &str, value: &str) {
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name)) {
        headers.push((name.to_owned(), value.to_owned()));
    }
}

fn basic_from_user(user_pass: &str) -> RequestAuth {
    let (username, password) = user_pass.split_once(':').unwrap_or((user_pass, ""));
    RequestAuth::Basic {
        username: username.to_owned(),
        password: password.to_owned(),
    }
}

/// Consume a single `Authorization` header into structured auth, leaving all
/// other headers intact. Mirrors the `.http` importer's behavior.
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
            if let Some(rest) = strip_case_insensitive_prefix(&value, "Basic ")
                && let Some(decoded) = decode_basic(rest.trim())
            {
                auth = decoded;
                consumed = true;
                continue;
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
    use crate::state::request::RequestAuth;

    #[test]
    fn parses_minimal_get() {
        let draft = parse_curl("curl https://example.com/ping").unwrap();
        assert_eq!(draft.method, "GET");
        assert_eq!(draft.url, "https://example.com/ping");
        assert!(draft.headers.is_empty());
        assert!(draft.body.is_none());
        assert_eq!(draft.auth, RequestAuth::None);
    }

    #[test]
    fn defaults_to_post_when_body_present() {
        let draft = parse_curl("curl https://example.com -d 'a=1'").unwrap();
        assert_eq!(draft.method, "POST");
        assert_eq!(draft.body.as_deref(), Some("a=1"));
    }

    #[test]
    fn explicit_method_wins_over_body_default() {
        let draft = parse_curl("curl -X PUT https://example.com -d 'a=1'").unwrap();
        assert_eq!(draft.method, "PUT");
    }

    #[test]
    fn multiple_data_flags_join_with_ampersand() {
        let draft = parse_curl("curl https://x.test -d a=1 -d b=2").unwrap();
        assert_eq!(draft.body.as_deref(), Some("a=1&b=2"));
    }

    #[test]
    fn collects_multiple_headers() {
        let draft = parse_curl("curl https://x.test -H 'X-A: 1' -H \"X-B: 2\"").unwrap();
        assert_eq!(
            draft.headers,
            vec![
                ("X-A".to_owned(), "1".to_owned()),
                ("X-B".to_owned(), "2".to_owned()),
            ]
        );
    }

    #[test]
    fn splits_query_params_from_url() {
        let draft = parse_curl("curl 'https://x.test/items?page=1&size=20'").unwrap();
        assert_eq!(draft.url, "https://x.test/items");
        assert_eq!(
            draft.query_params,
            vec![
                ("page".to_owned(), "1".to_owned()),
                ("size".to_owned(), "20".to_owned()),
            ]
        );
    }

    #[test]
    fn extracts_bearer_auth_from_header() {
        let draft = parse_curl("curl https://x.test -H 'Authorization: Bearer abc123'").unwrap();
        assert_eq!(
            draft.auth,
            RequestAuth::Bearer {
                token: "abc123".to_owned()
            }
        );
        assert!(draft.headers.is_empty());
    }

    #[test]
    fn user_flag_maps_to_basic_auth() {
        let draft = parse_curl("curl https://x.test -u alice:secret").unwrap();
        assert_eq!(
            draft.auth,
            RequestAuth::Basic {
                username: "alice".to_owned(),
                password: "secret".to_owned(),
            }
        );
    }

    #[test]
    fn user_flag_takes_precedence_over_header() {
        let draft =
            parse_curl("curl https://x.test -u alice:secret -H 'Authorization: Bearer zzz'")
                .unwrap();
        assert_eq!(
            draft.auth,
            RequestAuth::Basic {
                username: "alice".to_owned(),
                password: "secret".to_owned(),
            }
        );
        // The Authorization header is consumed, not left as a plain header.
        assert!(draft.headers.is_empty());
    }

    #[test]
    fn json_flag_sets_body_and_content_type() {
        let draft = parse_curl("curl https://x.test --json '{\"a\":1}'").unwrap();
        assert_eq!(draft.method, "POST");
        assert_eq!(draft.body.as_deref(), Some("{\"a\":1}"));
        assert!(
            draft
                .headers
                .iter()
                .any(|(k, v)| k == "Content-Type" && v == "application/json")
        );
    }

    #[test]
    fn bundled_short_flags_are_ignored_except_values() {
        let draft = parse_curl("curl -sSL https://x.test").unwrap();
        assert_eq!(draft.method, "GET");
        assert_eq!(draft.url, "https://x.test");
    }

    #[test]
    fn attached_short_value_is_parsed() {
        let draft = parse_curl("curl https://x.test -XPOST").unwrap();
        assert_eq!(draft.method, "POST");
    }

    #[test]
    fn long_value_flag_does_not_swallow_url() {
        let draft = parse_curl("curl https://x.test --max-time 5").unwrap();
        assert_eq!(draft.url, "https://x.test");
    }

    #[test]
    fn missing_url_errors() {
        assert_eq!(parse_curl("curl -X GET"), Err(CurlParseError::MissingUrl));
    }

    #[test]
    fn non_curl_errors() {
        assert_eq!(
            parse_curl("wget https://x.test"),
            Err(CurlParseError::NotCurl)
        );
    }

    #[test]
    fn unterminated_quote_errors() {
        assert_eq!(
            parse_curl("curl 'https://x.test"),
            Err(CurlParseError::UnterminatedQuote)
        );
    }

    #[test]
    fn realistic_devtools_command() {
        let cmd = "curl -X POST https://api.example.com/v1/users?team=42 \
                   -H 'Authorization: Bearer abc' \
                   -H 'Content-Type: application/json' \
                   -d '{\"name\":\"jim\"}'";
        let draft = parse_curl(cmd).unwrap();
        assert_eq!(draft.method, "POST");
        assert_eq!(draft.url, "https://api.example.com/v1/users");
        assert_eq!(
            draft.query_params,
            vec![("team".to_owned(), "42".to_owned())]
        );
        assert_eq!(
            draft.auth,
            RequestAuth::Bearer {
                token: "abc".to_owned()
            }
        );
        assert_eq!(
            draft.headers,
            vec![("Content-Type".to_owned(), "application/json".to_owned())]
        );
        assert_eq!(draft.body.as_deref(), Some("{\"name\":\"jim\"}"));
    }

    #[test]
    fn looks_like_curl_detection() {
        assert!(looks_like_curl("curl https://x.test"));
        assert!(looks_like_curl("  curl -X GET https://x.test"));
        assert!(looks_like_curl("CURL https://x.test"));
        assert!(!looks_like_curl("curl"));
        assert!(!looks_like_curl("https://curl.se"));
        assert!(!looks_like_curl(""));
    }
}
