//! Tier 2 foundation (blueprint §6.2, milestone M4): manual, previewed,
//! reversible system tweaks.
//!
//! The pipeline is strict: **detect → plan → apply → undo**.
//! - Nothing is written while planning: a plan captures the exact prior
//!   value of every registry value it would touch, which is what makes
//!   Undo possible even when no System Restore point could be created.
//! - Applying records an undo batch *for the operations that actually
//!   succeeded* before returning, and audit-logs the outcome.
//! - Machine-level (HKLM) writes and the restore point run in one
//!   elevated helper invocation of this same executable
//!   (`--tier2-elevated <batch.json>`, per-operation elevation, §7.2);
//!   user-level (HKCU) writes happen in-process. If the user declines
//!   the elevation prompt, machine-level changes are reported as failed
//!   and nothing pretends otherwise.
//!
//! All values are DWORDs and every toggle is a documented policy or
//! preference value. The pure logic (catalog, detect, plan, undo
//! bookkeeping) runs against a [`RegStore`] so it is fully testable
//! without touching a real registry.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Registry abstraction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Hive {
    #[serde(rename = "HKCU")]
    Hkcu,
    #[serde(rename = "HKLM")]
    Hklm,
}

impl Hive {
    pub fn label(self) -> &'static str {
        match self {
            Hive::Hkcu => "HKCU",
            Hive::Hklm => "HKLM",
        }
    }
}

/// The three registry operations Tier 2 needs. The real implementation is
/// backed by `winreg`; tests use an in-memory fake.
pub trait RegStore {
    fn get_dword(&self, hive: Hive, key: &str, name: &str) -> Option<u32>;
    fn set_dword(&self, hive: Hive, key: &str, name: &str, value: u32) -> std::io::Result<()>;
    fn delete_value(&self, hive: Hive, key: &str, name: &str) -> std::io::Result<()>;
}

/// Real registry access (Windows). Reads and HKCU writes work unelevated;
/// HKLM writes are only ever attempted by the elevated helper.
#[cfg(windows)]
pub struct WinRegStore;

#[cfg(windows)]
impl WinRegStore {
    fn root(hive: Hive) -> winreg::RegKey {
        match hive {
            Hive::Hkcu => winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER),
            Hive::Hklm => winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE),
        }
    }
}

#[cfg(windows)]
impl RegStore for WinRegStore {
    fn get_dword(&self, hive: Hive, key: &str, name: &str) -> Option<u32> {
        let key = Self::root(hive)
            .open_subkey_with_flags(key, winreg::enums::KEY_READ)
            .ok()?;
        key.get_value::<u32, _>(name).ok()
    }

    fn set_dword(&self, hive: Hive, key: &str, name: &str, value: u32) -> std::io::Result<()> {
        let (key, _) = Self::root(hive).create_subkey(key)?;
        key.set_value(name, &value)
    }

    fn delete_value(&self, hive: Hive, key: &str, name: &str) -> std::io::Result<()> {
        let key = Self::root(hive).open_subkey_with_flags(key, winreg::enums::KEY_SET_VALUE)?;
        key.delete_value(name)
    }
}

/// The store used on this platform: the real registry on Windows, an
/// empty read-only store elsewhere (state detection reports defaults,
/// which is what the browser UI preview relies on).
#[cfg(windows)]
pub fn system_store() -> WinRegStore {
    WinRegStore
}

#[cfg(not(windows))]
pub fn system_store() -> FakeStore {
    FakeStore::default()
}

/// In-memory store for tests and non-Windows previews.
#[cfg(any(test, not(windows)))]
#[derive(Debug, Default)]
pub struct FakeStore {
    values: std::cell::RefCell<std::collections::BTreeMap<(Hive, String, String), u32>>,
}

#[cfg(any(test, not(windows)))]
impl RegStore for FakeStore {
    fn get_dword(&self, hive: Hive, key: &str, name: &str) -> Option<u32> {
        self.values
            .borrow()
            .get(&(hive, key.to_string(), name.to_string()))
            .copied()
    }

    fn set_dword(&self, hive: Hive, key: &str, name: &str, value: u32) -> std::io::Result<()> {
        self.values
            .borrow_mut()
            .insert((hive, key.to_string(), name.to_string()), value);
        Ok(())
    }

