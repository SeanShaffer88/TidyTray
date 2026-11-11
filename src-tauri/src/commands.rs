//! Tauri commands: the only surface the UI talks to.

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::files::{self, FilesReport};
use crate::health::{self, Drive, MemoryInfo};
use crate::processes::{self, ProcessRow, SortBy};
use crate::safelist::{SafeList, SafeRule};
use crate::scanner::{self, CleanReport, ScanReport};
use crate::startup::{self, StartupItem};
use crate::state::{AppState, Settings, TidyError, TidyResult};

fn demo_mode() -> bool {
    std::env::var_os("TIDY_DEMO").is_some()
}

fn safe_list(settings: &Settings) -> SafeList {
    SafeList::new(&settings.safe_list)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub host: String,
    pub os: String,
    pub uptime_secs: u64,
    pub cpu_percent: f32,
    pub memory: MemoryInfo,
    pub drives: Vec<Drive>,
    pub top: Vec<ProcessRow>,
    pub reclaimable: u64,
    pub last_scan_ms: Option<u64>,
    pub dry_run: bool,
}

#[tauri::command]
pub fn get_overview(state: State<'_, AppState>) -> TidyResult<Overview> {
    let (reclaimable, last_scan_ms) = state
        .last_scan
        .lock()
        .ok()
        .and_then(|s| s.as_ref().map(|r| (selected_bytes(r), Some(r.duration_ms))))
        .unwrap_or((0, None));
    let sys = state.sys.lock().map_err(|_| TidyError::Other("system lock poisoned".into()))?;
    let mut top = processes::snapshot(&sys, SortBy::Cpu, 5);
    drop_history(&state, &mut top);
    Ok(Overview {
        host: sysinfo::System::host_name().unwrap_or_else(|| "This PC".into()),
        os: sysinfo::System::long_os_version().unwrap_or_else(|| "Windows".into()),
        uptime_secs: sysinfo::System::uptime(),
        cpu_percent: sys.global_cpu_usage(),
        memory: health::memory(&sys),
        drives: health::drives(reclaimable),
        top,
        reclaimable,
        last_scan_ms,
        dry_run: state.settings().dry_run,
    })
}

fn drop_history(state: &AppState, rows: &mut [ProcessRow]) {
    for r in rows {
        r.history = state.history_of(&r.name);
    }
}

fn selected_bytes(r: &ScanReport) -> u64 {
    r.categories.iter().filter(|c| c.selected).map(|c| c.bytes).sum()
}

/// Smart Scan. Runs on a worker thread so the window keeps painting.
#[tauri::command]
pub async fn smart_scan(state: State<'_, AppState>) -> TidyResult<ScanReport> {
    let settings = state.settings();
    let report = if demo_mode() {
        scanner::sample_report()
    } else {
        tauri::async_runtime::spawn_blocking(move || {
            let targets = scanner::default_targets(settings.min_temp_age_hours);
            scanner::scan(&targets, &safe_list(&settings), &settings.unchecked_categories)
        })
        .await
        .map_err(|e| TidyError::Other(e.to_string()))?
    };
    log::info!("smart scan: {} in {} ms", crate::state::human_bytes(report.total_bytes), report.duration_ms);
    if let Ok(mut last) = state.last_scan.lock() {
        *last = Some(report.clone());
    }
    Ok(report)
}

