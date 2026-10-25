//! In-place self-update from GitHub Releases.
//!
//! [`Updater`] is the state machine `app.rs` drives: it starts the release
//! check (automatically at most once a day in release builds, or on demand),
//! installs a verified binary over the running one, and restarts Probe on exit.
//!
//! An update is installed only if the release's `SHA256SUMS` carries a valid
//! signature from the maintainer's offline release key ([`signature`]) and the
//! binary matches it, so publishing a release on GitHub is not enough.

mod fetch;
mod install;
pub mod release;
mod signature;

use std::path::PathBuf;
use std::sync::mpsc;

use fetch::CheckOutcome;
pub use release::Release;

/// The version this binary reports. The release workflow sets `PROBE_VERSION`
/// from the git tag at compile time, so `Cargo.toml` never needs bumping.
/// Local builds don't have it and are marked `-dev`, which never parses as a
/// release version.
pub const VERSION: &str = match option_env!("PROBE_VERSION") {
    Some(version) => version,
    None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
};

/// Only release-workflow builds check by themselves or install: a local
/// `cargo run` / `cargo build --release` must not replace its own binary
/// with a downloaded one.
const IS_DEV_BUILD: bool = option_env!("PROBE_VERSION").is_none() || cfg!(debug_assertions);

#[derive(Debug)]
pub enum UpdateError {
    Http(String),
    Integrity(String),
    Install(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Http(s) => write!(f, "network error: {s}"),
            UpdateError::Integrity(s) => write!(f, "verification failed: {s}"),
            UpdateError::Install(s) => write!(f, "install failed: {s}"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// Where the update flow currently stands; read by the status bar badge and
/// the update dialog.
pub enum UpdatePhase {
    Idle,
    Checking {
        manual: bool,
        rx: mpsc::Receiver<Result<CheckOutcome, UpdateError>>,
    },
    UpToDate,
    Available(Box<Release>),
    Installing {
        release: Box<Release>,
        rx: mpsc::Receiver<Result<(), UpdateError>>,
    },
    Installed {
        version: String,
    },
    Failed {
        message: String,
        release_page: Option<String>,
    },
}

/// What happened since the last [`Updater::poll`], for `app.rs` to report
/// and persist.
#[derive(Debug, PartialEq, Eq)]
pub enum UpdateEvent {
    /// The check completed; `available` is the offered tag, if any.
    Checked {
        manual: bool,
        available: Option<String>,
    },
    CheckFailed {
        manual: bool,
        message: String,
    },
    Installed {
        version: String,
    },
    InstallFailed {
        message: String,
    },
}

pub struct Updater {
    phase: UpdatePhase,
    /// Captured at startup: once replaced on Linux, `current_exe()` resolves
    /// to `"… (deleted)"`.
    exe: Option<PathBuf>,
    dialog_open: bool,
    restart_requested: bool,
}

impl Updater {
    /// Start the updater, running the automatic check if one is due.
    pub fn new(last_checked_at: Option<i64>, now: i64) -> Self {
        let exe = match std::env::current_exe() {
            Ok(exe) => Some(exe),
            Err(error) => {
                tracing::warn!("self-update disabled: cannot locate executable: {error}");
                None
            }
        };
        if let Some(exe) = &exe {
            install::remove_leftovers(exe);
        }
        let mut updater = Self {
            phase: UpdatePhase::Idle,
            exe,
            dialog_open: false,
            restart_requested: false,
        };
        if !IS_DEV_BUILD && release::check_due(last_checked_at, now) {
            updater.check(false);
        }
        updater
    }

    pub fn phase(&self) -> &UpdatePhase {
        &self.phase
    }

    /// True while a background job is running and the UI should keep polling.
    pub fn is_busy(&self) -> bool {
        matches!(
            self.phase,
            UpdatePhase::Checking { .. } | UpdatePhase::Installing { .. }
        )
    }

    /// Start a release check unless one is running, an install is under way,
    /// or an update is already installed and waiting for a restart.
    pub fn check(&mut self, manual: bool) {
        if self.is_busy() || matches!(self.phase, UpdatePhase::Installed { .. }) {
            return;
        }
        self.phase = UpdatePhase::Checking {
            manual,
            rx: fetch::spawn_check(),
        };
    }

    /// Why *Install* is unavailable for this build, if it is.
    pub fn install_blocker(&self) -> Option<&'static str> {
        if IS_DEV_BUILD {
            Some("Development build — only published releases can update in place.")
        } else if self.exe.is_none() {
            Some("Probe could not locate its own executable.")
        } else {
            None
        }
    }

    /// Download and install the offered release.
    pub fn install(&mut self) {
        if self.install_blocker().is_some() {
            return;
        }
        let (UpdatePhase::Available(release), Some(exe)) = (&self.phase, &self.exe) else {
            return;
        };
        let release = release.clone();
        let rx = fetch::spawn_install((*release).clone(), exe.clone());
        self.phase = UpdatePhase::Installing { release, rx };
    }

    /// Advance the state machine if a background job finished.
    pub fn poll(&mut self) -> Option<UpdateEvent> {
        match &self.phase {
            UpdatePhase::Checking { manual, rx } => {
                let manual = *manual;
                let result = match rx.try_recv() {
                    Ok(result) => result,
                    Err(mpsc::TryRecvError::Empty) => return None,
                    Err(mpsc::TryRecvError::Disconnected) => Err(UpdateError::Http(
                        "update check stopped unexpectedly".to_owned(),
                    )),
                };
                Some(self.finish_check(manual, result))
            }
            UpdatePhase::Installing { release, rx } => {
                let result = match rx.try_recv() {
                    Ok(result) => result,
                    Err(mpsc::TryRecvError::Empty) => return None,
                    Err(mpsc::TryRecvError::Disconnected) => Err(UpdateError::Install(
                        "installer stopped unexpectedly".to_owned(),
                    )),
                };
                let version = release.tag_name.clone();
                let release_page = release.html_url.clone();
                Some(self.finish_install(version, release_page, result))
            }
            _ => None,
        }
    }

    fn finish_check(
        &mut self,
        manual: bool,
        result: Result<CheckOutcome, UpdateError>,
    ) -> UpdateEvent {
        match result {
            Ok(CheckOutcome::UpToDate) => {
                self.phase = UpdatePhase::UpToDate;
                UpdateEvent::Checked {
                    manual,
                    available: None,
                }
            }
            Ok(CheckOutcome::Available(release)) => {
                let available = Some(release.tag_name.clone());
                self.phase = UpdatePhase::Available(release);
                UpdateEvent::Checked { manual, available }
            }
            Err(error) => {
                // A failed check is not an update failure: go back to idle
                // so the badge offers another check.
                self.phase = UpdatePhase::Idle;
                UpdateEvent::CheckFailed {
                    manual,
                    message: error.to_string(),
                }
            }
        }
    }

    fn finish_install(
        &mut self,
        version: String,
        release_page: String,
        result: Result<(), UpdateError>,
    ) -> UpdateEvent {
        match result {
            Ok(()) => {
                self.phase = UpdatePhase::Installed {
                    version: version.clone(),
                };
                UpdateEvent::Installed { version }
            }
            Err(error) => {
                let message = error.to_string();
                self.phase = UpdatePhase::Failed {
                    message: message.clone(),
                    release_page: Some(release_page),
                };
                UpdateEvent::InstallFailed { message }
            }
        }
    }

    pub fn dialog_open(&self) -> bool {
        self.dialog_open
    }

    pub fn open_dialog(&mut self) {
        self.dialog_open = true;
    }

    /// Close the dialog. A failure has been seen once it is dismissed, so go
    /// back to idle and let the badge offer a fresh check.
    pub fn close_dialog(&mut self) {
        self.dialog_open = false;
        if matches!(self.phase, UpdatePhase::Failed { .. }) {
            self.phase = UpdatePhase::Idle;
        }
    }

    /// Ask for Probe to be relaunched when it exits. Only meaningful once
    /// the new binary is in place.
    pub fn request_restart(&mut self) {
        self.restart_requested = matches!(self.phase, UpdatePhase::Installed { .. });
    }

    /// The user backed out of closing (e.g. the unsaved-changes prompt).
    pub fn cancel_restart(&mut self) {
        self.restart_requested = false;
    }

    /// Launch the installed binary with the same arguments and working
    /// directory (so `./data` is the same workspace). Called from `on_exit`.
    pub fn restart_if_requested(&self) {
        if !self.restart_requested {
            return;
        }
        let Some(exe) = &self.exe else {
            return;
        };
        if let Err(error) = std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .spawn()
        {
            tracing::error!("restart after update failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn updater(phase: UpdatePhase) -> Updater {
        Updater {
            phase,
            exe: Some(PathBuf::from("/opt/probe/probe")),
            dialog_open: false,
            restart_requested: false,
        }
    }

    fn sample_release() -> Release {
        Release {
            tag_name: "v9.0.0".to_owned(),
            body: None,
            html_url: "https://github.com/neisep/Probe/releases/tag/v9.0.0".to_owned(),
            assets: Vec::new(),
        }
    }

    fn checking(manual: bool, result: Result<CheckOutcome, UpdateError>) -> Updater {
        let (tx, rx) = mpsc::channel();
        tx.send(result).unwrap();
        updater(UpdatePhase::Checking { manual, rx })
    }

    #[test]
    fn pending_check_reports_nothing() {
        let (_tx, rx) = mpsc::channel();
        let mut updater = updater(UpdatePhase::Checking { manual: true, rx });
        assert_eq!(updater.poll(), None);
        assert!(updater.is_busy());
    }

    #[test]
    fn available_release_is_reported_and_kept() {
        let mut updater = checking(
            false,
            Ok(CheckOutcome::Available(Box::new(sample_release()))),
        );
        assert_eq!(
            updater.poll(),
            Some(UpdateEvent::Checked {
                manual: false,
                available: Some("v9.0.0".to_owned())
            })
        );
        assert!(matches!(updater.phase(), UpdatePhase::Available(r) if r.tag_name == "v9.0.0"));
    }

    #[test]
    fn failed_check_returns_to_idle() {
        let mut updater = checking(true, Err(UpdateError::Http("offline".to_owned())));
        assert_eq!(
            updater.poll(),
            Some(UpdateEvent::CheckFailed {
                manual: true,
                message: "network error: offline".to_owned()
            })
        );
        assert!(matches!(updater.phase(), UpdatePhase::Idle));
    }

    #[test]
    fn crashed_check_thread_is_a_failure_not_a_hang() {
        let (tx, rx) = mpsc::channel::<Result<CheckOutcome, UpdateError>>();
        drop(tx);
        let mut updater = updater(UpdatePhase::Checking { manual: true, rx });
        assert!(matches!(
            updater.poll(),
            Some(UpdateEvent::CheckFailed { .. })
        ));
    }

    #[test]
    fn failed_install_keeps_the_release_page() {
        let (tx, rx) = mpsc::channel();
        tx.send(Err(UpdateError::Integrity("bad hash".to_owned())))
            .unwrap();
        let mut updater = updater(UpdatePhase::Installing {
            release: Box::new(sample_release()),
            rx,
        });
        assert!(matches!(
            updater.poll(),
            Some(UpdateEvent::InstallFailed { .. })
        ));
        assert!(matches!(
            updater.phase(),
            UpdatePhase::Failed { release_page: Some(page), .. } if page.ends_with("v9.0.0")
        ));
    }

    #[test]
    fn local_builds_report_a_dev_version_that_is_never_offered_updates() {
        // Only meaningful without the release workflow's PROBE_VERSION.
        if option_env!("PROBE_VERSION").is_none() {
            assert!(VERSION.ends_with("-dev"), "{VERSION}");
            assert_eq!(release::parse_version(VERSION), None);
        }
    }

    #[test]
    fn dismissing_a_failure_allows_a_new_check() {
        let mut updater = updater(UpdatePhase::Failed {
            message: "bad hash".to_owned(),
            release_page: None,
        });
        updater.open_dialog();
        updater.close_dialog();
        assert!(!updater.dialog_open());
        assert!(matches!(updater.phase(), UpdatePhase::Idle));
    }

    #[test]
    fn restart_is_only_armed_after_an_install() {
        let mut idle = updater(UpdatePhase::Idle);
        idle.request_restart();
        assert!(!idle.restart_requested);

        let mut installed = updater(UpdatePhase::Installed {
            version: "v9.0.0".to_owned(),
        });
        installed.request_restart();
        assert!(installed.restart_requested);
        installed.cancel_restart();
        assert!(!installed.restart_requested);
    }

    #[test]
    fn no_new_check_once_an_update_awaits_restart() {
        let mut updater = updater(UpdatePhase::Installed {
            version: "v9.0.0".to_owned(),
        });
        updater.check(true);
        assert!(matches!(updater.phase(), UpdatePhase::Installed { .. }));
    }
}
