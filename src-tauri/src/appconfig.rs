//! TidyTray's own settings for the autonomous engine (`config.json` in
//! `%LOCALAPPDATA%\TidyTray`), separate from the legacy Tidy window settings.
//!
//! The file carries a `schema_version`; migrations only ever move upward and
//! are explicit. Defaults are conservative: autonomous cleanup starts
//! disabled, and the engine stays in dry-run mode for its first 3 runs.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Schedule {
    pub mode: String,
    pub day: String,
    pub hour: u32,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            mode: "weekly".into(),
            day: "Sunday".into(),
            hour: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Autonomous {
    pub enabled: bool,
    pub dry_runs_completed: u32,
    pub schedule: Schedule,
    pub low_disk_trigger_gb: u64,
    pub pause_on_battery_below_pct: u32,
    pub pause_on_fullscreen: bool,
    pub max_total_gb_per_run: u64,
    pub categories_enabled: Vec<String>,
}

impl Default for Autonomous {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_runs_completed: 0,
            schedule: Schedule::default(),
            low_disk_trigger_gb: 10,
            pause_on_battery_below_pct: 30,
            pause_on_fullscreen: true,
            max_total_gb_per_run: 20,
            categories_enabled: [
                "temp_user",
                "browser_cache_chromium",
                "browser_cache_firefox",
                "gpu_shader_cache",
                "thumbnail_cache",
                "crash_dumps",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct QuarantineCfg {
    pub retention_days: u64,
}

impl Default for QuarantineCfg {
    fn default() -> Self {
        Self { retention_days: 7 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Ui {
    pub show_notifications: bool,
    pub theme: String,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            show_notifications: true,
            theme: "system".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub schema_version: u32,
    pub autonomous: Autonomous,
    pub quarantine: QuarantineCfg,
    pub ui: Ui,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            autonomous: Autonomous::default(),
            quarantine: QuarantineCfg::default(),
            ui: Ui::default(),
        }
    }
}

impl AppConfig {
    /// Parses config text, migrating older schemas upward. A file from a
    /// newer schema is refused rather than silently mangled.
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let version = v
            .get("schema_version")
            .and_then(|s| s.as_u64())
            .unwrap_or(0) as u32;
        if version > CURRENT_SCHEMA_VERSION {
            return Err(format!(
                "config schema_version {version} is newer than this build supports"
            ));
        }
        // Migration 0 → 1: version 0 means "no version field" (fresh/default
        // shape); parsing with serde defaults and stamping version 1 *is* the
        // migration. Future bumps add explicit steps here.
        let mut cfg: AppConfig = serde_json::from_value(v).map_err(|e| e.to_string())?;
        cfg.schema_version = CURRENT_SCHEMA_VERSION;
        cfg.autonomous.pause_on_battery_below_pct =
            cfg.autonomous.pause_on_battery_below_pct.min(100);
        cfg.autonomous.max_total_gb_per_run = cfg.autonomous.max_total_gb_per_run.clamp(1, 1024);
        Ok(cfg)
    }

    pub fn load_from(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|e| {
                log::warn!("TidyTray config ignored ({e}); using defaults");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Atomic save: write a sibling temp file, then rename over the target.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap_or_default())?;
        fs::rename(tmp, path)
    }

    /// True once the first-run dry-run phase is complete (blueprint §8.1).
    pub fn dry_run_phase_complete(&self) -> bool {
        self.autonomous.dry_runs_completed >= 3
    }
}

/// Root data folder: `%LOCALAPPDATA%\TidyTray` (OS temp dir as a fallback).
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("TidyTray")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

pub fn quarantine_dir() -> PathBuf {
    data_dir().join("Quarantine")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("Logs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative() {
        let c = AppConfig::default();
        assert!(
            !c.autonomous.enabled,
            "autonomous cleanup must start disabled"
        );
        assert_eq!(c.autonomous.dry_runs_completed, 0);
        assert_eq!(c.quarantine.retention_days, 7);
        assert!(!c.dry_run_phase_complete());
    }

    #[test]
    fn unversioned_config_migrates_to_current() {
        let c = AppConfig::parse(r#"{"autonomous":{"dry_runs_completed":3}}"#).unwrap();
        assert_eq!(c.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(c.dry_run_phase_complete());
        assert_eq!(
            c.autonomous.schedule.hour, 3,
            "missing fields take defaults"
        );
    }

    #[test]
    fn newer_schema_is_refused() {
        assert!(AppConfig::parse(r#"{"schema_version":99}"#).is_err());
    }

    #[test]
    fn save_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("tidytray-cfg-{}", std::process::id()));
        let path = dir.join("config.json");
        let mut c = AppConfig::default();
        c.autonomous.dry_runs_completed = 2;
        c.save_to(&path).unwrap();
        assert_eq!(AppConfig::load_from(&path), c);
        fs::remove_dir_all(dir).unwrap();
    }
}