/// Cleans the given categories. `dry_run` defaults to the setting (on by default);
/// a real run needs both `dry_run: false` here and dry-run disabled in settings.
/// Real runs move files into quarantine (blueprint M3) — nothing is deleted
/// outright; the move is audit-logged and expired quarantine content is
/// purged under the configured retention afterwards.
#[tauri::command]
pub async fn clean(state: State<'_, AppState>, categories: Vec<String>, dry_run: Option<bool>) -> TidyResult<CleanReport> {
    if state.last_scan.lock().map(|s| s.is_none()).unwrap_or(true) {
        return Err(TidyError::NoScan);
    }
    let settings = state.settings();
    let dry = settings.dry_run || dry_run.unwrap_or(true) || demo_mode();
    // Real cleans share the quarantine with the autonomous engine: take
    // the cross-process run lock so two processes never move files into
    // it at once. Dry runs only read, so they do not need it.
    let _run_lock = if dry {
        None
    } else {
        match crate::runlock::acquire(&crate::appconfig::data_dir()) {
            Some(lock) => Some(lock),
            None => {
                return Err(TidyError::Other(
                    "a cleaning run is already in progress in another TidyTray window or process"
                        .to_string(),
                ))
            }
        }
    };
    let report = tauri::async_runtime::spawn_blocking(move || {
        let _run_lock = _run_lock;
        let targets = scanner::default_targets(settings.min_temp_age_hours);
        let quarantine = crate::quarantine::Quarantine::new(crate::appconfig::quarantine_dir());
        let run_id = crate::audit::new_run_id();
        let report = scanner::clean(&targets, &categories, &safe_list(&settings), dry, &quarantine, &run_id);
        if !report.dry_run {
            let log = crate::audit::AuditLog::new(crate::appconfig::logs_dir());
            let _ = log.append(
                &crate::audit::LogEntry::new(&run_id, "run")
                    .size(report.freed_bytes)
                    .result("smart-clean: moved to quarantine"),
            );
            let cfg = crate::appconfig::AppConfig::load_from(&crate::appconfig::config_path());
            let retention = std::time::Duration::from_secs(cfg.quarantine.retention_days * 86_400);
            quarantine.purge_expired(retention, std::time::SystemTime::now());
        }
        report
    })
    .await
    .map_err(|e| TidyError::Other(e.to_string()))?;
    if !report.dry_run {
        if let Ok(mut last) = state.last_scan.lock() {
            *last = None;
        }
    }
    Ok(report)
}

#[tauri::command]
pub fn list_processes(state: State<'_, AppState>, sort: Option<SortBy>, limit: Option<usize>) -> TidyResult<Vec<ProcessRow>> {
    let sys = state.sys.lock().map_err(|_| TidyError::Other("system lock poisoned".into()))?;
    let mut rows = processes::snapshot(&sys, sort.unwrap_or_default(), limit.unwrap_or(60));
    drop(sys);
    drop_history(&state, &mut rows);
    Ok(rows)
}

#[tauri::command]
pub fn end_process(state: State<'_, AppState>, name: String) -> TidyResult<u32> {
    let sys = state.sys.lock().map_err(|_| TidyError::Other("system lock poisoned".into()))?;
    processes::end_process(&sys, &name).map_err(TidyError::Other)
}

#[tauri::command]
pub fn get_memory(state: State<'_, AppState>) -> TidyResult<MemoryInfo> {
    let sys = state.sys.lock().map_err(|_| TidyError::Other("system lock poisoned".into()))?;
    Ok(health::memory(&sys))
}

#[tauri::command]
pub fn purge_standby() -> TidyResult<()> {
    health::purge_standby().map_err(TidyError::NotImplemented)
}

#[tauri::command]
pub fn list_drives(state: State<'_, AppState>) -> Vec<Drive> {
    let junk = state.last_scan.lock().ok().and_then(|s| s.as_ref().map(selected_bytes)).unwrap_or(0);
    health::drives(junk)
}

#[tauri::command]
pub fn list_startup() -> Vec<StartupItem> {
    startup::list()
}

/// The per-user Startup folder, shown as a hint under the startup list.
#[tauri::command]
pub fn get_startup_folder() -> Option<String> {
    startup::startup_folder()
}

#[tauri::command]
pub fn set_startup_enabled(id: String, enabled: bool) -> TidyResult<()> {
    startup::set_enabled(&id, enabled).map_err(TidyError::Other)
}

