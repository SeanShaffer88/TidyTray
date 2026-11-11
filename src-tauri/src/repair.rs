//! Tier 2 system repair (blueprint §6.2): the operating system's own
//! repair tools — System File Checker and DISM — run on demand, as
//! administrator, with their output kept in a log file.
//!
//! The discipline mirrors the other Tier 2 surfaces: the action set is
//! a fixed catalog in the build, callers (window or CLI) can only name
//! a catalog action, and the elevated helper **re-validates the action
//! against the catalog** before running anything. Commands are fixed
//! argument arrays — no shell, no user-supplied text ever reaches the
//! process line. These tools repair Windows itself; there is no undo
//! artifact because the operation *is* the repair, and its full output
//! is preserved in the repair log and the audit log.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct RepairAction {
    pub id: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    /// Executable file name under `%SystemRoot%\System32`.
    pub file: &'static str,
    pub args: &'static [&'static str],
}

static ACTIONS: &[RepairAction] = &[
    RepairAction {
        id: "sfc",
        title: "System File Checker",
        desc: "Scans protected Windows system files and replaces corrupted ones from the component store.",
        file: "sfc.exe",
        args: &["/scannow"],
    },
    RepairAction {
        id: "dism",
        title: "Component store repair (DISM)",
        desc: "Checks the Windows component store for corruption and repairs it, which also fixes SFC failures.",
        file: "Dism.exe",
        args: &["/Online", "/Cleanup-Image", "/RestoreHealth"],
    },
];

