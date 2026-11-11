//! Install / remove the background daemon as a per-user service.
//!
//! * Linux  — a systemd user unit, falling back to an XDG autostart entry.
//! * macOS  — a launchd LaunchAgent.
//! * Windows — a Scheduled Task registered from XML (no PowerShell needed).
//!
//! "Is it running?" is answered platform-independently by looking at the
//! heartbeat file the daemon writes, so the GUI never has to parse localised
//! service-manager output.

use std::time::Duration;

use crate::config::Config;
use crate::paths;
use crate::watcher;

/// Windows Scheduled Task XML builder (kept platform independent so it can be
/// tested everywhere).
pub mod windows_task;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
#[path = "unsupported.rs"]
mod platform;

/// High level state of the background service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    NotInstalled,
    Installed,
    Running,
}

impl ServiceState {
    pub fn label(self) -> &'static str {
        match self {
            ServiceState::NotInstalled => "Not installed",
            ServiceState::Installed => "Installed (idle)",
            ServiceState::Running => "Running",
        }
    }

    pub fn is_running(self) -> bool {
        self == ServiceState::Running
    }
}

/// Detailed service status for display.
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub state: ServiceState,
    /// True when the service is set to start automatically at boot / login.
    pub autostart: bool,
    pub detail: String,
}

/// Install the daemon for the current user.
pub fn install(config: &Config) -> Result<String, String> {
    let binary = paths::daemon_binary();
    if !binary.exists() {
        return Err(format!(
            "could not find the daemon binary at {}.\nInstall both binaries together \
             (downloadification and downloadification-gui) and try again.",
            binary.display()
        ));
    }
    platform::install(config)
}

/// Remove the daemon for the current user.
pub fn uninstall() -> Result<String, String> {
    platform::uninstall()
}

/// Cheap check for whether the service appears to be installed.
pub fn is_installed() -> bool {
    platform::is_installed()
}

/// True when the service is configured to start automatically at boot / login.
pub fn is_autostart_enabled() -> bool {
    platform::is_autostart_enabled()
}

/// Enable or disable starting the service automatically at boot / login.
///
/// The service must already be installed; this only flips the auto-start flag,
/// so the daemon can be left installed-but-manual when disabled.
pub fn set_autostart(enabled: bool) -> Result<String, String> {
    if !platform::is_installed() {
        return Err("the background service is not installed yet; install it first".to_string());
    }
    platform::set_autostart(enabled)
}

/// Full status, including whether the daemon is currently alive.
pub fn status() -> ServiceInfo {
    let running = watcher::is_running(Duration::from_secs(30));
    let installed = platform::is_installed();
    let state = if running {
        ServiceState::Running
    } else if installed {
        ServiceState::Installed
    } else {
        ServiceState::NotInstalled
    };
    ServiceInfo {
        state,
        autostart: platform::is_autostart_enabled(),
        detail: platform::detail(),
    }
}

/// Whether a daemon is currently alive (heartbeat based).
pub fn is_running() -> bool {
    watcher::is_running(Duration::from_secs(30))
}

/// The most recent heartbeat age, if any.
pub fn heartbeat_age() -> Option<Duration> {
    watcher::heartbeat_age()
}

/// True when `name` is an executable on the `PATH`.
#[allow(dead_code)]
pub(crate) fn command_exists(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            let with_ext = dir.join(format!("{name}.exe"));
            if with_ext.is_file() {
                return true;
            }
        }
    }
    false
}

/// Quote a path for use in a command line / plist / unit file.
#[allow(dead_code)]
pub(crate) fn quote(path: &str) -> String {
    if path.contains(' ') || path.contains('"') {
        format!("\"{}\"", path.replace('"', "\\\""))
    } else {
        path.to_string()
    }
}