/// Large & duplicate files under `root` (defaults to the user profile).
#[tauri::command]
pub async fn find_large_files(state: State<'_, AppState>, root: Option<String>, min_mb: Option<u64>) -> TidyResult<FilesReport> {
    let settings = state.settings();
    let root = root
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .ok_or_else(|| TidyError::Other("no folder to scan".into()))?;
    if !root.is_dir() {
        return Err(TidyError::Other(format!("{} is not a folder", root.display())));
    }
    let safe = safe_list(&settings);
    if safe.protects(&root).is_some() {
        return Err(TidyError::Protected(root.display().to_string()));
    }
    let min = min_mb.unwrap_or(settings.large_file_min_mb) * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || files::scan(&root, min, &safe, 50))
        .await
        .map_err(|e| TidyError::Other(e.to_string()))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> TidyResult<Settings> {
    let clean = Settings::parse(&serde_json::to_string(&settings)?)?;
    clean.save()?;
    if let Ok(mut s) = state.settings.lock() {
        *s = clean.clone();
    }
    Ok(clean)
}

#[tauri::command]
pub fn get_safe_list(state: State<'_, AppState>) -> Vec<SafeRule> {
    safe_list(&state.settings()).rules()
}

/// Lets the UI explain why a path in a result list is greyed out.
#[tauri::command]
pub fn check_path(state: State<'_, AppState>, path: String) -> Option<&'static str> {
    safe_list(&state.settings()).protects(&PathBuf::from(path))
}

// ---------------------------------------------------------------------------
// Autonomous safety core (blueprint M1/M2): manifest-driven Tier 1 cleanup
// that only ever moves files to quarantine. Separate from the legacy Smart
// Clean above; the UI reaches the safety core only through these commands.
// All run orchestration lives in crate::safety so the tray, CLI and
// scheduler share exactly one policy path.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyCategory {
    pub id: String,
    pub display_name: String,
    pub requires_admin: bool,
    pub enabled_by_default: bool,
    pub min_age_days: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyStatus {
    pub manifest_version: String,
    pub categories: Vec<SafetyCategory>,
    pub enabled_categories: Vec<String>,
    pub autonomous_enabled: bool,
    pub dry_runs_completed: u32,
    pub dry_run_phase_complete: bool,
    pub quarantine_files: u64,
    pub quarantine_bytes: u64,
    pub running: bool,
    pub scheduler_installed: bool,
    pub schedule_mode: String,
    pub schedule_day: String,
    pub schedule_hour: u32,
    pub last_run: Option<crate::safety::LastRun>,
    pub preflight: crate::preflight::Facts,
    pub pause_on_fullscreen: bool,
    pub battery_pause_below_pct: u32,
}

#[tauri::command]
pub fn safety_status(state: tauri::State<'_, crate::safety::SafetyRuntime>) -> TidyResult<SafetyStatus> {
    let manifest = crate::manifest::Manifest::embedded().map_err(|e| TidyError::Other(e.to_string()))?;
    let cfg = crate::appconfig::AppConfig::load_from(&crate::appconfig::config_path());
    let q = crate::quarantine::Quarantine::new(crate::appconfig::quarantine_dir());
    Ok(SafetyStatus {
        manifest_version: manifest.manifest_version.clone(),
        categories: manifest
            .categories
            .iter()
            .map(|c| SafetyCategory {
                id: c.id.clone(),
                display_name: c.display_name.clone(),
                requires_admin: c.requires_admin,
                enabled_by_default: c.enabled_by_default,
                min_age_days: c.min_age_days,
            })
            .collect(),
        enabled_categories: cfg.autonomous.categories_enabled.clone(),
        autonomous_enabled: cfg.autonomous.enabled,
        dry_runs_completed: cfg.autonomous.dry_runs_completed,
        dry_run_phase_complete: cfg.dry_run_phase_complete(),
        quarantine_files: q.entries().len() as u64,
        quarantine_bytes: q.total_size(),
        running: state.is_running(),
        scheduler_installed: crate::scheduler::is_installed(),
        schedule_mode: cfg.autonomous.schedule.mode.clone(),
        schedule_day: cfg.autonomous.schedule.day.clone(),
        schedule_hour: cfg.autonomous.schedule.hour,
        last_run: crate::safety::read_last_run(),
        preflight: crate::preflight::collect(),
        pause_on_fullscreen: cfg.autonomous.pause_on_fullscreen,
        battery_pause_below_pct: cfg.autonomous.pause_on_battery_below_pct,
    })
}

/// Runs the Tier 1 safety cleaner through the shared policy path.
/// `apply = true` asks for a real run; it is honoured only once the 3-run
/// dry-run phase is complete. Returns a completed report or a skip reason
/// (battery / fullscreen pre-flight gate). Long-running: callers should
/// treat this as a background operation.
#[tauri::command]
pub async fn safety_clean_now(
    state: tauri::State<'_, crate::safety::SafetyRuntime>,
    apply: Option<bool>,
) -> TidyResult<crate::safety::RunOutcome> {
    let runtime = crate::safety::SafetyRuntime {
        cancel: state.cancel.clone(),
        running: state.running.clone(),
    };
    let apply = apply.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        crate::safety::run_safety_once(&runtime, apply, "ui")
    })
    .await
    .map_err(|e| TidyError::Other(e.to_string()))?
    .map_err(TidyError::Other)
}

