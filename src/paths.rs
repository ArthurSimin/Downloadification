//! Platform path resolution.
//!
//! Everything that needs to know "where does config live", "where does the log
//! live" or "where is the Downloads folder" goes through this module so the
//! behaviour is identical on Linux, macOS and Windows.

use std::io;
use std::path::{Path, PathBuf};

/// Reverse-DNS-ish application name used for all folders.
pub const APP_NAME: &str = "downloadification";

/// Human readable product name.
pub const PRODUCT_NAME: &str = "Downloadification";

/// Crate version, as compiled in.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The user's home directory (falls back to the current directory).
pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Directory that holds `config.toml`.
///
/// * Linux: `~/.config/downloadification`
/// * macOS: `~/Library/Application Support/downloadification`
/// * Windows: `%APPDATA%\downloadification`
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| home().join(".config"))
        .join(APP_NAME)
}

/// Directory that holds logs and runtime state (heartbeat, markers).
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                home().join("Library/Application Support")
            } else {
                home().join(".local").join("share")
            }
        })
        .join(APP_NAME)
}

/// Full path to the configuration file.
pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// Full path to the human readable log file.
pub fn log_file() -> PathBuf {
    data_dir().join("downloadification.log")
}

/// Heartbeat file written by the daemon; used to detect a running instance
/// without shelling out to a service manager.
pub fn heartbeat_file() -> PathBuf {
    data_dir().join("daemon.heartbeat")
}

/// Marker file written when the background service is installed on a platform
/// where detecting it is otherwise expensive (Windows scheduled tasks).
pub fn service_marker_file() -> PathBuf {
    data_dir().join("service.installed")
}

/// The default Downloads directory for the current user.
pub fn default_target_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| home().join("Downloads"))
}

/// Where `install.sh` places the compiled binaries.
pub fn install_bin_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home)
            .join("Downloadification")
            .join("bin")
    } else {
        home().join(".local").join("bin")
    }
}

/// Expand a leading `~` (and `~/`, `~\`) into the home directory. Empty input
/// falls back to the default Downloads directory.
pub fn expand_tilde(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return default_target_dir();
    }
    if trimmed == "~" {
        return home();
    }
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        return home().join(rest);
    }
    PathBuf::from(trimmed)
}

/// Turn an absolute path back into a `~`-prefixed string when it is inside the
/// home directory, so the config file stays portable between machines.
pub fn contract_tilde(path: &Path) -> String {
    let home = home();
    if let Ok(rel) = path.strip_prefix(&home) {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.is_empty() {
            return "~".to_string();
        }
        return format!("~/{rel}");
    }
    path.to_string_lossy().to_string()
}

/// The executable name of the headless binary for this platform.
pub fn daemon_binary_name() -> &'static str {
    if cfg!(windows) {
        "downloadification.exe"
    } else {
        "downloadification"
    }
}

/// The executable name of the GUI binary for this platform.
pub fn gui_binary_name() -> &'static str {
    if cfg!(windows) {
        "downloadification-gui.exe"
    } else {
        "downloadification-gui"
    }
}

/// Locate the headless `downloadification` binary.
///
/// Preference order:
/// 1. the currently running executable, if it *is* the daemon binary,
/// 2. a sibling of the currently running executable,
/// 3. the install `bin` directory.
///
/// This deliberately never returns the GUI binary, so installing the service
/// from the GUI can never end up running the GUI as a daemon.
pub fn daemon_binary() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let is_daemon = exe
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == daemon_binary_name())
            .unwrap_or(false);
        if is_daemon {
            return exe;
        }
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(daemon_binary_name());
            if candidate.exists() {
                return candidate;
            }
        }
    }
    install_bin_dir().join(daemon_binary_name())
}

/// Locate the GUI binary (used by `downloadification gui`).
pub fn gui_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(gui_binary_name());
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    let candidate = install_bin_dir().join(gui_binary_name());
    candidate.exists().then_some(candidate)
}

/// Create a directory (and parents) if it does not exist yet.
pub fn ensure_dir(path: &Path) -> io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Open a path with the platform's default handler.
pub fn open_path(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("explorer");
        command.arg(path);
        command
    };
    command.spawn().map(|_| ())
}

/// Reveal a file in its containing folder (or open a folder directly).
pub fn reveal_path(path: &Path) -> io::Result<()> {
    let target = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    open_path(&target)
}
