use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;

use crate::oauth::config::slugify_env_id;
use crate::oauth::flows::auth_code::{self, AuthCodeConfig};
use crate::oauth::flows::client_credentials::{self, ClientCredentialsConfig};
use crate::oauth::flows::device_code::{self, DeviceCodeConfig, DeviceCodeEvent};
use crate::oauth::{FlowKind, OAuthConfig, Token, TokenStore, storage, token_store};
use crate::state::AppState;

#[derive(Debug, Clone)]
enum FlowEvent {
    DeviceVerification {
        user_code: String,
        verification_uri: String,
        verification_uri_complete: Option<String>,
    },
    Completed(Token),
    Failed(String),
}

#[derive(Debug, Clone)]
struct DeviceVerification {
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
}

struct OAuthPanelState {
    loaded_env: Option<String>,
    env_id: String,
    config: OAuthConfig,
    last_saved: OAuthConfig,
    in_flight: Option<(FlowKind, mpsc::Receiver<FlowEvent>)>,
    device_verification: Option<DeviceVerification>,
    status_message: Option<String>,
}

impl Default for OAuthPanelState {
    fn default() -> Self {
        Self {
            loaded_env: None,
            env_id: String::new(),
            config: OAuthConfig::default(),
            last_saved: OAuthConfig::default(),
            in_flight: None,
            device_verification: None,
            status_message: None,
        }
    }
}

static PANEL_STATE: OnceLock<Mutex<OAuthPanelState>> = OnceLock::new();

fn panel_state() -> &'static Mutex<OAuthPanelState> {
    PANEL_STATE.get_or_init(|| Mutex::new(OAuthPanelState::default()))
}

pub fn show(ui: &mut egui::Ui, state: &AppState) {
    let env_name = match state.active_environment_name() {
        Some(name) => name.to_owned(),
        None => {
            ui.small("Select an environment to configure OAuth2.");
            return;
        }
    };
    let env_id = slugify_env_id(&env_name);

    let mut panel = panel_state().lock().unwrap_or_else(|e| e.into_inner());

    sync_if_env_changed(&mut panel, &env_name, &env_id);
    poll_flow_events(&mut panel);

    render_flow_selector(ui, &mut panel);
    ui.add_space(4.0);

    match panel.config.active_flow {
        Some(FlowKind::AuthCodePkce) => render_auth_code_section(ui, &mut panel),
        Some(FlowKind::ClientCredentials) => render_client_credentials_section(ui, &mut panel),
        Some(FlowKind::DeviceCode) => render_device_code_section(ui, &mut panel),
        None => {
            ui.small("Pick a flow to configure credentials.");
        }
    }

    if panel.config.active_flow.is_some() {
        ui.add_space(4.0);
        render_injection_section(ui, &mut panel);
    }

    if let Some(message) = panel.status_message.clone() {
        ui.add_space(6.0);
        ui.small(message);
    }

    persist_if_changed(&mut panel);
}

fn sync_if_env_changed(panel: &mut OAuthPanelState, env_name: &str, env_id: &str) {
    if panel.loaded_env.as_deref() == Some(env_name) {
        return;
    }
    let loaded = storage()
        .and_then(|s| s.load_oauth_config(env_id).ok())
        .unwrap_or_default();
    panel.env_id = env_id.to_owned();
    panel.loaded_env = Some(env_name.to_owned());
    panel.config = loaded.clone();
    panel.last_saved = loaded;
    panel.status_message = None;
    panel.in_flight = None;
    panel.device_verification = None;
}

fn persist_if_changed(panel: &mut OAuthPanelState) {
    if panel.config == panel.last_saved {
        return;
    }
    let Some(storage) = storage() else { return };
    match storage.save_oauth_config(&panel.env_id, &panel.config) {
        Ok(()) => panel.last_saved = panel.config.clone(),
        Err(error) => panel.status_message = Some(format!("Save failed: {error}")),
    }
}