/// Kill switch: ask the currently running safety cleanup to stop.
#[tauri::command]
pub fn safety_cancel(state: tauri::State<'_, crate::safety::SafetyRuntime>) -> bool {
    state.request_cancel()
}

/// Enables or disables autonomous cleanup. Enabling requires the dry-run
/// phase to be complete and installs the Task Scheduler entry; disabling
/// removes it.
#[tauri::command]
pub fn safety_set_autonomous(enabled: bool) -> TidyResult<bool> {
    let cfg_path = crate::appconfig::config_path();
    let mut cfg = crate::appconfig::AppConfig::load_from(&cfg_path);
    if enabled && !cfg.dry_run_phase_complete() {
        return Err(TidyError::Other(
            "autonomous cleanup unlocks after 3 completed dry runs".to_string(),
        ));
    }
    cfg.autonomous.enabled = enabled;
    cfg.save_to(&cfg_path).map_err(|e| TidyError::Other(e.to_string()))?;
    if enabled {
        crate::scheduler::install(&cfg.autonomous.schedule).map_err(TidyError::Other)?;
    } else {
        crate::scheduler::remove().map_err(TidyError::Other)?;
    }
    Ok(enabled)
}

/// Updates the autonomous schedule; reinstalls the task when enabled.
#[tauri::command]
pub fn safety_set_schedule(mode: String, day: String, hour: u32) -> TidyResult<()> {
    if mode != "weekly" && mode != "daily" {
        return Err(TidyError::Other(format!("unknown schedule mode: {mode}")));
    }
    let cfg_path = crate::appconfig::config_path();
    let mut cfg = crate::appconfig::AppConfig::load_from(&cfg_path);
    cfg.autonomous.schedule.mode = mode;
    cfg.autonomous.schedule.day = day;
    cfg.autonomous.schedule.hour = hour.min(23);
    cfg.save_to(&cfg_path).map_err(|e| TidyError::Other(e.to_string()))?;
    if cfg.autonomous.enabled {
        crate::scheduler::install(&cfg.autonomous.schedule).map_err(TidyError::Other)?;
    }
    Ok(())
}

/// Sets which Tier 1 categories are enabled. Unknown ids are rejected.
#[tauri::command]
pub fn safety_set_categories(enabled_ids: Vec<String>) -> TidyResult<Vec<String>> {
    let manifest = crate::manifest::Manifest::embedded().map_err(|e| TidyError::Other(e.to_string()))?;
    for id in &enabled_ids {
        if !manifest.categories.iter().any(|c| &c.id == id) {
            return Err(TidyError::Other(format!("unknown category id: {id}")));
        }
    }
    let cfg_path = crate::appconfig::config_path();
    let mut cfg = crate::appconfig::AppConfig::load_from(&cfg_path);
    cfg.autonomous.categories_enabled = enabled_ids;
    cfg.save_to(&cfg_path).map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(cfg.autonomous.categories_enabled.clone())
}

