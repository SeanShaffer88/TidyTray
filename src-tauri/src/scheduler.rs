//! Windows Task Scheduler integration for autonomous runs (blueprint §8.1).
//! Uses the stock `schtasks.exe` (no new dependencies, no service). The task
//! launches this executable with `--autonomous-run`, which performs one
//! policy-gated pass and exits. Argument construction is pure and tested;
//! only install/remove/status touch the OS.

use std::path::Path;
use std::process::Command;

use crate::appconfig::Schedule;

/// Task path as shown in Task Scheduler.
pub const TASK_NAME: &str = "TidyTray\\Autonomous Cleanup";

/// Build the `schtasks /Create` arguments for the given schedule.
pub fn create_args(schedule: &Schedule, exe: &Path) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "/Create".into(),
        "/TN".into(),
        TASK_NAME.into(),
        "/TR".into(),
        format!("\"{}\" --autonomous-run", exe.to_string_lossy()),
    ];
    match schedule.mode.as_str() {
        "daily" => {
            args.push("/SC".into());
            args.push("DAILY".into());
        }
        _ => {
            args.push("/SC".into());
            args.push("WEEKLY".into());
            args.push("/D".into());
            args.push(day_code(&schedule.day).into());
        }
    }
    args.push("/ST".into());
    args.push(format!("{:02}:00", schedule.hour.min(23)));
    args.push("/F".into());
    args
}

/// Map a config day name to schtasks' three-letter code.
pub fn day_code(day: &str) -> &'static str {
    match day.to_ascii_lowercase().as_str() {
        "monday" => "MON",
        "tuesday" => "TUE",
        "wednesday" => "WED",
        "thursday" => "THU",
        "friday" => "FRI",
        "saturday" => "SAT",
        _ => "SUN",
    }
}

fn run_schtasks(args: &[String]) -> Result<(), String> {
    let out = Command::new("schtasks")
        .args(args)
        .output()
        .map_err(|e| format!("could not run schtasks: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(format!("schtasks failed: {stderr}"))
    }
}

/// Create or replace the autonomous-cleanup task for the current config.
pub fn install(schedule: &Schedule) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find own path: {e}"))?;
    run_schtasks(&create_args(schedule, &exe))
}

/// Remove the task if present. Removing a missing task is not an error.
pub fn remove() -> Result<(), String> {
    let args: Vec<String> = vec![
        "/Delete".into(),
        "/TN".into(),
        TASK_NAME.into(),
        "/F".into(),
    ];
    match run_schtasks(&args) {
        Ok(()) => Ok(()),
        Err(_) if !is_installed() => Ok(()),
        Err(e) => Err(e),
    }
}

/// Whether the task currently exists.
pub fn is_installed() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn exe() -> PathBuf {
        PathBuf::from(r"C:\Program Files\TidyTray\tidy.exe")
    }

    #[test]
    fn weekly_args_carry_day_and_hour() {
        let s = Schedule {
            mode: "weekly".into(),
            day: "Sunday".into(),
            hour: 3,
        };
        let args = create_args(&s, &exe());
        assert!(args.windows(2).any(|w| w == ["/SC", "WEEKLY"]));
        assert!(args.windows(2).any(|w| w == ["/D", "SUN"]));
        assert!(args.windows(2).any(|w| w == ["/ST", "03:00"]));
        assert!(args
            .iter()
            .any(|a| a.contains("--autonomous-run") && a.contains("tidy.exe")));
        assert!(args.iter().any(|a| a == TASK_NAME));
    }

    #[test]
    fn daily_args_have_no_day() {
        let s = Schedule {
            mode: "daily".into(),
            day: "Sunday".into(),
            hour: 22,
        };
        let args = create_args(&s, &exe());
        assert!(args.windows(2).any(|w| w == ["/SC", "DAILY"]));
        assert!(!args.iter().any(|a| a == "/D"));
        assert!(args.windows(2).any(|w| w == ["/ST", "22:00"]));
    }

    #[test]
    fn day_codes_map_and_default() {
        assert_eq!(day_code("Monday"), "MON");
        assert_eq!(day_code("friday"), "FRI");
        assert_eq!(day_code("nonsense"), "SUN");
        assert_eq!(day_code(""), "SUN");
    }
}
