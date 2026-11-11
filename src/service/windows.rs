//! Windows service management: a per-user Scheduled Task registered from XML.
//!
//! Deliberately avoids the PowerShell cmdlets — the task XML is built in Rust
//! and registered with `schtasks.exe`, which behaves identically on every
//! Windows edition and locale.

use std::path::PathBuf;
use std::process::Command;

use log::{info, warn};

use super::windows_task::{encode_utf16le, task_xml};
use crate::config::Config;
use crate::paths;

const TASK_NAME: &str = "Downloadification";

fn xml_path() -> PathBuf {
    paths::data_dir().join("downloadification-task.xml")
}

fn current_user() -> String {
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let user = std::env::var("USERNAME").unwrap_or_default();
    if domain.is_empty() {
        user
    } else {
        format!("{domain}\\{user}")
    }
}

pub(super) fn install(_config: &Config) -> Result<String, String> {
    if let Err(err) = crate::config::Config::load_or_create() {
        warn!("Could not create default config: {err}");
    }
    let bin = paths::daemon_binary();
    let bin_str = bin.to_string_lossy().to_string();

    paths::ensure_dir(&paths::data_dir()).map_err(|e| e.to_string())?;
    let file = xml_path();
    let xml = task_xml(&bin_str, &current_user(), TASK_NAME);
    std::fs::write(&file, encode_utf16le(&xml)).map_err(|e| e.to_string())?;

    let output = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/XML",
            &file.to_string_lossy(),
            "/F",
        ])
        .output()
        .map_err(|e| format!("could not run schtasks: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "schtasks failed: {} {}",
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    // Start it right away so the user does not have to log out and back in.
    let _ = Command::new("schtasks")
        .args(["/Run", "/TN", TASK_NAME])
        .output();

    mark_installed(true);
    // A fresh install is always auto-start enabled.
    set_disabled_marker(false);
    info!("Installed scheduled task {TASK_NAME}");

    Ok(format!(
        "Installed Windows Scheduled Task \"{TASK_NAME}\".\n  Binary: {}\n  Task XML: {}\n  It runs automatically at logon (and was started now).",
        bin.display(),
        file.display()
    ))
}

pub(super) fn uninstall() -> Result<String, String> {
    let output = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .output()
        .map_err(|e| format!("could not run schtasks: {e}"))?;

    mark_installed(false);
    set_disabled_marker(false);
    let _ = std::fs::remove_file(xml_path());

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Ok("Nothing to remove; the task was not installed.".to_string());
        }
        return Ok(format!("Removed (task was not present: {stderr})"));
    }

    Ok(format!("Uninstalled scheduled task \"{TASK_NAME}\"."))
}

pub(super) fn is_installed() -> bool {
    paths::service_marker_file().exists()
}

pub(super) fn detail() -> String {
    if is_installed() {
        format!("Scheduled Task \"{TASK_NAME}\"")
    } else {
        "no service installed".to_string()
    }
}

/// The Scheduled Task XML carries a `<Enabled>true</Enabled>` flag, but reading
/// it back with `schtasks /Query` means parsing localised text. Instead we
/// remember the user's choice in a marker file; auto-start is on by default
/// once the task is installed.
pub(super) fn is_autostart_enabled() -> bool {
    is_installed() && !paths::autostart_disabled_marker_file().exists()
}

pub(super) fn set_autostart(enabled: bool) -> Result<String, String> {
    if !is_installed() {
        return Err("the Scheduled Task is not installed yet".to_string());
    }
    let verb = if enabled { "/ENABLE" } else { "/DISABLE" };
    let output = Command::new("schtasks")
        .args(["/Change", "/TN", TASK_NAME, verb])
        .output()
        .map_err(|e| format!("could not run schtasks: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "schtasks {verb} failed: {} {}",
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    set_disabled_marker(!enabled);
    info!(
        "{} scheduled task {TASK_NAME}",
        if enabled { "Enabled" } else { "Disabled" }
    );
    Ok(if enabled {
        "Auto-start enabled: the task runs at logon.".to_string()
    } else {
        "Auto-start disabled: the task stays registered but will not run at logon.".to_string()
    })
}

fn set_disabled_marker(disabled: bool) {
    let marker = paths::autostart_disabled_marker_file();
    if disabled {
        let _ = paths::ensure_dir(&paths::data_dir());
        let _ = std::fs::write(&marker, b"disabled\n");
    } else {
        let _ = std::fs::remove_file(&marker);
    }
}

fn mark_installed(installed: bool) {
    let marker = paths::service_marker_file();
    if installed {
        let _ = paths::ensure_dir(&paths::data_dir());
        let _ = std::fs::write(&marker, b"installed\n");
    } else {
        let _ = std::fs::remove_file(&marker);
    }
}