fn poll_flow_events(panel: &mut OAuthPanelState) {
    let Some((flow, _)) = panel.in_flight.as_ref() else {
        return;
    };
    let flow = *flow;
    loop {
        let event = {
            let Some((_, rx)) = panel.in_flight.as_ref() else {
                return;
            };
            match rx.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if panel.status_message.is_none() {
                        panel.status_message =
                            Some("Flow worker exited without result.".into());
                    }
                    panel.in_flight = None;
                    panel.device_verification = None;
                    return;
                }
            }
        };
        match event {
            FlowEvent::DeviceVerification {
                user_code,
                verification_uri,
                verification_uri_complete,
            } => {
                panel.device_verification = Some(DeviceVerification {
                    user_code,
                    verification_uri,
                    verification_uri_complete,
                });
                panel.status_message =
                    Some("Enter the code at the verification URL.".into());
            }
            FlowEvent::Completed(token) => {
                let scopes_len = token.scopes.len();
                let result = token_store().put(&panel.env_id, flow.as_str(), &token);
                panel.status_message = Some(match result {
                    Ok(()) => format!("Token acquired ({scopes_len} scopes)."),
                    Err(error) => format!("Token acquired but save failed: {error}"),
                });
                panel.in_flight = None;
                panel.device_verification = None;
            }
            FlowEvent::Failed(error) => {
                panel.status_message = Some(format!("Flow failed: {error}"));
                panel.in_flight = None;
                panel.device_verification = None;
            }
        }
    }
}

