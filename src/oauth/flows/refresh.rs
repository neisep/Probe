use std::time::{SystemTime, UNIX_EPOCH};

use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, RefreshToken, TokenResponse, TokenUrl,
};

use crate::oauth::{FlowKind, OAuthError, Token};

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

    let now = now_unix();
    let expires_at = response
        .expires_in()
        .map(|d| now.saturating_add(d.as_secs() as i64))
        .unwrap_or_else(|| now.saturating_add(3600));
    let scopes = response
        .scopes()
        .map(|s| s.iter().map(|sc| sc.to_string()).collect())
        .unwrap_or_else(|| fallback_scopes.to_vec());

    Ok(Token {
        flow,
        access_token: response.access_token().secret().clone(),
        refresh_token: response
            .refresh_token()
            .map(|rt| rt.secret().clone())
            .or_else(|| Some(config.refresh_token.clone())),
        expires_at,
        obtained_at: now,
        scopes,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
