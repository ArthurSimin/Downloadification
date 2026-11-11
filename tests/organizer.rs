//! End-to-end tests for the organizer, driven through the public API.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use downloadification::config::Config;
use downloadification::organizer::Organizer;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "downloadification-test-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn config_for(dir: &Path) -> Config {
    Config {
        target_dir: dir.display().to_string(),
        min_file_age_ms: 0,
        ..Config::default()
    }
}

fn write(dir: &Path, name: &str, contents: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn organize(config: Config) -> downloadification::organizer::OrganizeReport {
    Organizer::new(config).organize().expect("organize pass")
}

#[test]
fn sorts_files_folders_and_skips_partials() {
    let dir = TestDir::new();
    let root = dir.path();

    write(root, "photo.PNG", "img");
    write(root, "backup.tar.gz", "zip");
    write(root, "backup.tar.xz", "zip");
    write(root, "setup.exe", "bin");
    write(root, "theme.mkv", "vid");
    write(root, "mystery.qqq", "?");
    write(root, "song.mp3", "aud");
    write(root, "book.epub", "book");
    write(root, "chip.crdownload", "partial");
    write(root, "vendor.tar.part", "partial");
    write(root, "project/src/main.rs", "code");
    write(root, ".hidden.png", "hidden");

    let report = organize(config_for(root));

    assert!(root.join("Images/photo.PNG").is_file());
    assert!(root.join("Compressed/backup.tar.gz").is_file());
    assert!(root.join("Compressed/backup.tar.xz").is_file());
    assert!(root.join("Applications/Windows/setup.exe").is_file());
    assert!(root.join("Videos/theme.mkv").is_file());
    assert!(root.join("Audio/song.mp3").is_file());
    assert!(root.join("eBooks/book.epub").is_file());
    assert!(root.join("Unknown/mystery.qqq").is_file());
    assert!(root.join("Folders/project/src/main.rs").is_file());

    // In-progress downloads and hidden files stay put.
    assert!(root.join("chip.crdownload").is_file());
    assert!(root.join("vendor.tar.part").is_file());
    assert!(root.join(".hidden.png").is_file());

    assert_eq!(report.files_moved, 8);
    assert_eq!(report.folders_moved, 1);
    assert_eq!(report.errors, 0);
}

#[test]
fn duplicate_names_get_a_counter() {
    let dir = TestDir::new();
    let root = dir.path();

    write(root, "pic.png", "one");
    organize(config_for(root));
    write(root, "pic.png", "two");
    organize(config_for(root));
    write(root, "pic.png", "three");
    organize(config_for(root));

    assert!(root.join("Images/pic.png").is_file());
    assert!(root.join("Images/pic (1).png").is_file());
    assert!(root.join("Images/pic (2).png").is_file());

    // Multi-dot extensions keep their full suffix when renaming.
    write(root, "arc.tar.gz", "one");
    organize(config_for(root));
    write(root, "arc.tar.gz", "two");
    organize(config_for(root));
    write(root, "arc.tar.gz", "three");
    organize(config_for(root));
    assert!(root.join("Compressed/arc.tar.gz").is_file());
    assert!(root.join("Compressed/arc (1).tar.gz").is_file());
    assert!(root.join("Compressed/arc (2).tar.gz").is_file());
}

#[test]
fn repeat_runs_are_idempotent() {
    let dir = TestDir::new();
    let root = dir.path();
    write(root, "a.png", "x");
    write(root, "b.zip", "x");
    write(root, "folder/child.txt", "x");

    let first = organize(config_for(root));
    assert_eq!(first.total_moved(), 3);

    // Category folders themselves must never be treated as loose folders.
    let second = organize(config_for(root));
    assert_eq!(second.total_moved(), 0);
    assert_eq!(second.errors, 0);
}

#[test]
fn dry_run_changes_nothing() {
    let dir = TestDir::new();
    let root = dir.path();
    write(root, "a.png", "x");
    write(root, "folder/child.txt", "x");

    let mut config = config_for(root);
    config.dry_run = true;
    let report = organize(config);

    assert!(report.dry_run);
    assert_eq!(report.files_moved, 1);
    assert_eq!(report.folders_moved, 1);
    assert!(root.join("a.png").is_file());
    assert!(root.join("folder/child.txt").is_file());
    assert!(!root.join("Images").exists());
    assert!(!root.join("Folders").exists());
}

#[test]
fn category_overrides_and_disable_work() {
    let dir = TestDir::new();
    let root = dir.path();

    let mut config = config_for(root);
    config
        .categories
        .insert("Images".to_string(), vec!["foo".to_string()]);
    config.disable_categories.push("Videos".to_string());
    write(root, "a.foo", "x");
    write(root, "b.png", "x");
    write(root, "c.mkv", "x");

    organize(config);

    assert!(root.join("Images/a.foo").is_file());
    // Images was overridden, so png is no longer recognized.
    assert!(root.join("Unknown/b.png").is_file());
    // Videos was disabled, so mkv is unknown too.
    assert!(root.join("Unknown/c.mkv").is_file());
}

#[test]
fn organize_folders_can_be_disabled() {
    let dir = TestDir::new();
    let root = dir.path();
    write(root, "project/file.txt", "x");

    let mut config = config_for(root);
    config.organize_folders = false;
    organize(config);

    assert!(root.join("project/file.txt").is_file());
    assert!(!root.join("Folders").exists());
}

#[test]
fn missing_target_is_reported() {
    let config = Config {
        target_dir: "/nonexistent/downloadification-test".to_string(),
        ..Config::default()
    };
    let result = Organizer::new(config).organize();
    assert!(result.is_err());
}