fn render_flow_selector(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {
    let label = match panel.config.active_flow {
        Some(FlowKind::AuthCodePkce) => "Authorization Code + PKCE",
        Some(FlowKind::ClientCredentials) => "Client Credentials",
        Some(FlowKind::DeviceCode) => "Device Code",
        None => "— select —",
    };
    ui.horizontal(|ui| {
        ui.label("Flow");
        egui::ComboBox::from_id_salt("oauth_flow_selector")
            .selected_text(label)
            .show_ui(ui, |ui| {
                let mut flow = panel.config.active_flow;
                ui.selectable_value(
                    &mut flow,
                    Some(FlowKind::AuthCodePkce),
                    "Authorization Code + PKCE",
                );
                ui.selectable_value(
                    &mut flow,
                    Some(FlowKind::ClientCredentials),
                    "Client Credentials",
                );
                ui.selectable_value(&mut flow, Some(FlowKind::DeviceCode), "Device Code");
                if flow != panel.config.active_flow {
                    panel.config.active_flow = flow;
                }
            });
    });
}

fn render_auth_code_section(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {
    {
        let fields = &mut panel.config.auth_code;
        single_line(
            ui,
            "Authorize URL",
            &mut fields.auth_url,
            "https://example.com/authorize",
        );
        single_line(
            ui,
            "Token URL",
            &mut fields.token_url,
            "https://example.com/oauth/token",
        );
        single_line(ui, "Client ID", &mut fields.client_id, "");
        single_line(
            ui,
            "Client secret",
            &mut fields.client_secret,
            "public clients can leave blank",
        );
        single_line(
            ui,
            "Scopes",
            &mut fields.scopes,
            "space-separated, e.g. openid profile",
        );

        egui::CollapsingHeader::new("Advanced")
            .id_salt("oauth_auth_code_advanced")
            .default_open(false)
            .show(ui, |ui| {
                single_line(ui, "Audience", &mut fields.audience, "");
                single_line(ui, "Resource", &mut fields.resource, "");
                extra_params_editor(ui, &mut fields.extra_params);
            });
    }

    let snapshot = panel.config.auth_code.clone();
    let ready = !snapshot.auth_url.trim().is_empty()
        && !snapshot.token_url.trim().is_empty()
        && !snapshot.client_id.trim().is_empty();

    let (get_clicked, reset_clicked) =
        render_action_row(ui, panel, FlowKind::AuthCodePkce, ready);
    if get_clicked {
        let config = AuthCodeConfig {
            auth_url: snapshot.auth_url.trim().to_owned(),
            token_url: snapshot.token_url.trim().to_owned(),
            client_id: snapshot.client_id.trim().to_owned(),
            client_secret: normalized_optional(&snapshot.client_secret),
            scopes: snapshot.parsed_scopes(),
            audience: normalized_optional(&snapshot.audience),
            resource: normalized_optional(&snapshot.resource),
            extra_auth_params: snapshot
                .extra_params
                .into_iter()
                .filter(|(k, _)| !k.trim().is_empty())
                .map(|(k, v)| (k.trim().to_owned(), v))
                .collect(),
        };
        panel.in_flight = Some((FlowKind::AuthCodePkce, spawn_auth_code_flow(config)));
        panel.status_message = Some("Opening browser…".into());
    }
    if reset_clicked {
        reset_token(panel, FlowKind::AuthCodePkce);
    }
}

fn render_client_credentials_section(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {
    {
        let fields = &mut panel.config.client_credentials;
        single_line(
            ui,
            "Token URL",
            &mut fields.token_url,
            "https://example.com/oauth/token",
        );
        single_line(ui, "Client ID", &mut fields.client_id, "");
        single_line(ui, "Client secret", &mut fields.client_secret, "");
        single_line(
            ui,
            "Scopes",
            &mut fields.scopes,
            "space-separated (optional)",
        );

        egui::CollapsingHeader::new("Advanced")
            .id_salt("oauth_client_credentials_advanced")
            .default_open(false)
            .show(ui, |ui| {
                single_line(ui, "Audience", &mut fields.audience, "");
                single_line(ui, "Resource", &mut fields.resource, "");
                extra_params_editor(ui, &mut fields.extra_params);
            });
    }

    let snapshot = panel.config.client_credentials.clone();
    let ready = !snapshot.token_url.trim().is_empty()
        && !snapshot.client_id.trim().is_empty()
        && !snapshot.client_secret.trim().is_empty();

    let (get_clicked, reset_clicked) =
        render_action_row(ui, panel, FlowKind::ClientCredentials, ready);
    if get_clicked {
        let config = ClientCredentialsConfig {
            token_url: snapshot.token_url.trim().to_owned(),
            client_id: snapshot.client_id.trim().to_owned(),
            client_secret: snapshot.client_secret.trim().to_owned(),
            scopes: snapshot.parsed_scopes(),
            audience: normalized_optional(&snapshot.audience),
            resource: normalized_optional(&snapshot.resource),
            extra_token_params: snapshot
                .extra_params
                .into_iter()
                .filter(|(k, _)| !k.trim().is_empty())
                .map(|(k, v)| (k.trim().to_owned(), v))
                .collect(),
        };
        panel.in_flight = Some((
            FlowKind::ClientCredentials,
            spawn_client_credentials_flow(config),
        ));
        panel.status_message = Some("Requesting token…".into());
    }
    if reset_clicked {
        reset_token(panel, FlowKind::ClientCredentials);
    }
}

fn render_device_code_section(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {
    {
        let fields = &mut panel.config.device_code;
        single_line(
            ui,
            "Device auth URL",
            &mut fields.device_auth_url,
            "https://example.com/device",
        );
        single_line(
            ui,
            "Token URL",
            &mut fields.token_url,
            "https://example.com/oauth/token",
        );
        single_line(ui, "Client ID", &mut fields.client_id, "");
        single_line(
            ui,
            "Client secret",
            &mut fields.client_secret,
            "public clients can leave blank",
        );
        single_line(
            ui,
            "Scopes",
            &mut fields.scopes,
            "space-separated (optional)",
        );

        egui::CollapsingHeader::new("Advanced")
            .id_salt("oauth_device_code_advanced")
            .default_open(false)
            .show(ui, |ui| {
                single_line(ui, "Audience", &mut fields.audience, "");
                single_line(ui, "Resource", &mut fields.resource, "");
                extra_params_editor(ui, &mut fields.extra_params);
            });
    }

    if let Some(verification) = panel.device_verification.clone() {
        render_device_verification(ui, &verification);
    }

    let snapshot = panel.config.device_code.clone();
    let ready = !snapshot.device_auth_url.trim().is_empty()
        && !snapshot.token_url.trim().is_empty()
        && !snapshot.client_id.trim().is_empty();

    let (get_clicked, reset_clicked) = render_action_row(ui, panel, FlowKind::DeviceCode, ready);
    if get_clicked {
        let config = DeviceCodeConfig {
            device_auth_url: snapshot.device_auth_url.trim().to_owned(),
            token_url: snapshot.token_url.trim().to_owned(),
            client_id: snapshot.client_id.trim().to_owned(),
            client_secret: normalized_optional(&snapshot.client_secret),
            scopes: snapshot.parsed_scopes(),
            audience: normalized_optional(&snapshot.audience),
            resource: normalized_optional(&snapshot.resource),
            extra_token_params: snapshot
                .extra_params
                .into_iter()
                .filter(|(k, _)| !k.trim().is_empty())
                .map(|(k, v)| (k.trim().to_owned(), v))
                .collect(),
        };
        panel.in_flight = Some((FlowKind::DeviceCode, spawn_device_code_flow(config)));
        panel.status_message = Some("Requesting device code…".into());
        panel.device_verification = None;
    }
    if reset_clicked {
        reset_token(panel, FlowKind::DeviceCode);
    }
}

fn render_device_verification(ui: &mut egui::Ui, verification: &DeviceVerification) {
    ui.add_space(8.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.small("Enter this code on your other device:");
            ui.heading(
                egui::RichText::new(&verification.user_code)
                    .monospace()
                    .strong(),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.small("Verification URL:");
                if ui.link(&verification.verification_uri).clicked() {
                    let _ = webbrowser::open(&verification.verification_uri);
                }
            });
            if let Some(complete) = verification.verification_uri_complete.as_deref() {
                if ui.small_button("Open pre-filled verification URL").clicked() {
                    let _ = webbrowser::open(complete);
                }
            }
        });
    });
    ui.add_space(6.0);
}

