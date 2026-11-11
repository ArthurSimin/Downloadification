//! Graphical control panel for Downloadification.
//!
//! This is the "installer" replacement: one cross-platform window that edits
//! the configuration and installs/removes the background service. All real
//! work is delegated to the shared library.

// Never show a console window when the GUI is launched by double-clicking.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use slint::{Model, ModelRc, SharedString, VecModel};

use downloadification::config::Config;
use downloadification::{logger, organizer, paths, service};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let (initial, load_error) = match Config::load_or_create() {
        Ok(config) => (config, None),
        Err(err) => (
            Config::default(),
            Some(format!("Could not load config: {err}")),
        ),
    };

    logger::init(false, initial.log_level_filter(), Some(paths::log_file()));

    let ui = AppWindow::new()?;
    ui.set_app_version(paths::VERSION.into());

    let config = Rc::new(RefCell::new(initial.clone()));
    apply_config_to_ui(&ui, &initial);

    let model = Rc::new(VecModel::from(rows_from(&initial)));
    ui.set_categories(ModelRc::from(model.clone()));

    if let Some(message) = load_error {
        ui.set_message(message.into());
    } else {
        ui.set_message(format!("Config: {}", paths::config_file().display()).into());
    }

    refresh_service_ui(&ui);
    sync_logs(&ui);

    wire_callbacks(&ui, config.clone(), model.clone());

    ui.run()
}

fn rows_from(config: &Config) -> Vec<CategoryRow> {
    config
        .categories()
        .categories
        .iter()
        .map(|(name, exts)| CategoryRow {
            name: SharedString::from(name.clone()),
            exts: SharedString::from(exts.join(" ")),
        })
        .collect()
}

fn apply_config_to_ui(ui: &AppWindow, config: &Config) {
    ui.set_target_dir(config.target_dir.clone().into());
    ui.set_watch_enabled(config.watch);
    ui.set_dry_run(config.dry_run);
    ui.set_organize_folders(config.organize_folders);
    ui.set_settle_ms(clamp_i32(config.settle_time_ms));
    ui.set_sweep_secs(clamp_i32(config.sweep_interval_secs));
    ui.set_min_age_ms(clamp_i32(config.min_file_age_ms));
}

fn clamp_i32(value: u64) -> i32 {
    value.min(i32::MAX as u64) as i32
}

fn collect_config(ui: &AppWindow, base: &Config, model: &VecModel<CategoryRow>) -> Config {
    let mut config = base.clone();
    config.target_dir = ui.get_target_dir().to_string();
    config.watch = ui.get_watch_enabled();
    config.dry_run = ui.get_dry_run();
    config.organize_folders = ui.get_organize_folders();
    config.settle_time_ms = ui.get_settle_ms().max(0) as u64;
    config.sweep_interval_secs = ui.get_sweep_secs().max(5) as u64;
    config.min_file_age_ms = ui.get_min_age_ms().max(0) as u64;

    let mut categories = BTreeMap::new();
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        let name = row.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let exts: Vec<String> = row
            .exts
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .map(|ext| ext.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|ext| !ext.is_empty())
            .collect();
        if !exts.is_empty() {
            categories.insert(name, exts);
        }
    }

    config.categories = categories;
    // The panel shows the full resolved list, so saving must replace the
    // built-ins — otherwise deleting a category would have no effect.
    config.replace_categories = true;
    config
}

fn save_from_ui(
    ui: &AppWindow,
    config: &Rc<RefCell<Config>>,
    model: &Rc<VecModel<CategoryRow>>,
) -> Result<Config, String> {
    let collected = collect_config(ui, &config.borrow(), model);
    collected.save().map_err(|err| err.to_string())?;
    *config.borrow_mut() = collected.clone();
    Ok(collected)
}

fn refresh_service_ui(ui: &AppWindow) {
    let info = service::status();
    ui.set_service_running(info.state.is_running());
    ui.set_service_installed(service::is_installed());
    ui.set_service_text(format!("{} — {}", info.state.label(), info.detail).into());
}

fn sync_logs(ui: &AppWindow) {
    let lines: Vec<SharedString> = logger::recent(400)
        .into_iter()
        .map(SharedString::from)
        .collect();
    ui.set_log_lines(ModelRc::from(Rc::new(VecModel::from(lines))));
}

