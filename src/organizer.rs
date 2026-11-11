//! The organization engine: scan a directory and move entries into category
//! folders.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use log::{debug, error, info, warn};

use crate::categories::{resolve, CategorySet};
use crate::config::Config;

/// Outcome of a single organization pass.
#[derive(Debug, Clone, Default)]
pub struct OrganizeReport {
    pub files_moved: usize,
    pub folders_moved: usize,
    pub skipped: usize,
    pub errors: usize,
    pub dry_run: bool,
    pub duration_ms: u128,
    pub actions: Vec<String>,
}

impl OrganizeReport {
    pub fn total_moved(&self) -> usize {
        self.files_moved + self.folders_moved
    }

    /// A one line human readable summary.
    pub fn summary(&self) -> String {
        format!(
            "{} moved ({} files, {} folders), {} skipped, {} failed in {} ms{}",
            self.total_moved(),
            self.files_moved,
            self.folders_moved,
            self.skipped,
            self.errors,
            self.duration_ms,
            if self.dry_run { " (dry-run)" } else { "" }
        )
    }
}

/// Errors that can prevent an organization pass from running at all.
#[derive(Debug)]
pub enum OrganizeError {
    MissingTarget(PathBuf),
    Io(io::Error),
}

impl fmt::Display for OrganizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrganizeError::MissingTarget(path) => {
                write!(f, "target directory does not exist: {}", path.display())
            }
            OrganizeError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OrganizeError {}

impl From<io::Error> for OrganizeError {
    fn from(value: io::Error) -> Self {
        OrganizeError::Io(value)
    }
}

/// Reusable organizer. Holds the resolved category table and ignore matcher so
/// repeated passes (daemon mode) are cheap.
pub struct Organizer {
    config: Config,
    categories: CategorySet,
    ignore: GlobSet,
}

impl Organizer {
    pub fn new(config: Config) -> Self {
        let categories = resolve(&config);
        let ignore = build_ignore(&config.ignore);
        Self {
            config,
            categories,
            ignore,
        }
    }

    /// The configuration this organizer was built with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The resolved category set.
    pub fn categories(&self) -> &CategorySet {
        &self.categories
    }

