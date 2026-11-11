//! Smart Scan: a parallel directory walker over well-known junk locations, with
//! per-category size accounting and a dry-run-first cleaner.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::safelist::{expand_env, SafeList};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Regenerated automatically; removing it has no visible effect.
    None,
    /// Regenerated, but the next launch or first page load is slower.
    Low,
    /// Cannot be undone from inside Windows (Recycle Bin, update rollback).
    Review,
}

/// One cleanup category, e.g. "Browser caches", resolved to concrete folders.
#[derive(Debug, Clone)]
pub struct Target {
    pub id: &'static str,
    pub label: &'static str,
    pub explain: &'static str,
    pub risk: Risk,
    pub roots: Vec<PathBuf>,
    /// Only count files older than this (protects files that are in use right now).
    pub min_age: Duration,
    /// Optional file-name filter (`thumbcache_*` for the Explorer thumbnail cache).
    pub name_filter: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSample {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResult {
    pub id: String,
    pub label: String,
    pub explain: String,
    pub risk: Risk,
    pub bytes: u64,
    pub files: u64,
    pub protected: u64,
    pub roots: Vec<String>,
    pub largest: Vec<FileSample>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub duration_ms: u64,
    pub total_bytes: u64,
    pub total_files: u64,
    pub threads: usize,
    pub categories: Vec<CategoryResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub dry_run: bool,
    pub freed_bytes: u64,
    pub removed_files: u64,
    /// Files moved into quarantine by a real run (equal to removed_files
    /// unless a move failed); they stay restorable for the retention window.
    pub quarantined_files: u64,
    pub skipped_in_use: u64,
    pub skipped_protected: u64,
    pub per_category: Vec<(String, u64)>,
}

fn env_path(p: &str) -> Option<PathBuf> {
    expand_env(p).map(PathBuf::from)
}

/// Chromium keeps one cache per profile ("Default", "Profile 1", ...).
fn chromium_caches(user_data: &str) -> Vec<PathBuf> {
    let Some(base) = env_path(user_data) else {
        return vec![];
    };
    let Ok(entries) = fs::read_dir(&base) else {
        return vec![];
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name == "Default" || name.starts_with("Profile ") {
            for sub in [
                "Cache",
                "Code Cache",
                "GPUCache",
                "Service Worker\\CacheStorage",
            ] {
                out.push(e.path().join(sub));
            }
        }
    }
    out.push(base.join("ShaderCache"));
    out.push(base.join("GrShaderCache"));
    out
}

fn firefox_caches() -> Vec<PathBuf> {
    let Some(base) = env_path(r"%LOCALAPPDATA%\Mozilla\Firefox\Profiles") else {
        return vec![];
    };
    fs::read_dir(base)
        .map(|it| it.flatten().map(|e| e.path().join("cache2")).collect())
        .unwrap_or_default()
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().filter_map(|p| env_path(p)).collect()
}

/// The default Smart Scan plan. Every root is still checked against the safe-list.
pub fn default_targets(min_temp_age_hours: u64) -> Vec<Target> {
    let day = Duration::from_secs(min_temp_age_hours * 3600);
    let mut browser = chromium_caches(r"%LOCALAPPDATA%\Google\Chrome\User Data");
    browser.extend(chromium_caches(r"%LOCALAPPDATA%\Microsoft\Edge\User Data"));
    browser.extend(chromium_caches(
        r"%LOCALAPPDATA%\BraveSoftware\Brave-Browser\User Data",
    ));
    browser.extend(firefox_caches());
    vec![
        Target {
            id: "temp",
            label: "Temporary files",
            explain: "Leftovers from installers and apps. Only files older than a day are counted.",
            risk: Risk::None,
            roots: paths(&[r"%TEMP%", r"%WINDIR%\Temp"]),
            min_age: day,
            name_filter: None,
        },
        Target {
            id: "browser",
            label: "Browser caches",
            explain: "Images and scripts saved by Chrome, Edge, Brave and Firefox. Sites load a bit slower once.",
            risk: Risk::Low,
            roots: browser,
            min_age: Duration::ZERO,
            name_filter: None,
        },
        Target {
            id: "shader",
            label: "Shader caches",
            explain: "Compiled GPU shaders (NVIDIA, AMD, DirectX). Games rebuild them on the next launch.",
            risk: Risk::Low,
            roots: paths(&[
                r"%LOCALAPPDATA%\NVIDIA\DXCache",
                r"%LOCALAPPDATA%\NVIDIA\GLCache",
                r"%LOCALAPPDATA%\D3DSCache",
                r"%LOCALAPPDATA%\AMD\DxCache",
                r"%LOCALAPPDATA%\AMD\DxcCache",
            ]),
            min_age: Duration::ZERO,
            name_filter: None,
        },
        Target {
            id: "windows-update",
            label: "Windows Update cache",
            explain: "Update packages that are already installed. Needs admin rights to remove.",
            risk: Risk::Review,
            roots: paths(&[
                r"%WINDIR%\SoftwareDistribution\Download",
                r"%WINDIR%\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
            ]),
            min_age: day,
            name_filter: None,
        },
        Target {
            id: "crash-dumps",
            label: "Crash dumps & error reports",
            explain: "Memory dumps written when an app or Windows crashed. Useful only for debugging.",
            risk: Risk::None,
            roots: paths(&[
                r"%LOCALAPPDATA%\CrashDumps",
                r"%WINDIR%\Minidump",
                r"%PROGRAMDATA%\Microsoft\Windows\WER\ReportArchive",
                r"%PROGRAMDATA%\Microsoft\Windows\WER\ReportQueue",
            ]),
            min_age: Duration::ZERO,
            name_filter: None,
        },
        Target {
            id: "thumbnails",
            label: "Thumbnail cache",
            explain: "Explorer preview images. Rebuilt as you browse folders.",
            risk: Risk::None,
            roots: paths(&[r"%LOCALAPPDATA%\Microsoft\Windows\Explorer"]),
            min_age: Duration::ZERO,
            name_filter: Some("thumbcache_*"),
        },
        Target {
            id: "recycle-bin",
            label: "Recycle Bin",
            explain: "Files you deleted. Emptying it cannot be undone.",
            risk: Risk::Review,
            roots: paths(&[r"%SystemDrive%\$Recycle.Bin"]),
            min_age: Duration::ZERO,
            name_filter: None,
        },
    ]
}

/// Per-category counters updated lock-free by the walker threads.
#[derive(Default)]
struct Tally {
    bytes: AtomicU64,
    files: AtomicU64,
    protected: AtomicU64,
    largest: Mutex<Vec<FileSample>>,
}

impl Tally {
    fn add(&self, path: &Path, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.files.fetch_add(1, Ordering::Relaxed);
        if bytes < 1 << 20 {
            return; // only files >= 1 MiB are worth listing
        }
        if let Ok(mut top) = self.largest.lock() {
            top.push(FileSample {
                path: path.to_string_lossy().into_owned(),
                bytes,
            });
            top.sort_by_key(|s| std::cmp::Reverse(s.bytes));
            top.truncate(5);
        }
    }
}

/// Work queue shared by the walker threads: directories still to visit plus a
/// count of threads currently busy, so idle threads know when the walk is over.
struct Queue {
    dirs: Mutex<(VecDeque<(usize, PathBuf)>, usize)>,
    ready: Condvar,
}

/// Walks every root of every job in parallel with plain std threads and calls
/// `visit(job_index, path, metadata)` for each regular file. Junctions and
/// symlinks are never followed, so a scan cannot escape its roots.
pub fn walk_parallel<F>(roots: Vec<(usize, PathBuf)>, threads: usize, visit: F)
where
    F: Fn(usize, &Path, &fs::Metadata) + Send + Sync,
{
    let queue = Arc::new(Queue {
        dirs: Mutex::new((roots.into_iter().collect(), 0)),
        ready: Condvar::new(),
    });
    thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            let queue = Arc::clone(&queue);
            let visit = &visit;
            scope.spawn(move || loop {
                let job = {
                    let mut guard = queue.dirs.lock().unwrap();
                    loop {
                        if let Some(job) = guard.0.pop_front() {
                            guard.1 += 1;
                            break Some(job);
                        }
                        if guard.1 == 0 {
                            break None;
                        }
                        guard = queue.ready.wait(guard).unwrap();
                    }
                };
                let Some((idx, dir)) = job else {
                    queue.ready.notify_all();
                    return;
                };
                let mut found = Vec::new();
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let Ok(meta) = entry.metadata() else { continue };
                        let ft = meta.file_type();
                        if ft.is_symlink() || is_reparse_point(&meta) {
                            continue;
                        }
                        if ft.is_dir() {
                            found.push((idx, entry.path()));
                        } else if ft.is_file() {
                            visit(idx, &entry.path(), &meta);
                        }
                    }
                }
                let mut guard = queue.dirs.lock().unwrap();
                guard.0.extend(found);
                guard.1 -= 1;
                drop(guard);
                queue.ready.notify_all();
            });
        }
    });
}

