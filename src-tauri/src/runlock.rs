//! Cross-process exclusion for cleaning runs.
//!
//! The in-process guard in `safety.rs` cannot see the *other* TidyTray
//! process: the GUI and the scheduled headless `--autonomous-run` are
//! separate processes, and two of them moving files into the same
//! quarantine at once can overwrite each other's per-run index updates
//! (a lost index entry is a file the restore view cannot show). This
//! lock file in the data directory closes that gap: it records the
//! holder's pid, a second acquirer checks whether that pid is still
//! alive, and a dead holder's lock is taken over. Dropping the guard
//! removes the file (only when it still names the dropper's pid).

use std::io::Write;
use std::path::{Path, PathBuf};

pub struct RunLock {
    path: PathBuf,
    pid: u32,
}

fn try_create(path: &Path, pid: u32) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    write!(f, "{pid}")
}

fn read_holder(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(windows)]
fn holder_alive(path: &Path, pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let _ = path;
    if pid == 0 {
        return false;
    }
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        false
    } else {
        unsafe { CloseHandle(h) };
        true
    }
}

#[cfg(not(windows))]
fn holder_alive(path: &Path, pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // No portable pid-liveness without extra dependencies; treat a
    // fresh lock file as held and a day-old one as abandoned. This
    // path only runs in tests and previews — Windows uses the real
    // pid check above.
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().map(|e| e.as_secs() < 86_400).unwrap_or(true))
        .unwrap_or(false)
}

/// Takes the run lock in `dir`, or returns `None` when a live process
/// already holds it.
pub fn acquire(dir: &Path) -> Option<RunLock> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join("clean-run.lock");
    let me = std::process::id();
    if try_create(&path, me).is_ok() {
        return Some(RunLock { path, pid: me });
    }
    let holder = read_holder(&path);
    let alive = holder.map(|pid| holder_alive(&path, pid)).unwrap_or(false);
    if alive {
        return None;
    }
    // Stale or unreadable lock: remove it and win the create race, or
    // report contention when another taker got there first.
    let _ = std::fs::remove_file(&path);
    try_create(&path, me)
        .ok()
        .map(|_| RunLock { path, pid: me })
}

impl Drop for RunLock {
    fn drop(&mut self) {
        if read_holder(&self.path) == Some(self.pid) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tidytray-runlock-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn second_acquire_fails_while_held_and_succeeds_after_drop() {
        let dir = temp_dir("held");
        let first = acquire(&dir).expect("first acquire");
        assert!(acquire(&dir).is_none(), "held lock must refuse");
        drop(first);
        assert!(acquire(&dir).is_some(), "dropped lock must free");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dead_holder_lock_is_taken_over() {
        let dir = temp_dir("stale");
        std::fs::create_dir_all(&dir).unwrap();
        // Pid 0 is never a live process.
        std::fs::write(dir.join("clean-run.lock"), "0").unwrap();
        assert!(acquire(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
