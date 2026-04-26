use serde::{Deserialize, Serialize};

use super::FlowKind;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthConfig {
    #[serde(default)]
    pub active_flow: Option<FlowKind>,
    #[serde(default)]
    pub auth_code: AuthCodeFields,
    #[serde(default)]
    pub client_credentials: ClientCredentialsFields,
    #[serde(default)]
    pub device_code: DeviceCodeFields,
    #[serde(default)]
    pub injection: InjectionConfig,
}

impl OAuthConfig {
    pub fn token_endpoint(&self, flow: FlowKind) -> Option<TokenEndpoint> {
        match flow {
            FlowKind::AuthCodePkce => {
                let f = &self.auth_code;
                if f.token_url.trim().is_empty() || f.client_id.trim().is_empty() {
                    return None;
                }
                Some(TokenEndpoint {
                    token_url: f.token_url.trim().to_owned(),
                    client_id: f.client_id.trim().to_owned(),
                    client_secret: normalize_optional(&f.client_secret),
                })
            }
            FlowKind::ClientCredentials => {
                let f = &self.client_credentials;
                if f.token_url.trim().is_empty() || f.client_id.trim().is_empty() {
                    return None;
                }
                Some(TokenEndpoint {
                    token_url: f.token_url.trim().to_owned(),
                    client_id: f.client_id.trim().to_owned(),
                    client_secret: normalize_optional(&f.client_secret),
                })
            }
            FlowKind::DeviceCode => {
                let f = &self.device_code;
                if f.token_url.trim().is_empty() || f.client_id.trim().is_empty() {
                    return None;
                }
                Some(TokenEndpoint {
                    token_url: f.token_url.trim().to_owned(),
                    client_id: f.client_id.trim().to_owned(),
                    client_secret: normalize_optional(&f.client_secret),
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenEndpoint {
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InjectionConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_header_name")]
    pub header_name: String,
    #[serde(default = "default_header_prefix")]
    pub header_prefix: String,
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            header_name: default_header_name(),
            header_prefix: default_header_prefix(),
        }
    }
}

impl InjectionConfig {
    pub fn effective_header_name(&self) -> &str {
        let trimmed = self.header_name.trim();
        if trimmed.is_empty() {
            "Authorization"
        } else {
            trimmed
        }
    }

    pub fn format_header_value(&self, access_token: &str) -> String {
        let prefix = self.header_prefix.trim();
        if prefix.is_empty() {
            access_token.to_owned()
        } else {
            format!("{prefix} {access_token}")
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_header_name() -> String {
    "Authorization".to_owned()
}

fn default_header_prefix() -> String {
    "Bearer".to_owned()
}

pub(crate) fn normalize_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthCodeFields {
    #[serde(default)]
    pub auth_url: String,
    #[serde(default)]
    pub token_url: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub scopes: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default)]
    pub resource: String,
    #[serde(default)]
    pub extra_params: Vec<(String, String)>,
}

impl AuthCodeFields {
    pub fn parsed_scopes(&self) -> Vec<String> {
        self.scopes
            .split_whitespace()
            .map(|s| s.to_owned())
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCredentialsFields {
    #[serde(default)]
    pub token_url: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub scopes: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default)]
    pub resource: String,
    #[serde(default)]
    pub extra_params: Vec<(String, String)>,
}

impl ClientCredentialsFields {
    pub fn parsed_scopes(&self) -> Vec<String> {
        self.scopes
            .split_whitespace()
            .map(|s| s.to_owned())
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCodeFields {
    #[serde(default)]
    pub device_auth_url: String,
    #[serde(default)]
    pub token_url: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub scopes: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default)]
    pub resource: String,
    #[serde(default)]
    pub extra_params: Vec<(String, String)>,
}

impl DeviceCodeFields {
    pub fn parsed_scopes(&self) -> Vec<String> {
        self.scopes
            .split_whitespace()
            .map(|s| s.to_owned())
            .collect()
    }
}

pub fn slugify_env_id(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_underscore = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
            last_underscore = ch == '_' || ch == '-';
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    let trimmed = out.trim_matches(|c| c == '_' || c == '-').to_owned();
    if trimmed.is_empty() {
        "env".to_owned()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_preserves_alnum_and_separators() {
        assert_eq!(slugify_env_id("Dev"), "Dev");
        assert_eq!(slugify_env_id("dev-1"), "dev-1");
        assert_eq!(slugify_env_id("dev_1"), "dev_1");
    }

    #[test]
    fn slug_replaces_invalid_chars() {
        assert_eq!(slugify_env_id("dev env"), "dev_env");
        assert_eq!(slugify_env_id("prod/eu"), "prod_eu");
        assert_eq!(slugify_env_id("My API"), "My_API");
    }

    #[test]
    fn slug_collapses_runs_and_trims() {
        assert_eq!(slugify_env_id("  weird  name  "), "weird_name");
        assert_eq!(slugify_env_id("***"), "env");
        assert_eq!(slugify_env_id(""), "env");
    }

    #[test]
    fn parsed_scopes_splits_on_whitespace() {
        let fields = AuthCodeFields {
            scopes: "openid profile  offline_access".into(),
            ..Default::default()
        };
        assert_eq!(
            fields.parsed_scopes(),
            vec![
                "openid".to_owned(),
                "profile".to_owned(),
                "offline_access".to_owned(),
            ]
        );
    }

    #[test]
    fn token_endpoint_returns_none_when_fields_missing() {
        let config = OAuthConfig::default();
        assert!(config.token_endpoint(FlowKind::AuthCodePkce).is_none());
        assert!(config.token_endpoint(FlowKind::ClientCredentials).is_none());
        assert!(config.token_endpoint(FlowKind::DeviceCode).is_none());
    }

    #[test]
    fn token_endpoint_extracts_auth_code_fields() {
        let mut config = OAuthConfig::default();
        config.auth_code.token_url = "https://idp.example.com/token".into();
        config.auth_code.client_id = "my-app".into();
        config.auth_code.client_secret = "".into();

        let endpoint = config
            .token_endpoint(FlowKind::AuthCodePkce)
            .expect("expected auth code token endpoint");
        assert_eq!(endpoint.token_url, "https://idp.example.com/token");
        assert_eq!(endpoint.client_id, "my-app");
        assert!(endpoint.client_secret.is_none());
    }

    #[test]
    fn token_endpoint_extracts_client_credentials_fields() {
        let mut config = OAuthConfig::default();
        config.client_credentials.token_url = "https://idp.example.com/token".into();
        config.client_credentials.client_id = "service".into();
        config.client_credentials.client_secret = "shh".into();

        let endpoint = config
            .token_endpoint(FlowKind::ClientCredentials)
            .expect("expected client credentials token endpoint");
        assert_eq!(endpoint.client_secret.as_deref(), Some("shh"));
    }

    #[test]
    fn token_endpoint_extracts_device_code_fields() {
        let mut config = OAuthConfig::default();
        config.device_code.token_url = "https://idp.example.com/token".into();
        config.device_code.client_id = "device".into();

        let endpoint = config
            .token_endpoint(FlowKind::DeviceCode)
            .expect("expected device code token endpoint");
        assert_eq!(endpoint.token_url, "https://idp.example.com/token");
        assert_eq!(endpoint.client_id, "device");
        assert!(endpoint.client_secret.is_none());
    }

    #[test]
    fn injection_config_defaults_to_authorization_bearer_enabled() {
        let injection = InjectionConfig::default();
        assert!(injection.enabled);
        assert_eq!(injection.header_name, "Authorization");
        assert_eq!(injection.header_prefix, "Bearer");
    }

    #[test]
    fn injection_config_formats_bearer_value() {
        let injection = InjectionConfig::default();
        assert_eq!(injection.format_header_value("atk"), "Bearer atk");
        assert_eq!(injection.effective_header_name(), "Authorization");
    }

    #[test]
    fn injection_config_empty_prefix_is_raw_token() {
        let injection = InjectionConfig {
            enabled: true,
            header_name: "X-API-Key".into(),
            header_prefix: "".into(),
        };
        assert_eq!(injection.format_header_value("atk"), "atk");
        assert_eq!(injection.effective_header_name(), "X-API-Key");
    }

    #[test]
    fn injection_config_empty_header_name_falls_back_to_authorization() {
        let injection = InjectionConfig {
            enabled: true,
            header_name: "   ".into(),
            header_prefix: "Bearer".into(),
        };
        assert_eq!(injection.effective_header_name(), "Authorization");
    }

    #[test]
    fn oauth_config_default_includes_injection_defaults() {
        let config = OAuthConfig::default();
        assert!(config.injection.enabled);
        assert_eq!(config.injection.header_name, "Authorization");
        assert_eq!(config.injection.header_prefix, "Bearer");
    }

    #[test]
    fn legacy_oauth_config_without_injection_deserializes_with_defaults() {
        let json = r#"{"active_flow":"auth_code_pkce"}"#;
        let config: OAuthConfig = serde_json::from_str(json).unwrap();
        assert!(config.injection.enabled);
        assert_eq!(config.injection.header_name, "Authorization");
        assert_eq!(config.injection.header_prefix, "Bearer");
    }
}
