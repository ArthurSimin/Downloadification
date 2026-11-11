//! A tiny `log` facade that keeps the most recent messages in memory and,
//! optionally, appends them to a log file.
//!
//! The GUI polls [`recent`] to display a live activity feed, while the CLI
//! enables `console` mode so the same messages are printed to the terminal.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Rotate the log file once it grows past this size.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

struct Inner {
    lines: Mutex<VecDeque<String>>,
    cap: usize,
    console: bool,
    file: Mutex<Option<File>>,
    file_path: Option<PathBuf>,
    file_bytes: AtomicU64,
}

static LOGGER: OnceLock<Inner> = OnceLock::new();
static INITIALISED: OnceLock<()> = OnceLock::new();

/// Initialise the global logger. Safe to call more than once; only the first
/// call has any effect. `file` opens (and rotates) the append-only log.
pub fn init(console: bool, level: LevelFilter, file: Option<PathBuf>) {
    if INITIALISED.set(()).is_err() {
        return;
    }

    let file_path = file;
    let handle = file_path.clone().and_then(open_log_file);
    let bytes = handle
        .as_ref()
        .and_then(|f| f.metadata().ok())
        .map(|m| m.len())
        .unwrap_or(0);

    let inner = LOGGER.get_or_init(|| Inner {
        lines: Mutex::new(VecDeque::with_capacity(256)),
        cap: 1000,
        console,
        file: Mutex::new(handle),
        file_path,
        file_bytes: AtomicU64::new(bytes),
    });

    // Fails only if a logger is already installed, which the guard prevents.
    let _ = log::set_logger(inner);
    log::set_max_level(level);
}

fn open_log_file(path: PathBuf) -> Option<File> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_LOG_BYTES {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()
}

impl Log for Inner {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format_line(record);

        if self.console {
            match record.level() {
                Level::Error | Level::Warn => eprintln!("{line}"),
                _ => println!("{line}"),
            }
        }

        if let Ok(mut guard) = self.lines.lock() {
            while guard.len() >= self.cap {
                guard.pop_front();
            }
            guard.push_back(line.clone());
        }

        if let Ok(mut file) = self.file.lock() {
            if let Some(handle) = file.as_mut() {
                let _ = writeln!(handle, "{line}");
                let bytes = self
                    .file_bytes
                    .fetch_add(line.len() as u64 + 1, Ordering::Relaxed);
                if bytes > MAX_LOG_BYTES {
                    if let Some(path) = self.file_path.clone() {
                        let _ = handle.flush();
                        if let Some(new) = open_log_file(path) {
                            *handle = new;
                            self.file_bytes.store(0, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }

    fn flush(&self) {}
}

/// Returns the most recent `limit` log lines, oldest first.
pub fn recent(limit: usize) -> Vec<String> {
    let Some(inner) = LOGGER.get() else {
        return Vec::new();
    };
    let Ok(guard) = inner.lines.lock() else {
        return Vec::new();
    };
    let start = guard.len().saturating_sub(limit);
    guard.iter().skip(start).cloned().collect()
}

/// Drop all buffered lines.
pub fn clear() {
    if let Some(inner) = LOGGER.get() {
        if let Ok(mut guard) = inner.lines.lock() {
            guard.clear();
        }
    }
}

/// Read the last `limit` lines from the log file on disk.
pub fn read_log_tail(path: &Path, limit: usize) -> std::io::Result<Vec<String>> {
    let text = std::fs::read_to_string(path)?;
    let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..].to_vec())
}

/// Parse a textual log level (`trace`, `debug`, `info`, `warn`, `error`).
pub fn parse_level(value: &str) -> LevelFilter {
    match value.trim().to_ascii_lowercase().as_str() {
        "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "warn" | "warning" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "off" | "none" => LevelFilter::Off,
        _ => LevelFilter::Info,
    }
}

/// A ready-made filter derived from verbosity flags.
pub fn level_from_flags(verbose: u8, quiet: bool) -> LevelFilter {
    if quiet {
        LevelFilter::Warn
    } else {
        match verbose {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        }
    }
}

fn format_line(record: &Record) -> String {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    format!(
        "[{timestamp}] {:<5} {}",
        level_tag(record.level()),
        record.args()
    )
}

fn level_tag(level: Level) -> &'static str {
    match level {
        Level::Error => "ERROR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    }
}
