//! "Who's eating my PC": live processes grouped by app, with plain-English
//! explanations for the processes people most often google.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    System,
    Service,
    App,
    Browser,
    Game,
    Unknown,
}

/// A known process: what it is and whether ending it is harmless.
pub struct Known {
    pub exe: &'static str,
    pub title: &'static str,
    pub what: &'static str,
    pub kind: Kind,
    pub safe_to_end: bool,
}

macro_rules! known {
    ($($exe:literal, $title:literal, $kind:ident, $safe:literal, $what:literal;)*) => {
        pub static KNOWN: &[Known] = &[$(Known { exe: $exe, title: $title, what: $what, kind: Kind::$kind, safe_to_end: $safe }),*];
    };
}

known! {
    "system", "System", System, false, "The Windows kernel. High CPU here usually means a driver is busy (often storage or network).";
    "memory compression", "Memory Compression", System, false, "Windows squeezing idle memory instead of writing it to disk. Normal; it grows when RAM is tight.";
    "svchost.exe", "Service Host", Service, false, "Runs Windows services in groups. Check which service is busy before blaming it.";
    "dwm.exe", "Desktop Window Manager", System, false, "Draws every window and animation on screen. Spikes with many monitors or high refresh rates.";
    "explorer.exe", "Windows Explorer", System, false, "The taskbar, Start menu and File Explorer. Safe to restart if the taskbar freezes.";
    "csrss.exe", "Client Server Runtime", System, false, "Core Windows process. Never end it.";
    "lsass.exe", "Local Security Authority", System, false, "Handles sign-in and passwords. Never end it.";
    "msmpeng.exe", "Microsoft Defender Antivirus", Service, false, "Real-time malware scanning. Busy during scheduled scans and after big downloads.";
    "searchindexer.exe", "Windows Search Indexer", Service, false, "Builds the search index. Calms down after it catches up with new files.";
    "searchhost.exe", "Windows Search", App, true, "The Start menu search box. Restarts on its own.";
    "tiworker.exe", "Windows Modules Installer", Service, false, "Installs Windows updates. Let it finish; it stops by itself.";
    "wmiprvse.exe", "WMI Provider Host", Service, false, "Answers system queries from apps. Constant high CPU means an app is polling it too often.";
    "audiodg.exe", "Windows Audio Device Graph", Service, false, "Audio effects and mixing. High CPU often points to an audio enhancement driver.";
    "runtimebroker.exe", "Runtime Broker", Service, true, "Checks permissions for Store apps. Usually tiny.";
    "sihost.exe", "Shell Infrastructure Host", System, false, "Notifications, wallpaper and parts of the shell.";
    "ctfmon.exe", "Text Input Host", System, false, "Keyboard layouts, touch keyboard and handwriting input.";
    "spoolsv.exe", "Print Spooler", Service, false, "Queues print jobs for every installed printer.";
    "chrome.exe", "Google Chrome", Browser, true, "Every tab and extension is its own process, so Chrome shows up many times.";
    "msedge.exe", "Microsoft Edge", Browser, true, "One process per tab and extension. Sleeping tabs free most of this.";
    "firefox.exe", "Firefox", Browser, true, "Browser content and GPU processes.";
    "brave.exe", "Brave", Browser, true, "Chromium-based browser; one process per tab.";
    "msedgewebview2.exe", "Edge WebView2", App, true, "A browser engine embedded in other apps (Widgets, Teams, Outlook...).";
    "ms-teams.exe", "Microsoft Teams", App, true, "Chat and meetings. Heavy on memory while running in the background.";
    "onedrive.exe", "OneDrive", App, true, "Syncs files to the cloud. Busy after you add many files.";
    "steam.exe", "Steam", App, true, "Game launcher. steamwebhelper.exe is its built-in browser.";
    "steamwebhelper.exe", "Steam Web Helper", App, true, "The browser inside Steam that renders the store and library.";
    "epicgameslauncher.exe", "Epic Games Launcher", App, true, "Game launcher; keeps running in the tray after you close it.";
    "spotify.exe", "Spotify", App, true, "Music streaming. Several processes are normal.";
    "code.exe", "Visual Studio Code", App, true, "Code editor. Extensions run in their own helper processes.";
    "nvcontainer.exe", "NVIDIA Container", Service, true, "Hosts NVIDIA app features like overlays and driver updates.";
    "nvdisplay.container.exe", "NVIDIA Display Container", Service, false, "NVIDIA control panel and display settings.";
    "radeonsoftware.exe", "AMD Software", App, true, "AMD driver settings and overlay.";
    "phoneexperiencehost.exe", "Phone Link", App, true, "Connects your phone to Windows. Safe to close if you do not use it.";
    "widgets.exe", "Windows Widgets", App, true, "The weather and news board. Can be turned off in taskbar settings.";
    "obs64.exe", "OBS Studio", App, true, "Recording and streaming. CPU depends on encoder settings.";
    "cs2.exe", "Counter-Strike 2", Game, true, "A game. High CPU and GPU while playing is expected.";
}