pub fn find_action(id: &str) -> Option<&'static RepairAction> {
    ACTIONS.iter().find(|a| a.id == id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairActionState {
    pub id: String,
    pub title: String,
    pub desc: String,
}

pub fn action_states() -> Vec<RepairActionState> {
    ACTIONS
        .iter()
        .map(|a| RepairActionState {
            id: a.id.to_string(),
            title: a.title.to_string(),
            desc: a.desc.to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Elevated helper protocol (`--repair-elevated <batch.json>`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairBatch {
    pub action: String,
    /// Path of the log file the helper writes the tool's output to.
    pub log_file: String,
    /// Random token pairing this batch with its result file.
    #[serde(default)]
    pub nonce: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairResult {
    pub ok: bool,
    pub exit_code: Option<i32>,
    /// Echo of the batch nonce; the parent ignores results without it.
    #[serde(default)]
    pub nonce: String,
}

fn result_path(batch_path: &Path) -> PathBuf {
    batch_path.with_extension("result.json")
}

fn program_path(action: &RepairAction) -> PathBuf {
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(action.file)
}

/// Runs inside the elevated helper process: validates the action
/// against the catalog, runs the fixed command, saves its output to
/// the batch's log file, and writes the result next to the batch.
#[cfg(windows)]
pub fn run_repair_helper(batch_path: &Path) -> i32 {
    let Ok(text) = std::fs::read_to_string(batch_path) else {
        return 3;
    };
    let Ok(batch) = serde_json::from_str::<RepairBatch>(&text) else {
        return 3;
    };
    let mut result = RepairResult {
        nonce: batch.nonce.clone(),
        ..Default::default()
    };
    let Some(action) = find_action(&batch.action) else {
        let _ = std::fs::write(
            result_path(batch_path),
            serde_json::to_vec_pretty(&result).unwrap_or_default(),
        );
        return 2;
    };
    match std::process::Command::new(program_path(action))
        .args(action.args)
        .output()
    {
        Ok(out) => {
            result.ok = out.status.success();
            result.exit_code = out.status.code();
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            let _ = std::fs::write(&batch.log_file, text);
        }
        Err(e) => {
            let _ = std::fs::write(&batch.log_file, format!("failed to start: {e}"));
        }
    }
    let _ = std::fs::write(
        result_path(batch_path),
        serde_json::to_vec_pretty(&result).unwrap_or_default(),
    );
    if result.ok {
        0
    } else {
        2
    }
}

#[cfg(not(windows))]
pub fn run_repair_helper(_batch_path: &Path) -> i32 {
    3
}

/// Parent side (Windows): writes the batch, launches this executable
/// elevated via ShellExecuteExW ("runas"), waits for it, reads the
/// result. Repair tools can run for many minutes; the wait is 20.
#[cfg(windows)]
pub fn launch_repair(
    action_id: &str,
    dir: &Path,
    logs_dir: &Path,
) -> Result<(RepairResult, PathBuf), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    let action =
        find_action(action_id).ok_or_else(|| format!("unknown repair action {action_id:?}"))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(logs_dir).map_err(|e| e.to_string())?;
    let nonce = crate::audit::batch_token();
    let log_file = logs_dir.join(format!("repair-{}.log", &nonce[..16]));
    let batch = RepairBatch {
        action: action.id.to_string(),
        log_file: log_file.to_string_lossy().into_owned(),
        nonce: nonce.clone(),
    };
    let batch_path = dir.join(format!("repair-batch-{}.json", &nonce[..24]));
    std::fs::write(
        &batch_path,
        serde_json::to_vec_pretty(&batch).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let params = format!("--repair-elevated \"{}\"", batch_path.display());
    let wide =
        |s: &std::ffi::OsStr| -> Vec<u16> { s.encode_wide().chain(std::iter::once(0)).collect() };
    let verb = wide(std::ffi::OsStr::new("runas"));
    let file = wide(exe.as_os_str());
    let args = wide(std::ffi::OsStr::new(&params));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: std::ptr::null_mut(),
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: args.as_ptr(),
        lpDirectory: std::ptr::null(),
        nShow: 0,
        hInstApp: std::ptr::null_mut(),
        lpIDList: std::ptr::null_mut(),
        lpClass: std::ptr::null(),
        hkeyClass: std::ptr::null_mut(),
        dwHotKey: 0,
        Anonymous: unsafe { std::mem::zeroed() },
        hProcess: std::ptr::null_mut(),
    };
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 || info.hProcess.is_null() {
        let _ = std::fs::remove_file(&batch_path);
        return Err(
            "administrator approval: elevation was declined or failed to start".to_string(),
        );
    }
    let waited = unsafe { WaitForSingleObject(info.hProcess, 1_200_000) };
    unsafe { CloseHandle(info.hProcess) };
    if waited != 0 {
        // The repair may still complete; its output lands in the log
        // file either way. The batch stays for no one — there is no
        // undo store to reconcile — but the outcome stays honest.
        return Err(format!(
            "the repair is still running after 20 minutes; its report will be in {}",
            log_file.display()
        ));
    }
    let parsed = std::fs::read_to_string(result_path(&batch_path))
        .ok()
        .and_then(|t| serde_json::from_str::<RepairResult>(&t).ok());
    let result = match parsed {
        Some(r) if r.nonce == batch.nonce => Ok((r, log_file)),
        Some(_) => Err("the repair helper's result did not match its batch".to_string()),
        None => Err("the repair helper did not report a result".to_string()),
    };
    let _ = std::fs::remove_file(&batch_path);
    let _ = std::fs::remove_file(result_path(&batch_path));
    result
}

#[cfg(not(windows))]
pub fn launch_repair(
    _action_id: &str,
    _dir: &Path,
    _logs_dir: &Path,
) -> Result<(RepairResult, PathBuf), String> {
    Err("elevation is only available on Windows".to_string())
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairOutcome {
    pub ok: bool,
    pub exit_code: Option<i32>,
    /// Name of the log file holding the tool's full output.
    pub log_file: String,
    pub message: String,
}

/// Runs one catalog repair action end to end and audit-logs it.
pub fn run_repair(data_dir: &Path, logs_dir: &Path, action_id: &str) -> RepairOutcome {
    let log = crate::audit::AuditLog::new(logs_dir.to_path_buf());
    let Some(action) = find_action(action_id) else {
        return RepairOutcome {
            ok: false,
            exit_code: None,
            log_file: String::new(),
            message: format!("unknown repair action {action_id:?}"),
        };
    };
    let outcome = match launch_repair(action.id, data_dir, logs_dir) {
        Ok((result, log_path)) => RepairOutcome {
            ok: result.ok,
            exit_code: result.exit_code,
            log_file: log_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            message: match result.exit_code {
                Some(0) => format!(
                    "{} finished successfully. Full output: {}",
                    action.title,
                    log_path.display()
                ),
                Some(code) => format!(
                    "{} finished with exit code {code} — see its report in {}",
                    action.title,
                    log_path.display()
                ),
                None => format!("{} did not report an exit code.", action.title),
            },
        },
        Err(e) => RepairOutcome {
            ok: false,
            exit_code: None,
            log_file: String::new(),
            message: e,
        },
    };
    let _ = log.append(
        &crate::audit::LogEntry::new(&crate::audit::new_run_id(), "repair")
            .path(Path::new(action.id))
            .result(if outcome.ok { "ok" } else { "failed" }),
    );
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_actions_are_fixed_and_well_formed() {
        assert_eq!(action_states().len(), 2);
        for id in ["sfc", "dism"] {
            let a = find_action(id).expect("catalog action");
            assert!(a.file.ends_with(".exe"));
            assert!(!a.args.is_empty());
        }
        assert!(find_action("regedit").is_none());
        assert!(find_action("").is_none());
    }

    #[test]
    fn unknown_action_is_a_clean_failure() {
        let dir = std::env::temp_dir().join(format!("tidytray-repair-{}", std::process::id()));
        let out = run_repair(&dir, &dir.join("logs"), "nope");
        assert!(!out.ok);
        assert!(out.message.contains("unknown repair action"));
    }
}
