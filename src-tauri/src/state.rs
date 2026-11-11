//! Shared application state, persisted settings and the error type used by every command.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::scanner::ScanReport;

/// Number of CPU samples kept per process for the sparklines (one per second).
pub const HISTORY_LEN: usize = 40;

#[derive(Debug, thiserror::Error)]
pub enum TidyError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("settings file is invalid: {0}")]
    Config(#[from] serde_json::Error),
    #[error("{0} is protected by the safe-list")]
    Protected(String),
    #[error("no scan results yet - run Smart Scan first")]
    NoScan,
    #[error("not available yet: {0}")]
    NotImplemented(&'static str),
    #[error("{0}")]
    Other(String),
}

impl Serialize for TidyError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type TidyResult<T> = Result<T, TidyError>;

/// User settings stored in `%APPDATA%\TidyTray\settings.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// When true, cleanup only reports what it would delete. Ships enabled.
    pub dry_run: bool,
    /// Temp files younger than this are left alone (installers in progress etc.).
    pub min_temp_age_hours: u64,
    /// Extra user-defined safe-list patterns (`*` wildcard, case-insensitive).
    pub safe_list: Vec<String>,
    /// Category ids that are unchecked by default in the cleanup checklist.
    pub unchecked_categories: Vec<String>,
    /// Threshold for the large file finder, in MiB.
    pub large_file_min_mb: u64,
    pub start_minimized: bool,
    pub scan_on_launch: bool,
    /// UI theme: "dark" (GitHub-style, the default) or "light" (the original).
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dry_run: true,
            min_temp_age_hours: 24,
            safe_list: Vec::new(),
            unchecked_categories: vec!["recycle-bin".into()],
            large_file_min_mb: 500,
            start_minimized: false,
            scan_on_launch: true,
            theme: "dark".to_string(),
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        base.join("TidyTray").join("settings.json")
    }

    /// Loads settings, falling back to defaults when the file is missing or broken.
    pub fn load() -> Self {
        match fs::read_to_string(Self::path()) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|e| {
                log::warn!("settings ignored: {e}");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn parse(text: &str) -> TidyResult<Self> {
        let mut s: Settings = serde_json::from_str(text)?;
        s.min_temp_age_hours = s.min_temp_age_hours.min(24 * 30);
        s.large_file_min_mb = s.large_file_min_mb.clamp(50, 1024 * 64);
        s.safe_list.retain(|p| !p.trim().is_empty());
        Ok(s)
    }

    pub fn save(&self) -> TidyResult<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        // Write to a sibling file first so a crash never leaves half a settings file.
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }
}

/// Everything the commands share. Each field has its own lock so the
/// background sampler never blocks a running scan.
pub struct AppState {
    pub sys: Mutex<System>,
    pub cpu_history: Mutex<HashMap<String, VecDeque<f32>>>,
    pub last_scan: Mutex<Option<ScanReport>>,
    pub settings: Mutex<Settings>,
}

impl AppState {
    pub fn new() -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_memory(sysinfo::MemoryRefreshKind::everything())
                .with_cpu(sysinfo::CpuRefreshKind::everything())
                .with_processes(ProcessRefreshKind::everything()),
        );
        Self {
            sys: Mutex::new(sys),
            cpu_history: Mutex::new(HashMap::new()),
            last_scan: Mutex::new(None),
            settings: Mutex::new(Settings::load()),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Called once per second by the sampler thread: refreshes processes and
    /// appends one CPU sample per process name (all instances summed).
    pub fn sample(&self) {
        let totals = {
            let Ok(mut sys) = self.sys.lock() else { return };
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            sys.refresh_processes(ProcessesToUpdate::All, true);
            let cores = sys.cpus().len().max(1) as f32;
            let mut totals: HashMap<String, f32> = HashMap::new();
            for p in sys.processes().values() {
                let name = p.name().to_string_lossy().to_lowercase();
                *totals.entry(name).or_default() += p.cpu_usage() / cores;
            }
            totals
        };
        if let Ok(mut hist) = self.cpu_history.lock() {
            hist.retain(|name, _| totals.contains_key(name));
            for (name, cpu) in totals {
                let ring = hist.entry(name).or_insert_with(|| VecDeque::with_capacity(HISTORY_LEN));
                if ring.len() == HISTORY_LEN {
                    ring.pop_front();
                }
                ring.push_back((cpu * 10.0).round() / 10.0);
            }
        }
    }

    pub fn history_of(&self, name: &str) -> Vec<f32> {
        self.cpu_history
            .lock()
            .ok()
            .and_then(|h| h.get(&name.to_lowercase()).map(|r| r.iter().copied().collect()))
            .unwrap_or_default()
    }
}

/// "12.4 GB" style formatting shared by the tray tooltip and logs.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_are_safe() {
        let s = Settings::default();
        assert!(s.dry_run, "cleanup must start in dry-run mode");
        assert!(s.min_temp_age_hours >= 24);
    }

    #[test]
    fn settings_parse_fills_missing_fields_and_clamps() {
        let s = Settings::parse(r#"{"dryRun": false, "largeFileMinMb": 1, "safeList": ["", "D:\\Projects\\*"]}"#).unwrap();
        assert!(!s.dry_run);
        assert_eq!(s.theme, "dark", "missing theme must default to dark");
        assert_eq!(s.large_file_min_mb, 50);
        assert_eq!(s.safe_list, vec!["D:\\Projects\\*".to_string()]);
        assert_eq!(s.min_temp_age_hours, 24);
    }

    #[test]
    fn human_bytes_formats_like_the_ui() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(13_314_398_618), "12 GB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }
}