/// Restores one quarantined file to its original location.
#[tauri::command]
pub fn safety_restore(path: String) -> TidyResult<String> {
    let q = crate::quarantine::Quarantine::new(crate::appconfig::quarantine_dir());
    let restored = q.restore(&PathBuf::from(&path)).map_err(|e| TidyError::Other(e.to_string()))?;
    let log = crate::audit::AuditLog::new(crate::appconfig::logs_dir());
    let _ = log.append(
        &crate::audit::LogEntry::new("manual", "quarantine_restore")
            .path(&restored)
            .result("restored"),
    );
    Ok(restored.to_string_lossy().into_owned())
}

/// Manually empties quarantine (everything in it, regardless of age).
#[tauri::command]
pub fn safety_purge_quarantine() -> TidyResult<u64> {
    let q = crate::quarantine::Quarantine::new(crate::appconfig::quarantine_dir());
    let freed = q.purge_all();
    let log = crate::audit::AuditLog::new(crate::appconfig::logs_dir());
    let _ = log.append(&crate::audit::LogEntry::new("manual", "quarantine_purge").size(freed).result("manual"));
    Ok(freed)
}

/// Recent safety run attempts, newest first (Safety screen).
#[tauri::command]
pub fn safety_recent_runs(limit: Option<u32>) -> Vec<crate::safety::LastRun> {
    crate::safety::recent_runs(limit.unwrap_or(8) as usize)
}

/// Everything currently held in quarantine (Safety screen review list).
#[tauri::command]
pub fn safety_quarantine_list() -> Vec<crate::quarantine::QuarantineEntry> {
    crate::quarantine::Quarantine::new(crate::appconfig::quarantine_dir()).entries()
}

// ---------------------------------------------------------------------------
// Tier 2 tweaks (blueprint M4): manual, previewed, reversible. Staging lives
// in the UI; these commands detect, preview, apply (restore point + undo
// batch) and undo. The same operations back the --tweaks CLI flags.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tier2Change {
    pub id: String,
    pub applied: bool,
}

fn tier2_data_dir() -> std::path::PathBuf {
    crate::appconfig::data_dir()
}

#[tauri::command]
pub fn tier2_list() -> TidyResult<Vec<crate::tier2::ToggleState>> {
    let store = crate::tier2::system_store();
    Ok(crate::tier2::list_states(&store))
}

#[tauri::command]
pub fn tier2_preview(changes: Vec<Tier2Change>) -> TidyResult<Vec<crate::tier2::PlannedChange>> {
    let store = crate::tier2::system_store();
    let pairs: Vec<(String, bool)> = changes.iter().map(|c| (c.id.clone(), c.applied)).collect();
    Ok(crate::tier2::plan(&pairs, &store))
}

#[tauri::command]
pub async fn tier2_apply(changes: Vec<Tier2Change>) -> TidyResult<crate::tier2::ApplyOutcome> {
    let pairs: Vec<(String, bool)> = changes.iter().map(|c| (c.id.clone(), c.applied)).collect();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let store = crate::tier2::system_store();
        crate::tier2::apply_changes(&store, &tier2_data_dir(), &crate::appconfig::logs_dir(), &pairs, true)
    })
    .await
    .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(outcome)
}

#[tauri::command]
pub async fn tier2_undo_last() -> TidyResult<crate::tier2::UndoOutcome> {
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let store = crate::tier2::system_store();
        crate::tier2::undo_last(&store, &tier2_data_dir(), &crate::appconfig::logs_dir())
    })
    .await
    .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(outcome)
}

/// Number of changes in the latest undo batch, or null when there is
/// nothing to undo.
#[tauri::command]
pub fn tier2_undo_available() -> TidyResult<Option<usize>> {
    Ok(crate::tier2::undo_available(&tier2_data_dir()))
}

// ---------------------------------------------------------------------------
// Tier 2 apps (blueprint M5): bundled-app removal + winget install catalog.
// Removal goes through one elevated helper run with a restore point and
// records an undo batch; installs run winget with catalog ids only.
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn apps_removal_list() -> TidyResult<Vec<crate::apps::RemovalState>> {
    let installed = tauri::async_runtime::spawn_blocking(crate::apps::installed_packages)
        .await
        .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(crate::apps::removal_states(&installed))
}