    /// Run a single pass over the target directory.
    pub fn organize(&self) -> Result<OrganizeReport, OrganizeError> {
        let started = Instant::now();
        let mut report = OrganizeReport {
            dry_run: self.config.dry_run,
            ..Default::default()
        };

        let target = self.config.target_path();
        if !target.is_dir() {
            return Err(OrganizeError::MissingTarget(target));
        }

        info!("Organizing {}", target.display());

        let entries = fs::read_dir(&target)?;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    warn!("Could not read an entry in {}: {err}", target.display());
                    report.errors += 1;
                    continue;
                }
            };

            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(err) => {
                    warn!("Could not inspect {:?}: {err}", entry.path());
                    report.errors += 1;
                    continue;
                }
            };

            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() {
                continue;
            }
            if self.config.ignore_hidden && name.starts_with('.') {
                report.skipped += 1;
                continue;
            }

            if file_type.is_dir() {
                self.handle_folder(&entry, &name, &mut report);
            } else {
                self.handle_file(&entry, &name, &mut report);
            }
        }

        report.duration_ms = started.elapsed().as_millis();
        info!("Pass complete: {}", report.summary());
        Ok(report)
    }

    fn handle_folder(&self, entry: &fs::DirEntry, name: &str, report: &mut OrganizeReport) {
        // Never touch the folders we manage ourselves.
        if self.categories.is_managed_folder(name) {
            return;
        }
        if !self.config.organize_folders {
            return;
        }
        match self.move_into(&entry.path(), &self.config.folders_category, true) {
            Ok(moved) => {
                report.folders_moved += 1;
                report.actions.push(format!(
                    "{} {} -> {}/",
                    if moved { "Folder" } else { "[dry-run] Folder" },
                    name,
                    self.config.folders_category
                ));
            }
            Err(err) => {
                error!("Failed to move folder {name}: {err}");
                report.errors += 1;
            }
        }
    }

    fn handle_file(&self, entry: &fs::DirEntry, name: &str, report: &mut OrganizeReport) {
        if self.is_ignored(name) {
            debug!("Skipping in-progress download: {name}");
            report.skipped += 1;
            return;
        }
        if self.is_too_new(entry) {
            debug!("Skipping recently written file: {name}");
            report.skipped += 1;
            return;
        }

        let category = self
            .categories
            .classify(name)
            .unwrap_or_else(|| self.config.unknown_category.clone());

        match self.move_into(&entry.path(), &category, false) {
            Ok(moved) => {
                report.files_moved += 1;
                report.actions.push(format!(
                    "{} {} -> {}/",
                    if moved { "File" } else { "[dry-run] File" },
                    name,
                    category
                ));
            }
            Err(err) => {
                error!("Failed to move {name}: {err}");
                report.errors += 1;
            }
        }
    }

    fn is_ignored(&self, name: &str) -> bool {
        self.ignore.is_match(Path::new(name))
    }

    fn is_too_new(&self, entry: &fs::DirEntry) -> bool {
        if self.config.min_file_age_ms == 0 {
            return false;
        }
        let Ok(metadata) = entry.metadata() else {
            return false;
        };
        let Ok(modified) = metadata.modified() else {
            return false;
        };
        match SystemTime::now().duration_since(modified) {
            Ok(age) => age < Duration::from_millis(self.config.min_file_age_ms),
            // A timestamp in the future means clock skew; treat it as old.
            Err(_) => false,
        }
    }

    /// Move `src` into `category` under the target directory. Returns `true`
    /// when something actually moved, `false` in dry-run mode.
    fn move_into(&self, src: &Path, category: &str, is_dir: bool) -> io::Result<bool> {
        let target = self.config.target_path();
        let dest_dir = if category.is_empty() {
            target.clone()
        } else {
            target.join(category)
        };

        let dest = self.unique_destination(src, &dest_dir, is_dir)?;

        if self.config.dry_run {
            info!(
                "[dry-run] would move {} to {}",
                src.display(),
                dest.display()
            );
            return Ok(false);
        }

        if !dest_dir.exists() {
            fs::create_dir_all(&dest_dir)?;
        }

        move_path(src, &dest)?;
        info!(
            "Moved {} -> {}/",
            src.file_name().unwrap_or_default().to_string_lossy(),
            category
        );
        Ok(true)
    }

    /// Compute a destination path that does not collide with an existing file,
    /// renaming as `name (1).ext` when needed.
    fn unique_destination(&self, src: &Path, dest_dir: &Path, is_dir: bool) -> io::Result<PathBuf> {
        let name = src
            .file_name()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "source path has no file name")
            })?
            .to_string_lossy()
            .to_string();

        let candidate = dest_dir.join(&name);
        if !candidate.exists() {
            return Ok(candidate);
        }

        let (stem, ext) = split_name_ext(&name, is_dir, &self.categories);
        let mut counter: u32 = 1;
        loop {
            let renamed = format!("{stem} ({counter}){ext}");
            let candidate = dest_dir.join(&renamed);
            if !candidate.exists() {
                warn!("Duplicate detected, renaming to {renamed}");
                return Ok(candidate);
            }
            counter += 1;
            if counter > 100_000 {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "too many name collisions",
                ));
            }
        }
    }
}

fn build_ignore(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match GlobBuilder::new(pattern).case_insensitive(true).build() {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(err) => warn!("Ignoring invalid pattern {pattern:?}: {err}"),
        }
    }
    builder.build().unwrap_or_else(|_| GlobSet::empty())
}

/// Split a file name into `(stem, dot_extension)`, honouring multi-dot
/// extensions. Folders never get an extension.
fn split_name_ext(name: &str, is_dir: bool, categories: &CategorySet) -> (String, String) {
    if is_dir {
        return (name.to_string(), String::new());
    }
    if let Some(ext) = categories.extension_of(name) {
        let stem_len = name.len().saturating_sub(ext.len() + 1);
        if stem_len > 0 {
            return (name[..stem_len].to_string(), format!(".{ext}"));
        }
    }
    (name.to_string(), String::new())
}

/// Move a file or directory, falling back to copy + delete when the source and
/// destination live on different filesystems.
fn move_path(src: &Path, dest: &Path) -> io::Result<()> {
    match fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            let metadata = fs::symlink_metadata(src)?;
            if metadata.file_type().is_symlink() {
                // Copying a symlink would follow it; refuse rather than doing
                // something surprising.
                return Err(rename_err);
            }
            if metadata.is_dir() {
                if let Err(err) = copy_dir_all(src, dest) {
                    let _ = fs::remove_dir_all(dest);
                    return Err(err);
                }
                fs::remove_dir_all(src)?;
            } else {
                if let Err(err) = fs::copy(src, dest) {
                    let _ = fs::remove_file(dest);
                    return Err(err);
                }
                fs::remove_file(src)?;
            }
            Ok(())
        }
    }
}

fn copy_dir_all(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let to = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Convenience helper used by the CLI and GUI.
pub fn organize_once(config: &Config) -> Result<OrganizeReport, OrganizeError> {
    Organizer::new(config.clone()).organize()
}