    fn delete_value(&self, hive: Hive, key: &str, name: &str) -> std::io::Result<()> {
        self.values
            .borrow_mut()
            .remove(&(hive, key.to_string(), name.to_string()));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Toggle catalog
// ---------------------------------------------------------------------------

/// One registry value a toggle controls: the value when the tweak is
/// applied, and the value (or absence) that restores the Windows default.
/// `default: None` means the value is deleted — the correct revert for
/// policy values, where "not configured" is the default state.
#[derive(Debug, Clone, Copy)]
pub struct RegOp {
    pub hive: Hive,
    pub key: &'static str,
    pub name: &'static str,
    pub applied: u32,
    pub default: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct ToggleDef {
    pub id: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub group: &'static str,
    pub recommended: bool,
    /// Plain-words state labels for the preview ("Off for all apps").
    pub on_label: &'static str,
    pub off_label: &'static str,
    pub ops: &'static [RegOp],
}

impl ToggleDef {
    pub fn needs_admin(&self) -> bool {
        self.ops.iter().any(|op| op.hive == Hive::Hklm)
    }
}

const EXPLORER_ADV: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

static CATALOG: &[ToggleDef] = &[
    ToggleDef {
        id: "telemetry_basic",
        title: "Diagnostic data",
        desc: "Limits Windows diagnostic data to the basic level.",
        group: "Privacy",
        recommended: true,
        on_label: "Basic (minimum)",
        off_label: "Default",
        ops: &[RegOp {
            hive: Hive::Hklm,
            key: r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
            name: "AllowTelemetry",
            applied: 1,
            default: None,
        }],
    },
    ToggleDef {
        id: "advertising_id_off",
        title: "Advertising ID",
        desc: "Stops apps using your advertising ID for personalized ads.",
        group: "Privacy",
        recommended: true,
        on_label: "Off for all apps",
        off_label: "Default",
        ops: &[RegOp {
            hive: Hive::Hkcu,
            key: r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
            name: "Enabled",
            applied: 0,
            default: Some(1),
        }],
    },
    ToggleDef {
        id: "activity_history_off",
        title: "Activity history",
        desc: "Stops Windows storing your activity timeline on this device.",
        group: "Privacy",
        recommended: true,
        on_label: "Off",
        off_label: "Default",
        ops: &[
            RegOp {
                hive: Hive::Hklm,
                key: r"SOFTWARE\Policies\Microsoft\Windows\System",
                name: "EnableActivityFeed",
                applied: 0,
                default: None,
            },
            RegOp {
                hive: Hive::Hklm,
                key: r"SOFTWARE\Policies\Microsoft\Windows\System",
                name: "PublishUserActivities",
                applied: 0,
                default: None,
            },
        ],
    },
    ToggleDef {
        id: "tailored_experiences_off",
        title: "Tailored experiences",
        desc: "Turns off tips and suggestions based on diagnostic data.",
        group: "Privacy",
        recommended: true,
        on_label: "Off",
        off_label: "Default",
        ops: &[RegOp {
            hive: Hive::Hkcu,
            key: r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
            name: "DisableTailoredExperiencesWithDiagnosticData",
            applied: 1,
            default: None,
        }],
    },
    ToggleDef {
        id: "copilot_off",
        title: "Microsoft Copilot",
        desc: "Hides Copilot and turns off its taskbar button.",
        group: "Debloat",
        recommended: true,
        on_label: "Hidden; taskbar button off",
        off_label: "Default",
        ops: &[
            RegOp {
                hive: Hive::Hkcu,
                key: r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot",
                name: "TurnOffWindowsCopilot",
                applied: 1,
                default: None,
            },
            RegOp {
                hive: Hive::Hkcu,
                key: EXPLORER_ADV,
                name: "ShowCopilotButton",
                applied: 0,
                default: Some(1),
            },
        ],
    },
    ToggleDef {
        id: "widgets_off",
        title: "Widgets board",
        desc: "Turns off the Widgets board and its taskbar icon.",
        group: "Debloat",
        recommended: true,
        on_label: "Off",
        off_label: "Default",
        ops: &[RegOp {
            hive: Hive::Hkcu,
            key: EXPLORER_ADV,
            name: "TaskbarDa",
            applied: 0,
            default: Some(1),
        }],
    },
    ToggleDef {
        id: "search_highlights_off",
        title: "Search highlights",
        desc: "Removes rotating highlights and promotions from the search box.",
        group: "Debloat",
        recommended: true,
        on_label: "Off",
        off_label: "Default",
        ops: &[RegOp {
            hive: Hive::Hkcu,
            key: r"SOFTWARE\Microsoft\Windows\CurrentVersion\SearchSettings",
            name: "IsDynamicSearchBoxEnabled",
            applied: 0,
            default: Some(1),
        }],
    },
    ToggleDef {
        id: "file_extensions_on",
        title: "File extensions",
        desc: "Always shows file name extensions in Explorer.",
        group: "Explorer & taskbar",
        recommended: true,
        on_label: "Shown",
        off_label: "Hidden",
        ops: &[RegOp {
            hive: Hive::Hkcu,
            key: EXPLORER_ADV,
            name: "HideFileExt",
            applied: 0,
            default: Some(1),
        }],
    },
    ToggleDef {
        id: "chat_icon_off",
        title: "Chat icon",
        desc: "Removes the Teams Chat icon from the taskbar.",
        group: "Explorer & taskbar",
        recommended: true,
        on_label: "Removed",
        off_label: "Default",
        ops: &[RegOp {
            hive: Hive::Hkcu,
            key: EXPLORER_ADV,
            name: "TaskbarMn",
            applied: 0,
            default: Some(1),
        }],
    },
    ToggleDef {
        id: "long_paths_on",
        title: "Long paths",
        desc: "Allows apps to use file paths longer than 260 characters.",
        group: "Explorer & taskbar",
        recommended: false,
        on_label: "Allowed",
        off_label: "Default (260 max)",
        ops: &[RegOp {
            hive: Hive::Hklm,
            key: r"SYSTEM\CurrentControlSet\Control\FileSystem",
            name: "LongPathsEnabled",
            applied: 1,
            default: Some(0),
        }],
    },
];

pub fn catalog() -> &'static [ToggleDef] {
    CATALOG
}

pub fn find(id: &str) -> Option<&'static ToggleDef> {
    CATALOG.iter().find(|d| d.id == id)
}

// ---------------------------------------------------------------------------
// Detect + plan (no writes)
// ---------------------------------------------------------------------------

/// A toggle counts as applied only when *every* value it controls holds
/// its applied value.
pub fn detect(def: &ToggleDef, store: &dyn RegStore) -> bool {
    def.ops
        .iter()
        .all(|op| store.get_dword(op.hive, op.key, op.name) == Some(op.applied))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedOp {
    pub toggle_id: String,
    pub hive: Hive,
    pub key: String,
    pub name: String,
    /// Value before the change (`None` = the value does not exist).
    pub prior: Option<u32>,
    /// Value to write (`None` = delete the value).
    pub value: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedChange {
    pub id: String,
    pub title: String,
    pub to_applied: bool,
    pub from_label: String,
    pub to_label: String,
    pub needs_admin: bool,
    #[serde(skip)]
    pub ops: Vec<PlannedOp>,
}

/// Builds the preview for `(toggle id, target applied state)` pairs.
/// Toggles already in their target state are omitted — the preview lists
/// exactly what will change, nothing more.
pub fn plan(changes: &[(String, bool)], store: &dyn RegStore) -> Vec<PlannedChange> {
    let mut out = Vec::new();
    for (id, to_applied) in changes {
        let Some(def) = find(id) else { continue };
        let applied = detect(def, store);
        if applied == *to_applied {
            continue;
        }
        let ops = def
            .ops
            .iter()
            .map(|op| PlannedOp {
                toggle_id: def.id.to_string(),
                hive: op.hive,
                key: op.key.to_string(),
                name: op.name.to_string(),
                prior: store.get_dword(op.hive, op.key, op.name),
                value: if *to_applied {
                    Some(op.applied)
                } else {
                    op.default
                },
            })
            .collect();
        out.push(PlannedChange {
            id: def.id.to_string(),
            title: def.title.to_string(),
            to_applied: *to_applied,
            from_label: if applied { def.on_label } else { def.off_label }.to_string(),
            to_label: if *to_applied {
                def.on_label
            } else {
                def.off_label
            }
            .to_string(),
            needs_admin: def.needs_admin(),
            ops,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Execute + undo bookkeeping
// ---------------------------------------------------------------------------

/// Applies planned ops through a store; returns one error string per
/// failed op (empty = all succeeded). Successful ops are returned too,
/// so the caller can record undo information for exactly what changed.
pub fn execute(ops: &[PlannedOp], store: &dyn RegStore) -> (Vec<PlannedOp>, Vec<String>) {
    let mut done = Vec::new();
    let mut errors = Vec::new();
    for op in ops {
        let result = match op.value {
            Some(v) => store.set_dword(op.hive, &op.key, &op.name, v),
            None => store.delete_value(op.hive, &op.key, &op.name),
        };
        match result {
            Ok(()) => done.push(op.clone()),
            Err(e) => errors.push(format!("{} {}\\{}: {e}", op.hive.label(), op.key, op.name)),
        }
    }
    (done, errors)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoEntry {
    pub toggle_id: String,
    pub hive: Hive,
    pub key: String,
    pub name: String,
    pub prior: Option<u32>,
}

impl From<&PlannedOp> for UndoEntry {
    fn from(op: &PlannedOp) -> Self {
        Self {
            toggle_id: op.toggle_id.clone(),
            hive: op.hive,
            key: op.key.clone(),
            name: op.name.clone(),
            prior: op.prior,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoBatch {
    pub id: String,
    pub ts: String,
    pub entries: Vec<UndoEntry>,
}

pub fn undo_path(data_dir: &Path) -> PathBuf {
    data_dir.join("undo.json")
}

pub fn load_undo(path: &Path) -> Vec<UndoBatch> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_undo(path: &Path, batches: &[UndoBatch]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(batches).unwrap_or_default())?;
    std::fs::rename(tmp, path)
}

pub fn append_undo(path: &Path, batch: UndoBatch) -> std::io::Result<()> {
    let mut batches = load_undo(path);
    batches.push(batch);
    save_undo(path, &batches)
}

/// The registry writes that undo a batch: each value goes back to its
/// recorded prior (deleted when it did not exist before).
pub fn undo_ops(batch: &UndoBatch) -> Vec<PlannedOp> {
    batch
        .entries
        .iter()
        .map(|e| PlannedOp {
            toggle_id: e.toggle_id.clone(),
            hive: e.hive,
            key: e.key.clone(),
            name: e.name.clone(),
            prior: None,
            value: e.prior,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Elevated helper protocol (`--tier2-elevated <batch.json>`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElevatedBatch {
    pub restore_point: bool,
    pub ops: Vec<PlannedOp>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElevatedResult {
    pub restore_point_created: bool,
    pub done: Vec<PlannedOp>,
    pub errors: Vec<String>,
}

fn result_path(batch_path: &Path) -> PathBuf {
    batch_path.with_extension("result.json")
}

/// Runs inside the elevated helper process: optionally creates a System
/// Restore point, then applies the batch's (machine-level) ops against
/// the real registry and writes the result file next to the batch.
#[cfg(windows)]
pub fn run_elevated_helper(batch_path: &Path) -> i32 {
    let Ok(text) = std::fs::read_to_string(batch_path) else {
        return 3;
    };
    let Ok(batch) = serde_json::from_str::<ElevatedBatch>(&text) else {
        return 3;
    };
    let mut result = ElevatedResult::default();
    if batch.restore_point {
        result.restore_point_created = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Checkpoint-Computer -Description 'TidyTray tweaks' -RestorePointType 'MODIFY_SETTINGS'",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    let (done, errors) = execute(&batch.ops, &WinRegStore);
    result.done = done;
    result.errors = errors;
    let _ = std::fs::write(
        result_path(batch_path),
        serde_json::to_vec_pretty(&result).unwrap_or_default(),
    );
    if result.errors.is_empty() {
        0
    } else {
        2
    }
}

#[cfg(not(windows))]
pub fn run_elevated_helper(_batch_path: &Path) -> i32 {
    3
}

/// Parent side (Windows): writes the batch, launches this executable
/// elevated via ShellExecuteExW ("runas"), waits for it, reads the
/// result. `Err` means the helper never ran — most commonly the user
/// declined the elevation prompt.
#[cfg(windows)]
pub fn launch_elevated(batch: &ElevatedBatch, dir: &Path) -> Result<ElevatedResult, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let batch_path = dir.join(format!("tier2-batch-{}.json", crate::audit::new_run_id()));
    std::fs::write(
        &batch_path,
        serde_json::to_vec_pretty(batch).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let params = format!("--tier2-elevated \"{}\"", batch_path.display());
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
        return Err("elevation was declined or failed to start".to_string());
    }
    // Restore points can take a while; allow up to 3 minutes.
    unsafe { WaitForSingleObject(info.hProcess, 180_000) };
    unsafe { CloseHandle(info.hProcess) };
    let result = std::fs::read_to_string(result_path(&batch_path))
        .ok()
        .and_then(|text| serde_json::from_str::<ElevatedResult>(&text).ok())
        .ok_or_else(|| "the elevated helper did not report a result".to_string());
    let _ = std::fs::remove_file(&batch_path);
    let _ = std::fs::remove_file(result_path(&batch_path));
    result
}

#[cfg(not(windows))]
pub fn launch_elevated(_batch: &ElevatedBatch, _dir: &Path) -> Result<ElevatedResult, String> {
    Err("elevation is only available on Windows".to_string())
}

// ---------------------------------------------------------------------------
// Apply / undo orchestration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleState {
    pub id: String,
    pub title: String,
    pub desc: String,
    pub group: String,
    pub needs_admin: bool,
    pub recommended: bool,
    pub applied: bool,
    pub on_label: String,
    pub off_label: String,
}

pub fn list_states(store: &dyn RegStore) -> Vec<ToggleState> {
    catalog()
        .iter()
        .map(|def| ToggleState {
            id: def.id.to_string(),
            title: def.title.to_string(),
            desc: def.desc.to_string(),
            group: def.group.to_string(),
            needs_admin: def.needs_admin(),
            recommended: def.recommended,
            applied: detect(def, store),
            on_label: def.on_label.to_string(),
            off_label: def.off_label.to_string(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub applied_ids: Vec<String>,
    pub failed: Vec<String>,
    pub restore_point_created: bool,
    /// True when changes were staged but nothing needed doing.
    pub nothing_to_do: bool,
}

/// Applies staged changes end-to-end (see the module docs for the exact
/// discipline). `data_dir` holds the undo store and helper batch files;
/// audit entries are appended to `logs_dir`.
pub fn apply_changes(
    store: &dyn RegStore,
    data_dir: &Path,
    logs_dir: &Path,
    changes: &[(String, bool)],
    want_restore_point: bool,
) -> ApplyOutcome {
    let planned = plan(changes, store);
    let log = crate::audit::AuditLog::new(logs_dir.to_path_buf());
    if planned.is_empty() {
        return ApplyOutcome {
            applied_ids: Vec::new(),
            failed: Vec::new(),
            restore_point_created: false,
            nothing_to_do: true,
        };
    }
    let all_ops: Vec<PlannedOp> = planned.iter().flat_map(|c| c.ops.iter().cloned()).collect();
    let (user_ops, admin_ops): (Vec<PlannedOp>, Vec<PlannedOp>) = all_ops
        .iter()
        .cloned()
        .partition(|op| op.hive == Hive::Hkcu);

    let mut done: Vec<PlannedOp> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut restore_point_created = false;

    // One elevation covers the restore point and every machine-level op.
    if !admin_ops.is_empty() || want_restore_point {
        let batch = ElevatedBatch {
            restore_point: want_restore_point,
            ops: admin_ops,
        };
        match launch_elevated(&batch, data_dir) {
            Ok(result) => {
                restore_point_created = result.restore_point_created;
                done.extend(result.done);
                failed.extend(result.errors);
            }
            Err(e) => failed.push(format!("administrator approval: {e}")),
        }
    }
    let (user_done, user_errors) = execute(&user_ops, store);
    done.extend(user_done);
    failed.extend(user_errors);

    // Record undo information for exactly what succeeded.
    let done_ids: Vec<String> = {
        let mut ids: Vec<String> = done.iter().map(|op| op.toggle_id.clone()).collect();
        ids.sort();
        ids.dedup();
        ids
    };
    if !done.is_empty() {
        let batch = UndoBatch {
            id: crate::audit::new_run_id(),
            ts: crate::audit::iso_now(),
            entries: done.iter().map(UndoEntry::from).collect(),
        };
        let _ = append_undo(&undo_path(data_dir), batch);
    }
    let attempted: Vec<String> = planned.iter().map(|c| c.id.clone()).collect();
    let failed_ids: Vec<String> = attempted
        .iter()
        .filter(|id| !done_ids.contains(id))
        .cloned()
        .collect();
    for id in &attempted {
        let entry = crate::audit::LogEntry::new(&crate::audit::new_run_id(), "tier2")
            .result(if done_ids.contains(id) {
                "applied"
            } else {
                "failed"
            })
            .path(Path::new(id));
        let _ = log.append(&entry);
    }
    ApplyOutcome {
        applied_ids: done_ids,
        failed: failed_ids,
        restore_point_created,
        nothing_to_do: false,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoOutcome {
    pub restored_ids: Vec<String>,
    pub failed: Vec<String>,
    pub nothing_to_undo: bool,
}

/// Reverts the most recent applied batch. Entries that fail to revert
/// are kept as a fresh batch so Undo can be retried.
pub fn undo_last(store: &dyn RegStore, data_dir: &Path, logs_dir: &Path) -> UndoOutcome {
    let path = undo_path(data_dir);
    let mut batches = load_undo(&path);
    let Some(batch) = batches.pop() else {
        return UndoOutcome {
            restored_ids: Vec::new(),
            failed: Vec::new(),
            nothing_to_undo: true,
        };
    };
    let _ = save_undo(&path, &batches);
    let ops = undo_ops(&batch);
    let (user_ops, admin_ops): (Vec<PlannedOp>, Vec<PlannedOp>) =
        ops.iter().cloned().partition(|op| op.hive == Hive::Hkcu);
    let mut done: Vec<PlannedOp> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    if !admin_ops.is_empty() {
        match launch_elevated(
            &ElevatedBatch {
                restore_point: false,
                ops: admin_ops,
            },
            data_dir,
        ) {
            Ok(result) => {
                done.extend(result.done);
                failed.extend(result.errors);
            }
            Err(e) => failed.push(format!("administrator approval: {e}")),
        }
    }
    let (user_done, user_errors) = execute(&user_ops, store);
    done.extend(user_done);
    failed.extend(user_errors);

    if !failed.is_empty() {
        // Keep the entries that could not be reverted so Undo can retry.
        let done_keys: Vec<(Hive, String, String)> = done
            .iter()
            .map(|op| (op.hive, op.key.clone(), op.name.clone()))
            .collect();
        let remaining: Vec<UndoEntry> = batch
            .entries
            .iter()
            .filter(|e| !done_keys.contains(&(e.hive, e.key.clone(), e.name.clone())))
            .cloned()
            .collect();
        if !remaining.is_empty() {
            let _ = append_undo(
                &path,
                UndoBatch {
                    id: batch.id.clone(),
                    ts: batch.ts.clone(),
                    entries: remaining,
                },
            );
        }
    }
    let log = crate::audit::AuditLog::new(logs_dir.to_path_buf());
    let mut restored: Vec<String> = done.iter().map(|op| op.toggle_id.clone()).collect();
    restored.sort();
    restored.dedup();
    for id in &restored {
        let _ = log.append(
            &crate::audit::LogEntry::new(&crate::audit::new_run_id(), "tier2-undo")
                .path(Path::new(id)),
        );
    }
    UndoOutcome {
        restored_ids: restored,
        failed,
        nothing_to_undo: false,
    }
}

/// Whether an undo batch exists, and how many changes it holds.
pub fn undo_available(data_dir: &Path) -> Option<usize> {
    load_undo(&undo_path(data_dir))
        .last()
        .map(|b| b.entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(hive: Hive, key: &str, name: &str, value: u32) -> FakeStore {
        let store = FakeStore::default();
        store.set_dword(hive, key, name, value).unwrap();
        store
    }

    #[test]
    fn catalog_is_well_formed() {
        let mut ids = std::collections::BTreeSet::new();
        for def in catalog() {
            assert!(ids.insert(def.id), "duplicate toggle id {}", def.id);
            assert!(!def.ops.is_empty(), "{} has no ops", def.id);
            assert!(!def.title.is_empty() && !def.desc.is_empty());
        }
        assert_eq!(catalog().len(), 10);
        assert!(catalog().iter().filter(|d| d.recommended).count() >= 8);
    }

    #[test]
    fn detect_requires_every_op_at_its_applied_value() {
        let def = find("copilot_off").unwrap();
        let store = FakeStore::default();
        assert!(!detect(def, &store));
        store
            .set_dword(
                Hive::Hkcu,
                r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot",
                "TurnOffWindowsCopilot",
                1,
            )
            .unwrap();
        assert!(!detect(def, &store), "taskbar button still on: not applied");
        store
            .set_dword(Hive::Hkcu, EXPLORER_ADV, "ShowCopilotButton", 0)
            .unwrap();
        assert!(detect(def, &store));
    }

    #[test]
    fn plan_captures_priors_and_labels_and_skips_noops() {
        let def = find("file_extensions_on").unwrap();
        let store = store_with(Hive::Hkcu, EXPLORER_ADV, "HideFileExt", 1);
        let planned = plan(&[("file_extensions_on".into(), true)], &store);
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].from_label, "Hidden");
        assert_eq!(planned[0].to_label, "Shown");
        assert_eq!(planned[0].ops[0].prior, Some(1));
        assert_eq!(planned[0].ops[0].value, Some(0));
        assert!(!planned[0].needs_admin);
        let _ = def;
        // Already in target state -> nothing planned.
        let store2 = store_with(Hive::Hkcu, EXPLORER_ADV, "HideFileExt", 0);
        assert!(plan(&[("file_extensions_on".into(), true)], &store2).is_empty());
        // Admin toggles are flagged.
        let planned3 = plan(&[("telemetry_basic".into(), true)], &FakeStore::default());
        assert!(planned3[0].needs_admin);
        // Un-applying a policy deletes the value; a preference restores its default.
        let store4 = store_with(
            Hive::Hklm,
            r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
            "AllowTelemetry",
            1,
        );
        let un = plan(&[("telemetry_basic".into(), false)], &store4);
        assert_eq!(un[0].ops[0].value, None);
    }

    #[test]
    fn execute_then_undo_restores_exact_prior_state() {
        let store = FakeStore::default();
        // One value pre-exists with a non-default value; the other is absent.
        store
            .set_dword(Hive::Hkcu, EXPLORER_ADV, "HideFileExt", 1)
            .unwrap();
        let planned = plan(
            &[
                ("file_extensions_on".into(), true),
                ("widgets_off".into(), true),
            ],
            &store,
        );
        let ops: Vec<PlannedOp> = planned.iter().flat_map(|c| c.ops.iter().cloned()).collect();
        let (done, errors) = execute(&ops, &store);
        assert!(errors.is_empty());
        assert!(detect(find("file_extensions_on").unwrap(), &store));
        assert!(detect(find("widgets_off").unwrap(), &store));

        let batch = UndoBatch {
            id: "t".into(),
            ts: "t".into(),
            entries: done.iter().map(UndoEntry::from).collect(),
        };
        let (undone, errors) = execute(&undo_ops(&batch), &store);
        assert!(errors.is_empty());
        assert_eq!(undone.len(), 2);
        assert_eq!(
            store.get_dword(Hive::Hkcu, EXPLORER_ADV, "HideFileExt"),
            Some(1)
        );
        assert_eq!(
            store.get_dword(Hive::Hkcu, EXPLORER_ADV, "TaskbarDa"),
            None,
            "a value that did not exist before must be absent again"
        );
    }

    #[test]
    fn undo_store_roundtrips_through_disk() {
        let dir = std::env::temp_dir().join(format!("tidytray-t2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = undo_path(&dir);
        assert!(load_undo(&path).is_empty());
        let batch = UndoBatch {
            id: "b1".into(),
            ts: "2026-11-10T00:00:00Z".into(),
            entries: vec![UndoEntry {
                toggle_id: "widgets_off".into(),
                hive: Hive::Hkcu,
                key: EXPLORER_ADV.into(),
                name: "TaskbarDa".into(),
                prior: None,
            }],
        };
        append_undo(&path, batch).unwrap();
        let loaded = load_undo(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].entries[0].toggle_id, "widgets_off");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
