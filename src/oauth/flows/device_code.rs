use std::time::{SystemTime, UNIX_EPOCH};

use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, DeviceAuthorizationUrl, Scope, TokenResponse, TokenUrl,
};

use crate::oauth::{FlowKind, Token};

#[derive(Debug, Clone)]
pub struct DeviceCodeConfig {
    pub device_auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
    pub audience: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DeviceCodeEvent {
    PendingUser {
        user_code: String,
        verification_uri: String,
        verification_uri_complete: Option<String>,
    },
    Completed(Token),
    Failed(String),
}

pub async fn run<F>(config: &DeviceCodeConfig, on_event: F)
where
    F: Fn(DeviceCodeEvent) + Send + 'static,
{
    let client = match build_client(config) {
        Ok(client) => client,
        Err(error) => {
            on_event(DeviceCodeEvent::Failed(error));
            return;
        }
    };

    let mut device_req = match client.exchange_device_code() {
        Ok(req) => req,
        Err(error) => {
            on_event(DeviceCodeEvent::Failed(format!(
                "device exchange setup: {error}"
            )));
            return;
        }
    };
    for scope in &config.scopes {
        device_req = device_req.add_scope(Scope::new(scope.clone()));
    }
    if let Some(audience) = &config.audience {
        device_req = device_req.add_extra_param("audience", audience.clone());
    }

    let device_response: oauth2::StandardDeviceAuthorizationResponse =
        match device_req.request_async(async_http_client).await {
            Ok(response) => response,
            Err(error) => {
                on_event(DeviceCodeEvent::Failed(format!(
                    "device authorization: {error}"
                )));
                return;
            }
        };

    on_event(DeviceCodeEvent::PendingUser {
        user_code: device_response.user_code().secret().clone(),
        verification_uri: device_response.verification_uri().as_str().to_owned(),
        verification_uri_complete: device_response
            .verification_uri_complete()
            .map(|uri| uri.secret().clone()),
    });

    let token_response = match client
        .exchange_device_access_token(&device_response)
        .request_async(async_http_client, tokio::time::sleep, None)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            on_event(DeviceCodeEvent::Failed(format!("token polling: {error}")));
            return;
        }
    };

    let now = now_unix();
    let expires_at = token_response
        .expires_in()
        .map(|d| now.saturating_add(d.as_secs() as i64))
        .unwrap_or_else(|| now.saturating_add(3600));
    let scopes = token_response
        .scopes()
        .map(|s| s.iter().map(|sc| sc.to_string()).collect())
        .unwrap_or_else(|| config.scopes.clone());

    let token = Token {
        flow: FlowKind::DeviceCode,
        access_token: token_response.access_token().secret().clone(),
        refresh_token: token_response.refresh_token().map(|rt| rt.secret().clone()),
        expires_at,
        obtained_at: now,
        scopes,
    };

    on_event(DeviceCodeEvent::Completed(token));
}

fn build_client(config: &DeviceCodeConfig) -> Result<BasicClient, String> {
    let token_url =
        TokenUrl::new(config.token_url.clone()).map_err(|e| format!("token_url: {e}"))?;
    let device_auth_url = DeviceAuthorizationUrl::new(config.device_auth_url.clone())
        .map_err(|e| format!("device_auth_url: {e}"))?;
    let auth_url = AuthUrl::new("http://localhost/".to_owned())
        .map_err(|e| format!("auth_url placeholder: {e}"))?;

    Ok(BasicClient::new(
        ClientId::new(config.client_id.clone()),
        config
            .client_secret
            .as_ref()
            .map(|s| ClientSecret::new(s.clone())),
        auth_url,
        Some(token_url),
    )
    .set_device_authorization_url(device_auth_url))
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
    use std::sync::{Arc, Mutex};

    #[test]
    fn bad_device_auth_url_emits_failed_event() {
        let config = DeviceCodeConfig {
            device_auth_url: "not a url".into(),
            token_url: "https://example.com/token".into(),
            client_id: "client".into(),
            client_secret: None,
            scopes: vec![],
            audience: None,
        };
        let events: Arc<Mutex<Vec<DeviceCodeEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_capture = events.clone();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            run(&config, move |event| {
                events_capture.lock().unwrap().push(event);
            })
            .await;
        });

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(matches!(captured[0], DeviceCodeEvent::Failed(_)));
    }

    #[test]
    fn bad_token_url_emits_failed_event() {
        let config = DeviceCodeConfig {
            device_auth_url: "https://example.com/device".into(),
            token_url: "::::bad".into(),
            client_id: "client".into(),
            client_secret: None,
            scopes: vec![],
            audience: None,
        };
        let events: Arc<Mutex<Vec<DeviceCodeEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_capture = events.clone();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            run(&config, move |event| {
                events_capture.lock().unwrap().push(event);
            })
            .await;
        });

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(matches!(captured[0], DeviceCodeEvent::Failed(_)));
    }
}