fn wire_callbacks(ui: &AppWindow, config: Rc<RefCell<Config>>, model: Rc<VecModel<CategoryRow>>) {
    // ---------------------------------------------------------------- browsing
    ui.on_browse_target({
        let ui_weak = ui.as_weak();
        let config = config.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let mut dialog = rfd::FileDialog::new().set_title("Choose the folder to organize");
            let current = config.borrow().target_path();
            if current.is_dir() {
                dialog = dialog.set_directory(&current);
            }
            if let Some(folder) = dialog.pick_folder() {
                let text = paths::contract_tilde(&folder);
                ui.set_target_dir(text.clone().into());
                config.borrow_mut().target_dir = text;
            }
        }
    });

    // ------------------------------------------------------------------- save
    ui.on_save_config({
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let model = model.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            match save_from_ui(&ui, &config, &model) {
                Ok(_) => {
                    ui.set_message(format!("Saved to {}", paths::config_file().display()).into())
                }
                Err(err) => ui.set_message(format!("Save failed: {err}").into()),
            }
        }
    });

    // --------------------------------------------------------------- organize
    ui.on_organize_now({
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let model = model.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let cfg = match save_from_ui(&ui, &config, &model) {
                Ok(cfg) => cfg,
                Err(err) => {
                    ui.set_message(format!("Save failed: {err}").into());
                    return;
                }
            };

            ui.set_busy(true);
            ui.set_message("Organizing…".into());
            let weak = ui_weak.clone();
            std::thread::spawn(move || {
                let outcome = organizer::organize_once(&cfg);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_busy(false);
                        match outcome {
                            Ok(report) => {
                                ui.set_message(report.summary().into());
                            }
                            Err(err) => {
                                ui.set_message(format!("Error: {err}").into());
                            }
                        }
                    }
                });
            });
        }
    });

    // ---------------------------------------------------------------- service
    ui.on_install_service({
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let model = model.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let cfg = match save_from_ui(&ui, &config, &model) {
                Ok(cfg) => cfg,
                Err(err) => {
                    ui.set_message(format!("Save failed: {err}").into());
                    return;
                }
            };
            match service::install(&cfg) {
                Ok(message) => ui.set_message(message.into()),
                Err(err) => ui.set_message(format!("Install failed: {err}").into()),
            }
            refresh_service_ui(&ui);
        }
    });

    ui.on_uninstall_service({
        let ui_weak = ui.as_weak();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            match service::uninstall() {
                Ok(message) => ui.set_message(message.into()),
                Err(err) => ui.set_message(format!("Uninstall failed: {err}").into()),
            }
            refresh_service_ui(&ui);
        }
    });

    // ------------------------------------------------------------------ files
    ui.on_open_config({
        let ui_weak = ui.as_weak();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                match paths::reveal_path(&paths::config_file()) {
                    Ok(()) => ui.set_message("Opened the config folder.".into()),
                    Err(err) => ui.set_message(format!("Could not open folder: {err}").into()),
                }
            }
        }
    });

    ui.on_open_log({
        let ui_weak = ui.as_weak();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                let log = paths::log_file();
                if !log.exists() {
                    ui.set_message("No log file yet — run the organizer once.".into());
                    return;
                }
                match paths::open_path(&log) {
                    Ok(()) => ui.set_message("Opened the log file.".into()),
                    Err(err) => ui.set_message(format!("Could not open log: {err}").into()),
                }
            }
        }
    });

    // -------------------------------------------------------------- categories
    ui.on_add_category({
        let model = model.clone();
        move || {
            model.push(CategoryRow {
                name: SharedString::from("New category"),
                exts: SharedString::from(""),
            });
        }
    });

    ui.on_remove_category({
        let model = model.clone();
        move |index| {
            let index = index.max(0) as usize;
            if index < model.row_count() {
                model.remove(index);
            }
        }
    });

    ui.on_rename_category({
        let model = model.clone();
        move |index, value| {
            let index = index.max(0) as usize;
            if let Some(mut row) = model.row_data(index) {
                row.name = value;
                model.set_row_data(index, row);
            }
        }
    });

    ui.on_edit_category_exts({
        let model = model.clone();
        move |index, value| {
            let index = index.max(0) as usize;
            if let Some(mut row) = model.row_data(index) {
                row.exts = value;
                model.set_row_data(index, row);
            }
        }
    });

    ui.on_reset_defaults({
        let ui_weak = ui.as_weak();
        let config = config.clone();
        let model = model.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let mut cfg = config.borrow().clone();
            cfg.replace_categories = false;
            cfg.categories.clear();
            *config.borrow_mut() = cfg.clone();
            model.set_vec(rows_from(&cfg));
            if let Err(err) = cfg.save() {
                ui.set_message(format!("Could not save defaults: {err}").into());
            } else {
                ui.set_message("Restored the built-in category defaults.".into());
            }
        }
    });

    // -------------------------------------------------------------------- poll
    ui.on_poll({
        let ui_weak = ui.as_weak();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            sync_logs(&ui);
            ui.set_service_running(service::is_running());
            ui.set_service_installed(service::is_installed());
        }
    });
}