#[cfg(windows)]
pub(crate) fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(crate) fn is_reparse_point(_meta: &fs::Metadata) -> bool {
    false
}

fn old_enough(meta: &fs::Metadata, min_age: Duration, now: SystemTime) -> bool {
    if min_age.is_zero() {
        return true;
    }
    meta.modified()
        .ok()
        .and_then(|m| now.duration_since(m).ok())
        .is_some_and(|age| age >= min_age)
}

fn name_ok(target: &Target, path: &Path) -> bool {
    match target.name_filter {
        None => true,
        Some(f) => path.file_name().is_some_and(|n| {
            crate::safelist::wildcard_match(f, &n.to_string_lossy().to_lowercase())
        }),
    }
}

pub fn worker_count() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, 12)
}

fn job_list(targets: &[Target], safe: &SafeList) -> Vec<(usize, PathBuf)> {
    targets
        .iter()
        .enumerate()
        .flat_map(|(i, t)| t.roots.iter().map(move |r| (i, r.clone())))
        .filter(|(_, r)| r.is_dir() && safe.protects(r).is_none())
        .collect()
}

/// Runs Smart Scan over `targets`. Typically finishes in 1-3 s on an SSD.
pub fn scan(targets: &[Target], safe: &SafeList, unchecked: &[String]) -> ScanReport {
    let started = Instant::now();
    let now = SystemTime::now();
    let tallies: Vec<Tally> = targets.iter().map(|_| Tally::default()).collect();
    let threads = worker_count();
    walk_parallel(job_list(targets, safe), threads, |idx, path, meta| {
        let target = &targets[idx];
        if !name_ok(target, path) || !old_enough(meta, target.min_age, now) {
            return;
        }
        if safe.protects(path).is_some() {
            tallies[idx].protected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        tallies[idx].add(path, meta.len());
    });
    let categories: Vec<CategoryResult> = targets
        .iter()
        .zip(tallies)
        .map(|(t, tally)| CategoryResult {
            id: t.id.into(),
            label: t.label.into(),
            explain: t.explain.into(),
            risk: t.risk,
            bytes: tally.bytes.into_inner(),
            files: tally.files.into_inner(),
            protected: tally.protected.into_inner(),
            roots: t
                .roots
                .iter()
                .map(|r| r.to_string_lossy().into_owned())
                .collect(),
            largest: tally.largest.into_inner().unwrap_or_default(),
            selected: !unchecked.iter().any(|u| u == t.id),
        })
        .collect();
    ScanReport {
        duration_ms: started.elapsed().as_millis() as u64,
        total_bytes: categories.iter().map(|c| c.bytes).sum(),
        total_files: categories.iter().map(|c| c.files).sum(),
        threads,
        categories,
    }
}

/// Cleans the selected categories by moving files into `quarantine` under
/// `run_id` (blueprint M3: Smart Clean no longer deletes directly). With
/// `dry_run` nothing is moved: the walk runs exactly the same way and only
/// the counters are reported. Folders themselves (and the category roots)
/// are always kept.
pub fn clean(
    targets: &[Target],
    selected: &[String],
    safe: &SafeList,
    dry_run: bool,
    quarantine: &crate::quarantine::Quarantine,
    run_id: &str,
) -> CleanReport {
    let chosen: Vec<Target> = targets
        .iter()
        .filter(|t| selected.iter().any(|s| s == t.id))
        .cloned()
        .collect();
    let now = SystemTime::now();
    let freed: Vec<AtomicU64> = chosen.iter().map(|_| AtomicU64::new(0)).collect();
    let (removed, in_use, protected) = (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0));
    walk_parallel(
        job_list(&chosen, safe),
        worker_count(),
        |idx, path, meta| {
            let target = &chosen[idx];
            if !name_ok(target, path) || !old_enough(meta, target.min_age, now) {
                return;
            }
            if safe.protects(path).is_some() {
                protected.fetch_add(1, Ordering::Relaxed);
                return;
            }
            if target.id == "recycle-bin" && !dry_run {
                // TODO(owner): call SHEmptyRecycleBinW (needs the Win32_UI_Shell feature)
                // instead of deleting $R/$I pairs by hand, so Explorer stays in sync.
                return;
            }
            let ok = if dry_run {
                true
            } else {
                quarantine.move_file(run_id, target.id, path).is_ok()
            };
            if ok {
                freed[idx].fetch_add(meta.len(), Ordering::Relaxed);
                removed.fetch_add(1, Ordering::Relaxed);
            } else {
                in_use.fetch_add(1, Ordering::Relaxed);
            }
        },
    );
    let per_category: Vec<(String, u64)> = chosen
        .iter()
        .zip(&freed)
        .map(|(t, f)| (t.id.to_string(), f.load(Ordering::Relaxed)))
        .collect();
    let removed_files = removed.into_inner();
    CleanReport {
        dry_run,
        freed_bytes: per_category.iter().map(|(_, b)| b).sum(),
        removed_files,
        quarantined_files: if dry_run { 0 } else { removed_files },
        skipped_in_use: in_use.into_inner(),
        skipped_protected: protected.into_inner(),
        per_category,
    }
}

