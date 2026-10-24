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

use oauth2::TokenResponse;
use oauth2::basic::BasicTokenType;

use crate::oauth::{FlowKind, Token, jwt_exp, now_unix};

/// Last-resort lifetime, used only when the token endpoint omits `expires_in`
/// *and* the access token carries no readable `exp` claim.
const DEFAULT_TOKEN_LIFETIME_SECONDS: i64 = 3600;

pub(crate) fn build_cached_token<R: TokenResponse<BasicTokenType>>(
    response: &R,
    flow: FlowKind,
    fallback_scopes: &[String],
    fallback_refresh_token: Option<&str>,
) -> Token {
    let now = now_unix();
    // Expiry, most authoritative source first: what the token endpoint said,
    // then the token's own `exp` claim if it is a JWT, then a guess. Reading
    // `exp` keeps Probe from renewing hourly against a token that is actually
    // valid for a day — and from attaching a dead one that only lived five
    // minutes.
    let access_token = response.access_token().secret().clone();
    let expires_at = response
        .expires_in()
        .map(|d| now.saturating_add(d.as_secs() as i64))
        .or_else(|| jwt_exp::exp_claim(&access_token))
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
        access_token,
        refresh_token,
        expires_at,
        obtained_at: now,
        scopes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use oauth2::basic::BasicTokenResponse;
    use oauth2::{AccessToken, EmptyExtraTokenFields};
    use std::time::Duration;

    /// A JWT-shaped access token whose payload claims `exp`.
    fn jwt_with_exp(exp: i64) -> String {
        format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#),
            URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{exp}}}"#).as_bytes())
        )
    }

    fn response(access_token: &str, expires_in: Option<u64>) -> BasicTokenResponse {
        let mut response = BasicTokenResponse::new(
            AccessToken::new(access_token.to_owned()),
            BasicTokenType::Bearer,
            EmptyExtraTokenFields {},
        );
        response.set_expires_in(expires_in.map(Duration::from_secs).as_ref());
        response
    }

    #[test]
    fn expires_in_from_the_endpoint_wins() {
        let now = now_unix();
        // The endpoint says 300 s; the JWT claims a year. Trust the endpoint.
        let token = build_cached_token(
            &response(&jwt_with_exp(now + 31_536_000), Some(300)),
            FlowKind::ClientCredentials,
            &[],
            None,
        );
        assert!((token.expires_at - (now + 300)).abs() <= 1);
    }

    #[test]
    fn falls_back_to_the_jwt_exp_claim() {
        let now = now_unix();
        let exp = now + 86_400;
        let token = build_cached_token(
            &response(&jwt_with_exp(exp), None),
            FlowKind::ClientCredentials,
            &[],
            None,
        );
        // A day-long token must not be treated as a one-hour one.
        assert_eq!(token.expires_at, exp);
    }

    #[test]
    fn falls_back_to_the_default_for_an_opaque_token() {
        let now = now_unix();
        let token = build_cached_token(
            &response("2YotnFZFEjr1zCsicMWpAA", None),
            FlowKind::ClientCredentials,
            &[],
            None,
        );
        assert!((token.expires_at - (now + DEFAULT_TOKEN_LIFETIME_SECONDS)).abs() <= 1);
    }

    #[test]
    fn fallback_refresh_token_is_kept_when_the_response_omits_one() {
        let token = build_cached_token(
            &response("atk", Some(60)),
            FlowKind::AuthCodePkce,
            &[],
            Some("previous-rtk"),
        );
        assert_eq!(token.refresh_token.as_deref(), Some("previous-rtk"));
    }

    #[test]
    fn fallback_scopes_are_used_when_the_response_omits_them() {
        let token = build_cached_token(
            &response("atk", Some(60)),
            FlowKind::ClientCredentials,
            &["read".to_owned(), "write".to_owned()],
            None,
        );
        assert_eq!(token.scopes, vec!["read".to_owned(), "write".to_owned()]);
    }
}
