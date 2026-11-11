//! Headless command line interface and background daemon.

use std::path::PathBuf;
use std::process::Command;

use clap::{ArgAction, Parser, Subcommand};

use downloadification::config::Config;
use downloadification::{logger, organizer, paths, service, watcher};

#[derive(Parser, Debug)]
#[command(
    name = "downloadification",
    version = downloadification::paths::VERSION,
    about = "Automatically organize your Downloads folder",
    long_about = "Downloadification sorts files (and loose folders) in your Downloads folder \
                  into categorized sub-folders.\n\nRun `downloadification` with no command to \
                  organize once, or `downloadification daemon` to keep watching in the \
                  background."
)]
struct Cli {
    /// Use a specific configuration file
    #[arg(short, long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Override the target directory
    #[arg(short, long, global = true, value_name = "DIR")]
    target: Option<String>,

    /// Log what would happen without moving anything
    #[arg(long, global = true)]
    dry_run: bool,

    /// Increase verbosity (-v, -vv)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    /// Only show warnings and errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Machine readable output (status/run)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Organize once and exit (the default)
    Run,
    /// Watch continuously and organize in the background
    Daemon,
    /// Install the background service for the current user
    Install,
    /// Remove the background service
    Uninstall,
    /// Show configuration and service status
    Status,
    /// Diagnose common problems
    Doctor,
    /// Print the configuration file path
    Path,
    /// Show the most recent log lines
    Log {
        /// Number of lines to show
        #[arg(short, long, default_value_t = 40)]
        lines: usize,
    },
    /// Launch the graphical control panel
    Gui,
}

fn main() {
    let mut cli = Cli::parse();
    let mut config = load_config(&cli);
    apply_overrides(&mut config, &cli);

    // Explicit flags win; otherwise honour the configured log level.
    let level = if cli.verbose > 0 || cli.quiet {
        logger::level_from_flags(cli.verbose, cli.quiet)
    } else {
        config.log_level_filter()
    };
    logger::init(true, level, Some(paths::log_file()));

    let command = cli.command.take().unwrap_or(Commands::Run);
    match command {
        Commands::Run => run_once(&config, cli.json),
        Commands::Daemon => run_daemon(config),
        Commands::Install => install(&config),
        Commands::Uninstall => uninstall(),
        Commands::Status => status(&config, cli.json),
        Commands::Doctor => doctor(&config),
        Commands::Path => println!("{}", config_path(&cli).display()),
        Commands::Log { lines } => show_log(lines),
        Commands::Gui => launch_gui(),
    }
}

fn config_path(cli: &Cli) -> PathBuf {
    cli.config.clone().unwrap_or_else(paths::config_file)
}

fn load_config(cli: &Cli) -> Config {
    let result = match &cli.config {
        Some(path) => Config::load_from(path),
        None => Config::load_or_create(),
    };
    match result {
        Ok(config) => config,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(2);
        }
    }
}

fn apply_overrides(config: &mut Config, cli: &Cli) {
    if let Some(target) = &cli.target {
        config.target_dir = target.clone();
    }
    if cli.dry_run {
        config.dry_run = true;
    }
}

