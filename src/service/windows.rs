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

fn mark_installed(installed: bool) {
    let marker = paths::service_marker_file();
    if installed {
        let _ = paths::ensure_dir(&paths::data_dir());
        let _ = std::fs::write(&marker, b"installed\n");
    } else {
        let _ = std::fs::remove_file(&marker);
    }
}
