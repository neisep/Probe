use base64::Engine;

use crate::runtime::{
    AsyncRequest, ResolutionError, ResolutionErrorKind, ResolutionValues, UnresolvedBehavior,
    resolve_body_text, resolve_headers, resolve_text_with_behavior,
};
use crate::state::request::{ApiKeyLocation, RequestAuth};
use crate::state::AppState;

pub fn active_resolution_values(state: &AppState) -> ResolutionValues {
    state.active_variables().cloned().unwrap_or_default()
}

pub fn prepare_request_draft(
    request: &crate::state::RequestDraft,
    resolution_values: &ResolutionValues,
) -> Result<AsyncRequest, ResolutionError> {
    let resolved_url = resolve_text_with_behavior(
        "url",
        &request.url,
        resolution_values,
        UnresolvedBehavior::Error,
    )?;
    let mut resolved_headers = resolve_headers(
        &request.headers,
        resolution_values,
        UnresolvedBehavior::Error,
    )?;
    let resolved_body = resolve_body_text(
        request.body.as_ref().map(|body| body.as_bytes()),
        resolution_values,
        UnresolvedBehavior::Error,
    )?;
    let mut resolved_query_params = Vec::with_capacity(request.query_params.len());

    for (index, (name, value)) in request.query_params.iter().enumerate() {
        let resolved_name = resolve_text_with_behavior(
            &format!("query[{index}].name"),
            name,
            resolution_values,
            UnresolvedBehavior::Error,
        )?;
        if resolved_name.trim().is_empty() {
            continue;
        }

        let resolved_value = resolve_text_with_behavior(
            &format!("query[{index}].value"),
            value,
            resolution_values,
            UnresolvedBehavior::Error,
        )?;
        resolved_query_params.push((resolved_name, resolved_value));
    }
    let resolved_auth = resolve_request_auth(&request.auth, resolution_values)?;
    apply_auth_headers(&mut resolved_headers, resolved_auth.headers)?;
    resolved_query_params.extend(resolved_auth.query_params);

    Ok(AsyncRequest {
        url: build_request_url(&resolved_url, &resolved_query_params)?,
        method: request.method.clone(),
        headers: resolved_headers,
        body: resolved_body,
    })
}

#[derive(Default)]
struct ResolvedAuth {
    headers: Vec<(String, String)>,
    query_params: Vec<(String, String)>,
}

