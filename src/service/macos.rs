//! macOS service management: a launchd LaunchAgent.

use std::path::PathBuf;
use std::process::Command;

use log::{info, warn};

use super::quote;
use crate::config::Config;
use crate::paths;

const LABEL: &str = "com.downloadification.agent";

fn launch_agents_dir() -> PathBuf {
    paths::home().join("Library").join("LaunchAgents")
}

fn plist_path() -> PathBuf {
    launch_agents_dir().join(format!("{LABEL}.plist"))
}

fn plist_contents(bin: &str) -> String {
    let log = paths::log_file().to_string_lossy().to_string();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\t<string>{label}</string>\n\
         \t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>{bin}</string>\n\t\t<string>daemon</string>\n\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\
         \t<key>KeepAlive</key>\n\t<true/>\n\
         \t<key>ProcessType</key>\n\t<string>Background</string>\n\
         \t<key>StandardOutPath</key>\n\t<string>{log}</string>\n\
         \t<key>StandardErrorPath</key>\n\t<string>{log}</string>\n\
         </dict>\n\
         </plist>\n",
        label = escape(LABEL),
        bin = escape(bin),
        log = escape(&log)
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn install(_config: &Config) -> Result<String, String> {
    if let Err(err) = crate::config::Config::load_or_create() {
        warn!("Could not create default config: {err}");
    }
    let bin = paths::daemon_binary();
    let bin_str = quote(&bin.to_string_lossy());

    paths::ensure_dir(&launch_agents_dir()).map_err(|e| e.to_string())?;
    let path = plist_path();

    // Replace any previous registration.
    let _ = Command::new("launchctl")
        .arg("unload")
        .arg("-w")
        .arg(&path)
        .output();

    std::fs::write(&path, plist_contents(&bin_str)).map_err(|e| e.to_string())?;

    let output = Command::new("launchctl")
        .arg("load")
        .arg("-w")
        .arg(&path)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "launchctl load failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    info!("Installed launchd agent {LABEL}");
    Ok(format!(
        "Installed launchd LaunchAgent.\n  Plist: {}\n  Binary: {}",
        path.display(),
        bin.display()
    ))
}

pub(super) fn uninstall() -> Result<String, String> {
    let path = plist_path();
    if !path.exists() {
        return Ok("Nothing to remove; the service was not installed.".to_string());
    }
    let _ = Command::new("launchctl")
        .arg("unload")
        .arg("-w")
        .arg(&path)
        .output();
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    Ok(format!("Uninstalled.\n  removed {}", path.display()))
}

pub(super) fn is_installed() -> bool {
    plist_path().exists()
}

pub(super) fn detail() -> String {
    if is_installed() {
        format!("launchd agent at {}", plist_path().display())
    } else {
        "no service installed".to_string()
    }
}
