#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! TidyTray - PC cleanup & health for Windows.

mod appconfig;
mod apps;
mod audit;
mod cleaner;
mod commands;
mod lockcheck;
mod manifest;
mod preflight;
mod quarantine;
mod runlock;
mod repair;
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
mod tier2;
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
    let open = MenuItem::with_id(app, "open", "Open TidyTray", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "Run Smart Scan", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let safety_status = MenuItem::with_id(app, "safety_status", safety_status_text(), false, None::<&str>)?;
    let safety_clean = MenuItem::with_id(app, "safety_clean", "Run Safety Cleanup", true, None::<&str>)?;
    let safety_pause = MenuItem::with_id(app, "safety_pause", "Pause Safety Cleanup", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit TidyTray", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &scan, &sep, &safety_status, &safety_clean, &safety_pause, &sep2, &quit],
    )?;
    if let Ok(mut guard) = SAFETY_STATUS_ITEM.lock() {
        *guard = Some(safety_status);
    }
    let mut tray = TrayIconBuilder::with_id("tidy-tray")
        .tooltip("TidyTray")
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

/// Low-disk trigger (blueprint section 8): when autonomous cleanup is
/// enabled and free space is below the configured threshold, start a
/// gated run (the run itself still passes the pre-flight gate and the
/// dry-run phase). Edge-triggered with hysteresis: it will not fire
/// again until space recovers above the threshold plus 1 GB.
fn check_low_disk_trigger(app: &AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static LATCH: AtomicBool = AtomicBool::new(false);
    let cfg = appconfig::AppConfig::load_from(&appconfig::config_path());
    if !cfg.autonomous.enabled {
        return;
    }
    let threshold = cfg.autonomous.low_disk_trigger_gb;
    let facts = preflight::collect();
    if safety::low_disk_due(facts.free_space_gb, threshold) {
        if !LATCH.swap(true, Ordering::Relaxed) {
            let handle = app.clone();
            std::thread::spawn(move || {
                let state = handle.state::<safety::SafetyRuntime>();
                let runtime = safety::SafetyRuntime {
                    cancel: state.cancel.clone(),
                    running: state.running.clone(),
                };
                let _ = safety::run_safety_once(&runtime, true, "low-disk");
                refresh_safety_status_item();
            });
        }
    } else if facts.free_space_gb > threshold as f64 + 1.0 {
        LATCH.store(false, Ordering::Relaxed);
    }
}

/// Samples CPU per process once a second (sparklines) and refreshes the tray tooltip.
fn spawn_sampler(app: AppHandle) {
    std::thread::Builder::new()
        .name("tidy-sampler".into())
        .spawn(move || {
            let mut tick: u64 = 0;
            loop {
                let state = app.state::<AppState>();
                // While the window is hidden in the tray, sample at
                // a tenth of the rate: a resident utility should cost
                // almost nothing when nobody is looking at it.
                let hidden = app
                    .get_webview_window("main")
                    .map(|w| !w.is_visible().unwrap_or(true))
                    .unwrap_or(false);
                if hidden && !tick.is_multiple_of(10) {
                    tick += 1;
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                state.sample();
                if tick.is_multiple_of(60) {
                    check_low_disk_trigger(&app);
                }
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

/// The release binary is a GUI-subsystem process, which Windows does
/// not give a console by default — without this, the headless CLI
/// modes would print nothing when run from PowerShell or cmd.
#[cfg(windows)]
fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Hand-rolled single-instance guard (no plugin): a named mutex held
/// for the process lifetime. Returns false when another TidyTray GUI
/// process already holds it.
#[cfg(windows)]
fn acquire_single_instance() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "TidyTraySingleInstance\0".encode_utf16().collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        return true; // cannot tell — never block startup on the guard itself
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle) };
        return false;
    }
    let _ = handle; // never closed: the mutex lives as long as the process
    true
}

#[cfg(windows)]
fn show_already_running() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
    let text: Vec<u16> = "TidyTray is already running — look for its icon in the system tray.\0"
        .encode_utf16()
        .collect();
    let caption: Vec<u16> = "TidyTray\0".encode_utf16().collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn main() {
    // Headless entry points (full CLI parity): the Task Scheduler task and
    // scripts use these; they never open a window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if args.iter().any(|a| a.starts_with("--")) {
        attach_parent_console();
    }
    if args.iter().any(|a| a == "--autonomous-run") {
        std::process::exit(safety::headless_autonomous_run());
    }
    if let Some(pos) = args.iter().position(|a| a == "--apps-elevated") {
        // Elevated helper for app removal (restore point + Remove-AppxPackage).
        let code = args
            .get(pos + 1)
            .map(|p| apps::run_removal_helper(std::path::Path::new(p)))
            .unwrap_or(3);
        std::process::exit(code);
    }
    if args.iter().any(|a| a == "--apps") {
        let states = apps::removal_states(&apps::installed_packages());
        println!("{}", serde_json::to_string_pretty(&states).unwrap_or_default());
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--apps-remove") {
        let packages: Vec<String> = args.get(pos + 1).cloned().unwrap_or_default().split(',').map(|p| p.to_string()).collect();
        let outcome = apps::remove_apps(&appconfig::data_dir(), &appconfig::logs_dir(), &packages);
        println!("{}", serde_json::to_string_pretty(&outcome).unwrap_or_default());
        std::process::exit(if outcome.failed.is_empty() { 0 } else { 2 });
    }
    if args.iter().any(|a| a == "--apps-undo") {
        let outcome = apps::undo_last_removal(&appconfig::data_dir(), &appconfig::logs_dir());
        println!("{}", serde_json::to_string_pretty(&outcome).unwrap_or_default());
        std::process::exit(if outcome.failed.is_empty() { 0 } else { 2 });
    }
    if args.iter().any(|a| a == "--install-list") {
        let installed = apps::winget_installed().unwrap_or_default();
        let states = apps::install_states(&installed);
        println!("{}", serde_json::to_string_pretty(&states).unwrap_or_default());
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--install") {
        let ids: Vec<String> = args.get(pos + 1).cloned().unwrap_or_default().split(',').map(|p| p.to_string()).collect();
        let results = apps::winget_install(&ids);
        for (id, ok, msg) in &results {
            println!("{id}: {}", if *ok { "installed" } else { msg });
        }
        std::process::exit(if results.iter().all(|(_, ok, _)| *ok) { 0 } else { 2 });
    }
    if args.iter().any(|a| a == "--profiles") {
        let profiles = apps::load_profiles(&appconfig::data_dir());
        println!("{}", serde_json::to_string_pretty(&profiles).unwrap_or_default());
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--tier2-elevated") {
        // Elevated helper for Tier 2 machine-level writes + restore point.
        // Runs the batch, writes its result file, exits before any UI starts.
        let code = args
            .get(pos + 1)
            .map(|p| tier2::run_elevated_helper(std::path::Path::new(p)))
            .unwrap_or(3);
        std::process::exit(code);
    }
    if let Some(pos) = args.iter().position(|a| a == "--repair-elevated") {
        // Elevated helper for system repair (SFC / DISM).
        let code = args
            .get(pos + 1)
            .map(|p| repair::run_repair_helper(std::path::Path::new(p)))
            .unwrap_or(3);
        std::process::exit(code);
    }
    if let Some(pos) = args.iter().position(|a| a == "--repair") {
        let action = args.get(pos + 1).cloned().unwrap_or_default();
        let outcome = repair::run_repair(&appconfig::data_dir(), &appconfig::logs_dir(), &action);
        println!("{}", serde_json::to_string_pretty(&outcome).unwrap_or_default());
        std::process::exit(if outcome.ok { 0 } else { 2 });
    }
    if args.iter().any(|a| a == "--repair-list") {
        println!("{}", serde_json::to_string_pretty(&repair::action_states()).unwrap_or_default());
        return;
    }
    if args.iter().any(|a| a == "--tweaks") {
        let store = tier2::system_store();
        let states = tier2::list_states(&store);
        println!("{}", serde_json::to_string_pretty(&states).unwrap_or_default());
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--tweak-apply") {
        // --tweak-apply id:on,id:off — same previewed pipeline as the window:
        // restore point attempted, undo batch recorded, outcome as JSON.
        let spec = args.get(pos + 1).cloned().unwrap_or_default();
        let changes: Vec<(String, bool)> = spec
            .split(',')
            .filter_map(|part| part.rsplit_once(':').map(|(id, v)| (id.to_string(), v == "on")))
            .collect();
        let store = tier2::system_store();
        let outcome = tier2::apply_changes(&store, &appconfig::data_dir(), &appconfig::logs_dir(), &changes, true);
        println!("{}", serde_json::to_string_pretty(&outcome).unwrap_or_default());
        std::process::exit(if outcome.failed.is_empty() { 0 } else { 2 });
    }
    if args.iter().any(|a| a == "--tweak-undo") {
        let store = tier2::system_store();
        let outcome = tier2::undo_last(&store, &appconfig::data_dir(), &appconfig::logs_dir());
        println!("{}", serde_json::to_string_pretty(&outcome).unwrap_or_default());
        std::process::exit(if outcome.failed.is_empty() { 0 } else { 2 });
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
    #[cfg(windows)]
    if !acquire_single_instance() {
        show_already_running();
        return;
    }
    tauri::Builder::default()
        .manage(AppState::new())
        .manage(safety::SafetyRuntime::new())
        .setup(|app| {
            build_tray(app)?;
            spawn_sampler(app.handle().clone());
            std::thread::spawn(|| {
                let cfg = appconfig::AppConfig::load_from(&appconfig::config_path());
                if cfg.autonomous.enabled && scheduler::is_installed() {
                    scheduler::heal_if_moved(&cfg.autonomous.schedule);
                }
            });
            let settings = app.state::<AppState>().settings();
            if settings.start_minimized {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.minimize();
                }
            }
            log::info!("TidyTray started, dry-run = {}", settings.dry_run);
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
            commands::safety_recent_runs,
            commands::safety_quarantine_list,
            commands::safety_open_logs,
            commands::safety_set_pauses,
            commands::tier2_list,
            commands::tier2_preview,
            commands::tier2_apply,
            commands::tier2_undo_last,
            commands::tier2_undo_available,
            commands::apps_removal_list,
            commands::apps_remove,
            commands::apps_undo_last,
            commands::apps_undo_available,
            commands::apps_install_list,
            commands::apps_install,
            commands::apps_profiles,
            commands::apps_save_profile,
            commands::apps_delete_profile,
            commands::repair_list,
            commands::repair_run,
            commands::check_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TidyTray");
}
