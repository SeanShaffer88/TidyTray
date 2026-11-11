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
/// a real delete needs both `dry_run: false` here and dry-run disabled in settings.
#[tauri::command]
pub async fn clean(state: State<'_, AppState>, categories: Vec<String>, dry_run: Option<bool>) -> TidyResult<CleanReport> {
    if state.last_scan.lock().map(|s| s.is_none()).unwrap_or(true) {
        return Err(TidyError::NoScan);
    }
    let settings = state.settings();
    let dry = settings.dry_run || dry_run.unwrap_or(true) || demo_mode();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let targets = scanner::default_targets(settings.min_temp_age_hours);
        scanner::clean(&targets, &categories, &safe_list(&settings), dry)
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
// Autonomous safety core (blueprint M1): manifest-driven Tier 1 cleanup that
// only ever moves files to quarantine. Separate from the legacy Smart Clean
// above; the UI reaches the safety core only through these commands.
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
    pub autonomous_enabled: bool,
    pub dry_runs_completed: u32,
    pub dry_run_phase_complete: bool,
    pub quarantine_files: u64,
    pub quarantine_bytes: u64,
}

fn safety_ctx() -> crate::cleaner::RunContext {
    crate::cleaner::RunContext::new(
        crate::manifest::real_env(),
        crate::appconfig::quarantine_dir(),
        crate::appconfig::logs_dir(),
    )
}

#[tauri::command]
pub fn safety_status() -> TidyResult<SafetyStatus> {
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
        autonomous_enabled: cfg.autonomous.enabled,
        dry_runs_completed: cfg.autonomous.dry_runs_completed,
        dry_run_phase_complete: cfg.dry_run_phase_complete(),
        quarantine_files: q.entries().len() as u64,
        quarantine_bytes: q.total_size(),
    })
}

/// Runs the Tier 1 safety cleaner. A real (non-dry) run is only honoured
/// once the 3-run dry-run phase is complete; before that the request is
/// downgraded to a dry run, and each completed dry run is counted.
#[tauri::command]
pub async fn safety_clean_now(dry_run: Option<bool>) -> TidyResult<crate::cleaner::RunReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let manifest = crate::manifest::Manifest::embedded().map_err(|e| TidyError::Other(e.to_string()))?;
        let cfg_path = crate::appconfig::config_path();
        let mut cfg = crate::appconfig::AppConfig::load_from(&cfg_path);
        let want_dry = dry_run.unwrap_or(true);
        let mode = if want_dry || !cfg.dry_run_phase_complete() {
            crate::cleaner::RunMode::DryRun
        } else {
            crate::cleaner::RunMode::Apply
        };
        let report = crate::cleaner::run(&manifest, &cfg, &safety_ctx(), mode);
        if report.dry_run && !report.cancelled {
            cfg.autonomous.dry_runs_completed = cfg.autonomous.dry_runs_completed.saturating_add(1);
            let _ = cfg.save_to(&cfg_path);
        }
        Ok(report)
    })
    .await
    .map_err(|e| TidyError::Other(e.to_string()))?
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
