//! Configuration model (`config.toml`).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::categories::{self, CategorySet};
use crate::paths;

/// Default configuration file contents, written on first run.
pub const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../config/config.toml");

/// The default glob patterns that identify an in-progress download.
pub fn default_ignore_patterns() -> Vec<String> {
    [
        "*.crdownload",
        "*.part",
        "*.partial",
        "*.download",
        "*.opdownload",
        "*.filepart",
        "*.!ut",
        "*.aria2",
        "*.tmp",
        "*.temp",
        "*.downloading",
        "*.crdownloading",
        "unconfirmed *.crdownload",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Everything the organization engine needs to know.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Directory to organise. Empty means the platform Downloads folder.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target_dir: String,

    /// Watch the target directory continuously (daemon mode).
    pub watch: bool,

    /// Log what would happen without touching any file.
    pub dry_run: bool,

    /// Move loose folders into the folders category.
    pub organize_folders: bool,

    /// Category used for folders.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub folders_category: String,

    /// Category used for files whose type is unknown.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub unknown_category: String,

    /// Skip dot-files (and dot-folders).
    pub ignore_hidden: bool,

    /// Quiet period (milliseconds) before the watcher reacts to changes.
    pub settle_time_ms: u64,

    /// How often, in seconds, the daemon performs a full backup scan.
    pub sweep_interval_secs: u64,

    /// Ignore files modified more recently than this (milliseconds).
    pub min_file_age_ms: u64,

    /// `trace` | `debug` | `info` | `warn` | `error`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub log_level: String,

    /// Extra glob patterns treated as "still downloading".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,

    /// Built-in categories to turn off by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disable_categories: Vec<String>,

    /// When true, the `categories` table *replaces* the built-in set instead of
    /// extending it. The GUI sets this so deletions stick.
    pub replace_categories: bool,

    /// Category name -> extensions. Names may contain `/` for sub-categories.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub categories: BTreeMap<String, Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_dir: String::new(),
            watch: true,
            dry_run: false,
            organize_folders: true,
            folders_category: "Folders".to_string(),
            unknown_category: "Unknown".to_string(),
            ignore_hidden: true,
            settle_time_ms: 3000,
            sweep_interval_secs: 60,
            min_file_age_ms: 1500,
            log_level: "info".to_string(),
            ignore: default_ignore_patterns(),
            disable_categories: Vec::new(),
            replace_categories: false,
            categories: BTreeMap::new(),
        }
    }
}

/// Errors raised while loading or saving configuration.
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
    Serialize(toml::ser::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "{e}"),
            ConfigError::Parse(e) => write!(f, "invalid configuration: {e}"),
            ConfigError::Serialize(e) => write!(f, "failed to serialize configuration: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(value: std::io::Error) -> Self {
        ConfigError::Io(value)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(value: toml::de::Error) -> Self {
        ConfigError::Parse(value)
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(value: toml::ser::Error) -> Self {
        ConfigError::Serialize(value)
    }
}

impl Config {
    /// The effective target directory with `~` expanded.
    pub fn target_path(&self) -> PathBuf {
        paths::expand_tilde(&self.target_dir)
    }

    /// The resolved category set (built-ins merged with the config table).
    pub fn categories(&self) -> CategorySet {
        categories::resolve(self)
    }

    /// Load configuration from an explicit path.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&text)?;
        Ok(config)
    }

    /// Load configuration from the default location, falling back to defaults
    /// when the file does not exist yet.
    pub fn load() -> Result<Self, ConfigError> {
        let path = paths::config_file();
        if path.exists() {
            Self::load_from(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Load from the default location, creating it from the template if needed.
    pub fn load_or_create() -> Result<Self, ConfigError> {
        let path = paths::config_file();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, DEFAULT_CONFIG_TEMPLATE)?;
        }
        Self::load_from(&path)
    }

    /// Serialize to a TOML string.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Save to an explicit path, creating parent directories as needed.
    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml()?)?;
        Ok(())
    }

    /// Save to the default configuration location.
    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to(&paths::config_file())
    }

    /// Resolve the log level, honouring an invalid value by falling back to info.
    pub fn log_level_filter(&self) -> log::LevelFilter {
        crate::logger::parse_level(&self.log_level)
    }
}
