//! The autonomous cleaning engine.
//!
//! It can only ever act on the categories in the embedded manifest
//! (see `manifest.rs`). For each enabled category it expands the manifest's
//! path templates, walks them without following links, and applies the age,
//! lock and size-cap rules. In dry-run mode nothing is touched. In a real
//! run files are *moved to quarantine* — this module contains no file
//! deletion at all; permanent deletion lives only in `quarantine.rs`'s
//! retention purge.
//!
//! A run is cooperatively cancellable through an `AtomicBool` kill switch:
//! the flag is checked for every file, so a run halts well within 5 s.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime};

use serde::Serialize;

use crate::appconfig::AppConfig;
use crate::audit::{AuditLog, LogEntry};
use crate::lockcheck::is_locked;
use crate::manifest::{expand_template, Category, Manifest};
use crate::quarantine::Quarantine;
use crate::safelist::wildcard_match;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    DryRun,
    Apply,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryReport {
    pub id: String,
    pub files: u64,
    pub bytes: u64,
    pub skipped_locked: u64,
    pub skipped_age: u64,
    pub skipped_size_cap: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub run_id: String,
    pub dry_run: bool,
    pub cancelled: bool,
    pub duration_ms: u64,
    pub total_files: u64,
    pub total_bytes: u64,
    pub categories: Vec<CategoryReport>,
}

/// Everything a run needs, bundled so tests can point it at temp folders
/// with a fake environment instead of the real machine.
pub struct RunContext {
    pub env: HashMap<String, String>,
    pub quarantine: Quarantine,
    pub audit: AuditLog,
    pub cancel: std::sync::Arc<AtomicBool>,
}

impl RunContext {
    /// Constructor with a shared cancellation flag, so the tray kill switch
    /// and the UI can stop a run started elsewhere (blueprint §8.1).
    pub fn with_cancel(
        env: HashMap<String, String>,
        quarantine_root: PathBuf,
        logs_dir: PathBuf,
        cancel: std::sync::Arc<AtomicBool>,
    ) -> Self {
        Self {
            env,
            quarantine: Quarantine::new(quarantine_root),
            audit: AuditLog::new(logs_dir),
            cancel,
        }
    }

    /// The kill switch: wired to the tray Pause item in M2; the engine
    /// itself and its tests use it now.
    #[allow(dead_code)]
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Expands one manifest template into concrete root paths. A `*` segment
/// matches one directory level; a `*` inside the final segment matches file
/// names. Missing locations simply expand to nothing.
pub fn expand_template_paths(template: &str, env: &HashMap<String, String>) -> Vec<PathBuf> {
    let Some(expanded) = expand_template(template, env) else {
        return Vec::new();
    };
    let (root, segments) = split_root(&expanded);
    expand_segments(&segments, 0, PathBuf::from(root))
}

/// Splits an expanded Windows path into its root and remaining segments.
/// Keeping the root intact matters: joining segments onto an empty base
/// would turn `C:\Users` into the drive-relative `C:Users`. Pure string
/// work, so it is testable on any OS.
fn split_root(expanded: &str) -> (String, Vec<String>) {
    let bytes = expanded.as_bytes();
    let is_drive = bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic();
    if is_drive || expanded.starts_with("\\\\") {
        if let Some(i) = expanded[2..].find('\\') {
            let cut = i + 3; // root includes the separator after the drive/share
            let segments = expanded[cut..].split('\\').map(|s| s.to_string()).collect();
            return (expanded[..cut].to_string(), segments);
        }
    }
    (
        String::new(),
        expanded.split('\\').map(|s| s.to_string()).collect(),
    )
}

fn expand_segments(segments: &[String], idx: usize, base: PathBuf) -> Vec<PathBuf> {
    if idx == segments.len() {
        return vec![base];
    }
    let seg = &segments[idx];
    if seg.is_empty() {
        return expand_segments(segments, idx + 1, base);
    }
    if !seg.contains('*') {
        // Keep drive roots / UNC prefixes intact: the first segment of an
        // expanded Windows path is like `C:` and PathBuf handles the rest.
        return expand_segments(segments, idx + 1, base.join(seg));
    }
    let last = idx + 1 == segments.len();
    let Ok(entries) = fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if !wildcard_match(&seg.to_lowercase(), &name) {
            continue;
        }
        let ft = e.file_type().ok();
        // A wildcard segment matches directories; the final segment may
        // also match files (e.g. `thumbcache_*.db`).
        if ft.map(|t| t.is_dir()).unwrap_or(false)
            || (last && ft.map(|t| t.is_file()).unwrap_or(false))
        {
            out.extend(expand_segments(segments, idx + 1, e.path()));
        }
    }
    out
}