#[tauri::command]
pub async fn apps_remove(packages: Vec<String>) -> TidyResult<crate::apps::RemovalOutcome> {
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        crate::apps::remove_apps(&crate::appconfig::data_dir(), &crate::appconfig::logs_dir(), &packages)
    })
    .await
    .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(outcome)
}

#[tauri::command]
pub async fn apps_undo_last() -> TidyResult<crate::apps::AppUndoOutcome> {
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        crate::apps::undo_last_removal(&crate::appconfig::data_dir(), &crate::appconfig::logs_dir())
    })
    .await
    .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(outcome)
}

#[tauri::command]
pub fn apps_undo_available() -> TidyResult<Option<usize>> {
    Ok(crate::apps::removal_undo_available(&crate::appconfig::data_dir()))
}

/// Install catalog + detection. `winget_available` is false when winget
/// is missing or errors — the tab then explains instead of failing.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallList {
    pub winget_available: bool,
    pub apps: Vec<crate::apps::InstallState>,
}

#[tauri::command]
pub async fn apps_install_list() -> TidyResult<InstallList> {
    let detected = tauri::async_runtime::spawn_blocking(crate::apps::winget_installed)
        .await
        .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(InstallList {
        winget_available: detected.is_some(),
        apps: crate::apps::install_states(&detected.unwrap_or_default()),
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallItemResult {
    pub id: String,
    pub ok: bool,
    pub message: String,
}

#[tauri::command]
pub async fn apps_install(ids: Vec<String>) -> TidyResult<Vec<InstallItemResult>> {
    let results = tauri::async_runtime::spawn_blocking(move || crate::apps::winget_install(&ids))
        .await
        .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(results
        .into_iter()
        .map(|(id, ok, message)| InstallItemResult { id, ok, message })
        .collect())
}

#[tauri::command]
pub fn apps_profiles() -> TidyResult<Vec<crate::apps::Profile>> {
    Ok(crate::apps::load_profiles(&crate::appconfig::data_dir()))
}

#[tauri::command]
pub fn apps_save_profile(name: String, ids: Vec<String>) -> TidyResult<Vec<crate::apps::Profile>> {
    // Profile ids are validated against the install catalog on save.
    let valid: Vec<String> = ids
        .into_iter()
        .filter(|id| crate::apps::install_catalog().iter().any(|e| e.id == id))
        .collect();
    crate::apps::save_profile(&crate::appconfig::data_dir(), &name, &valid)
        .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(crate::apps::load_profiles(&crate::appconfig::data_dir()))
}

#[tauri::command]
pub fn apps_delete_profile(name: String) -> TidyResult<Vec<crate::apps::Profile>> {
    crate::apps::delete_profile(&crate::appconfig::data_dir(), &name)
        .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(crate::apps::load_profiles(&crate::appconfig::data_dir()))
}

/// Opens the audit-log folder in Explorer.
#[tauri::command]
pub fn safety_open_logs() -> TidyResult<()> {
    let dir = crate::appconfig::logs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| TidyError::Other(e.to_string()))?;
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(())
}

/// Sets the pre-flight pause preferences: fullscreen pause on/off and the
/// battery pause threshold (0 = never pause for battery).
#[tauri::command]
pub fn safety_set_pauses(pause_on_fullscreen: bool, battery_pause_below_pct: u32) -> TidyResult<()> {
    let cfg_path = crate::appconfig::config_path();
    let mut cfg = crate::appconfig::AppConfig::load_from(&cfg_path);
    cfg.autonomous.pause_on_fullscreen = pause_on_fullscreen;
    cfg.autonomous.pause_on_battery_below_pct = battery_pause_below_pct.min(100);
    cfg.save_to(&cfg_path).map_err(|e| TidyError::Other(e.to_string()))?;
    Ok(())
}
