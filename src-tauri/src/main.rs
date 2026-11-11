#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Tidy - PC cleanup & health for Windows.

mod appconfig;
mod audit;
mod cleaner;
mod commands;
mod lockcheck;
mod manifest;
mod quarantine;
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
    let quit = MenuItem::with_id(app, "quit", "Quit Tidy", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &scan, &sep, &quit])?;
    let mut tray = TrayIconBuilder::with_id("tidy-tray")
        .tooltip("Tidy")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "scan" => {
                show_main(app);
                let _ = app.emit("tray-scan", ());
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
    tauri::Builder::default()
        .manage(AppState::new())
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
            commands::safety_restore,
            commands::safety_purge_quarantine,
            commands::check_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tidy");
}