/// Collects candidate files under one root: regular files only, symlinks
/// and junctions never followed, folders themselves never returned.
fn collect_candidates(root: &Path, out: &mut Vec<(PathBuf, fs::Metadata)>) {
    let Ok(meta) = fs::metadata(root) else { return };
    if meta.is_file() {
        out.push((root.to_path_buf(), meta));
        return;
    }
    if !meta.is_dir() {
        return;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(m) = e.metadata() else { continue };
            let ft = m.file_type();
            if ft.is_symlink() || crate::scanner::is_reparse_point(&m) {
                continue;
            }
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                out.push((e.path(), m));
            }
        }
    }
}

fn old_enough(meta: &fs::Metadata, category: &Category, now: SystemTime) -> bool {
    let min = category.min_age();
    if min.is_zero() {
        return true;
    }
    meta.modified()
        .ok()
        .and_then(|m| now.duration_since(m).ok())
        .map(|age| age >= min)
        .unwrap_or(false)
}

/// Runs one cleaning pass over the enabled categories of `manifest`.
pub fn run(manifest: &Manifest, config: &AppConfig, ctx: &RunContext, mode: RunMode) -> RunReport {
    let started = Instant::now();
    let now = SystemTime::now();
    let run_id = crate::audit::new_run_id();
    let dry = mode == RunMode::DryRun;
    let log = &ctx.audit;
    let _ = log.append(&LogEntry::new(&run_id, "run_start").result(if dry {
        "dry_run"
    } else {
        "apply"
    }));

    let total_cap = config
        .autonomous
        .max_total_gb_per_run
        .saturating_mul(1024 * 1024 * 1024);
    let mut total_files = 0u64;
    let mut total_bytes = 0u64;
    let mut cancelled = false;
    let mut reports = Vec::new();

    for category in &manifest.categories {
        if cancelled {
            break;
        }
        if !config
            .autonomous
            .categories_enabled
            .iter()
            .any(|id| id == &category.id)
        {
            continue;
        }
        let mut rep = CategoryReport {
            id: category.id.clone(),
            ..Default::default()
        };
        let mut roots = Vec::new();
        for t in &category.paths {
            roots.extend(expand_template_paths(t, &ctx.env));
        }
        let mut candidates = Vec::new();
        for root in &roots {
            collect_candidates(root, &mut candidates);
        }
        for (path, meta) in candidates {
            if ctx.cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            let size = meta.len();
            if !old_enough(&meta, category, now) {
                rep.skipped_age += 1;
                let _ = log.append(
                    &LogEntry::new(&run_id, "skip_age")
                        .category(&category.id)
                        .path(&path)
                        .size(size),
                );
                continue;
            }
            if rep.bytes.saturating_add(size) > category.max_bytes()
                || total_bytes.saturating_add(size) > total_cap
            {
                rep.skipped_size_cap += 1;
                let _ = log.append(
                    &LogEntry::new(&run_id, "skip_size_cap")
                        .category(&category.id)
                        .path(&path)
                        .size(size),
                );
                continue;
            }
            if category.skip_locked && is_locked(&path) {
                rep.skipped_locked += 1;
                let _ = log.append(
                    &LogEntry::new(&run_id, "skip_locked")
                        .category(&category.id)
                        .path(&path)
                        .size(size),
                );
                continue;
            }
            if dry {
                rep.files += 1;
                rep.bytes += size;
                total_files += 1;
                total_bytes += size;
                let _ = log.append(
                    &LogEntry::new(&run_id, "delete")
                        .category(&category.id)
                        .path(&path)
                        .size(size)
                        .result("dry_run"),
                );
                continue;
            }
            match ctx.quarantine.move_file(&run_id, &category.id, &path) {
                Ok(entry) => {
                    rep.files += 1;
                    rep.bytes += size;
                    total_files += 1;
                    total_bytes += size;
                    let _ = log.append(
                        &LogEntry::new(&run_id, "delete")
                            .category(&category.id)
                            .path(&path)
                            .size(size)
                            .result("quarantined")
                            .quarantine_path(Path::new(&entry.quarantine_path)),
                    );
                }
                Err(e) => {
                    rep.errors += 1;
                    let _ = log.append(
                        &LogEntry::new(&run_id, "error")
                            .category(&category.id)
                            .path(&path)
                            .size(size)
                            .result(e.to_string()),
                    );
                }
            }
        }
        reports.push(rep);
    }

    // Post-run housekeeping: expired quarantine runs are purged (the only
    // permanent deletion in the engine, and only of quarantined files).
    let retention =
        std::time::Duration::from_secs(config.quarantine.retention_days.saturating_mul(86_400));
    let purged = ctx.quarantine.purge_expired(retention, SystemTime::now());
    if purged > 0 {
        let _ = log.append(
            &LogEntry::new(&run_id, "quarantine_purge")
                .size(purged)
                .result("retention_expired"),
        );
    }

    let duration_ms = started.elapsed().as_millis() as u64;
    let mut end = LogEntry::new(&run_id, "run_end").size(total_bytes);
    end.duration_ms = Some(duration_ms);
    let _ = log.append(&end.result(if cancelled { "cancelled" } else { "complete" }));

    RunReport {
        run_id,
        dry_run: dry,
        cancelled,
        duration_ms,
        total_files,
        total_bytes,
        categories: reports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Fixture {
        base: PathBuf,
        ctx: RunContext,
        config: AppConfig,
    }

    fn fixture(tag: &str) -> Fixture {
        let base =
            std::env::temp_dir().join(format!("tidytray-clean-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let temp_root = base.join("TempRoot");
        fs::create_dir_all(temp_root.join("sub")).unwrap();
        let mut old = fs::File::create(temp_root.join("old.tmp")).unwrap();
        old.write_all(&[1u8; 100]).unwrap();
        let mut old2 = fs::File::create(temp_root.join("sub").join("old2.tmp")).unwrap();
        old2.write_all(&[2u8; 200]).unwrap();
        let mut fresh = fs::File::create(temp_root.join("fresh.tmp")).unwrap();
        fresh.write_all(&[3u8; 50]).unwrap();
        // Age the "old" files 10 days into the past via the manifest trick:
        // instead of touching mtimes (no portable API in std), the test
        // manifest uses min_age_days 0 for its category and a second
        // category with a huge age to exercise the age skip.
        let mut env = HashMap::new();
        env.insert("TEMP".to_string(), temp_root.to_string_lossy().into_owned());
        let ctx = RunContext::with_cancel(
            env,
            base.join("Quarantine"),
            base.join("Logs"),
            std::sync::Arc::new(AtomicBool::new(false)),
        );
        let mut config = AppConfig::default();
        config.autonomous.categories_enabled = vec!["gpu_shader_cache".into()];
        Fixture { base, ctx, config }
    }

    fn test_manifest(min_age_days: u64) -> Manifest {
        let cats = serde_json::json!([{
            "id": "gpu_shader_cache", "display_name": "GPU Shader Cache",
            "paths": ["${TEMP}"], "min_age_days": min_age_days,
            "requires_admin": false, "skip_locked": true,
            "max_gb_per_run": 5, "enabled_by_default": true, "tier": 1
        }]);
        let canon = serde_json::to_string(&cats).unwrap();
        let doc = serde_json::json!({
            "schema_version": 1, "manifest_version": "test",
            "checksum": crate::sha256::sha256_hex(canon.as_bytes()),
            "categories": cats
        });
        Manifest::parse(&doc.to_string()).unwrap()
    }

    #[test]
    fn dry_run_reports_but_moves_nothing() {
        let fx = fixture("dry");
        let report = run(&test_manifest(0), &fx.config, &fx.ctx, RunMode::DryRun);
        assert_eq!(report.total_files, 3);
        assert_eq!(report.total_bytes, 350);
        assert!(fx.base.join("TempRoot").join("old.tmp").exists());
        assert!(fx.ctx.quarantine.entries().is_empty());
        fs::remove_dir_all(fx.base).unwrap();
    }

    #[test]
    fn apply_run_quarantines_and_can_restore() {
        let fx = fixture("apply");
        let report = run(&test_manifest(0), &fx.config, &fx.ctx, RunMode::Apply);
        assert_eq!(report.total_files, 3);
        assert!(!fx.base.join("TempRoot").join("old.tmp").exists());
        let entries = fx.ctx.quarantine.entries();
        assert_eq!(entries.len(), 3);
        let e = entries
            .iter()
            .find(|e| e.original_path.ends_with("old.tmp"))
            .unwrap();
        fx.ctx
            .quarantine
            .restore(Path::new(&e.quarantine_path))
            .unwrap();
        assert_eq!(
            fs::read(fx.base.join("TempRoot").join("old.tmp")).unwrap(),
            vec![1u8; 100]
        );
        fs::remove_dir_all(fx.base).unwrap();
    }

    #[test]
    fn age_rule_skips_everything_when_threshold_is_huge() {
        let fx = fixture("age");
        let report = run(&test_manifest(36_500), &fx.config, &fx.ctx, RunMode::Apply);
        assert_eq!(report.total_files, 0);
        assert_eq!(report.categories[0].skipped_age, 3);
        fs::remove_dir_all(fx.base).unwrap();
    }

    #[test]
    fn cancel_flag_stops_the_run() {
        let fx = fixture("cancel");
        fx.ctx.request_cancel();
        let report = run(&test_manifest(0), &fx.config, &fx.ctx, RunMode::Apply);
        assert!(report.total_files <= 1, "cancel must stop the run promptly");
        fs::remove_dir_all(fx.base).unwrap();
    }

    #[test]
    fn disabled_categories_are_not_touched() {
        let mut fx = fixture("disabled");
        fx.config.autonomous.categories_enabled = vec!["temp_user".into()];
        let report = run(&test_manifest(0), &fx.config, &fx.ctx, RunMode::Apply);
        assert_eq!(report.total_files, 0);
        assert!(fx.base.join("TempRoot").join("old.tmp").exists());
        fs::remove_dir_all(fx.base).unwrap();
    }

    #[test]
    fn split_root_keeps_windows_drive_root_intact() {
        // Regression: joining segments onto an empty base produced the
        // drive-relative `C:Users\...` on Windows (CI caught this; the
        // Linux harness could not).
        let (root, segs) = split_root(r"C:\Users\a\AppData\Local\Temp");
        assert_eq!(root, r"C:\");
        assert_eq!(segs, vec!["Users", "a", "AppData", "Local", "Temp"]);
        let (root, segs) = split_root(r"D:\Cache");
        assert_eq!(root, r"D:\");
        assert_eq!(segs, vec!["Cache"]);
        // Non-Windows-shaped strings keep the plain split.
        let (root, segs) = split_root("/tmp/x\\y");
        assert_eq!(root, "");
        assert_eq!(segs, vec!["/tmp/x", "y"]);
    }

    #[test]
    fn template_expansion_handles_profile_wildcards() {
        let base = std::env::temp_dir().join(format!("tidytray-expand-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let prof = base
            .join("User Data")
            .join("Default")
            .join("Cache")
            .join("Cache_Data");
        fs::create_dir_all(&prof).unwrap();
        let mut env = HashMap::new();
        env.insert(
            "LOCALAPPDATA".to_string(),
            base.to_string_lossy().into_owned(),
        );
        let got = expand_template_paths(r"${LOCALAPPDATA}\User Data\*\Cache\Cache_Data", &env);
        assert_eq!(got, vec![prof]);
        fs::remove_dir_all(base).unwrap();
    }
}
