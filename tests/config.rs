//! Configuration round-trip tests.

use std::collections::BTreeMap;

use downloadification::config::Config;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "downloadification-config-{}-{name}.toml",
        std::process::id()
    ))
}

#[test]
fn defaults_round_trip_through_toml() {
    let config = Config::default();
    let text = config.to_toml().unwrap();
    let parsed: Config = toml::from_str(&text).unwrap();

    assert_eq!(parsed.watch, config.watch);
    assert_eq!(parsed.organize_folders, config.organize_folders);
    assert_eq!(parsed.settle_time_ms, config.settle_time_ms);
    assert_eq!(parsed.sweep_interval_secs, config.sweep_interval_secs);
    assert_eq!(parsed.min_file_age_ms, config.min_file_age_ms);
    assert_eq!(parsed.ignore, config.ignore);
}

#[test]
fn save_and_load_preserves_custom_categories() {
    let path = temp_path("custom");
    let _ = std::fs::remove_file(&path);

    let mut categories = BTreeMap::new();
    categories.insert("Images".to_string(), vec!["png".to_string()]);
    categories.insert("Screenshots".to_string(), vec!["shot".to_string()]);

    let config = Config {
        target_dir: "~/Downloads/Test".to_string(),
        dry_run: true,
        settle_time_ms: 1234,
        categories,
        replace_categories: true,
        ..Config::default()
    };

    config.save_to(&path).unwrap();
    let loaded = Config::load_from(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded.target_dir, "~/Downloads/Test");
    assert!(loaded.dry_run);
    assert_eq!(loaded.settle_time_ms, 1234);
    assert!(loaded.replace_categories);
    assert_eq!(
        loaded.categories.get("Screenshots").unwrap(),
        &vec!["shot".to_string()]
    );
}

#[test]
fn partial_config_uses_defaults() {
    let config: Config = toml::from_str("dry_run = true\n").unwrap();
    assert!(config.dry_run);
    assert_eq!(config.folders_category, "Folders");
    assert_eq!(config.unknown_category, "Unknown");
    assert_eq!(config.settle_time_ms, 3000);
    assert!(!config.ignore.is_empty());
}

#[test]
fn the_shipped_template_parses() {
    let config: Config = toml::from_str(downloadification::config::DEFAULT_CONFIG_TEMPLATE)
        .expect("shipped config/config.toml must parse");
    assert_eq!(config.folders_category, "Folders");
    assert!(config.organize_folders);
}