fn render_action_row(
    ui: &mut egui::Ui,
    panel: &OAuthPanelState,
    flow: FlowKind,
    ready: bool,
) -> (bool, bool) {
    let stored = token_store().get(&panel.env_id, flow.as_str()).ok().flatten();
    let in_flight = panel
        .in_flight
        .as_ref()
        .map(|(f, _)| *f == flow)
        .unwrap_or(false);

    ui.add_space(6.0);
    render_token_pill(ui, stored.as_ref(), in_flight);

    let get_label = if in_flight { "Getting token…" } else { "Get token" };
    ui.horizontal(|ui| {
        let get = ui
            .add_enabled(!in_flight && ready, egui::Button::new(get_label))
            .clicked();
        let reset = ui
            .add_enabled(stored.is_some() && !in_flight, egui::Button::new("Reset token"))
            .clicked();
        (get, reset)
    })
    .inner
}

fn reset_token(panel: &mut OAuthPanelState, flow: FlowKind) {
    match token_store().delete(&panel.env_id, flow.as_str()) {
        Ok(()) => panel.status_message = Some("Token cleared.".into()),
        Err(error) => panel.status_message = Some(format!("Reset failed: {error}")),
    }
}

fn render_token_pill(ui: &mut egui::Ui, token: Option<&Token>, in_flight: bool) {
    ui.horizontal(|ui| {
        ui.small("Status:");
        if in_flight {
            ui.small(
                egui::RichText::new("waiting for token")
                    .color(egui::Color32::from_rgb(244, 180, 0)),
            );
            return;
        }
        let Some(token) = token else {
            ui.small(egui::RichText::new("no token").color(egui::Color32::from_rgb(150, 150, 150)));
            return;
        };
        let now = now_unix();
        if token.is_expired(now) {
            ui.small(egui::RichText::new("expired").color(egui::Color32::from_rgb(219, 68, 55)));
        } else {
            let seconds = token.expires_at.saturating_sub(now);
            ui.small(
                egui::RichText::new(format!("expires in {}", humanize_duration(seconds)))
                    .color(egui::Color32::from_rgb(52, 168, 83)),
            );
        }
    });
}

