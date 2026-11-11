//! Linux service management: systemd user unit with an XDG autostart fallback.

use std::path::PathBuf;
use std::process::Command;

use log::{info, warn};

use super::{command_exists, quote};
use crate::config::Config;
use crate::paths;

const UNIT_NAME: &str = "downloadification.service";
const AUTOSTART_NAME: &str = "downloadification.desktop";

fn systemd_user_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| paths::home().join(".config"))
        .join("systemd")
        .join("user")
}

fn autostart_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| paths::home().join(".config"))
        .join("autostart")
}

fn unit_path() -> PathBuf {
    systemd_user_dir().join(UNIT_NAME)
}

fn autostart_path() -> PathBuf {
    autostart_dir().join(AUTOSTART_NAME)
}

fn user_systemd_usable() -> bool {
    if !command_exists("systemctl") {
        return false;
    }
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn unit_contents(bin: &str) -> String {
    format!(
        "[Unit]\n\
         Description=Downloadification - automatic Downloads organizer\n\
         Documentation=https://github.com/ArthurSimin/Downloadification\n\n\
         [Service]\n\
         Type=simple\n\
         ExecStart={bin} daemon\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         # Hardening\n\
         NoNewPrivileges=true\n\
         ProtectSystem=strict\n\
         ProtectHome=read-only\n\
         ReadWritePaths=%h/Downloads %h/.local/share/downloadification\n\
         # If you change target_dir in the config, add its path above.\n\n\
         [Install]\n\
         WantedBy=default.target\n",
        bin = quote(bin)
    )
}

fn autostart_contents(bin: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Downloadification\n\
         Comment=Automatically organize your Downloads folder\n\
         Exec={bin} daemon\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n\
         NoDisplay=false\n",
        bin = quote(bin)
    )
}

pub(super) fn install(_config: &Config) -> Result<String, String> {
    if let Err(err) = crate::config::Config::load_or_create() {
        warn!("Could not create default config: {err}");
    }
    let bin = paths::daemon_binary();
    let bin_str = bin.to_string_lossy().to_string();

    if user_systemd_usable() {
        paths::ensure_dir(&systemd_user_dir()).map_err(|e| e.to_string())?;
        std::fs::write(unit_path(), unit_contents(&bin_str)).map_err(|e| e.to_string())?;

        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();
        let output = Command::new("systemctl")
            .args(["--user", "enable", "--now", UNIT_NAME])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "systemctl enable failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        info!("Installed systemd user unit {UNIT_NAME}");
        return Ok(format!(
            "Installed systemd user service.\n  Unit: {}\n  Binary: {}",
            unit_path().display(),
            bin.display()
        ));
    }

    // Fallback: XDG autostart, works without a systemd user session.
    paths::ensure_dir(&autostart_dir()).map_err(|e| e.to_string())?;
    std::fs::write(autostart_path(), autostart_contents(&bin_str)).map_err(|e| e.to_string())?;
    info!("Installed XDG autostart entry {AUTOSTART_NAME}");
    Ok(format!(
        "systemd user session unavailable; installed an XDG autostart entry instead.\n  Entry: {}\n  Binary: {}\n  Note: it starts on your next login.",
        autostart_path().display(),
        bin.display()
    ))
}

pub(super) fn uninstall() -> Result<String, String> {
    let mut removed = Vec::new();

    if unit_path().exists() {
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", UNIT_NAME])
            .output();
        std::fs::remove_file(unit_path()).map_err(|e| e.to_string())?;
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();
        removed.push(format!("removed {}", unit_path().display()));
    }

    if autostart_path().exists() {
        std::fs::remove_file(autostart_path()).map_err(|e| e.to_string())?;
        removed.push(format!("removed {}", autostart_path().display()));
    }

    if removed.is_empty() {
        return Ok("Nothing to remove; the service was not installed.".to_string());
    }
    Ok(format!("Uninstalled.\n  {}", removed.join("\n  ")))
}

pub(super) fn is_installed() -> bool {
    unit_path().exists() || autostart_path().exists()
}

