//! Quarantine: the only destination the autonomous engine moves files to.
//!
//! Files are never deleted by the autonomous engine. A run moves its files
//! into `Quarantine\<run-id>\<category>\` with an `index.json` per run that
//! records every file's original path, so anything can be restored within
//! the retention window (default 7 days). Permanent deletion happens only
//! here, in `purge_expired` / `purge_all`, and only for files already inside
//! quarantine.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineEntry {
    pub original_path: String,
    pub quarantine_path: String,
    pub category: String,
    pub size: u64,
    /// ISO-8601 UTC timestamp of the move.
    pub ts: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RunIndex {
    run_id: String,
    entries: Vec<QuarantineEntry>,
}

#[derive(Debug)]
pub struct Quarantine {
    root: PathBuf,
    /// Serializes index read-modify-write inside `move_file` so the legacy
    /// scanner's parallel walker can share one instance safely.
    move_lock: std::sync::Mutex<()>,
}

impl Quarantine {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            move_lock: std::sync::Mutex::new(()),
        }
    }

    /// Kept for the M2 quarantine viewer.
    #[allow(dead_code)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn run_dir(&self, run_id: &str) -> PathBuf {
        self.root.join(run_id)
    }

    fn index_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("index.json")
    }

    fn load_index(&self, run_id: &str) -> RunIndex {
        fs::read_to_string(self.index_path(run_id))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| RunIndex {
                run_id: run_id.to_string(),
                entries: Vec::new(),
            })
    }

    fn save_index(&self, index: &RunIndex) -> std::io::Result<()> {
        let dir = self.run_dir(&index.run_id);
        fs::create_dir_all(&dir)?;
        let tmp = dir.join("index.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(index).unwrap_or_default())?;
        fs::rename(tmp, self.index_path(&index.run_id))
    }

    /// Moves one file into quarantine for `run_id`/`category` and records it
    /// in the run index. Rename is tried first (atomic on one volume); a
    /// cross-volume move falls back to copy + verify size + remove source.
    pub fn move_file(
        &self,
        run_id: &str,
        category: &str,
        src: &Path,
    ) -> std::io::Result<QuarantineEntry> {
        let size = fs::metadata(src)?.len();
        let _guard = self
            .move_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = self.run_dir(run_id).join(category);
        fs::create_dir_all(&dir)?;
        let name = src
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // Flat per-category folder plus a counter keeps names unique even
        // when two profiles hold files with the same name; the index is the
        // map back to the original path.
        let mut dest = dir.join(&name);
        let mut n = 0u32;
        while dest.exists() {
            n += 1;
            dest = dir.join(format!("{name}.{n}"));
        }
        if fs::rename(src, &dest).is_err() {
            fs::copy(src, &dest)?;
            if fs::metadata(&dest)?.len() != size {
                let _ = fs::remove_file(&dest);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "copy size mismatch",
                ));
            }
            fs::remove_file(src)?;
        }
        let entry = QuarantineEntry {
            original_path: src.to_string_lossy().into_owned(),
            quarantine_path: dest.to_string_lossy().into_owned(),
            category: category.to_string(),
            size,
            ts: crate::audit::iso_now(),
        };
        let mut index = self.load_index(run_id);
        index.entries.push(entry.clone());
        self.save_index(&index)?;
        Ok(entry)
    }

    /// All recorded entries across all runs currently in quarantine.
    pub fn entries(&self) -> Vec<QuarantineEntry> {
        let mut out = Vec::new();
        if let Ok(runs) = fs::read_dir(&self.root) {
            for run in runs.flatten() {
                if run.path().is_dir() {
                    if let Some(id) = run.file_name().to_str() {
                        out.extend(self.load_index(id).entries);
                    }
                }
            }
        }
        out
    }

    /// Finds the run + entry for a quarantined file path.
    fn find(&self, quarantine_path: &Path) -> Option<(String, QuarantineEntry)> {
        let wanted = quarantine_path.to_string_lossy().into_owned();
        if let Ok(runs) = fs::read_dir(&self.root) {
            for run in runs.flatten() {
                if let Some(id) = run.file_name().to_str() {
                    let index = self.load_index(id);
                    if let Some(e) = index.entries.iter().find(|e| e.quarantine_path == wanted) {
                        return Some((id.to_string(), e.clone()));
                    }
                }
            }
        }
        None
    }

    /// Restores one quarantined file to its original path. Refuses to
    /// overwrite an existing file — the caller surfaces that choice.
    pub fn restore(&self, quarantine_path: &Path) -> std::io::Result<PathBuf> {
        let (run_id, entry) = self.find(quarantine_path).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "file is not in the quarantine index",
            )
        })?;
        let dest = PathBuf::from(&entry.original_path);
        if dest.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "original location already has a file with this name",
            ));
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::rename(quarantine_path, &dest).is_err() {
            // Cross-volume fallback gets the same discipline as
            // move_file: the copy is size-verified before the
            // quarantined original is removed, so an interrupted copy
            // can never leave a truncated file in place of the real one.
            let size = fs::metadata(quarantine_path)?.len();
            fs::copy(quarantine_path, &dest)?;
            if fs::metadata(&dest)?.len() != size {
                let _ = fs::remove_file(&dest);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "copy size mismatch",
                ));
            }
            fs::remove_file(quarantine_path)?;
        }
        let mut index = self.load_index(&run_id);
        index
            .entries
            .retain(|e| e.quarantine_path != entry.quarantine_path);
        self.save_index(&index)?;
        Ok(dest)
    }

    /// Permanently removes everything in quarantine (manual user action
    /// from the tray UI). Returns bytes freed.
    pub fn purge_all(&self) -> u64 {
        let freed = self.total_size();
        if let Ok(runs) = fs::read_dir(&self.root) {
            for run in runs.flatten() {
                let path = run.path();
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                } else {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        freed
    }

    /// Total bytes currently held in quarantine (index-based).
    pub fn total_size(&self) -> u64 {
        self.entries().iter().map(|e| e.size).sum()
    }

    /// Permanently removes run folders whose newest entry is older than
    /// `retention`. Returns bytes freed. This is the one place quarantined
    /// files are truly deleted.
    pub fn purge_expired(&self, retention: Duration, now: SystemTime) -> u64 {
        let mut freed = 0u64;
        if let Ok(runs) = fs::read_dir(&self.root) {
            for run in runs.flatten() {
                let path = run.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(id) = run.file_name().to_str().map(|s| s.to_string()) else {
                    continue;
                };
                // Expiry is judged by the newest entry timestamp in
                // the run's index — the data the retention promise is
                // about — not the folder mtime (restoring a file
                // rewrites the index inside the folder and used to
                // extend every other file's retention). Folders with
                // no readable index fall back to their mtime.
                let index = self.load_index(&id);
                let newest = index
                    .entries
                    .iter()
                    .filter_map(|e| crate::audit::unix_from_iso(&e.ts))
                    .max();
                let expired = match newest {
                    Some(ts) => {
                        let now_secs = now
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        now_secs.saturating_sub(ts) > retention.as_secs()
                    }
                    None => match fs::metadata(&path).and_then(|m| m.modified()) {
                        Ok(mtime) => now
                            .duration_since(mtime)
                            .map(|age| age > retention)
                            .unwrap_or(false),
                        Err(_) => false,
                    },
                };
                if expired {
                    freed += index.entries.iter().map(|e| e.size).sum::<u64>();
                    let _ = fs::remove_dir_all(&path);
                }
            }
        }
        freed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(tag: &str) -> (PathBuf, Quarantine, PathBuf) {
        let base = std::env::temp_dir().join(format!("tidytray-quar-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let src_dir = base.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        (
            base.clone(),
            Quarantine::new(base.join("Quarantine")),
            src_dir,
        )
    }

    #[test]
    fn move_and_restore_round_trip_is_byte_identical() {
        let (base, q, src_dir) = setup("round");
        let f = src_dir.join("cache.bin");
        fs::write(&f, vec![7u8; 4096]).unwrap();
        let entry = q.move_file("run1", "temp_user", &f).unwrap();
        assert!(!f.exists());
        assert_eq!(q.total_size(), 4096);
        let restored = q.restore(Path::new(&entry.quarantine_path)).unwrap();
        assert_eq!(restored, f);
        assert_eq!(fs::read(&f).unwrap(), vec![7u8; 4096]);
        assert_eq!(q.total_size(), 0);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn restore_refuses_to_overwrite() {
        let (base, q, src_dir) = setup("clash");
        let f = src_dir.join("a.tmp");
        fs::write(&f, b"old").unwrap();
        let entry = q.move_file("run1", "temp_user", &f).unwrap();
        fs::write(&f, b"new").unwrap();
        assert!(q.restore(Path::new(&entry.quarantine_path)).is_err());
        assert_eq!(fs::read(&f).unwrap(), b"new");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn purge_expired_removes_only_old_runs() {
        let (base, q, src_dir) = setup("purge");
        let f = src_dir.join("a.tmp");
        fs::write(&f, vec![1u8; 100]).unwrap();
        q.move_file("run-old", "temp_user", &f).unwrap();
        let freed = q.purge_expired(
            Duration::ZERO,
            SystemTime::now() + Duration::from_secs(3600),
        );
        assert_eq!(freed, 100);
        assert!(q.entries().is_empty());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn same_named_files_from_different_folders_coexist() {
        let (base, q, src_dir) = setup("names");
        let a = src_dir.join("one");
        let b = src_dir.join("two");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("cache.db"), b"a").unwrap();
        fs::write(b.join("cache.db"), b"b").unwrap();
        q.move_file("run1", "thumbnail_cache", &a.join("cache.db"))
            .unwrap();
        q.move_file("run1", "thumbnail_cache", &b.join("cache.db"))
            .unwrap();
        assert_eq!(q.entries().len(), 2);
        fs::remove_dir_all(base).unwrap();
    }
}
