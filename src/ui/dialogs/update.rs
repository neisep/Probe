use eframe::egui;

use crate::ui::theme;
use crate::update::UpdatePhase;

pub enum UpdateDialogAction {
    None,
    Close,
    Install,
    Restart,
    OpenReleasePage(String),
}

/// The update window. `install_blocker` explains why *Install* is disabled
/// for this build, if it is.
pub fn show(
    ctx: &egui::Context,
    phase: &UpdatePhase,
    install_blocker: Option<&str>,
) -> UpdateDialogAction {
    let mut action = UpdateDialogAction::None;
    let current = concat!("v", env!("CARGO_PKG_VERSION"));

    egui::Window::new("Update Probe")
        .collapsible(false)
        .resizable(true)
        .default_width(480.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| match phase {
            UpdatePhase::Available(release) => {
                ui.label(
                    egui::RichText::new(format!("Probe {} is available", release.tag_name))
                        .strong(),
                );
                ui.small(format!("You have {current}."));
                ui.add_space(6.0);
                let notes = release.body.as_deref().unwrap_or("").trim();
                if !notes.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(notes).monospace().small());
                        });
                    ui.add_space(6.0);
                }
                ui.horizontal(|ui| {
                    let install = ui
                        .add_enabled(install_blocker.is_none(), egui::Button::new("Install"))
                        .on_hover_text("Download, verify, and replace this copy of Probe");
                    if let Some(reason) = install_blocker {
                        install.on_disabled_hover_text(reason);
                    } else if install.clicked() {
                        action = UpdateDialogAction::Install;
                    }
                    if ui.button("Release page").clicked() {
                        action = UpdateDialogAction::OpenReleasePage(release.html_url.clone());
                    }
                    if ui.button("Later").clicked() {
                        action = UpdateDialogAction::Close;
                    }
                });
            }
            UpdatePhase::Installing { release, .. } => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Downloading and verifying {}…", release.tag_name));
                });
                ui.add_space(6.0);
                if ui.button("Hide").clicked() {
                    action = UpdateDialogAction::Close;
                }
            }
            UpdatePhase::Installed { version } => {
                ui.label(egui::RichText::new(format!("Probe {version} is installed")).strong());
                ui.small("Restart Probe to start using it.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Restart now").clicked() {
                        action = UpdateDialogAction::Restart;
                    }
                    if ui.button("Later").clicked() {
                        action = UpdateDialogAction::Close;
                    }
                });
            }
            UpdatePhase::Failed {
                message,
                release_page,
            } => {
                ui.label(
                    egui::RichText::new("Update failed")
                        .color(theme::DANGER)
                        .strong(),
                );
                ui.label(message);
                ui.small("Your current version of Probe is unchanged.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if let Some(page) = release_page
                        && ui.button("Release page").clicked()
                    {
                        action = UpdateDialogAction::OpenReleasePage(page.clone());
                    }
                    if ui.button("Close").clicked() {
                        action = UpdateDialogAction::Close;
                    }
                });
            }
            // Nothing to show; the badge only opens the dialog in the
            // phases above.
            UpdatePhase::Idle | UpdatePhase::Checking { .. } | UpdatePhase::UpToDate => {
                action = UpdateDialogAction::Close;
            }
        });

    action
}

pub enum UpdateBadgeAction {
    None,
    Check,
    OpenDialog,
}

/// The version label at the left of the status bar. It doubles as the
/// manual "check for updates" control and as the notice when an update is
/// available or installed.
pub fn badge(ui: &mut egui::Ui, phase: &UpdatePhase) -> UpdateBadgeAction {
    let current = concat!("v", env!("CARGO_PKG_VERSION"));
    let (text, color, hover, on_click) = match phase {
        UpdatePhase::Checking { .. } => (
            "Checking for updates…".to_owned(),
            theme::TEXT_MUTED,
            None,
            UpdateBadgeAction::None,
        ),
        UpdatePhase::Available(release) => (
            format!("Update available: {}", release.tag_name),
            theme::ACCENT_STRONG,
            Some("Show what's new and install"),
            UpdateBadgeAction::OpenDialog,
        ),
        UpdatePhase::Installing { release, .. } => (
            format!("Installing {}…", release.tag_name),
            theme::TEXT_MUTED,
            None,
            UpdateBadgeAction::OpenDialog,
        ),
        UpdatePhase::Installed { version } => (
            format!("Restart to use {version}"),
            theme::ACCENT_STRONG,
            Some("Restart Probe to finish updating"),
            UpdateBadgeAction::OpenDialog,
        ),
        UpdatePhase::Failed { .. } => (
            "Update failed".to_owned(),
            theme::DANGER,
            Some("Show details"),
            UpdateBadgeAction::OpenDialog,
        ),
        UpdatePhase::Idle | UpdatePhase::UpToDate => (
            current.to_owned(),
            theme::TEXT_MUTED,
            Some("Check for updates"),
            UpdateBadgeAction::Check,
        ),
    };

    let clickable = !matches!(on_click, UpdateBadgeAction::None);
    let mut response = ui.add(
        egui::Label::new(egui::RichText::new(text).color(color).small())
            .selectable(false)
            .sense(egui::Sense::click()),
    );
    if clickable {
        response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    let response = match hover {
        Some(hover) => response.on_hover_text(hover),
        None => response,
    };
    if response.clicked() {
        on_click
    } else {
        UpdateBadgeAction::None
    }
}
