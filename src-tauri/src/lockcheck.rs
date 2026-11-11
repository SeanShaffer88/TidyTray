//! Best-effort "file is in use" detection for the autonomous cleaner.
//!
//! Locked or in-use files are always skipped, never forced (blueprint §3.6).
//! The check opens the file asking for exclusive access: on Windows that
//! fails while another process holds the file open. It is a pre-flight hint —
//! the quarantine move re-checks by simply failing, and a failed move is
//! recorded as a skip, never retried with force.

use std::fs::OpenOptions;
use std::path::Path;

pub fn is_locked(path: &Path) -> bool {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false, // missing files are the walker's problem, not a lock
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_NONE (share_mode 0): fails if anyone else has it open.
        OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .is_err()
    }
    #[cfg(not(windows))]
    {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .is_err()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_directory_are_not_locked() {
        assert!(!is_locked(Path::new("/definitely/not/here.bin")));
        assert!(!is_locked(&std::env::temp_dir()));
    }

    #[test]
    fn plain_file_is_not_locked() {
        let p = std::env::temp_dir().join(format!("tidytray-lock-{}.tmp", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        assert!(!is_locked(&p));
        std::fs::remove_file(p).unwrap();
    }
}
