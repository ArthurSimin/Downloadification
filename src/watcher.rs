//! Long-running daemon: watches the target directory and organizes on change,
//! with a periodic sweep as a safety net so nothing is ever "forgotten".

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use notify::{Event, RecursiveMode, Watcher};

use crate::config::Config;
use crate::organizer::Organizer;
use crate::paths;

/// Path of the heartbeat file.
pub fn heartbeat_path() -> PathBuf {
    paths::heartbeat_file()
}

/// Record that the daemon is alive right now.
pub fn write_heartbeat() {
    let path = paths::heartbeat_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let stamp = chrono::Local::now().to_rfc3339();
    let _ = std::fs::write(path, format!("{}\n{}\n", std::process::id(), stamp));
}

/// Age of the heartbeat, or `None` if there is no daemon.
pub fn heartbeat_age() -> Option<Duration> {
    let metadata = std::fs::metadata(paths::heartbeat_file()).ok()?;
    let modified = metadata.modified().ok()?;
    std::time::SystemTime::now().duration_since(modified).ok()
}

/// True when a daemon wrote a heartbeat within `max_age`.
pub fn is_running(max_age: Duration) -> bool {
    heartbeat_age().map(|age| age <= max_age).unwrap_or(false)
}

/// Remove the heartbeat file (called on a clean shutdown).
pub fn clear_heartbeat() {
    let _ = std::fs::remove_file(paths::heartbeat_file());
}

/// Run the daemon until the process is terminated.
pub fn run_daemon(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let organizer = Organizer::new(config.clone());
    info!(
        "Downloadification daemon started (pid {})",
        std::process::id()
    );
    write_heartbeat();

    let target = config.target_path();
    if target.is_dir() {
        run_pass(&organizer);
    } else {
        warn!("Target directory does not exist yet: {}", target.display());
    }

    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        // A closed receiver just means we are shutting down.
        let _ = tx.send(result);
    })?;
    let mut watching = try_watch(&mut watcher, &target);

    let settle = Duration::from_millis(config.settle_time_ms.max(250));
    let sweep = Duration::from_secs(config.sweep_interval_secs.max(5));
    let heartbeat_every = Duration::from_secs(5);

    let mut pending = false;
    let mut last_event = Instant::now();
    let mut last_sweep = Instant::now();
    let mut last_heartbeat = Instant::now();

    loop {
        if watching {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(event)) => {
                    if should_react(&event) {
                        pending = true;
                        last_event = Instant::now();
                    }
                }
                Ok(Err(err)) => warn!("Watcher error: {err}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    error!("Watcher stopped; falling back to periodic scanning");
                    watching = false;
                    pending = true;
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(1000));
            pending = true;
        }

        // React once the directory has been quiet for the settle period.
        if pending && last_event.elapsed() >= settle {
            pending = false;
            run_pass(&organizer);
        }

        // Periodic safety-net sweep.
        if last_sweep.elapsed() >= sweep {
            last_sweep = Instant::now();
            if !watching {
                watching = try_watch(&mut watcher, &target);
            }
            run_pass(&organizer);
        }

        if last_heartbeat.elapsed() >= heartbeat_every {
            last_heartbeat = Instant::now();
            write_heartbeat();
        }
    }
}

fn try_watch(watcher: &mut notify::RecommendedWatcher, target: &Path) -> bool {
    if !target.is_dir() {
        return false;
    }
    match watcher.watch(target, RecursiveMode::NonRecursive) {
        Ok(()) => {
            info!("Watching {}", target.display());
            true
        }
        Err(err) => {
            warn!("Could not watch {}: {err}", target.display());
            false
        }
    }
}

fn should_react(event: &Event) -> bool {
    !event.kind.is_access()
}

fn run_pass(organizer: &Organizer) {
    match organizer.organize() {
        Ok(report) => {
            if report.total_moved() > 0 || report.errors > 0 {
                info!("Organized: {}", report.summary());
            } else {
                debug!("Nothing to organize: {}", report.summary());
            }
        }
        Err(err) => warn!("Organization pass failed: {err}"),
    }
}

/// Run a single pass, used by the CLI `run` command.
pub fn run_once(
    config: &Config,
) -> Result<crate::organizer::OrganizeReport, crate::organizer::OrganizeError> {
    Organizer::new(config.clone()).organize()
}
