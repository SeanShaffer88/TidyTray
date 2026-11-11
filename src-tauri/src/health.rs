//! Memory pressure and drive usage.

use serde::Serialize;
use sysinfo::{DiskKind, Disks, System};

const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Pressure {
    Normal,
    Elevated,
    High,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    /// Standby + modified file cache (memory Windows will hand back on demand).
    pub cached: u64,
    /// Working set of the "Memory Compression" process.
    pub compressed: u64,
    pub committed: u64,
    pub commit_limit: u64,
    pub load_percent: u32,
    /// 0-100 score combining load and commit charge; drives the gauge.
    pub pressure_score: u32,
    pub pressure: Pressure,
    pub swap_used: u64,
    pub process_count: u32,
    pub handle_count: u32,
}

#[cfg(windows)]
fn native_counters() -> Option<(u32, u64, u64, u64, u32, u32)> {
    use windows_sys::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: both structs are plain C data, zero-initialised, with their size field set
    // as the API requires; the pointers are valid for the duration of the calls.
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) == 0 {
            return None;
        }
        let mut perf: PERFORMANCE_INFORMATION = std::mem::zeroed();
        let cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
        if GetPerformanceInfo(&mut perf, cb) == 0 {
            return None;
        }
        let page = perf.PageSize as u64;
        Some((
            status.dwMemoryLoad,
            perf.SystemCache as u64 * page,
            perf.CommitTotal as u64 * page,
            perf.CommitLimit as u64 * page,
            perf.ProcessCount,
            perf.HandleCount,
        ))
    }
}

#[cfg(not(windows))]
fn native_counters() -> Option<(u32, u64, u64, u64, u32, u32)> {
    None
}

/// Combines physical load and commit charge. Commit near the limit is what
/// actually causes "out of memory" dialogs, so it weighs more as it climbs.
pub fn pressure_score(load_percent: u32, committed: u64, commit_limit: u64) -> (u32, Pressure) {
    let commit = if commit_limit == 0 { 0.0 } else { committed as f64 / commit_limit as f64 * 100.0 };
    let score = (load_percent as f64 * 0.6 + commit * 0.4).round().clamp(0.0, 100.0) as u32;
    let level = match score {
        0..=64 => Pressure::Normal,
        65..=84 => Pressure::Elevated,
        _ => Pressure::High,
    };
    (score, level)
}

pub fn memory(sys: &System) -> MemoryInfo {
    let total = sys.total_memory();
    let available = sys.available_memory();
    let compressed = sys
        .processes()
        .values()
        .find(|p| p.name().eq_ignore_ascii_case("Memory Compression"))
        .map(|p| p.memory())
        .unwrap_or(0);
    let (load, cached, committed, commit_limit, procs, handles) = native_counters().unwrap_or((
        (100 - available * 100 / total.max(1)) as u32,
        0,
        sys.used_memory() + sys.used_swap(),
        total + sys.total_swap(),
        sys.processes().len() as u32,
        0,
    ));
    let (pressure_score, pressure) = pressure_score(load, committed, commit_limit);
    MemoryInfo {
        total,
        used: total.saturating_sub(available),
        available,
        cached,
        compressed,
        committed,
        commit_limit,
        load_percent: load,
        pressure_score,
        pressure,
        swap_used: sys.used_swap(),
        process_count: procs,
        handle_count: handles,
    }
}

/// Clears the standby list so cached pages become free memory.
pub fn purge_standby() -> Result<(), &'static str> {
    // TODO(owner): enable SeProfileSingleProcessPrivilege, then call
    // NtSetSystemInformation(SystemMemoryListInformation = 80, MemoryPurgeStandbyList = 4).
    // Requires admin; surface a UAC prompt from the UI before calling.
    Err("standby list purge (needs admin, coming in 1.1)")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: &'static str,
    pub label: &'static str,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    pub mount: String,
    pub label: String,
    pub file_system: String,
    pub kind: &'static str,
    pub total: u64,
    pub free: u64,
    pub removable: bool,
    /// Used space split into categories; `estimated` until the indexer lands.
    pub segments: Vec<Segment>,
    pub estimated: bool,
}

