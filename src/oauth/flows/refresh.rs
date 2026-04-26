use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, RefreshToken, TokenUrl,
};

use crate::oauth::{FlowKind, OAuthError, Token};
use super::build_cached_token;

#[derive(Debug, Clone)]
pub struct RefreshConfig {
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub refresh_token: String,
}

pub async fn run(
    config: &RefreshConfig,
    flow: FlowKind,
    fallback_scopes: &[String],
) -> Result<Token, OAuthError> {
    let token_url = TokenUrl::new(config.token_url.clone())
        .map_err(|e| OAuthError::Config(format!("token_url: {e}")))?;
    let auth_url = AuthUrl::new("http://localhost/".to_owned())
        .map_err(|e| OAuthError::Config(format!("auth_url placeholder: {e}")))?;

    let client = BasicClient::new(
        ClientId::new(config.client_id.clone()),
        config
            .client_secret
            .as_ref()
            .map(|s| ClientSecret::new(s.clone())),
        auth_url,
        Some(token_url),
    );

    let response = client
        .exchange_refresh_token(&RefreshToken::new(config.refresh_token.clone()))
        .request_async(async_http_client)
        .await
        .map_err(|e| OAuthError::Http(format!("refresh failed: {e}")))?;

    Ok(build_cached_token(
        &response,
        flow,
        fallback_scopes,
        Some(config.refresh_token.as_str()),
    ))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_token_url_returns_config_error() {
        let config = RefreshConfig {
            token_url: "::::bad".into(),
            client_id: "c".into(),
            client_secret: None,
            refresh_token: "r".into(),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = rt
            .block_on(async { run(&config, FlowKind::AuthCodePkce, &[]).await })
            .expect_err("bad token url should surface config error");
        assert!(matches!(error, OAuthError::Config(_)));
    }
}