pub(super) fn detail() -> String {
    if unit_path().exists() {
        format!("systemd user unit at {}", unit_path().display())
    } else if autostart_path().exists() {
        format!("XDG autostart entry at {}", autostart_path().display())
    } else {
        "no service installed".to_string()
    }
}

/// Interpret the single token `systemctl --user is-enabled` prints.
///
/// The tokens are stable English identifiers, so this is not affected by the
/// user's locale (`LANG`), unlike the human readable `status` output.
fn parse_is_enabled(stdout: &str) -> bool {
    matches!(
        stdout.trim(),
        "enabled" | "enabled-runtime" | "linked" | "linked-runtime" | "alias" | "indirect"
    )
}

pub(super) fn is_autostart_enabled() -> bool {
    if unit_path().exists() {
        Command::new("systemctl")
            .args(["--user", "is-enabled", UNIT_NAME])
            .output()
            .map(|out| parse_is_enabled(&String::from_utf8_lossy(&out.stdout)))
            .unwrap_or(false)
    } else {
        // The XDG autostart entry only exists while auto-start is wanted.
        autostart_path().exists()
    }
}

pub(super) fn set_autostart(enabled: bool) -> Result<String, String> {
    if unit_path().exists() {
        let verb = if enabled { "enable" } else { "disable" };
        let output = Command::new("systemctl")
            .args(["--user", verb, UNIT_NAME])
            .output()
            .map_err(|e| format!("could not run systemctl: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "systemctl {verb} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        info!("{verb}d systemd user unit {UNIT_NAME}");
        return Ok(if enabled {
            "Auto-start enabled: the service starts at boot.".to_string()
        } else {
            "Auto-start disabled: the service stays installed but will not start at boot."
                .to_string()
        });
    }

    // XDG autostart fallback: the entry itself is the auto-start mechanism.
    if enabled {
        let bin = paths::daemon_binary();
        let bin_str = bin.to_string_lossy().to_string();
        paths::ensure_dir(&autostart_dir()).map_err(|e| e.to_string())?;
        std::fs::write(autostart_path(), autostart_contents(&bin_str))
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "Auto-start enabled (XDG autostart entry at {}).",
            autostart_path().display()
        ))
    } else if autostart_path().exists() {
        std::fs::remove_file(autostart_path()).map_err(|e| e.to_string())?;
        Ok("Auto-start disabled (removed the XDG autostart entry).".to_string())
    } else {
        Ok("The service is not installed, so there is nothing to disable.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_enabled_token_parsing() {
        assert!(parse_is_enabled("enabled\n"));
        assert!(parse_is_enabled("enabled-runtime"));
        assert!(parse_is_enabled("alias"));
        assert!(!parse_is_enabled("disabled\n"));
        assert!(!parse_is_enabled("masked"));
        assert!(!parse_is_enabled("not-found\n"));
        assert!(!parse_is_enabled(""));
    }

    #[test]
    fn unit_starts_at_boot_and_auto_restarts() {
        let unit = unit_contents("/home/u/.local/bin/downloadification");

        // Enabled at boot via the standard user target.
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("ExecStart=/home/u/.local/bin/downloadification daemon"));
        assert!(unit.contains("Restart=on-failure"));

        // Must NOT be ordered after default.target: combined with
        // WantedBy=default.target that forms an ordering cycle which systemd
        // breaks by deleting the start job, so the service would silently not
        // start at boot.
        assert!(!unit.contains("After=default.target"));
    }

    #[test]
    fn unit_quotes_paths_with_spaces() {
        let unit = unit_contents("/home/a b/bin/downloadification");
        assert!(unit.contains("ExecStart=\"/home/a b/bin/downloadification\" daemon"));
    }

    #[test]
    fn autostart_entry_runs_the_daemon() {
        let entry = autostart_contents("/home/u/.local/bin/downloadification");
        assert!(entry.contains("Exec=/home/u/.local/bin/downloadification daemon"));
        assert!(entry.contains("[Desktop Entry]"));
    }
}
