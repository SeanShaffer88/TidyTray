//! Append-only JSONL audit log for the autonomous cleaner.
//!
//! One file per day (`clean-YYYY-MM-DD.jsonl`) under the TidyTray data folder.
//! Every autonomous action — run start/end, quarantine move, skip, restore,
//! purge — is recorded here so the user can always see what happened.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// One audit-log event. Field names follow the blueprint log schema (§7.2).
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    /// ISO-8601 UTC timestamp, e.g. `2026-01-15T10:32:11.123Z`.
    pub ts: String,
    pub run_id: String,
    pub event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quarantine_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

impl LogEntry {
    pub fn new(run_id: &str, event: &'static str) -> Self {
        Self {
            ts: iso_now(),
            run_id: run_id.to_string(),
            event,
            category: None,
            path: None,
            size: None,
            result: None,
            quarantine_path: None,
            duration_ms: None,
        }
    }

    pub fn category(mut self, id: &str) -> Self {
        self.category = Some(id.to_string());
        self
    }

    pub fn path(mut self, p: &std::path::Path) -> Self {
        self.path = Some(p.to_string_lossy().into_owned());
        self
    }

    pub fn size(mut self, n: u64) -> Self {
        self.size = Some(n);
        self
    }

    pub fn result(mut self, r: impl Into<String>) -> Self {
        self.result = Some(r.into());
        self
    }

    pub fn quarantine_path(mut self, p: &std::path::Path) -> Self {
        self.quarantine_path = Some(p.to_string_lossy().into_owned());
        self
    }
}

/// Append-only writer rooted at a logs directory.
#[derive(Debug, Clone)]
pub struct AuditLog {
    dir: PathBuf,
}

impl AuditLog {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Kept for the M2 logs viewer, which lists files from this directory.
    #[allow(dead_code)]
    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    /// Appends one entry to today's file, creating the directory if needed.
    pub fn append(&self, entry: &LogEntry) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let file = self.dir.join(format!("clean-{}.jsonl", &entry.ts[..10]));
        let mut f = OpenOptions::new().create(true).append(true).open(file)?;
        let line = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
        writeln!(f, "{line}")
    }
}

/// Generates a run id from time, process and a counter — unique enough for a
/// local log/quarantine folder name without an extra dependency.
pub fn new_run_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{ms:x}-{:x}-{n:x}", std::process::id())
}

/// Current UTC time as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn iso_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    iso_from_unix(now.as_secs(), now.subsec_millis())
}

pub fn iso_from_unix(secs: u64, millis: u32) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        tod / 3600,
        (tod / 60) % 60,
        tod % 60
    )
}

/// Days-since-epoch → (year, month, day). Howard Hinnant's civil calendar
/// algorithm; avoids a date/time dependency.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_089), (2025, 1, 1)); // 2025-01-01
        assert_eq!(iso_from_unix(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_unix(1_735_689_600, 5), "2025-01-01T00:00:00.005Z");
    }

    #[test]
    fn append_writes_valid_jsonl() {
        let dir = std::env::temp_dir().join(format!("tidytray-audit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let log = AuditLog::new(dir.clone());
        let e = LogEntry::new("run1", "run_start");
        log.append(&e).unwrap();
        let files: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        let text = fs::read_to_string(files[0].path()).unwrap();
        let v: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(v["event"], "run_start");
        assert_eq!(v["run_id"], "run1");
        fs::remove_dir_all(dir).unwrap();
    }
}
