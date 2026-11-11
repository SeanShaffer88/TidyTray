//! Tier 2 app management (blueprint §6.2, milestone M5): the bundled-app
//! removal catalog and the winget install catalog with profiles.
//!
//! Removal follows the same discipline as the Tweaks pipeline:
//! staged selection → preview (with each app's reinstall source shown
//! *before* anything happens) → restore point → elevated removal →
//! recorded undo batch. Undo reinstalls through winget where a winget
//! id is known; Store-only apps are reported honestly as manual
//! reinstalls instead of pretending to be automated.
//!
//! The catalog deliberately excludes anything Windows needs to run and
//! anything that is a core utility (Terminal, Calculator, Notepad,
//! Paint, Photos stays Advanced). Detection and list parsing are pure
//! functions over command output, so they are testable off-Windows.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Removal catalog
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Caution,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reinstall {
    /// `winget install --id <id>` brings it back automatically.
    Winget(&'static str),
    /// Available in the Microsoft Store; reinstall is a manual step.
    Store,
    /// No reliable automated reinstall (e.g. the Store itself).
    Manual,
}

#[derive(Debug, Clone, Copy)]
pub struct AppEntry {
    pub display: &'static str,
    /// AppX package name as reported by Get-AppxPackage.
    pub package: &'static str,
    pub risk: Risk,
    pub reinstall: Reinstall,
}

static REMOVAL_CATALOG: &[AppEntry] = &[
    AppEntry {
        display: "Microsoft Copilot",
        package: "Microsoft.Copilot",
        risk: Risk::Safe,
        reinstall: Reinstall::Winget("Microsoft.Copilot"),
    },
    AppEntry {
        display: "Microsoft Clipchamp",
        package: "Clipchamp.Clipchamp",
        risk: Risk::Safe,
        reinstall: Reinstall::Winget("Clipchamp.Clipchamp"),
    },
    AppEntry {
        display: "Candy Crush Saga",
        package: "king.com.CandyCrushSaga",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Candy Crush Soda Saga",
        package: "king.com.CandyCrushSodaSaga",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft Solitaire Collection",
        package: "Microsoft.MicrosoftSolitaireCollection",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Spotify (preinstalled)",
        package: "SpotifyAB.SpotifyMusic",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft News",
        package: "Microsoft.BingNews",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Weather",
        package: "Microsoft.BingWeather",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Tips",
        package: "Microsoft.Getstarted",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Get Help",
        package: "Microsoft.GetHelp",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft To Do",
        package: "Microsoft.Todos",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "People",
        package: "Microsoft.People",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Sticky Notes",
        package: "Microsoft.MicrosoftStickyNotes",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Skype",
        package: "Microsoft.SkypeApp",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Groove Music",
        package: "Microsoft.ZuneMusic",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Movies & TV",
        package: "Microsoft.ZuneVideo",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Mail and Calendar",
        package: "Microsoft.windowscommunicationsapps",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Feedback Hub",
        package: "Microsoft.WindowsFeedbackHub",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Maps",
        package: "Microsoft.WindowsMaps",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Mixed Reality Portal",
        package: "Microsoft.MixedReality.Portal",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Phone Link",
        package: "Microsoft.YourPhone",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Xbox Game Bar",
        package: "Microsoft.XboxGamingOverlay",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Xbox",
        package: "Microsoft.GamingApp",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Teams (personal)",
        package: "MicrosoftTeams",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Cortana",
        package: "Microsoft.549981C3F5F10",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft Store",
        package: "Microsoft.WindowsStore",
        risk: Risk::Advanced,
        reinstall: Reinstall::Manual,
    },
    // --- Catalog growth (2026-11-11 review, Batch C): well-known
    // inbox and preload packages. Badges stay conservative: anything
    // holding user content or feeding a device feature is Caution or
    // Advanced, and only Store reinstalls are claimed where the Store
    // listing is the reliable route back.
    AppEntry {
        display: "Candy Crush Friends Saga",
        package: "king.com.CandyCrushFriendsSaga",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Bubble Witch 3 Saga",
        package: "king.com.BubbleWitch3Saga",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft Mahjong",
        package: "Microsoft.MicrosoftMahjong",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft Jigsaw",
        package: "Microsoft.MicrosoftJigsaw",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Netflix (preinstalled)",
        package: "4DF9E0F8.Netflix",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Office (Microsoft 365 hub)",
        package: "Microsoft.MicrosoftOfficeHub",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Mobile Plans",
        package: "Microsoft.OneConnect",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Print 3D",
        package: "Microsoft.Print3D",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Paint 3D",
        package: "Microsoft.MSPaint",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "3D Viewer",
        package: "Microsoft.Microsoft3DViewer",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Sound Recorder",
        package: "Microsoft.WindowsSoundRecorder",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Alarms & Clock",
        package: "Microsoft.WindowsAlarms",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft Wallet",
        package: "Microsoft.Wallet",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Dev Home",
        package: "Microsoft.Windows.DevHome",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "PC Manager",
        package: "Microsoft.MicrosoftPCManager",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Dolby Access (preinstalled)",
        package: "DolbyLaboratories.DolbyAccess",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Start Experiences",
        package: "Microsoft.StartExperiencesApp",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Xbox Game Overlay",
        package: "Microsoft.XboxGameOverlay",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Xbox Speech to Text Overlay",
        package: "Microsoft.XboxSpeechToTextOverlay",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "OneNote",
        package: "Microsoft.Office.OneNote",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Camera",
        package: "Microsoft.WindowsCamera",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Photos",
        package: "Microsoft.Windows.Photos",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Outlook (new)",
        package: "Microsoft.OutlookForWindows",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Teams",
        package: "MSTeams",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Power Automate",
        package: "Microsoft.PowerAutomateDesktop",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Quick Assist",
        package: "MicrosoftCorporationII.QuickAssist",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Xbox Identity Provider",
        package: "Microsoft.XboxIdentityProvider",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Minecraft for Windows",
        package: "Microsoft.MinecraftUWP",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Realtek Audio Control",
        package: "RealtekSemiconductorCorp.RealtekAudioControl",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Take a Test",
        package: "Microsoft.Windows.SecureAssessmentBrowser",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "VP9 Video Extensions",
        package: "Microsoft.VP9VideoExtensions",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "HEIF Image Extensions",
        package: "Microsoft.HEIFImageExtension",
        risk: Risk::Advanced,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Windows Web Experience Pack (Widgets platform)",
        package: "MicrosoftWindows.Client.WebExperience",
        risk: Risk::Advanced,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Gaming Services",
        package: "Microsoft.GamingServices",
        risk: Risk::Advanced,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Store Purchase App",
        package: "Microsoft.StorePurchaseApp",
        risk: Risk::Advanced,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "App Installer (winget)",
        package: "Microsoft.DesktopAppInstaller",
        risk: Risk::Advanced,
        reinstall: Reinstall::Manual,
    },
    AppEntry {
        display: "Sway",
        package: "Microsoft.Office.Sway",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Messages",
        package: "Microsoft.Messaging",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Bing Finance",
        package: "Microsoft.BingFinance",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Bing Sports",
        package: "Microsoft.BingSports",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Bing Travel",
        package: "Microsoft.BingTravel",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Health & Fitness",
        package: "Microsoft.BingHealthAndFitness",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "App Connector",
        package: "Microsoft.Appconnector",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Power BI",
        package: "Microsoft.MicrosoftPowerBIForWindows",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Speed Test",
        package: "Microsoft.NetworkSpeedTest",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Fresh Paint",
        package: "Microsoft.FreshPaint",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Remote Desktop",
        package: "Microsoft.RemoteDesktop",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Whiteboard",
        package: "Microsoft.Whiteboard",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Xbox Console Companion",
        package: "Microsoft.XboxApp",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Xbox TCUI",
        package: "Microsoft.Xbox.TCUI",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Narrator QuickStart",
        package: "Microsoft.NarratorQuickStart",
        risk: Risk::Caution,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft Family",
        package: "MicrosoftCorporationII.MicrosoftFamily",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Windows Scan",
        package: "Microsoft.WindowsScan",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Microsoft Journal",
        package: "Microsoft.MicrosoftJournal",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Word Mobile",
        package: "Microsoft.Office.Word",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Excel Mobile",
        package: "Microsoft.Office.Excel",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "PowerPoint Mobile",
        package: "Microsoft.Office.PowerPoint",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
    AppEntry {
        display: "Office Lens",
        package: "Microsoft.OfficeLens",
        risk: Risk::Safe,
        reinstall: Reinstall::Store,
    },
];

pub fn removal_catalog() -> &'static [AppEntry] {
    REMOVAL_CATALOG
}

pub fn find_app(package: &str) -> Option<&'static AppEntry> {
    REMOVAL_CATALOG.iter().find(|a| a.package == package)
}