pub fn lookup(exe: &str) -> Option<&'static Known> {
    let exe = exe.to_lowercase();
    KNOWN.iter().find(|k| k.exe == exe)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    pub name: String,
    pub title: String,
    pub what: String,
    pub kind: Kind,
    pub safe_to_end: bool,
    pub instances: u32,
    pub pids: Vec<u32>,
    /// Share of total CPU across all cores (0-100), like Task Manager.
    pub cpu: f32,
    pub memory: u64,
    pub disk_read_per_sec: u64,
    pub history: Vec<f32>,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    #[default]
    Cpu,
    Memory,
    Name,
}

/// Turns raw processes into rows, one per executable name.
pub fn snapshot(sys: &System, sort: SortBy, limit: usize) -> Vec<ProcessRow> {
    let cores = sys.cpus().len().max(1) as f32;
    let mut groups: HashMap<String, ProcessRow> = HashMap::new();
    for (pid, p) in sys.processes() {
        let name = p.name().to_string_lossy().into_owned();
        let key = name.to_lowercase();
        let row = groups.entry(key).or_insert_with(|| new_row(&name));
        row.instances += 1;
        row.pids.push(pid.as_u32());
        row.cpu += p.cpu_usage() / cores;
        row.memory += p.memory();
        row.disk_read_per_sec += p.disk_usage().read_bytes;
    }
    let mut rows: Vec<ProcessRow> = groups.into_values().collect();
    for r in &mut rows {
        r.cpu = (r.cpu * 10.0).round() / 10.0;
    }
    sort_rows(&mut rows, sort);
    rows.truncate(limit);
    rows
}

fn new_row(name: &str) -> ProcessRow {
    let known = lookup(name);
    ProcessRow {
        name: name.to_string(),
        title: known.map(|k| k.title.to_string()).unwrap_or_else(|| pretty_name(name)),
        what: known.map(|k| k.what.to_string()).unwrap_or_default(),
        kind: known.map(|k| k.kind).unwrap_or(Kind::Unknown),
        safe_to_end: known.map(|k| k.safe_to_end).unwrap_or(false),
        instances: 0,
        pids: Vec::new(),
        cpu: 0.0,
        memory: 0,
        disk_read_per_sec: 0,
        history: Vec::new(),
    }
}

pub fn sort_rows(rows: &mut [ProcessRow], sort: SortBy) {
    match sort {
        SortBy::Cpu => rows.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.cmp(&a.memory))),
        SortBy::Memory => rows.sort_by_key(|r| std::cmp::Reverse(r.memory)),
        SortBy::Name => rows.sort_by_key(|r| r.title.to_lowercase()),
    }
}

/// "nvcontainer.exe" -> "Nvcontainer" for processes we know nothing about.
pub fn pretty_name(exe: &str) -> String {
    let stem = exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(exe);
    let mut chars = stem.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Ends every instance of an app. Refuses for system processes.
pub fn end_process(sys: &System, name: &str) -> Result<u32, String> {
    if lookup(name).is_some_and(|k| !k.safe_to_end) {
        return Err(format!("{name} is part of Windows and should not be ended"));
    }
    let key = name.to_lowercase();
    let mut ended = 0;
    for p in sys.processes().values() {
        if p.name().to_string_lossy().to_lowercase() == key && p.kill() {
            ended += 1;
        }
    }
    Ok(ended)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, cpu: f32, memory: u64) -> ProcessRow {
        let mut r = new_row(title);
        r.cpu = cpu;
        r.memory = memory;
        r
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(lookup("MsMpEng.exe").unwrap().title, "Microsoft Defender Antivirus");
        assert!(lookup("totally-unknown.exe").is_none());
    }

    #[test]
    fn known_table_has_no_duplicates_and_explains_everything() {
        let mut seen = std::collections::HashSet::new();
        for k in KNOWN {
            assert!(seen.insert(k.exe), "duplicate entry {}", k.exe);
            assert!(k.what.len() > 20 && k.what.ends_with('.'), "weak explanation for {}", k.exe);
            assert_eq!(k.exe, k.exe.to_lowercase());
        }
    }

    #[test]
    fn sorting() {
        let mut rows = vec![row("b.exe", 1.0, 900), row("a.exe", 9.5, 100), row("c.exe", 1.0, 5000)];
        sort_rows(&mut rows, SortBy::Cpu);
        assert_eq!(rows[0].name, "a.exe");
        assert_eq!(rows[1].name, "c.exe", "ties broken by memory");
        sort_rows(&mut rows, SortBy::Memory);
        assert_eq!(rows[0].name, "c.exe");
        sort_rows(&mut rows, SortBy::Name);
        assert_eq!(rows[0].name, "a.exe");
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty_name("blender.exe"), "Blender");
        assert_eq!(pretty_name("Registry"), "Registry");
        assert_eq!(pretty_name(""), "");
    }
}