/// Realistic report for demo mode (`TIDY_DEMO=1`) and UI development.
pub fn sample_report() -> ScanReport {
    const GB: u64 = 1024 * 1024 * 1024;
    let rows: [(&str, &str, Risk, f64, u64); 7] = [
        ("temp", "Temporary files", Risk::None, 4.12, 18_420),
        ("browser", "Browser caches", Risk::Low, 3.05, 41_337),
        ("shader", "Shader caches", Risk::Low, 2.36, 3_104),
        (
            "windows-update",
            "Windows Update cache",
            Risk::Review,
            1.74,
            212,
        ),
        (
            "crash-dumps",
            "Crash dumps & error reports",
            Risk::None,
            0.82,
            37,
        ),
        ("thumbnails", "Thumbnail cache", Risk::None, 0.31, 14),
        ("recycle-bin", "Recycle Bin", Risk::Review, 0.42, 96),
    ];
    let categories: Vec<CategoryResult> = rows
        .iter()
        .map(|(id, label, risk, gb, files)| CategoryResult {
            id: id.to_string(),
            label: label.to_string(),
            explain: String::new(),
            risk: *risk,
            bytes: (gb * GB as f64) as u64,
            files: *files,
            protected: 0,
            roots: vec![],
            largest: vec![],
            selected: *id != "recycle-bin",
        })
        .collect();
    ScanReport {
        duration_ms: 2_840,
        total_bytes: categories.iter().map(|c| c.bytes).sum(),
        total_files: categories.iter().map(|c| c.files).sum(),
        threads: 8,
        categories,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tidy-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b/c")).unwrap();
        fs::write(root.join("one.tmp"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("a/two.tmp"), vec![0u8; 2000]).unwrap();
        fs::write(root.join("a/b/c/three.tmp"), vec![0u8; 3000]).unwrap();
        fs::write(root.join("a/b/keep.kdbx"), vec![0u8; 500]).unwrap();
        root
    }

    fn target(root: &Path) -> Target {
        Target {
            id: "temp",
            label: "Temp",
            explain: "",
            risk: Risk::None,
            roots: vec![root.to_path_buf()],
            min_age: Duration::ZERO,
            name_filter: None,
        }
    }

    #[test]
    fn scan_counts_nested_files_and_respects_safelist() {
        let root = temp_tree("scan");
        let report = scan(&[target(&root)], &SafeList::with_roots(&[], &[]), &[]);
        let cat = &report.categories[0];
        assert_eq!(cat.bytes, 6000);
        assert_eq!(cat.files, 3);
        assert_eq!(cat.protected, 1);
        fs::remove_dir_all(root).unwrap();
    }

    fn temp_quarantine(tag: &str) -> (PathBuf, crate::quarantine::Quarantine) {
        let dir = std::env::temp_dir().join(format!("tidy-test-q-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let q = crate::quarantine::Quarantine::new(dir.clone());
        (dir, q)
    }

    #[test]
    fn dry_run_deletes_nothing() {
        let root = temp_tree("dry");
        let t = [target(&root)];
        let (_qd, q) = temp_quarantine("dry");
        let report = clean(
            &t,
            &["temp".into()],
            &SafeList::with_roots(&[], &[]),
            true,
            &q,
            "run-dry",
        );
        assert_eq!(report.freed_bytes, 6000);
        assert_eq!(report.quarantined_files, 0);
        assert!(root.join("a/b/c/three.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn real_clean_keeps_protected_files_and_folders() {
        let root = temp_tree("real");
        let t = [target(&root)];
        let (qd, q) = temp_quarantine("real");
        let report = clean(
            &t,
            &["temp".into()],
            &SafeList::with_roots(&[], &[]),
            false,
            &q,
            "run-real",
        );
        assert_eq!(report.removed_files, 3);
        assert_eq!(report.quarantined_files, 3);
        assert!(root.join("a/b/keep.kdbx").exists());
        assert!(root.join("a/b/c").is_dir());
        // Moved, not deleted: every file is in quarantine and restorable.
        assert!(!root.join("one.tmp").exists());
        let entries = q.entries();
        assert_eq!(entries.len(), 3);
        let restored = q
            .restore(std::path::Path::new(&entries[0].quarantine_path))
            .unwrap();
        assert!(restored.exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(qd).unwrap();
    }

    #[test]
    fn unselected_categories_are_untouched() {
        let root = temp_tree("unsel");
        let (_qd, q) = temp_quarantine("unsel");
        let report = clean(
            &[target(&root)],
            &["browser".into()],
            &SafeList::with_roots(&[], &[]),
            false,
            &q,
            "run-unsel",
        );
        assert_eq!(report.removed_files, 0);
        assert!(root.join("one.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sample_report_totals_add_up() {
        let r = sample_report();
        assert_eq!(
            r.total_bytes,
            r.categories.iter().map(|c| c.bytes).sum::<u64>()
        );
        assert!(r.total_bytes > 12 * 1024 * 1024 * 1024);
    }
}