impl Reinstall {
    pub fn label(&self) -> String {
        match self {
            Reinstall::Winget(id) => format!("winget · {id}"),
            Reinstall::Store => "Microsoft Store".to_string(),
            Reinstall::Manual => "Manual reinstall".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Install catalog + profiles
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct InstallEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub desc: &'static str,
}

static INSTALL_CATALOG: &[InstallEntry] = &[
    InstallEntry {
        id: "Mozilla.Firefox",
        name: "Firefox",
        category: "Browsers",
        desc: "Web browser by Mozilla.",
    },
    InstallEntry {
        id: "Google.Chrome",
        name: "Google Chrome",
        category: "Browsers",
        desc: "Web browser by Google.",
    },
    InstallEntry {
        id: "Brave.Brave",
        name: "Brave",
        category: "Browsers",
        desc: "Privacy-focused web browser.",
    },
    InstallEntry {
        id: "VideoLAN.VLC",
        name: "VLC media player",
        category: "Media",
        desc: "Plays nearly every audio and video format.",
    },
    InstallEntry {
        id: "Spotify.Spotify",
        name: "Spotify",
        category: "Media",
        desc: "Music and podcast streaming.",
    },
    InstallEntry {
        id: "OBSProject.OBSStudio",
        name: "OBS Studio",
        category: "Media",
        desc: "Recording and live streaming.",
    },
    InstallEntry {
        id: "7zip.7zip",
        name: "7-Zip",
        category: "Utilities",
        desc: "File archiver for zip, 7z, rar and more.",
    },
    InstallEntry {
        id: "Notepad++.Notepad++",
        name: "Notepad++",
        category: "Utilities",
        desc: "Lightweight text and code editor.",
    },
    InstallEntry {
        id: "Microsoft.PowerToys",
        name: "PowerToys",
        category: "Utilities",
        desc: "Microsoft's power-user utilities.",
    },
    InstallEntry {
        id: "voidtools.Everything",
        name: "Everything",
        category: "Utilities",
        desc: "Instant file search by name.",
    },
    InstallEntry {
        id: "ShareX.ShareX",
        name: "ShareX",
        category: "Utilities",
        desc: "Screenshots, annotation and sharing.",
    },
    InstallEntry {
        id: "Microsoft.VisualStudioCode",
        name: "Visual Studio Code",
        category: "Development",
        desc: "Code editor by Microsoft.",
    },
    InstallEntry {
        id: "Microsoft.WindowsTerminal",
        name: "Windows Terminal",
        category: "Development",
        desc: "Modern terminal for shells and WSL.",
    },
    InstallEntry {
        id: "Git.Git",
        name: "Git",
        category: "Development",
        desc: "Version control.",
    },
    InstallEntry {
        id: "OpenJS.NodeJS.LTS",
        name: "Node.js (LTS)",
        category: "Development",
        desc: "JavaScript runtime, long-term support build.",
    },
    InstallEntry {
        id: "Python.Python.3.13",
        name: "Python 3.13",
        category: "Development",
        desc: "Python programming language.",
    },
    InstallEntry {
        id: "Discord.Discord",
        name: "Discord",
        category: "Communication",
        desc: "Chat, voice and communities.",
    },
    InstallEntry {
        id: "Zoom.Zoom",
        name: "Zoom",
        category: "Communication",
        desc: "Video meetings.",
    },
    InstallEntry {
        id: "Telegram.TelegramDesktop",
        name: "Telegram Desktop",
        category: "Communication",
        desc: "Messaging app.",
    },
    InstallEntry {
        id: "Valve.Steam",
        name: "Steam",
        category: "Gaming",
        desc: "Game library and store by Valve.",
    },
    InstallEntry {
        id: "GOG.Galaxy",
        name: "GOG Galaxy",
        category: "Gaming",
        desc: "Game library across stores.",
    },
];

pub fn install_catalog() -> &'static [InstallEntry] {
    INSTALL_CATALOG
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub ids: Vec<String>,
    pub builtin: bool,
}

pub fn builtin_profiles() -> Vec<Profile> {
    vec![
        Profile {
            name: "Essentials".to_string(),
            ids: [
                "Mozilla.Firefox",
                "VideoLAN.VLC",
                "7zip.7zip",
                "Notepad++.Notepad++",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            builtin: true,
        },
        Profile {
            name: "Dev workstation".to_string(),
            ids: [
                "Microsoft.VisualStudioCode",
                "Microsoft.WindowsTerminal",
                "Git.Git",
                "OpenJS.NodeJS.LTS",
                "Python.Python.3.13",
                "Microsoft.PowerToys",
                "7zip.7zip",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            builtin: true,
        },
    ]
}

pub fn profiles_path(data_dir: &Path) -> PathBuf {
    data_dir.join("profiles.json")
}

pub fn load_profiles(data_dir: &Path) -> Vec<Profile> {
    let mut out = builtin_profiles();
    if let Ok(text) = std::fs::read_to_string(profiles_path(data_dir)) {
        if let Ok(mut saved) = serde_json::from_str::<Vec<Profile>>(&text) {
            for p in saved.iter_mut() {
                p.builtin = false;
            }
            out.extend(saved);
        }
    }
    out
}

pub fn save_profile(data_dir: &Path, name: &str, ids: &[String]) -> std::io::Result<()> {
    let path = profiles_path(data_dir);
    let mut saved: Vec<Profile> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    saved.retain(|p| p.name != name);
    saved.push(Profile {
        name: name.to_string(),
        ids: ids.to_vec(),
        builtin: false,
    });
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&saved).unwrap_or_default())
}

pub fn delete_profile(data_dir: &Path, name: &str) -> std::io::Result<()> {
    let path = profiles_path(data_dir);
    let mut saved: Vec<Profile> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    saved.retain(|p| p.name != name);
    std::fs::write(&path, serde_json::to_vec_pretty(&saved).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Detection (pure parsers over command output)
// ---------------------------------------------------------------------------

/// Parses `Get-AppxPackage | Select-Object -ExpandProperty Name` output.
pub fn parse_appx_names(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Which install-catalog ids appear in `winget list` output. winget's
/// table format varies, so detection is a conservative substring match
/// per known id — a miss just shows "not installed", never the reverse.
pub fn parse_winget_installed(output: &str) -> BTreeSet<String> {
    INSTALL_CATALOG
        .iter()
        .filter(|e| output.contains(e.id))
        .map(|e| e.id.to_string())
        .collect()
}

#[cfg(windows)]
pub fn installed_packages() -> BTreeSet<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-AppxPackage | Select-Object -ExpandProperty Name",
        ])
        .output();
    match output {
        Ok(o) => parse_appx_names(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => BTreeSet::new(),
    }
}

#[cfg(not(windows))]
pub fn installed_packages() -> BTreeSet<String> {
    BTreeSet::new()
}

#[cfg(windows)]
pub fn winget_installed() -> Option<BTreeSet<String>> {
    let output = std::process::Command::new("winget")
        .args(["list", "--accept-source-agreements"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_winget_installed(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(not(windows))]
pub fn winget_installed() -> Option<BTreeSet<String>> {
    None
}

// ---------------------------------------------------------------------------
// Removal: elevated helper protocol (`--apps-elevated <batch.json>`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalBatch {
    pub restore_point: bool,
    pub packages: Vec<String>,
    /// Random token pairing this batch with its result file. Filled in
    /// by the parent at launch; echoed back by the helper.
    #[serde(default)]
    pub nonce: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalResult {
    pub restore_point_created: bool,
    pub removed: Vec<String>,
    pub errors: Vec<String>,
    /// Echo of the batch nonce; the parent ignores results without it.
    #[serde(default)]
    pub nonce: String,
}

fn removal_result_path(batch_path: &Path) -> PathBuf {
    batch_path.with_extension("result.json")
}

/// The PowerShell that removes one package for the current user and its
/// provisioned copy (so it does not come back for new users). Package
/// names come from the fixed catalog only — never from user input —
/// and are still single-quote escaped.
pub fn removal_script(package: &str) -> String {
    let p = package.replace('\'', "''");
    format!(
        "$ErrorActionPreference='Stop'; \
         Get-AppxPackage -Name '{p}' | Remove-AppxPackage; \
         Get-AppxProvisionedPackage -Online | Where-Object {{ $_.DisplayName -eq '{p}' }} | Remove-AppxProvisionedPackage -Online | Out-Null"
    )
}

#[cfg(windows)]
pub fn run_removal_helper(batch_path: &Path) -> i32 {
    let Ok(text) = std::fs::read_to_string(batch_path) else {
        return 3;
    };
    let Ok(batch) = serde_json::from_str::<RemovalBatch>(&text) else {
        return 3;
    };
    let mut result = RemovalResult {
        nonce: batch.nonce.clone(),
        ..Default::default()
    };
    if batch.restore_point {
        result.restore_point_created = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                   "Checkpoint-Computer -Description 'TidyTray app removal' -RestorePointType 'MODIFY_SETTINGS'"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    for package in &batch.packages {
        // Only catalog packages may ever be removed, even by the helper.
        if find_app(package).is_none() {
            result
                .errors
                .push(format!("{package}: not in the removal catalog"));
            continue;
        }
        match std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &removal_script(package),
            ])
            .status()
        {
            Ok(s) if s.success() => result.removed.push(package.clone()),
            Ok(s) => result
                .errors
                .push(format!("{package}: removal exited with {s}")),
            Err(e) => result.errors.push(format!("{package}: {e}")),
        }
    }
    let _ = std::fs::write(
        removal_result_path(batch_path),
        serde_json::to_vec_pretty(&result).unwrap_or_default(),
    );
    if result.errors.is_empty() {
        0
    } else {
        2
    }
}

#[cfg(not(windows))]
pub fn run_removal_helper(_batch_path: &Path) -> i32 {
    3
}

/// Parent side: one elevated run covers the restore point + all removals.
#[cfg(windows)]
pub fn launch_removal(mut batch: RemovalBatch, dir: &Path) -> Result<RemovalResult, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    batch.nonce = crate::audit::batch_token();
    let batch_path = dir.join(format!("apps-batch-{}.json", &batch.nonce[..24]));
    std::fs::write(
        &batch_path,
        serde_json::to_vec_pretty(&batch).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let params = format!("--apps-elevated \"{}\"", batch_path.display());
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
    // Removals plus a restore point can take a while; allow 5 minutes.
    let waited = unsafe { WaitForSingleObject(info.hProcess, 300_000) };
    unsafe { CloseHandle(info.hProcess) };
    if waited != 0 {
        // Still running (or the wait failed): removals may still land,
        // so the batch stays on disk for the next remove/undo to
        // reconcile into the undo store.
        return Err(
            "the elevated helper did not finish in time; any removals it completes              will be recorded for Undo on the next remove or undo"
                .to_string(),
        );
    }
    let parsed = std::fs::read_to_string(removal_result_path(&batch_path))
        .ok()
        .and_then(|t| serde_json::from_str::<RemovalResult>(&t).ok());
    let result = match parsed {
        Some(r) if r.nonce == batch.nonce => Ok(r),
        Some(_) => Err("the elevated helper's result did not match its batch".to_string()),
        None => Err("the elevated helper did not report a result".to_string()),
    };
    let _ = std::fs::remove_file(&batch_path);
    let _ = std::fs::remove_file(removal_result_path(&batch_path));
    result
}

#[cfg(not(windows))]
pub fn launch_removal(_batch: RemovalBatch, _dir: &Path) -> Result<RemovalResult, String> {
    Err("elevation is only available on Windows".to_string())
}

// ---------------------------------------------------------------------------
// Removal orchestration + undo records
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedRecord {
    pub package: String,
    pub display: String,
    /// The winget id that brings it back automatically, when one exists.
    pub winget_id: Option<String>,
    /// Plain-words reinstall source shown in the UI ("Microsoft Store"…).
    pub reinstall_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalUndoBatch {
    pub id: String,
    pub ts: String,
    pub entries: Vec<RemovedRecord>,
}

pub fn apps_undo_path(data_dir: &Path) -> PathBuf {
    data_dir.join("apps-undo.json")
}

pub fn load_removal_undo(path: &Path) -> Vec<RemovalUndoBatch> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_removal_undo(path: &Path, batches: &[RemovalUndoBatch]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(batches).unwrap_or_default())?;
    std::fs::rename(tmp, path)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalOutcome {
    pub removed: Vec<String>,
    pub failed: Vec<String>,
    pub restore_point_created: bool,
}

/// Folds results from removal helpers that outlived their parent's
/// wait into the removal undo store, so a slow-but-successful removal
/// never leaves an app with no way back. Batch files with no result
/// after 15 minutes are dropped. Returns how many were folded in.
pub fn reconcile_orphans(data_dir: &Path, logs_dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(data_dir) else {
        return 0;
    };
    let log = crate::audit::AuditLog::new(logs_dir.to_path_buf());
    let mut folded = 0;
    for entry in rd.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("apps-batch-") || !name.ends_with(".json") || name.contains("result") {
            continue;
        }
        let rpath = removal_result_path(&path);
        if rpath.exists() {
            let batch = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<RemovalBatch>(&t).ok());
            let result = std::fs::read_to_string(&rpath)
                .ok()
                .and_then(|t| serde_json::from_str::<RemovalResult>(&t).ok());
            if let (Some(b), Some(r)) = (batch, result) {
                if r.nonce == b.nonce && !r.removed.is_empty() {
                    let entries: Vec<RemovedRecord> = r
                        .removed
                        .iter()
                        .filter_map(|p| {
                            find_app(p).map(|a| RemovedRecord {
                                package: p.clone(),
                                display: a.display.to_string(),
                                winget_id: match a.reinstall {
                                    Reinstall::Winget(id) => Some(id.to_string()),
                                    _ => None,
                                },
                                reinstall_label: a.reinstall.label(),
                            })
                        })
                        .collect();
                    if !entries.is_empty() {
                        let mut batches = load_removal_undo(&apps_undo_path(data_dir));
                        batches.push(RemovalUndoBatch {
                            id: crate::audit::new_run_id(),
                            ts: crate::audit::iso_now(),
                            entries,
                        });
                        if save_removal_undo(&apps_undo_path(data_dir), &batches).is_ok() {
                            let _ = log.append(
                                &crate::audit::LogEntry::new(
                                    &crate::audit::new_run_id(),
                                    "app-remove",
                                )
                                .result("reconciled"),
                            );
                            folded += 1;
                        }
                    }
                }
            }
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&rpath);
        } else {
            let stale = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| t.elapsed().map(|e| e.as_secs() > 900).unwrap_or(false))
                .unwrap_or(false);
            if stale {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    folded
}

/// Removes the selected catalog packages through one elevated run and
/// records an undo batch for exactly what was removed.
pub fn remove_apps(data_dir: &Path, logs_dir: &Path, packages: &[String]) -> RemovalOutcome {
    let _ = reconcile_orphans(data_dir, logs_dir);
    let log = crate::audit::AuditLog::new(logs_dir.to_path_buf());
    // Intersect with the catalog — arbitrary package names never reach
    // the helper, whatever the caller passed.
    let chosen: Vec<String> = packages
        .iter()
        .filter(|p| find_app(p).is_some())
        .cloned()
        .collect();
    let mut failed: Vec<String> = packages
        .iter()
        .filter(|p| find_app(p).is_none())
        .map(|p| format!("{p}: not in the removal catalog"))
        .collect();
    if chosen.is_empty() {
        return RemovalOutcome {
            removed: Vec::new(),
            failed,
            restore_point_created: false,
        };
    }
    let batch = RemovalBatch {
        restore_point: true,
        packages: chosen,
        nonce: String::new(),
    };
    let (removed, restore_point_created) = match launch_removal(batch, data_dir) {
        Ok(result) => {
            failed.extend(result.errors);
            (result.removed, result.restore_point_created)
        }
        Err(e) => {
            failed.push(e);
            (Vec::new(), false)
        }
    };
    if !removed.is_empty() {
        let entries = removed
            .iter()
            .filter_map(|p| {
                find_app(p).map(|a| RemovedRecord {
                    package: p.clone(),
                    display: a.display.to_string(),
                    winget_id: match a.reinstall {
                        Reinstall::Winget(id) => Some(id.to_string()),
                        _ => None,
                    },
                    reinstall_label: a.reinstall.label(),
                })
            })
            .collect();
        let mut batches = load_removal_undo(&apps_undo_path(data_dir));
        batches.push(RemovalUndoBatch {
            id: crate::audit::new_run_id(),
            ts: crate::audit::iso_now(),
            entries,
        });
        let _ = save_removal_undo(&apps_undo_path(data_dir), &batches);
    }
    for p in packages {
        let _ = log.append(
            &crate::audit::LogEntry::new(&crate::audit::new_run_id(), "app-remove")
                .result(if removed.contains(p) {
                    "removed"
                } else {
                    "failed"
                })
                .path(Path::new(p)),
        );
    }
    RemovalOutcome {
        removed,
        failed,
        restore_point_created,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUndoOutcome {
    pub reinstalled: Vec<String>,
    /// Display names that need a manual Store reinstall.
    pub manual: Vec<String>,
    pub failed: Vec<String>,
    pub nothing_to_undo: bool,
}

/// Reinstalls the most recent removal batch: winget where an id is
/// known; Store/Manual entries are reported, not faked.
pub fn undo_last_removal(data_dir: &Path, logs_dir: &Path) -> AppUndoOutcome {
    let _ = reconcile_orphans(data_dir, logs_dir);
    let path = apps_undo_path(data_dir);
    let mut batches = load_removal_undo(&path);
    let Some(batch) = batches.pop() else {
        return AppUndoOutcome {
            reinstalled: Vec::new(),
            manual: Vec::new(),
            failed: Vec::new(),
            nothing_to_undo: true,
        };
    };
    let log = crate::audit::AuditLog::new(logs_dir.to_path_buf());
    let mut outcome = AppUndoOutcome {
        reinstalled: Vec::new(),
        manual: Vec::new(),
        failed: Vec::new(),
        nothing_to_undo: false,
    };
    let mut keep: Vec<RemovedRecord> = Vec::new();
    for entry in &batch.entries {
        match &entry.winget_id {
            Some(id) => {
                #[cfg(windows)]
                let ok = winget_install_one(id).0;
                #[cfg(not(windows))]
                let ok = {
                    let _ = id;
                    false
                };
                if ok {
                    outcome.reinstalled.push(entry.display.clone());
                } else {
                    outcome.failed.push(entry.display.clone());
                    keep.push(entry.clone());
                }
            }
            None => outcome.manual.push(entry.display.clone()),
        }
        let _ = log.append(
            &crate::audit::LogEntry::new(&crate::audit::new_run_id(), "app-undo")
                .path(Path::new(&entry.package)),
        );
    }
    // Only genuinely failed winget reinstalls stay undoable; manual ones
    // are reported and cleared (keeping them would re-report forever).
    if !keep.is_empty() {
        batches.push(RemovalUndoBatch {
            entries: keep,
            ..batch
        });
    }
    let _ = save_removal_undo(&path, &batches);
    outcome
}

pub fn removal_undo_available(data_dir: &Path) -> Option<usize> {
    load_removal_undo(&apps_undo_path(data_dir))
        .last()
        .map(|b| b.entries.len())
}

// ---------------------------------------------------------------------------
// Installs (winget)
// ---------------------------------------------------------------------------

/// Runs `winget install` for each id sequentially; returns
/// (id, success, short message) per id. Unknown ids (not in the
/// catalog) are refused — winget only ever receives catalog ids or
/// profile ids, which are validated against the catalog on save.
/// One raw winget install. Callers validate the id first: the install
/// tab against the install catalog, removal-undo against the removal
/// catalog's recorded reinstall ids.
#[cfg(windows)]
fn winget_install_one(id: &str) -> (bool, String) {
    match std::process::Command::new("winget")
        .args([
            "install",
            "--id",
            id,
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .output()
    {
        Ok(o) if o.status.success() => (true, "installed".to_string()),
        Ok(o) => (false, format!("winget exited with {}", o.status)),
        Err(e) => (false, e.to_string()),
    }
}

#[cfg(windows)]
pub fn winget_install(ids: &[String]) -> Vec<(String, bool, String)> {
    ids.iter()
        .map(|id| {
            if !INSTALL_CATALOG.iter().any(|e| e.id == id) {
                return (id.clone(), false, "not in the install catalog".to_string());
            }
            let (ok, msg) = winget_install_one(id);
            (id.clone(), ok, msg)
        })
        .collect()
}

#[cfg(not(windows))]
pub fn winget_install(ids: &[String]) -> Vec<(String, bool, String)> {
    ids.iter()
        .map(|id| {
            (
                id.clone(),
                false,
                "winget is only available on Windows".to_string(),
            )
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalState {
    pub display: String,
    pub package: String,
    pub risk: Risk,
    pub reinstall_label: String,
    pub installed: bool,
}

pub fn removal_states(installed: &BTreeSet<String>) -> Vec<RemovalState> {
    removal_catalog()
        .iter()
        .map(|a| RemovalState {
            display: a.display.to_string(),
            package: a.package.to_string(),
            risk: a.risk,
            reinstall_label: a.reinstall.label(),
            installed: installed.contains(a.package),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallState {
    pub id: String,
    pub name: String,
    pub category: String,
    pub desc: String,
    pub installed: bool,
}

pub fn install_states(installed: &BTreeSet<String>) -> Vec<InstallState> {
    install_catalog()
        .iter()
        .map(|e| InstallState {
            id: e.id.to_string(),
            name: e.name.to_string(),
            category: e.category.to_string(),
            desc: e.desc.to_string(),
            installed: installed.contains(e.id),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orphaned_removal_result_is_reconciled_into_undo() {
        let dir =
            std::env::temp_dir().join(format!("tidytray-apps-reconcile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let package = removal_catalog()[0].package.to_string();
        let batch = RemovalBatch {
            restore_point: false,
            packages: vec![package.clone()],
            nonce: "tok456".to_string(),
        };
        let batch_path = dir.join("apps-batch-abc.json");
        std::fs::write(&batch_path, serde_json::to_vec(&batch).unwrap()).unwrap();
        let result = RemovalResult {
            restore_point_created: false,
            removed: vec![package],
            errors: Vec::new(),
            nonce: "tok456".to_string(),
        };
        std::fs::write(
            removal_result_path(&batch_path),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        let logs = dir.join("logs");
        assert_eq!(reconcile_orphans(&dir, &logs), 1);
        let undo = load_removal_undo(&apps_undo_path(&dir));
        assert_eq!(undo.len(), 1);
        assert_eq!(undo[0].entries.len(), 1);
        assert!(!batch_path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn catalogs_are_well_formed() {
        let mut seen = BTreeSet::new();
        for a in removal_catalog() {
            assert!(seen.insert(a.package), "duplicate package {}", a.package);
            assert!(!a.display.is_empty());
        }
        // Nothing Windows needs may ever be listed.
        for banned in [
            "Microsoft.WindowsTerminal",
            "Microsoft.WindowsCalculator",
            "Microsoft.WindowsNotepad",
            "Microsoft.ScreenSketch",
        ] {
            assert!(find_app(banned).is_none(), "{banned} must not be removable");
        }
        let mut ids = BTreeSet::new();
        for e in install_catalog() {
            assert!(ids.insert(e.id), "duplicate winget id {}", e.id);
        }
        for p in builtin_profiles() {
            for id in &p.ids {
                assert!(
                    ids.contains(id.as_str()),
                    "profile {} references unknown id {id}",
                    p.name
                );
            }
        }
    }

    #[test]
    fn parsers_read_command_output() {
        let appx = "Microsoft.Copilot\r\nClipchamp.Clipchamp\r\n\r\nMicrosoft.BingNews\r\n";
        let names = parse_appx_names(appx);
        assert_eq!(names.len(), 3);
        assert!(names.contains("Microsoft.Copilot"));
        let listing = "Name        Id\r\nFirefox     Mozilla.Firefox   128.0\r\n7-Zip       7zip.7zip         24.0\r\n";
        let installed = parse_winget_installed(listing);
        assert!(installed.contains("Mozilla.Firefox"));
        assert!(installed.contains("7zip.7zip"));
        assert!(!installed.contains("Valve.Steam"));
    }

    #[test]
    fn removal_script_targets_only_the_named_package() {
        let script = removal_script("Microsoft.Copilot");
        assert!(script.contains("Get-AppxPackage -Name 'Microsoft.Copilot'"));
        assert!(script.contains("Remove-AppxProvisionedPackage"));
        // Quote escaping holds even for hostile input.
        assert!(removal_script("a'b").contains("'a''b'"));
    }

    #[test]
    fn profiles_roundtrip_and_undo_records() {
        let dir = std::env::temp_dir().join(format!("tidytray-apps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        save_profile(&dir, "Mine", &["7zip.7zip".to_string()]).unwrap();
        let profiles = load_profiles(&dir);
        assert!(profiles.iter().any(|p| p.name == "Essentials" && p.builtin));
        assert!(profiles
            .iter()
            .any(|p| p.name == "Mine" && !p.builtin && p.ids == ["7zip.7zip"]));
        delete_profile(&dir, "Mine").unwrap();
        assert!(!load_profiles(&dir).iter().any(|p| p.name == "Mine"));
        assert!(removal_undo_available(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn states_reflect_detection() {
        let installed: BTreeSet<String> = ["Microsoft.Copilot".to_string()].into_iter().collect();
        let states = removal_states(&installed);
        assert!(
            states
                .iter()
                .find(|s| s.package == "Microsoft.Copilot")
                .unwrap()
                .installed
        );
        assert!(
            !states
                .iter()
                .find(|s| s.package == "Microsoft.BingNews")
                .unwrap()
                .installed
        );
        let copilot = states
            .iter()
            .find(|s| s.package == "Microsoft.Copilot")
            .unwrap();
        assert_eq!(copilot.reinstall_label, "winget · Microsoft.Copilot");
    }
}
