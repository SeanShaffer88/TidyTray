//! Pre-flight environment checks for safety cleanup runs (blueprint §8.1):
//! battery level, fullscreen apps, input idle time and free disk space.
//! The skip decision is a pure function over collected facts so it is fully
//! testable; only fact collection touches the OS.

use serde::Serialize;

use crate::appconfig::AppConfig;

/// Environment facts consulted before a run.
#[derive(Debug, Clone, Serialize)]
pub struct Facts {
    /// Battery charge 0..=100, or None on desktops / when unknown.
    pub battery_percent: Option<u32>,
    pub on_ac_power: bool,
    pub fullscreen_app: bool,
    pub idle_seconds: u64,
    pub free_space_gb: f64,
}

impl Facts {
    /// Benign facts (nothing blocks a run) — used off Windows and as a
    /// fallback when an OS query fails.
    pub fn benign() -> Facts {
        Facts {
            battery_percent: None,
            on_ac_power: true,
            fullscreen_app: false,
            idle_seconds: 0,
            free_space_gb: f64::INFINITY,
        }
    }
}

/// Returns Some(skip reason) when a run must not start under this config,
/// None when it may proceed. Pure: no OS access.
pub fn skip_reason(facts: &Facts, config: &AppConfig) -> Option<String> {
    let auto = &config.autonomous;
    if !facts.on_ac_power {
        if let Some(pct) = facts.battery_percent {
            if pct < auto.pause_on_battery_below_pct {
                return Some(format!(
                    "battery at {pct}% (pause threshold {}%)",
                    auto.pause_on_battery_below_pct
                ));
            }
        }
    }
    if auto.pause_on_fullscreen && facts.fullscreen_app {
        return Some("a fullscreen app is active".to_string());
    }
    None
}

/// Collect the current facts. Never fails: any query error degrades to
/// the benign value for that fact.
pub fn collect() -> Facts {
    #[cfg(windows)]
    {
        collect_windows()
    }
    #[cfg(not(windows))]
    {
        Facts::benign()
    }
}

#[cfg(windows)]
fn collect_windows() -> Facts {
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    let mut facts = Facts::benign();

    unsafe {
        let mut status = SYSTEM_POWER_STATUS {
            ACLineStatus: 0,
            BatteryFlag: 0,
            BatteryLifePercent: 255,
            SystemStatusFlag: 0,
            BatteryLifeTime: 0,
            BatteryFullLifeTime: 0,
        };
        if GetSystemPowerStatus(&mut status) != 0 {
            facts.on_ac_power = status.ACLineStatus == 1;
            if status.BatteryLifePercent <= 100 {
                facts.battery_percent = Some(status.BatteryLifePercent as u32);
            }
        }
    }

    facts.fullscreen_app = fullscreen_windows();
    facts.idle_seconds = idle_seconds_windows();

    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if let Some(root) = Path::new(&local).ancestors().last() {
            let root_buf: PathBuf = root.to_path_buf();
            if let Some(gb) = free_space_gb_windows(&root_buf) {
                facts.free_space_gb = gb;
            }
        }
    }
    facts
}

#[cfg(windows)]
fn fullscreen_windows() -> bool {
    use windows_sys::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    unsafe {
        let mut state = 0i32;
        let hr = SHQueryUserNotificationState(&mut state);
        hr == 0 && (state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE)
    }
}

#[cfg(windows)]
fn idle_seconds_windows() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut info) != 0 {
            let now = GetTickCount();
            return now.wrapping_sub(info.dwTime) as u64 / 1000;
        }
        0
    }
}

#[cfg(windows)]
fn free_space_gb_windows(root: &std::path::Path) -> Option<f64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = root
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok != 0 {
        Some(free as f64 / 1_073_741_824.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(battery: Option<u32>, ac: bool, fullscreen: bool) -> Facts {
        Facts {
            battery_percent: battery,
            on_ac_power: ac,
            fullscreen_app: fullscreen,
            idle_seconds: 0,
            free_space_gb: 100.0,
        }
    }

    #[test]
    fn low_battery_on_battery_power_skips() {
        let cfg = AppConfig::default();
        assert!(skip_reason(&facts(Some(20), false, false), &cfg).is_some());
        // On AC power the same charge does not block.
        assert!(skip_reason(&facts(Some(20), true, false), &cfg).is_none());
        // Above the threshold does not block.
        assert!(skip_reason(&facts(Some(80), false, false), &cfg).is_none());
        // Unknown battery (desktop) does not block.
        assert!(skip_reason(&facts(None, false, false), &cfg).is_none());
    }

    #[test]
    fn fullscreen_skips_only_when_enabled() {
        let cfg = AppConfig::default();
        assert!(skip_reason(&facts(None, true, true), &cfg).is_some());
        let mut cfg2 = AppConfig::default();
        cfg2.autonomous.pause_on_fullscreen = false;
        assert!(skip_reason(&facts(None, true, true), &cfg2).is_none());
    }

    #[test]
    fn battery_pause_threshold_zero_never_skips() {
        let mut cfg = AppConfig::default();
        cfg.autonomous.pause_on_battery_below_pct = 0;
        assert!(skip_reason(&facts(Some(5), false, false), &cfg).is_none());
    }
}