fn resolve_request_auth(
    auth: &RequestAuth,
    resolution_values: &ResolutionValues,
) -> Result<ResolvedAuth, ResolutionError> {
    match auth {
        RequestAuth::None => Ok(ResolvedAuth::default()),
        RequestAuth::Bearer { token } => {
            let token = resolve_text_with_behavior(
                "auth.bearer.token",
                token,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            if token.trim().is_empty() {
                return Err(invalid_request_error(
                    "auth",
                    "bearer token cannot be empty",
                ));
            }

            Ok(ResolvedAuth {
                headers: vec![("Authorization".to_owned(), format!("Bearer {token}"))],
                query_params: Vec::new(),
            })
        }
        RequestAuth::Basic { username, password } => {
            let username = resolve_text_with_behavior(
                "auth.basic.username",
                username,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            let password = resolve_text_with_behavior(
                "auth.basic.password",
                password,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            if username.is_empty() && password.is_empty() {
                return Err(invalid_request_error(
                    "auth",
                    "basic auth requires a username or password",
                ));
            }

            let encoded =
                base64::prelude::BASE64_STANDARD.encode(format!("{username}:{password}"));
            Ok(ResolvedAuth {
                headers: vec![("Authorization".to_owned(), format!("Basic {encoded}"))],
                query_params: Vec::new(),
            })
        }
        RequestAuth::ApiKey {
            location,
            name,
            value,
        } => {
            let name = resolve_text_with_behavior(
                "auth.api_key.name",
                name,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            let value = resolve_text_with_behavior(
                "auth.api_key.value",
                value,
                resolution_values,
                UnresolvedBehavior::Error,
            )?;
            if name.trim().is_empty() {
                return Err(invalid_request_error(
                    "auth",
                    "api key name cannot be empty",
                ));
            }
            if value.trim().is_empty() {
                return Err(invalid_request_error(
                    "auth",
                    "api key value cannot be empty",
                ));
            }

            match location {
                ApiKeyLocation::Header => Ok(ResolvedAuth {
                    headers: vec![(name, value)],
                    query_params: Vec::new(),
                }),
                ApiKeyLocation::Query => Ok(ResolvedAuth {
                    headers: Vec::new(),
                    query_params: vec![(name, value)],
                }),
            }
        }
    }
}

fn apply_auth_headers(
    existing_headers: &mut Vec<(String, String)>,
    auth_headers: Vec<(String, String)>,
) -> Result<(), ResolutionError> {
    for (auth_name, _) in &auth_headers {
        if existing_headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(auth_name))
        {
            return Err(invalid_request_error(
                "auth",
                &format!("auth header '{auth_name}' conflicts with an existing header"),
            ));
        }
    }

    existing_headers.extend(auth_headers);
    Ok(())
}

fn invalid_request_error(target: &str, details: &str) -> ResolutionError {
    ResolutionError {
        kind: ResolutionErrorKind::InvalidPlaceholder,
        target: target.to_owned(),
        placeholder: None,
        details: Some(details.to_owned()),
    }
}

pub fn build_request_url(
    base_url: &str,
    query_params: &[(String, String)],
) -> Result<String, ResolutionError> {
    if query_params.is_empty() {
        return Ok(base_url.to_owned());
    }

    let mut url = reqwest::Url::parse(base_url).map_err(|error| ResolutionError {
        kind: ResolutionErrorKind::InvalidPlaceholder,
        target: "url".to_owned(),
        placeholder: None,
        details: Some(format!("invalid url: {error}")),
    })?;
    {
        let mut serializer = url.query_pairs_mut();
        for (name, value) in query_params {
            serializer.append_pair(name, value);
        }
    }

    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::{build_request_url, prepare_request_draft};
    use crate::state::request::{ApiKeyLocation, RequestAuth};
    use crate::state::RequestDraft;
    use std::collections::BTreeMap;

    #[test]
    fn build_request_url_appends_encoded_query_params() {
        let request_url = build_request_url(
            "https://example.com/items#details",
            &[
                ("page".to_owned(), "1".to_owned()),
                ("search".to_owned(), "hello world".to_owned()),
            ],
        )
        .expect("query params should build a valid url");
        let url = reqwest::Url::parse(&request_url).expect("built url should parse");
        let query_pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();

        assert_eq!(url.fragment(), Some("details"));
        assert_eq!(
            query_pairs,
            vec![
                ("page".to_owned(), "1".to_owned()),
                ("search".to_owned(), "hello world".to_owned()),
            ]
        );
    }

    #[test]
    fn prepare_request_draft_resolves_query_placeholders() {
        let mut request = RequestDraft::default_request();
        request.set_url("https://example.com/items");
        request.query_params = vec![("search".to_owned(), "{{term}}".to_owned())];

        let mut values = BTreeMap::new();
        values.insert("term".to_owned(), "hello world".to_owned());

        let prepared = prepare_request_draft(&request, &values)
            .expect("request draft should resolve placeholders into query params");
        let url = reqwest::Url::parse(&prepared.url).expect("prepared url should parse");
        let query_pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();

        assert_eq!(
            query_pairs,
            vec![("search".to_owned(), "hello world".to_owned())]
        );
    }

    #[test]
    fn prepare_request_draft_injects_bearer_auth_header() {
        let mut request = RequestDraft::default_request();
        request.auth = RequestAuth::Bearer {
            token: "{{TOKEN}}".to_owned(),
        };
        let mut values = BTreeMap::new();
        values.insert("TOKEN".to_owned(), "secret".to_owned());

        let prepared =
            prepare_request_draft(&request, &values).expect("bearer auth should resolve");

        assert!(
            prepared
                .headers
                .iter()
                .any(|(name, value)| name == "Authorization" && value == "Bearer secret")
        );
    }

    #[test]
    fn prepare_request_draft_injects_basic_auth_header() {
        let mut request = RequestDraft::default_request();
        request.auth = RequestAuth::Basic {
            username: "aladdin".to_owned(),
            password: "open sesame".to_owned(),
        };

        let prepared =
            prepare_request_draft(&request, &BTreeMap::new()).expect("basic auth should encode");

        assert!(prepared.headers.iter().any(|(name, value)| {
            name == "Authorization" && value == "Basic YWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        }));
    }

    #[test]
    fn prepare_request_draft_injects_query_api_key() {
        let mut request = RequestDraft::default_request();
        request.auth = RequestAuth::ApiKey {
            location: ApiKeyLocation::Query,
            name: "api_key".to_owned(),
            value: "{{KEY}}".to_owned(),
        };
        let mut values = BTreeMap::new();
        values.insert("KEY".to_owned(), "secret".to_owned());

        let prepared =
            prepare_request_draft(&request, &values).expect("query api key should resolve");
        let url = reqwest::Url::parse(&prepared.url).expect("prepared url should parse");
        let query_pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();

        assert_eq!(
            query_pairs,
            vec![("api_key".to_owned(), "secret".to_owned())]
        );
    }

    #[test]
    fn prepare_request_draft_rejects_auth_header_conflicts() {
        let mut request = RequestDraft::default_request();
        request.headers = vec![("Authorization".to_owned(), "Bearer manual".to_owned())];
        request.auth = RequestAuth::Bearer {
            token: "generated".to_owned(),
        };

        let error = prepare_request_draft(&request, &BTreeMap::new())
            .expect_err("conflicting authorization header should fail");

        assert_eq!(error.target, "auth");
        assert!(
            error
                .details
                .as_deref()
                .unwrap_or_default()
                .contains("conflicts with an existing header")
        );
    }
}
