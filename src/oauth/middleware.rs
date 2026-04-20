use std::time::{SystemTime, UNIX_EPOCH};

use crate::oauth::config::slugify_env_id;
use crate::oauth::flows::refresh::{self, RefreshConfig};
use crate::oauth::{FileTokenStore, FlowKind, OAuthError, Token, TokenStore};
use crate::persistence::FileStorage;

const REFRESH_BUFFER_SECONDS: i64 = 60;

pub fn resolve_authorization(env_name: &str) -> Result<Option<String>, OAuthError> {
    resolve_authorization_at(env_name, crate::oauth::DATA_DIR)
}

pub(crate) fn resolve_authorization_at(
    env_name: &str,
    base_dir: &str,
) -> Result<Option<String>, OAuthError> {
    let Ok(storage) = FileStorage::new(base_dir) else {
        return Ok(None);
    };
    let env_id = slugify_env_id(env_name);

    let Ok(config) = storage.load_oauth_config(&env_id) else {
        return Ok(None);
    };
    let Some(flow) = config.active_flow else {
        return Ok(None);
    };

    let token_store = FileTokenStore::new(base_dir);
    let Some(token) = token_store.get(&env_id, flow.as_str())? else {
        return Ok(None);
    };

    let now = now_unix();
    if !token.expires_within(now, REFRESH_BUFFER_SECONDS) {
        return Ok(Some(bearer(&token)));
    }

    if let Some(refresh_token) = token.refresh_token.clone() {
        let Some(endpoint) = config.token_endpoint(flow) else {
            return Err(OAuthError::Config(
                "token endpoint missing for refresh".into(),
            ));
        };
        let refreshed = block_on_refresh(
            RefreshConfig {
                token_url: endpoint.token_url,
                client_id: endpoint.client_id,
                client_secret: endpoint.client_secret,
                refresh_token,
            },
            flow,
            &token.scopes,
        )?;
        token_store.put(&env_id, flow.as_str(), &refreshed)?;
        return Ok(Some(bearer(&refreshed)));
    }

    if token.is_expired(now) {
        Err(OAuthError::AuthDenied(
            "token expired and no refresh token available".into(),
        ))
    } else {
        Ok(Some(bearer(&token)))
    }
}

fn bearer(token: &Token) -> String {
    format!("Bearer {}", token.access_token)
}

fn block_on_refresh(
    config: RefreshConfig,
    flow: FlowKind,
    fallback_scopes: &[String],
) -> Result<Token, OAuthError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| OAuthError::Http(format!("refresh runtime: {e}")))?;
    runtime.block_on(async { refresh::run(&config, flow, fallback_scopes).await })
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
    use crate::oauth::{FlowKind, OAuthConfig, Token};

    fn temp_dir() -> std::path::PathBuf {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("probe-oauth-mw-{nanos}"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn returns_none_when_no_config() {
        let base = temp_dir();
        let result = resolve_authorization_at("dev", base.to_str().unwrap()).unwrap();
        assert!(result.is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn returns_none_when_flow_unset() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();
        storage
            .save_oauth_config("dev", &OAuthConfig::default())
            .unwrap();

        let result = resolve_authorization_at("dev", base.to_str().unwrap()).unwrap();
        assert!(result.is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn returns_none_when_no_token_stored() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();
        let mut config = OAuthConfig::default();
        config.active_flow = Some(FlowKind::ClientCredentials);
        config.client_credentials.token_url = "https://example.com/token".into();
        config.client_credentials.client_id = "svc".into();
        storage.save_oauth_config("dev", &config).unwrap();

        let result = resolve_authorization_at("dev", base.to_str().unwrap()).unwrap();
        assert!(result.is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn returns_bearer_when_token_valid() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();
        let mut config = OAuthConfig::default();
        config.active_flow = Some(FlowKind::ClientCredentials);
        config.client_credentials.token_url = "https://example.com/token".into();
        config.client_credentials.client_id = "svc".into();
        storage.save_oauth_config("dev", &config).unwrap();

        let token_store = FileTokenStore::new(&base);
        let token = Token {
            flow: FlowKind::ClientCredentials,
            access_token: "atk".into(),
            refresh_token: None,
            expires_at: now_unix() + 3600,
            obtained_at: now_unix(),
            scopes: vec![],
        };
        token_store.put("dev", "client_credentials", &token).unwrap();

        let result = resolve_authorization_at("dev", base.to_str().unwrap()).unwrap();
        assert_eq!(result.as_deref(), Some("Bearer atk"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn errors_when_expired_without_refresh() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();
        let mut config = OAuthConfig::default();
        config.active_flow = Some(FlowKind::ClientCredentials);
        config.client_credentials.token_url = "https://example.com/token".into();
        config.client_credentials.client_id = "svc".into();
        storage.save_oauth_config("dev", &config).unwrap();

        let token_store = FileTokenStore::new(&base);
        let token = Token {
            flow: FlowKind::ClientCredentials,
            access_token: "atk".into(),
            refresh_token: None,
            expires_at: now_unix() - 10,
            obtained_at: now_unix() - 3600,
            scopes: vec![],
        };
        token_store.put("dev", "client_credentials", &token).unwrap();

        let error =
            resolve_authorization_at("dev", base.to_str().unwrap()).expect_err("expected error");
        assert!(matches!(error, OAuthError::AuthDenied(_)));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn slugifies_env_name_consistently() {
        let base = temp_dir();
        let storage = FileStorage::new(&base).unwrap();
        let mut config = OAuthConfig::default();
        config.active_flow = Some(FlowKind::ClientCredentials);
        config.client_credentials.token_url = "https://example.com/token".into();
        config.client_credentials.client_id = "svc".into();
        storage.save_oauth_config("My_Env", &config).unwrap();

        let token_store = FileTokenStore::new(&base);
        let token = Token {
            flow: FlowKind::ClientCredentials,
            access_token: "atk".into(),
            refresh_token: None,
            expires_at: now_unix() + 3600,
            obtained_at: now_unix(),
            scopes: vec![],
        };
        token_store.put("My_Env", "client_credentials", &token).unwrap();

        let result =
            resolve_authorization_at("My Env", base.to_str().unwrap()).unwrap();
        assert_eq!(result.as_deref(), Some("Bearer atk"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
