use oauth2::reqwest::async_http_client;
use oauth2::{Scope, TokenUrl};

use crate::oauth::{FlowKind, OAuthError, Token};

use super::{build_basic_client_with_token_only, build_cached_token, collect_extra_params};

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
    let client = build_basic_client_with_token_only(
        &config.client_id,
        Some(&config.client_secret),
        token_url,
    )?;

    let mut request = client.exchange_client_credentials();
    for scope in &config.scopes {
        request = request.add_scope(Scope::new(scope.clone()));
    }
    for (k, v) in collect_extra_params(config.audience.as_deref(), config.resource.as_deref(), &config.extra_token_params) {
        request = request.add_extra_param(k, v);
    }

    let response = request
        .request_async(async_http_client)
        .await
        .map_err(|e| OAuthError::Http(format!("client credentials token exchange failed: {e}")))?;

    Ok(build_cached_token(&response, FlowKind::ClientCredentials, &config.scopes, None))
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
