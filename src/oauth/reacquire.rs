//! Silent (non-interactive) token re-acquisition.
//!
//! When a stored token is due for renewal and carries no refresh token, most
//! flows are stuck: getting a new token means sending the user back through a
//! browser or a device-code prompt. Client credentials is the exception —
//! it is a machine-to-machine grant with no human step, and its token
//! responses normally omit `refresh_token` precisely because replaying the
//! grant is the intended way to renew. So an expired client-credentials token
//! is re-acquired rather than reported as an error.
//!
//! This module only *builds the request*; the middleware owns when to run it.

use crate::oauth::OAuthConfig;
use crate::oauth::flows::client_credentials::ClientCredentialsConfig;

/// Build a client-credentials request from the environment's saved config, or
/// `None` when the config cannot authenticate on its own — the same
/// completeness bar the OAuth panel applies before enabling "Get token".
pub(crate) fn client_credentials_config(config: &OAuthConfig) -> Option<ClientCredentialsConfig> {
    let fields = &config.client_credentials;
    let token_url = fields.token_url.trim();
    let client_id = fields.client_id.trim();
    let client_secret = fields.client_secret.trim();
    if token_url.is_empty() || client_id.is_empty() || client_secret.is_empty() {
        return None;
    }

    Some(ClientCredentialsConfig {
        token_url: token_url.to_owned(),
        client_id: client_id.to_owned(),
        client_secret: client_secret.to_owned(),
        scopes: fields.parsed_scopes(),
        audience: crate::oauth::config::normalize_optional(&fields.audience),
        resource: crate::oauth::config::normalize_optional(&fields.resource),
        extra_token_params: fields
            .extra_params
            .iter()
            .filter(|(key, _)| !key.trim().is_empty())
            .map(|(key, value)| (key.trim().to_owned(), value.clone()))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_config() -> OAuthConfig {
        OAuthConfig {
            client_credentials: crate::oauth::config::ClientCredentialsFields {
                token_url: " https://example.com/token ".into(),
                client_id: " svc ".into(),
                client_secret: " s3cret ".into(),
                scopes: "read   write".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn builds_a_trimmed_request_from_a_complete_config() {
        let built = client_credentials_config(&complete_config()).expect("complete config");
        assert_eq!(built.token_url, "https://example.com/token");
        assert_eq!(built.client_id, "svc");
        assert_eq!(built.client_secret, "s3cret");
        assert_eq!(built.scopes, vec!["read".to_owned(), "write".to_owned()]);
        assert_eq!(built.audience, None);
        assert_eq!(built.resource, None);
        assert!(built.extra_token_params.is_empty());
    }

    #[test]
    fn carries_audience_resource_and_extra_params() {
        let mut config = complete_config();
        config.client_credentials.audience = "https://api.example.com".into();
        config.client_credentials.resource = "urn:thing".into();
        config.client_credentials.extra_params = vec![
            ("tenant".into(), "acme".into()),
            ("  ".into(), "junk".into()),
        ];

        let built = client_credentials_config(&config).expect("complete config");
        assert_eq!(built.audience.as_deref(), Some("https://api.example.com"));
        assert_eq!(built.resource.as_deref(), Some("urn:thing"));
        assert_eq!(
            built.extra_token_params,
            vec![("tenant".to_owned(), "acme".to_owned())]
        );
    }

    #[test]
    fn rejects_an_incomplete_config() {
        for strip in ["token_url", "client_id", "client_secret"] {
            let mut config = complete_config();
            match strip {
                "token_url" => config.client_credentials.token_url = "  ".into(),
                "client_id" => config.client_credentials.client_id = String::new(),
                _ => config.client_credentials.client_secret = String::new(),
            }
            assert!(
                client_credentials_config(&config).is_none(),
                "missing {strip} should block silent re-acquisition"
            );
        }
    }
}
