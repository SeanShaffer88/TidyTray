#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Tidy - PC cleanup & health for Windows.

mod appconfig;
mod audit;
mod cleaner;
mod commands;
mod lockcheck;
mod manifest;
mod preflight;
mod quarantine;
mod safety;
mod scheduler;
#[cfg(test)]
mod safety_tests;
mod sha256;
mod files;
mod health;
mod processes;
mod safelist;
mod scanner;
mod startup;
mod state;

use std::time::Duration;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use state::AppState;

/// Tray status line text, from the last recorded safety run.
fn safety_status_text() -> String {
    match safety::read_last_run() {
        Some(lr) => format!(
            "Safety cleanup: {} \u{00b7} {} files \u{00b7} {:.1} MB{}",
            lr.outcome,
            lr.files,
            lr.bytes as f64 / 1_000_000.0,
            if lr.dry_run { " (dry run)" } else { "" }
        ),
        None => "Safety cleanup: never run".to_string(),
    }
}

static SAFETY_STATUS_ITEM: std::sync::Mutex<Option<MenuItem<tauri::Wry>>> =
    std::sync::Mutex::new(None);

fn refresh_safety_status_item() {
    if let Ok(guard) = SAFETY_STATUS_ITEM.lock() {
        if let Some(item) = guard.as_ref() {
            let _ = item.set_text(safety_status_text());
        }
    }
}

fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_minimized().unwrap_or(false) {
            let _ = win.unminimize();
        }
        let _ = win.show();
        let _ = win.set_focus();
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Tidy", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "Run Smart Scan", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let safety_status = MenuItem::with_id(app, "safety_status", safety_status_text(), false, None::<&str>)?;
    let safety_clean = MenuItem::with_id(app, "safety_clean", "Run Safety Cleanup", true, None::<&str>)?;
    let safety_pause = MenuItem::with_id(app, "safety_pause", "Pause Safety Cleanup", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Tidy", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &scan, &sep, &safety_status, &safety_clean, &safety_pause, &sep2, &quit],
    )?;
    if let Ok(mut guard) = SAFETY_STATUS_ITEM.lock() {
        *guard = Some(safety_status);
    }
    let mut tray = TrayIconBuilder::with_id("tidy-tray")
        .tooltip("Tidy")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "scan" => {
                show_main(app);
                let _ = app.emit("tray-scan", ());
            }
            "safety_clean" => {
                let handle = app.clone();
                std::thread::spawn(move || {
                    let state = handle.state::<safety::SafetyRuntime>();
                    let runtime = safety::SafetyRuntime {
                        cancel: state.cancel.clone(),
                        running: state.running.clone(),
                    };
                    let _ = safety::run_safety_once(&runtime, true, "tray");
                    refresh_safety_status_item();
                });
            }
            "safety_pause" => {
                // Kill switch (blueprint section 8.1): stop the current run.
                app.state::<safety::SafetyRuntime>().request_cancel();
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Samples CPU per process once a second (sparklines) and refreshes the tray tooltip.
fn spawn_sampler(app: AppHandle) {
    std::thread::Builder::new()
        .name("tidy-sampler".into())
        .spawn(move || {
            let mut tick: u64 = 0;
            loop {
                let state = app.state::<AppState>();
                state.sample();
                if tick.is_multiple_of(15) {
                    if let (Some(tray), Ok(sys)) = (app.tray_by_id("tidy-tray"), state.sys.lock()) {
                        let _ = tray.set_tooltip(Some(health::tray_summary(&sys)));
                    }
                }
                tick += 1;
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("failed to start sampler thread");
}

fn main() {
    // Headless entry points (full CLI parity): the Task Scheduler task and
    // scripts use these; they never open a window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--autonomous-run") {
        std::process::exit(safety::headless_autonomous_run());
    }
    if args.iter().any(|a| a == "--clean-now") {
        std::process::exit(safety::headless_clean_now(args.iter().any(|a| a == "--apply")));
    }
    if args.iter().any(|a| a == "--status") {
        let cfg = appconfig::AppConfig::load_from(&appconfig::config_path());
        let snapshot = serde_json::json!({
            "autonomousEnabled": cfg.autonomous.enabled,
            "dryRunsCompleted": cfg.autonomous.dry_runs_completed,
            "dryRunPhaseComplete": cfg.dry_run_phase_complete(),
            "schedulerInstalled": scheduler::is_installed(),
            "lastRun": safety::read_last_run(),
        });
        println!("{}", serde_json::to_string_pretty(&snapshot).unwrap_or_default());
        return;
    }
    tauri::Builder::default()
        .manage(AppState::new())
        .manage(safety::SafetyRuntime::new())
        .setup(|app| {
            build_tray(app)?;
            spawn_sampler(app.handle().clone());
            let settings = app.state::<AppState>().settings();
            if settings.start_minimized {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.minimize();
                }
            }
            log::info!("Tidy started, dry-run = {}", settings.dry_run);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_overview,
            commands::smart_scan,
            commands::clean,
            commands::list_processes,
            commands::end_process,
            commands::get_memory,
            commands::purge_standby,
            commands::list_drives,
            commands::list_startup,
            commands::get_startup_folder,
            commands::set_startup_enabled,
            commands::find_large_files,
            commands::get_settings,
            commands::save_settings,
            commands::get_safe_list,
            commands::safety_status,
            commands::safety_clean_now,
            commands::safety_cancel,
            commands::safety_set_autonomous,
            commands::safety_set_schedule,
            commands::safety_set_categories,
            commands::safety_restore,
            commands::safety_purge_quarantine,
            commands::check_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tidy");
}
