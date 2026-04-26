pub mod auth_code;
pub mod client_credentials;
pub mod device_code;
pub mod refresh;

use oauth2::basic::BasicClient;
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl};

use crate::oauth::OAuthError;

/// Builds a `BasicClient` for flows that only need a token endpoint.
/// The `oauth2` crate requires an `AuthUrl` even for non-interactive flows;
/// this helper hides that wart behind a stable placeholder.
pub(crate) fn build_basic_client_with_token_only(
    client_id: &str,
    client_secret: Option<&str>,
    token_url: TokenUrl,
) -> Result<BasicClient, OAuthError> {
    let auth_url = AuthUrl::new("http://localhost/".to_owned())
        .expect("static placeholder URL is always valid");
    Ok(BasicClient::new(
        ClientId::new(client_id.to_owned()),
        client_secret.map(|s| ClientSecret::new(s.to_owned())),
        auth_url,
        Some(token_url),
    ))
}

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
        let v = v.trim();
        if !k.is_empty() && !params.iter().any(|(existing, _)| existing == k) {
            params.push((k.to_owned(), v.to_owned()));
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
