//! Fallback service backend for platforms without a supported service manager.

use crate::config::Config;

pub(super) fn install(_config: &Config) -> Result<String, String> {
    Err(
        "Automatic service installation is not supported on this platform. \
         Run `downloadification daemon` from your own startup mechanism instead."
            .to_string(),
    )
}

pub(super) fn uninstall() -> Result<String, String> {
    Err("Automatic service removal is not supported on this platform.".to_string())
}

pub(super) fn is_installed() -> bool {
    false
}

pub(super) fn is_autostart_enabled() -> bool {
    false
}

pub(super) fn set_autostart(_enabled: bool) -> Result<String, String> {
    Err("Auto-start is not supported on this platform.".to_string())
}

pub(super) fn detail() -> String {
    "unsupported platform".to_string()
}
