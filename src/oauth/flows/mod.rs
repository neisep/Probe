pub mod auth_code;
pub mod client_credentials;
pub mod device_code;
pub mod refresh;

pub(crate) fn collect_extra_params(
    audience: Option<&str>,
    resource: Option<&str>,
    extra: &[(String, String)],
) -> Vec<(String, String)> {
    let mut params = Vec::new();
    if let Some(a) = audience {
        params.push(("audience".to_owned(), a.to_owned()));
    }
    if let Some(r) = resource {
        params.push(("resource".to_owned(), r.to_owned()));
    }
    for (k, v) in extra {
        let k = k.trim();
        if !k.is_empty() {
            params.push((k.to_owned(), v.clone()));
        }
    }
    params
}


use oauth2::basic::BasicTokenType;
use oauth2::TokenResponse;

use crate::oauth::{now_unix, FlowKind, Token};

const DEFAULT_TOKEN_LIFETIME_SECONDS: i64 = 3600;

pub(crate) fn build_cached_token<R: TokenResponse<BasicTokenType>>(
    response: &R,
    flow: FlowKind,
    fallback_scopes: &[String],
    fallback_refresh_token: Option<&str>,
) -> Token {
    let now = now_unix();
    let expires_at = response
        .expires_in()
        .map(|d| now.saturating_add(d.as_secs() as i64))
        .unwrap_or_else(|| now.saturating_add(DEFAULT_TOKEN_LIFETIME_SECONDS));
    let scopes = response
        .scopes()
        .map(|s| s.iter().map(|sc| sc.to_string()).collect())
        .unwrap_or_else(|| fallback_scopes.to_vec());
    let refresh_token = response
        .refresh_token()
        .map(|rt| rt.secret().clone())
        .or_else(|| fallback_refresh_token.map(str::to_owned));

    Token {
        flow,
        access_token: response.access_token().secret().clone(),
        refresh_token,
        expires_at,
        obtained_at: now,
        scopes,
    }
}
