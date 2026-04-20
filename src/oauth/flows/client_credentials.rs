use std::time::{SystemTime, UNIX_EPOCH};

use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, Scope, TokenResponse, TokenUrl,
};

use crate::oauth::{FlowKind, OAuthError, Token};

#[derive(Debug, Clone)]
pub struct ClientCredentialsConfig {
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,
    pub audience: Option<String>,
    pub resource: Option<String>,
    pub extra_token_params: Vec<(String, String)>,
}

pub async fn run(config: &ClientCredentialsConfig) -> Result<Token, OAuthError> {
    let token_url = TokenUrl::new(config.token_url.clone())
        .map_err(|e| OAuthError::Config(format!("token_url: {e}")))?;
    let auth_url = AuthUrl::new("http://localhost/".to_owned())
        .map_err(|e| OAuthError::Config(format!("auth_url placeholder: {e}")))?;

    let client = BasicClient::new(
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
        auth_url,
        Some(token_url),
    );

    let mut request = client.exchange_client_credentials();
    for scope in &config.scopes {
        request = request.add_scope(Scope::new(scope.clone()));
    }
    if let Some(audience) = &config.audience {
        request = request.add_extra_param("audience", audience.clone());
    }
    if let Some(resource) = &config.resource {
        request = request.add_extra_param("resource", resource.clone());
    }
    for (k, v) in &config.extra_token_params {
        request = request.add_extra_param(k.clone(), v.clone());
    }

    let response = request
        .request_async(async_http_client)
        .await
        .map_err(|e| OAuthError::Http(format!("client credentials token exchange failed: {e}")))?;

    let now = now_unix();
    let expires_at = response
        .expires_in()
        .map(|d| now.saturating_add(d.as_secs() as i64))
        .unwrap_or_else(|| now.saturating_add(3600));
    let scopes = response
        .scopes()
        .map(|s| s.iter().map(|sc| sc.to_string()).collect())
        .unwrap_or_else(|| config.scopes.clone());

    Ok(Token {
        flow: FlowKind::ClientCredentials,
        access_token: response.access_token().secret().clone(),
        refresh_token: response.refresh_token().map(|rt| rt.secret().clone()),
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
        let config = ClientCredentialsConfig {
            token_url: "not a url".into(),
            client_id: "svc".into(),
            client_secret: "secret".into(),
            scopes: vec![],
            audience: None,
            resource: None,
            extra_token_params: vec![],
        };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = rt
            .block_on(async { run(&config).await })
            .expect_err("bad token url should surface config error");
        assert!(matches!(error, OAuthError::Config(_)));
    }
}