pub fn drives(cleanable_on_system: u64) -> Vec<Drive> {
    let disks = Disks::new_with_refreshed_list();
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
    let mut out: Vec<Drive> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| {
            let mount = d.mount_point().to_string_lossy().trim_end_matches('\\').to_uppercase();
            let is_system = mount == system_drive;
            let used = d.total_space().saturating_sub(d.available_space());
            Drive {
                label: label_for(&d.name().to_string_lossy(), is_system),
                file_system: d.file_system().to_string_lossy().into_owned(),
                kind: match d.kind() {
                    DiskKind::SSD => "SSD",
                    DiskKind::HDD => "HDD",
                    _ => "Drive",
                },
                total: d.total_space(),
                free: d.available_space(),
                removable: d.is_removable(),
                segments: estimate_segments(used, is_system, if is_system { cleanable_on_system } else { 0 }),
                estimated: true,
                mount,
            }
        })
        .collect();
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out
}

fn label_for(name: &str, is_system: bool) -> String {
    match (name.trim(), is_system) {
        ("", true) => "Windows".into(),
        ("", false) => "Local Disk".into(),
        (n, _) => n.to_string(),
    }
}

/// Splits used space into the categories of the storage bar.
/// TODO(owner): replace the ratios with a real MFT-based indexer (USN journal),
/// keeping this as the instant first paint while indexing runs.
pub fn estimate_segments(used: u64, is_system: bool, cleanable: u64) -> Vec<Segment> {
    let cleanable = cleanable.min(used);
    let rest = used - cleanable;
    let ratios: &[(&'static str, &'static str, f64)] = if is_system {
        &[("system", "Windows", 0.22), ("apps", "Apps", 0.31), ("games", "Games", 0.27), ("docs", "Documents", 0.08), ("media", "Photos & video", 0.07)]
    } else {
        &[("games", "Games", 0.48), ("media", "Photos & video", 0.29), ("docs", "Documents", 0.09), ("apps", "Apps", 0.06)]
    };
    let mut segs: Vec<Segment> =
        ratios.iter().map(|(id, label, r)| Segment { id, label, bytes: (rest as f64 * r) as u64 }).collect();
    let assigned: u64 = segs.iter().map(|s| s.bytes).sum();
    segs.push(Segment { id: "other", label: "Other", bytes: rest - assigned });
    if cleanable > 0 {
        segs.push(Segment { id: "junk", label: "Cleanable", bytes: cleanable });
    }
    segs
}

/// Short summary for the tray tooltip, e.g. "C: 61 GB free · RAM 68%".
pub fn tray_summary(sys: &System) -> String {
    let m = memory(sys);
    let free = drives(0).first().map(|d| d.free / GIB).unwrap_or(0);
    format!("TidyTray - {free} GB free, RAM {}%", m.load_percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_levels() {
        assert_eq!(pressure_score(40, 10, 100).1, Pressure::Normal);
        assert_eq!(pressure_score(80, 60, 100).1, Pressure::Elevated);
        assert_eq!(pressure_score(95, 95, 100), (95, Pressure::High));
        assert_eq!(pressure_score(50, 0, 0).0, 30, "zero commit limit must not divide by zero");
    }

    #[test]
    fn segments_sum_to_used() {
        for (used, sys, junk) in [(400 * GIB, true, 12 * GIB), (1300 * GIB, false, 0), (GIB, true, 5 * GIB)] {
            let segs = estimate_segments(used, sys, junk);
            assert_eq!(segs.iter().map(|s| s.bytes).sum::<u64>(), used);
        }
    }

    #[test]
    fn cleanable_segment_only_when_present() {
        assert!(estimate_segments(100 * GIB, true, 0).iter().all(|s| s.id != "junk"));
        assert!(estimate_segments(100 * GIB, true, GIB).iter().any(|s| s.id == "junk"));
    }
}