fn run_once(config: &Config, json: bool) {
    match organizer::organize_once(config) {
        Ok(report) => {
            if json {
                println!(
                    "{{\"files_moved\":{},\"folders_moved\":{},\"skipped\":{},\"errors\":{},\"dry_run\":{},\"duration_ms\":{}}}",
                    report.files_moved,
                    report.folders_moved,
                    report.skipped,
                    report.errors,
                    report.dry_run,
                    report.duration_ms
                );
            } else {
                println!("{}", report.summary());
                for action in &report.actions {
                    println!("  {action}");
                }
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}

fn run_daemon(config: Config) {
    #[cfg(windows)]
    hide_console();

    if let Err(err) = watcher::run_daemon(config) {
        log::error!("daemon stopped: {err}");
        watcher::clear_heartbeat();
        std::process::exit(1);
    }
    watcher::clear_heartbeat();
}

fn install(config: &Config) {
    match service::install(config) {
        Ok(message) => println!("{message}"),
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}

fn uninstall() {
    match service::uninstall() {
        Ok(message) => println!("{message}"),
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
    watcher::clear_heartbeat();
}

fn status(config: &Config, json: bool) {
    let target = config.target_path();
    let categories = config.categories();
    let info = service::status();
    let heartbeat = service::heartbeat_age();

    if json {
        println!(
            "{{\"version\":\"{}\",\"config\":\"{}\",\"target\":\"{}\",\"target_exists\":{},\"categories\":{},\"service\":\"{}\",\"running\":{},\"dry_run\":{}}}",
            paths::VERSION,
            escape(&config_path_str()),
            escape(&target.display().to_string()),
            target.is_dir(),
            categories.categories.len(),
            info.state.label(),
            info.state.is_running(),
            config.dry_run
        );
        return;
    }

    println!("Downloadification {}", paths::VERSION);
    println!("  Config      : {}", config_path_str());
    println!(
        "  Target      : {}{}",
        target.display(),
        if target.is_dir() { "" } else { "  (missing)" }
    );
    println!("  Categories  : {}", categories.categories.len());
    println!("  Dry-run     : {}", config.dry_run);
    println!("  Service     : {} — {}", info.state.label(), info.detail);
    match heartbeat {
        Some(age) => println!("  Heartbeat   : {}s ago", age.as_secs()),
        None => println!("  Heartbeat   : none (daemon not running)"),
    }
    println!("  Log         : {}", paths::log_file().display());
}

fn doctor(config: &Config) {
    println!("Downloadification {}", paths::VERSION);
    let mut problems = 0usize;

    let config_path = paths::config_file();
    if config_path.exists() {
        println!("  [ok]   config found: {}", config_path.display());
    } else {
        problems += 1;
        println!("  [warn] config missing: {}", config_path.display());
    }

    let target = config.target_path();
    if target.is_dir() {
        println!("  [ok]   target exists: {}", target.display());
        let probe = target.join(".downloadification-write-test");
        match std::fs::write(&probe, b"ok") {
            Ok(()) => {
                let _ = std::fs::remove_file(&probe);
                println!("  [ok]   target is writable");
            }
            Err(err) => {
                problems += 1;
                println!("  [fail] target is not writable: {err}");
            }
        }
    } else {
        problems += 1;
        println!("  [fail] target does not exist: {}", target.display());
    }

    let categories = config.categories();
    if categories.categories.is_empty() {
        problems += 1;
        println!("  [fail] no categories configured");
    } else {
        println!(
            "  [ok]   {} categories configured",
            categories.categories.len()
        );
    }

    let info = service::status();
    println!("  [info] service: {} — {}", info.state.label(), info.detail);
    if info.state.is_running() {
        println!("  [ok]   daemon heartbeat is fresh");
    } else {
        println!("  [info] daemon is not currently running");
    }

    println!("  [info] log: {}", paths::log_file().display());
    if problems == 0 {
        println!("\nNo problems found.");
    } else {
        println!("\n{problems} problem(s) found.");
        std::process::exit(1);
    }
}

fn show_log(lines: usize) {
    let path = paths::log_file();
    match logger::read_log_tail(&path, lines) {
        Ok(tail) => {
            for line in tail {
                println!("{line}");
            }
        }
        Err(err) => {
            eprintln!("could not read {}: {err}", path.display());
            std::process::exit(1);
        }
    }
}

fn launch_gui() {
    let Some(gui) = paths::gui_binary() else {
        eprintln!(
            "error: could not find {}. Build it with `cargo build --release` or install it first.",
            paths::gui_binary_name()
        );
        std::process::exit(1);
    };
    match Command::new(&gui).spawn() {
        Ok(_) => println!("Launched {}", gui.display()),
        Err(err) => {
            eprintln!("error: failed to launch {}: {err}", gui.display());
            std::process::exit(1);
        }
    }
}

fn config_path_str() -> String {
    paths::config_file().display().to_string()
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(windows)]
fn hide_console() {
    use windows_sys::Win32::System::Console::GetConsoleWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    // Safe enough: the console handle is only used to hide our own window.
    unsafe {
        let handle = GetConsoleWindow();
        if !handle.is_null() {
            ShowWindow(handle, SW_HIDE);
        }
    }
}
