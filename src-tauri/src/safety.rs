//! Shared orchestration for safety cleanup runs (blueprint §8.1): one code
//! path used by the UI commands, the tray menu, the CLI flags and the
//! scheduled `--autonomous-run` entry. Holds the process-wide kill switch
//! and run state, applies the pre-flight gate, enforces the dry-run phase,
//! and records a `last_run.json` summary after every attempt.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::appconfig::{self, AppConfig};
use crate::audit::{iso_now, AuditLog, LogEntry};
use crate::cleaner::{self, RunContext, RunMode, RunReport};
use crate::manifest::Manifest;
use crate::preflight;

/// Process-wide safety-run state, registered with Tauri as managed state.
pub struct SafetyRuntime {
    pub cancel: Arc<AtomicBool>,
    pub running: Arc<AtomicBool>,
}

impl SafetyRuntime {
    pub fn new() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Kill switch: ask the current run (if any) to stop.
    pub fn request_cancel(&self) -> bool {
        let was = self.running.load(Ordering::Relaxed);
        self.cancel.store(true, Ordering::Relaxed);
        was
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Default for SafetyRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of the most recent run attempt, persisted for tray/UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastRun {
    pub at: String,
    pub source: String,
    pub outcome: String, // "complete" | "cancelled" | "skipped" | "error"
    #[serde(default)]
    pub detail: String,
    pub dry_run: bool,
    pub files: u64,
    pub bytes: u64,
}

fn last_run_path() -> std::path::PathBuf {
    appconfig::data_dir().join("last_run.json")
}

pub fn read_last_run() -> Option<LastRun> {
    let text = std::fs::read_to_string(last_run_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_last_run(lr: &LastRun) {
    if let Some(parent) = last_run_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(lr) {
        let _ = std::fs::write(last_run_path(), text);
    }
    append_history(lr);
}

fn history_path() -> std::path::PathBuf {
    appconfig::data_dir().join("runs.json")
}

fn append_history(lr: &LastRun) {
    let mut runs = read_history();
    runs.push(lr.clone());
    if runs.len() > 50 {
        let excess = runs.len() - 50;
        runs.drain(..excess);
    }
    if let Ok(text) = serde_json::to_string_pretty(&runs) {
        let _ = std::fs::write(history_path(), text);
    }
}

fn read_history() -> Vec<LastRun> {
    std::fs::read_to_string(history_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Most recent run attempts, newest first (for the Safety screen).
pub fn recent_runs(limit: usize) -> Vec<LastRun> {
    let mut runs = read_history();
    runs.reverse();
    runs.truncate(limit);
    runs
}

fn record(
    source: &str,
    outcome: &str,
    detail: String,
    dry_run: bool,
    files: u64,
    bytes: u64,
) -> LastRun {
    let lr = LastRun {
        at: iso_now(),
        source: source.to_string(),
        outcome: outcome.to_string(),
        detail,
        dry_run,
        files,
        bytes,
    };
    write_last_run(&lr);
    lr
}

/// Result of one orchestrated run attempt.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunOutcome {
    Completed { report: Box<RunReport> },
    Skipped { reason: String },
}

/// Run one safety cleanup pass under the full policy: single-run guard,
/// pre-flight gate, dry-run phase enforcement, audit + last-run records.
/// `apply_requested` asks for a real run; it is honoured only once the
/// dry-run phase is complete.
pub fn run_safety_once(
    runtime: &SafetyRuntime,
    apply_requested: bool,
    source: &str,
) -> Result<RunOutcome, String> {
    if runtime
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("a safety cleanup is already running".to_string());
    }
    // Cross-process exclusion: the scheduled headless run is a second
    // process, and the in-process guard above cannot see it. The lock
    // file also covers a second GUI process.
    let _lock = match crate::runlock::acquire(&appconfig::data_dir()) {
        Some(lock) => lock,
        None => {
            runtime.running.store(false, Ordering::Relaxed);
            return Err(
                "a safety cleanup is already running in another TidyTray process".to_string(),
            );
        }
    };
    let result = run_inner(runtime, apply_requested, source);
    runtime.running.store(false, Ordering::Relaxed);
    result
}

fn run_inner(
    runtime: &SafetyRuntime,
    apply_requested: bool,
    source: &str,
) -> Result<RunOutcome, String> {
    let manifest = Manifest::embedded().map_err(|e| e.to_string())?;
    let cfg_path = appconfig::config_path();
    let mut config = AppConfig::load_from(&cfg_path);

    // Pre-flight gate (battery / fullscreen per config).
    let facts = preflight::collect();
    if let Some(reason) = preflight::skip_reason(&facts, &config) {
        let log = AuditLog::new(appconfig::logs_dir());
        let _ = log.append(
            &LogEntry::new(&crate::audit::new_run_id(), "run").result(format!("skipped: {reason}")),
        );
        record(source, "skipped", reason.clone(), false, 0, 0);
        return Ok(RunOutcome::Skipped { reason });
    }

    let mode = if apply_requested && config.dry_run_phase_complete() {
        RunMode::Apply
    } else {
        RunMode::DryRun
    };
    runtime.cancel.store(false, Ordering::Relaxed);
    let ctx = RunContext::with_cancel(
        crate::manifest::real_env(),
        appconfig::quarantine_dir(),
        appconfig::logs_dir(),
        runtime.cancel.clone(),
    );
    let report = cleaner::run(&manifest, &config, &ctx, mode);

    if report.dry_run && !report.cancelled {
        config.autonomous.dry_runs_completed =
            config.autonomous.dry_runs_completed.saturating_add(1);
        let _ = config.save_to(&cfg_path);
    }
    record(
        source,
        if report.cancelled {
            "cancelled"
        } else {
            "complete"
        },
        String::new(),
        report.dry_run,
        report.total_files,
        report.total_bytes,
    );
    Ok(RunOutcome::Completed {
        report: Box::new(report),
    })
}

/// The blueprint's low-disk trigger: a gated run is due when free
/// space on the system drive drops below the configured threshold.
/// A threshold of 0 disables the trigger. The run it starts goes
/// through the same pre-flight gate and dry-run phase as every other
/// trigger — this predicate only decides *when to try*.
pub fn low_disk_due(free_space_gb: f64, threshold_gb: u64) -> bool {
    threshold_gb > 0 && free_space_gb.is_finite() && free_space_gb < threshold_gb as f64
}

/// Entry for `--autonomous-run` (Task Scheduler). Exit codes: 0 ran or
/// cleanly skipped / not enabled, 2 on error.
pub fn headless_autonomous_run() -> i32 {
    let config = AppConfig::load_from(&appconfig::config_path());
    if !config.autonomous.enabled {
        return 0;
    }
    let runtime = SafetyRuntime::new();
    match run_safety_once(&runtime, true, "scheduled") {
        Ok(_) => 0,
        Err(e) => {
            record("scheduled", "error", e, false, 0, 0);
            2
        }
    }
}

/// Entry for `--clean-now [--apply]`: prints the outcome as JSON.
pub fn headless_clean_now(apply: bool) -> i32 {
    let runtime = SafetyRuntime::new();
    match run_safety_once(&runtime, apply, "cli") {
        Ok(outcome) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&outcome).unwrap_or_default()
            );
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_cancel_reports_running_state() {
        let rt = SafetyRuntime::new();
        assert!(!rt.is_running());
        assert!(!rt.request_cancel(), "no run in progress to cancel");
    }

    #[test]
    fn low_disk_trigger_respects_threshold_and_zero_disable() {
        assert!(low_disk_due(9.9, 10));
        assert!(!low_disk_due(10.0, 10));
        assert!(!low_disk_due(0.5, 0), "zero threshold disables the trigger");
        assert!(!low_disk_due(f64::INFINITY, 10));
    }

    #[test]
    fn last_run_round_trips() {
        let lr = LastRun {
            at: "2026-11-10T22:00:00Z".into(),
            source: "tray".into(),
            outcome: "complete".into(),
            detail: String::new(),
            dry_run: true,
            files: 3,
            bytes: 42,
        };
        let text = serde_json::to_string(&lr).unwrap();
        let back: LastRun = serde_json::from_str(&text).unwrap();
        assert_eq!(back.source, "tray");
        assert_eq!(back.bytes, 42);
    }
}