fn render_injection_section(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {
    egui::CollapsingHeader::new("Header injection")
        .id_salt("oauth_injection")
        .default_open(false)
        .show(ui, |ui| {
            let inj = &mut panel.config.injection;
            ui.horizontal(|ui| {
                ui.checkbox(&mut inj.enabled, "Inject token into requests");
            });
            ui.add_enabled_ui(inj.enabled, |ui| {
                single_line(ui, "Header name", &mut inj.header_name, "Authorization");
                single_line(ui, "Header prefix", &mut inj.header_prefix, "Bearer");
            });
        });
}

fn extra_params_editor(ui: &mut egui::Ui, params: &mut Vec<(String, String)>) {
    ui.add_space(4.0);
    ui.label("Extra params");
    let mut remove_index = None;
    for (index, (key, value)) in params.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(key)
                    .desired_width(140.0)
                    .hint_text("key"),
            );
            ui.add(
                egui::TextEdit::singleline(value)
                    .desired_width(220.0)
                    .hint_text("value"),
            );
            if ui.small_button("✕").clicked() {
                remove_index = Some(index);
            }
        });
    }
    if let Some(index) = remove_index {
        params.remove(index);
    }
    if ui.small_button("+ Add param").clicked() {
        params.push((String::new(), String::new()));
    }
}

fn humanize_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn normalized_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn single_line(ui: &mut egui::Ui, label: &str, buffer: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::TextEdit::singleline(buffer)
                .desired_width(f32::INFINITY)
                .hint_text(hint),
        );
    });
}

fn spawn_auth_code_flow(config: AuthCodeConfig) -> mpsc::Receiver<FlowEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = tx.send(FlowEvent::Failed(format!("runtime: {error}")));
                return;
            }
        };
        let result = runtime.block_on(async { auth_code::run(&config).await });
        let event = match result {
            Ok(token) => FlowEvent::Completed(token),
            Err(error) => FlowEvent::Failed(error.to_string()),
        };
        let _ = tx.send(event);
    });
    rx
}

fn spawn_client_credentials_flow(config: ClientCredentialsConfig) -> mpsc::Receiver<FlowEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = tx.send(FlowEvent::Failed(format!("runtime: {error}")));
                return;
            }
        };
        let result = runtime.block_on(async { client_credentials::run(&config).await });
        let event = match result {
            Ok(token) => FlowEvent::Completed(token),
            Err(error) => FlowEvent::Failed(error.to_string()),
        };
        let _ = tx.send(event);
    });
    rx
}

fn spawn_device_code_flow(config: DeviceCodeConfig) -> mpsc::Receiver<FlowEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = tx.send(FlowEvent::Failed(format!("runtime: {error}")));
                return;
            }
        };
        let tx_closure = tx.clone();
        runtime.block_on(async move {
            device_code::run(&config, move |event| {
                let mapped = match event {
                    DeviceCodeEvent::PendingUser {
                        user_code,
                        verification_uri,
                        verification_uri_complete,
                    } => FlowEvent::DeviceVerification {
                        user_code,
                        verification_uri,
                        verification_uri_complete,
                    },
                    DeviceCodeEvent::Completed(token) => FlowEvent::Completed(token),
                    DeviceCodeEvent::Failed(error) => FlowEvent::Failed(error),
                };
                let _ = tx_closure.send(mapped);
            })
            .await;
        });
    });
    rx
}
