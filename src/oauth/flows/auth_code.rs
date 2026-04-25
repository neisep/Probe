use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope, TokenUrl,
};

use crate::oauth::browser::{LoopbackListener, open_url};
use crate::oauth::pkce;
use crate::oauth::{FlowKind, OAuthError, Token};

use super::{build_cached_token, collect_extra_params};

#[derive(Debug, Clone)]
pub struct AuthCodeConfig {
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
    pub audience: Option<String>,
    pub resource: Option<String>,
    pub extra_auth_params: Vec<(String, String)>,
}

pub async fn run(config: &AuthCodeConfig) -> Result<Token, OAuthError> {
    let listener = LoopbackListener::bind().await?;
    let redirect_uri = listener.redirect_uri("/callback");

    let client = build_client(config, &redirect_uri)?;

    let (challenge, verifier) = pkce::generate();

    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(challenge);
    for scope in &config.scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.clone()));
    }
    for (k, v) in collect_extra_params(config.audience.as_deref(), config.resource.as_deref(), &config.extra_auth_params) {
        auth_request = auth_request.add_extra_param(k, v);
    }

    let (auth_url_full, csrf_token) = auth_request.url();

    open_url(auth_url_full.as_str())?;

    let params = listener.accept_once().await?;

    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| error.clone());
        return Err(OAuthError::AuthDenied(description));
    }

    let returned_state = params.get("state").cloned().unwrap_or_default();
    if returned_state != *csrf_token.secret() {
        return Err(OAuthError::StateMismatch);
    }

    let code = params
        .get("code")
        .cloned()
        .ok_or_else(|| OAuthError::Parse("callback missing `code`".into()))?;

    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| OAuthError::Http(format!("token exchange failed: {e}")))?;

    Ok(build_cached_token(&token_response, FlowKind::AuthCodePkce, &config.scopes, None))
}

fn build_client(config: &AuthCodeConfig, redirect_uri: &str) -> Result<BasicClient, OAuthError> {
    let auth_url = AuthUrl::new(config.auth_url.clone())
        .map_err(|e| OAuthError::Config(format!("auth_url: {e}")))?;
    let token_url = TokenUrl::new(config.token_url.clone())
        .map_err(|e| OAuthError::Config(format!("token_url: {e}")))?;
    let redirect = RedirectUrl::new(redirect_uri.to_owned())
        .map_err(|e| OAuthError::Config(format!("redirect_uri: {e}")))?;

    Ok(BasicClient::new(
        ClientId::new(config.client_id.clone()),
        config
            .client_secret
            .as_ref()
            .map(|s| ClientSecret::new(s.clone())),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(redirect))
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> AuthCodeConfig {
        AuthCodeConfig {
            auth_url: "https://example.com/authorize".into(),
            token_url: "https://example.com/token".into(),
            client_id: "client-123".into(),
            client_secret: None,
            scopes: vec!["openid".into(), "profile".into()],
            audience: Some("api://example".into()),
            resource: None,
            extra_auth_params: vec![("prompt".into(), "consent".into())],
        }
    }

    #[test]
    fn build_client_rejects_bad_urls() {
        let mut config = sample_config();
        config.auth_url = "not a url".into();
        assert!(matches!(
            build_client(&config, "http://127.0.0.1:1234/cb"),
            Err(OAuthError::Config(_))
        ));
    }

    #[test]
    fn build_client_accepts_valid_urls() {
        let config = sample_config();
        let client = build_client(&config, "http://127.0.0.1:1234/cb").unwrap();
        // Sanity: generate an auth URL and confirm PKCE + state + scopes land in it.
        let (challenge, _verifier) = pkce::generate();
        let (url, _csrf) = client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(challenge)
            .add_scope(Scope::new("openid".into()))
            .url();

        let query: std::collections::HashMap<String, String> = url
            .query_pairs()
            .into_owned()
            .collect();
        assert_eq!(query.get("client_id").map(String::as_str), Some("client-123"));
        assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(query.contains_key("code_challenge"));
        assert!(query.contains_key("state"));
        assert_eq!(query.get("scope").map(String::as_str), Some("openid"));
    }
}
