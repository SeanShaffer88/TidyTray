# Project Blueprint: Tidy Tray — Safe Autonomous Cache Cleaner + Manual Debloat Suite

> **Owner rules applied (2026-11-10):** where this blueprint conflicts with the
> project owner's standing rules, the standing rules win. Specifically: releases
> ship only as `TidyTray-v{VERSION}-x64-Setup.exe` + `TidyTray-v{VERSION}-x64-Portable.zip`
> + `SHA256SUMS.txt`; there is no MSI, no winget or Chocolatey distribution, no
> code signing (binaries ship unsigned), and builds are x64 only. The affected
> sections (1, 12, 13, 16, 17) have been edited to match.

> **Reference feature source:** [https://github.com/stars/SeanShaffer88/lists/cleanup-debloat](https://github.com/stars/SeanShaffer88/lists/cleanup-debloat)
>
> This blueprint defines the complete workflow, architecture, safety model, feature tiers, module contracts, data schemas, testing strategy, and release plan for forking [BeeCaptivate/pc-cleanup-optimizer](https://github.com/BeeCaptivate/pc-cleanup-optimizer) (Tidy) into a **tray-resident, safety-first Windows maintenance tool** with an **autonomous cache/temp cleanup core** and a **manual, opt-in debloat/tweak suite**.

---

## 1. Project Identity

| Field | Value |
|---|---|
| **Working name** | Tidy Tray (fork of Tidy) |
| **Tagline** | *Conservative automatic cache cleanup. Powerful debloat — on your command.* |
| **Primary platform** | Windows 10 (1809+) and Windows 11 (x64) |
| **Distribution** | GitHub Releases (Setup.exe + Portable.zip + SHA256SUMS.txt) |
| **License** | Inherit from upstream (verify MIT/Apache before forking); keep attribution |
| **Stack** | Rust + Tauri v2 (WebView2), PowerShell 5.1/7 for debloat actions, Windows Task Scheduler + in-process `tokio` scheduler |
| **Core principle** | **Autonomous mode can only touch regenerable caches/temp. Everything that changes system state is manual, previewed, logged, and reversible.** |

---

## 2. Reference Material & Inspiration Map

Primary curated list: **[cleanup-debloat (SeanShaffer88)](https://github.com/stars/SeanShaffer88/lists/cleanup-debloat)**

Feature influence map (read-only inspiration; do **not** copy code without license check):

| Upstream project | Features borrowed into Tidy Tray | Tier |
|---|---|---|
| **Tidy (BeeCaptivate)** | Base scanner, safe-list, tray shell, parallel walker, dry-run | Base |
| **Win11Debloat** | AppX removal catalog (90+ apps), OneDrive/Edge/Copilot/Widgets/Recall toggles | Tier 2 |
| **WinUtil (Chris Titus)** | Tweaks catalog, startup management, AppX list, install via winget | Tier 2 |
| **WinScript** | Config-file driven unattended setup pattern (inspiration for "profiles") | Tier 2 |
| **ET-Optimizer** | Performance tweak groups, service/Scheduled Task disabling (curated subset only) | Tier 2 |
| **RepairKit** | SFC/DISM/WMI/Windows Update repair actions | Tier 2 |
| **Win11Cleaner** | Conservative temp/junk cleanup rules | Tier 1 |
| **BleachBit** | Cache category taxonomy (browser cache vs. cookies separation) | Tier 1 |
| **WindowsMize / Windows10Debloater** | Curated scheduled-task disable lists | Tier 2 |
| **NoPilot** | Copilot disable toggle | Tier 2 |
| **Windows-Optimize-Harden-Debloat** | Reversible policy toggles for telemetry/privacy | Tier 2 |
| **Windows Tweaks (PSReadLine history cleaner)** | PowerShell & terminal history cleanup | Tier 1 |
| **W10Privacy** | App-permission denials, telemetry/diagnostics, speech & typing personalisation, Edge policies | Tier 2 |
| **O&O ShutUp10++** | Privacy categories, "Recommended" preset, settings export/import, Office/Copilot/Recall toggles | Tier 2 |
| **Winaero Tweaker** | Ads/unwanted-app toggles, Windows 11 classic context menu (small safe subset only) | Tier 2 |
| **XToolbox** | Curated install catalog pattern, collections/playbooks, upgrade detection, install history (pattern only — its code is GPL-2.0 and is not reused) | Tier 2 |

> Tier 2 mining note (2026-11-10): W10Privacy and ShutUp10++ overlap heavily and
> feed one combined Privacy & Telemetry catalog. Rejected from all four sources:
> Defender/SmartScreen disabling, permanent Windows Update disabling, and bulk
> "disable everything" actions (Tier 3). Winaero's appearance/context-menu bulk
> and W10Privacy's HOSTS-file telemetry blocking are deferred. Every mined tweak
> is verified against Microsoft's own documentation before it is implemented.

---

## 3. Non-Negotiable Safety Principles

These are **invariants** enforced by the architecture. Any PR violating them is rejected.

1. **Autonomous engine has no capability to delete arbitrary paths.** It can only act on `category_id`s declared in a versioned, checksummed manifest embedded at build time.
2. **Autonomous engine never touches user data:** documents, downloads, desktop, pictures, videos, music, browser cookies/history/sessions/local storage/IndexedDB/passwords, save games, installers, user-created files anywhere.
3. **Autonomous engine never modifies registry, services, scheduled tasks, drivers, or system policies.**
4. **Every autonomous deletion is logged** with timestamp, category, path, size, and result. Logs are append-only and queryable from tray UI.
5. **All autonomous deletions go to a quarantine folder first** (default: `%LOCALAPPDATA%\TidyTray\Quarantine\<run-id>`), retained **7 days**, then purged. User can restore any file within that window.
6. **Locked/in-use files are skipped, never forced.** No `FILE_FLAG_DELETE_ON_CLOSE`, no shadow-copy tricks, no reboot-delete.
7. **Conservative age thresholds** (default ≥7 days for temp, ≥14 days for browser caches, ≥30 days for crash dumps/WER).
8. **Dry-run is the default** for first 3 runs on any machine. Autonomous deletion only enabled after user opts in.
9. **Kill switch:** tray menu "Pause autonomous cleanup" immediately halts any in-flight run (cooperative cancellation, ≤5s).
10. **Battery & fullscreen awareness:** autonomous runs pause on battery below 30%, and while a fullscreen app or known game/meeting process is foreground.
11. **Tier 2 debloat actions require:** explicit user click → preview/dry-run diff → confirmation dialog → **System Restore point** (or a generated undo script if SR is disabled) → execution → log.
12. **Restore path always exists.** Every Tier 2 action produces a reversible artifact (registry export, AppX reinstall via winget, service re-enable script, etc.).
13. **No telemetry.** No analytics, no phone-home, no crash upload without explicit user consent.
14. **No absolute claims.** UI copy uses "conservative," "regenerable cache," "reversible" — never "100% safe" or "guaranteed."

---

## 4. Feature Tiers

### 🟢 Tier 1 — Autonomous-Safe (runs on schedule, no user interaction)

| Category ID | Display Name | Paths (templated) | Min Age | Admin | Lock Skip | Max per Run |
|---|---|---|---|---|---|---|
| `temp_user` | User Temp Files | `%TEMP%`, `%LOCALAPPDATA%\Temp` | 7d | no | yes | 5 GB |
| `temp_windows` | Windows Temp Files | `%SystemRoot%\Temp` | 30d | yes | yes | 2 GB |
| `browser_cache_chromium` | Chromium Browser Cache | `…\User Data\*\Cache\Cache_Data`, `…\Code Cache`, `…\GPUCache` | 14d | no | yes | 3 GB |
| `browser_cache_firefox` | Firefox Cache | `…\Profiles\*\cache2` | 14d | no | yes | 2 GB |
| `gpu_shader_cache` | GPU Shader Cache | `%LOCALAPPDATA%\D3DSCache`, `…\NVIDIA\DXCache`, `…\AMD\DxCache` | 7d | no | yes | 1 GB |
| `thumbnail_cache` | Thumbnail & Icon Cache | `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*.db` | n/a | no | yes | best-effort |
| `crash_dumps` | Crash Dumps & WER | `%LOCALAPPDATA%\CrashDumps`, `%ProgramData%\Microsoft\Windows\WER\Temp` | 30d | no | yes | 1 GB |
| `delivery_optimization` | Delivery Optimization Cache | `%ProgramData%\Microsoft\Windows\DeliveryOptimization\Cache` | 30d | yes | yes | 2 GB |
| `ps_history` | PowerShell & Terminal History | `%APPDATA%\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt`, VS Code terminal history | n/a | no | no | full file |

**Rules:** All paths templated against known environment variables. Manifest is embedded at build time as a version-locked, checksummed JSON resource; **not user-editable in autonomous mode**.

### 🟡 Tier 2 — Manual Opt-In (tray-triggered, previewed, reversible)

Grouped into tab-like sections in tray UI:

- **App Removal** (AppX/bloatware)
  - Catalog of ~90 removable apps (from Win11Debloat inspiration), each with risk badge: *Safe*, *Caution*, *Required*.
  - Default view shows *Safe* only. *Required* hidden behind "Show all."
  - Per-app preview: package name, publisher, size, "used in last 30 days?" heuristic (via AppX activity API where available).
- **Feature Debloat Toggles**
  - OneDrive (uninstall / disable sync)
  - Edge (hide / prevent auto-launch — **never** uninstall)
  - Copilot (policy toggle)
  - Widgets, Recall, Search highlights, Taskbar chat
  - Advertising ID, Activity history, Tailored experiences
- **Performance Tweaks**
  - Power plan (Ultimate / Balanced / High Performance)
  - Startup app management (toggle individual entries)
  - Curated Scheduled Task disables (telemetry, CEIP, Feedback, etc.) — **not** bulk "disable all tasks"
  - Background app permissions
- **Privacy & Telemetry**
  - Reversible registry policy toggles, each mapped to an "undo" entry.
- **System Repair**
  - SFC / DISM / WMI / Windows Update reset / Component Store cleanup — **manual buttons only**, each shows estimated duration and requires confirmation.
- **Setup / Install**
  - Winget-based bulk install from a curated "essentials" list (browser, archiver, media player, etc.)
  - User can save selections as a **Profile** JSON (WinScript-inspired) for reuse.

**Every Tier 2 action generates:**
1. A pre-action **preview diff** (human-readable: "Will remove X, change Y, disable Z").
2. A **System Restore point** (best-effort; if SR disabled, fall back to generated undo script).
3. An **undo artifact** stored in `%LOCALAPPDATA%\TidyTray\Undo\<timestamp>\`.
4. A structured log entry in `%LOCALAPPDATA%\TidyTray\Logs\debloat-<date>.jsonl`.

### 🔴 Tier 3 — Excluded

Not implemented. Documented in `docs/EXCLUDED.md` for transparency:

- RAM cleaners / memory optimization
- Generic registry cleaners
- Windows Defender / Security Center removal
- Permanent Windows Update disabling (pause/defer toggles are allowed)
- Bulk scheduled-task disable ("disable everything")
- Driver removal, service disabling beyond curated safe list
- Any action that touches other user profiles automatically

---

## 5. High-Level Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Tidy Tray (single .exe)                       │
│                                                                      │
│  ┌─────────────────────┐   ┌─────────────────────┐   ┌───────────┐  │
│  │  Tray UI (Tauri)     │   │  Debloat Engine     │   │  Config   │  │
│  │  - Menu              │◀─▶│  - Preview/Diff     │◀─▶│  Store    │  │
│  │  - Status window     │   │  - PS subprocess    │   │  (JSON)   │  │
│  │  - Logs viewer       │   │  - Restore point    │   └───────────┘  │
│  └─────────┬───────────┘   └──────────┬──────────┘                  │
│            │                          │                             │
│            ▼                          ▼                             │
│  ┌────────────────────────────────────────────────────────────┐     │
│  │                    Orchestrator (Rust)                     │     │
│  │  - IPC command handler                                     │     │
│  │  - Scheduler (tokio + Windows Task Scheduler backup)       │     │
│  │  - Run-state machine                                       │     │
│  └──────────┬─────────────────────────────┬───────────────────┘     │
│             │                             │                         │
│             ▼                             ▼                         │
│  ┌─────────────────────┐         ┌─────────────────────┐            │
│  │  Autonomous Cleaner │         │  Audit & Undo       │            │
│  │  - Manifest loader  │         │  - JSONL logger     │            │
│  │  - Scanner          │         │  - Quarantine mgr   │            │
│  │  - Lock checker     │         │  - Undo executor    │            │
│  │  - Quarantine move  │         └─────────────────────┘            │
│  │  - Delete engine    │                                            │
│  └──────────┬──────────┘                                            │
│             │                                                       │
│             ▼                                                       │
│  ┌───────────────────────────────────────────────────────────┐      │
│  │  OS Layer (Windows APIs, via `windows` crate)             │      │
│  │  - File ops, lock detection (Restart Manager / handle)    │      │
│  │  - Task Scheduler, Restore Points, UAC elevation          │      │
│  └───────────────────────────────────────────────────────────┘      │
└──────────────────────────────────────────────────────────────────────┘
```

### Data Flow — Autonomous Run

```
Trigger (schedule | low-disk | manual "Clean now")
      │
      ▼
Pre-flight checks (battery, fullscreen, dry-run remaining, kill switch)
      │
      ▼
Load & verify manifest (build-embedded, checksum-validated)
      │
      ▼
For each enabled Tier 1 category:
   ├─ Expand path templates (env vars only)
   ├─ Walk directories (respect age threshold, skip symlinks/junctions)
   ├─ Lock check per file (skip if locked/in-use)
   ├─ Enforce per-run & per-category size caps
   ├─ Move to Quarantine/<run-id>/... (atomic rename; fall back to copy+delete)
   └─ Record result in JSONL log
      │
      ▼
Post-run: emit event to tray (silent notification), update status, prune old quarantine (>7d)
```

### Data Flow — Tier 2 Debloat Action

```
User clicks action in tray UI
      │
      ▼
Generate Preview (what will change) ──► show diff dialog
      │
      ▼
User confirms
      │
      ▼
Create System Restore Point (or write undo.ps1)
      │
      ▼
Elevate if required (UAC prompt)
      │
      ▼
Execute PowerShell module for action ──► stream output to log
      │
      ▼
Persist undo artifact + log entry
      │
      ▼
Refresh UI, show result summary with "Undo" button
```

---

## 6. Repository Structure

```
tidy-tray/
├── .github/
│   ├── workflows/
│   │   ├── ci.yml                 # build + test on windows-latest
│   │   ├── release.yml            # tag → build → sign → publish
│   │   └── safety-audit.yml       # static check: no arbitrary path deletion
│   └── ISSUE_TEMPLATE/
├── Cargo.toml                     # workspace
├── Cargo.lock
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── manifest/
│   │   └── cleaner-manifest.json  # build-embedded, checksummed
│   └── src/
│       ├── main.rs                # tray bootstrap
│       ├── lib.rs
│       ├── orchestrator.rs        # command dispatch, state machine
│       ├── scheduler.rs           # tokio timer + Task Scheduler registration
│       ├── cleaner/
│       │   ├── mod.rs
│       │   ├── manifest.rs        # load & validate manifest
│       │   ├── scanner.rs         # templated paths, age filter
│       │   ├── lock.rs            # file-in-use detection
│       │   ├── quarantine.rs      # move + restore
│       │   └── delete.rs          # ONLY reachable via quarantine
│       ├── debloat/
│       │   ├── mod.rs
│       │   ├── preview.rs
│       │   ├── executor.rs        # spawns PS module, captures stdout
│       │   ├── restore_point.rs
│       │   └── actions/           # one module per action (appx, onedrive, copilot, ...)
│       ├── audit/
│       │   ├── mod.rs
│       │   ├── jsonl_logger.rs
│       │   └── undo.rs
│       ├── config.rs              # serde-backed settings
│       ├── os/
│       │   ├── windows_api.rs
│       │   ├── elevate.rs
│       │   └── task_scheduler.rs
│       └── tray.rs
├── scripts/
│   ├── appx-catalog.ps1
│   ├── tweaks-catalog.ps1
│   ├── repair/                    # SFC, DISM, WU reset modules
│   └── install-essentials.ps1
├── ui/                            # minimal Tauri UI (status + settings + logs + debloat)
│   ├── index.html
│   ├── settings.html
│   ├── debloat.html
│   └── logs.html
├── tests/
│   ├── unit/
│   ├── integration/
│   └── safety/                    # property tests, manifest boundary tests
├── docs/
│   ├── ARCHITECTURE.md
│   ├── SAFETY.md
│   ├── EXCLUDED.md
│   ├── MANIFEST_SCHEMA.md
│   ├── TROUBLESHOOTING.md
│   └── BLUEPRINT.md               # this file
└── README.md
```

---

## 7. Core Module Contracts

### 7.1 Manifest (`cleaner-manifest.json`)

```jsonc
{
  "schema_version": 1,
  "manifest_version": "2026.01.1",
  "checksum": "<sha256 of categories block>",
  "categories": [
    {
      "id": "browser_cache_chromium",
      "display_name": "Chromium Browser Cache",
      "paths": [
        "${LOCALAPPDATA}\\Google\\Chrome\\User Data\\*\\Cache\\Cache_Data",
        "${LOCALAPPDATA}\\Google\\Chrome\\User Data\\*\\Code Cache",
        "${LOCALAPPDATA}\\Google\\Chrome\\User Data\\*\\GPUCache",
        "${LOCALAPPDATA}\\Microsoft\\Edge\\User Data\\*\\Cache\\Cache_Data"
      ],
      "min_age_days": 14,
      "requires_admin": false,
      "skip_locked": true,
      "max_gb_per_run": 3,
      "enabled_by_default": true,
      "tier": 1
    }
  ]
}
```

**Contract rules:**
- Paths must be **explicit templates** — no `**`, no `C:\Users\*\*`, no drive roots.
- Glob only allowed for a **single segment** (`User Data\*\Cache_Data` matches one profile dir).
- Manifest is embedded via `include_str!` at build time. Runtime never loads a manifest from disk in autonomous mode.
- A CI job validates every path template against a rule engine (see §11.3).

### 7.2 Log Entry (`logs/clean-YYYY-MM-DD.jsonl`)

```json
{
  "ts": "2026-01-15T10:32:11.123Z",
  "run_id": "a3f9...",
  "event": "delete",
  "category": "temp_user",
  "path": "C:\\Users\\me\\AppData\\Local\\Temp\\foo.tmp",
  "size": 1024,
  "result": "quarantined",
  "quarantine_path": "...\\Quarantine\\a3f9\\temp_user\\...",
  "duration_ms": 3
}
```

Events: `run_start`, `run_end`, `delete`, `skip_locked`, `skip_age`, `skip_size_cap`, `error`, `quarantine_restore`, `quarantine_purge`, `debloat_preview`, `debloat_apply`, `debloat_undo`.

### 7.3 Config (`config.json` in `%LOCALAPPDATA%\TidyTray`)

```jsonc
{
  "schema_version": 1,
  "autonomous": {
    "enabled": false,
    "dry_runs_completed": 0,
    "schedule": { "mode": "weekly", "day": "Sunday", "hour": 3 },
    "low_disk_trigger_gb": 10,
    "pause_on_battery_below_pct": 30,
    "pause_on_fullscreen": true,
    "max_total_gb_per_run": 20,
    "categories_enabled": ["temp_user", "browser_cache_chromium", "gpu_shader_cache"]
  },
  "quarantine": { "retention_days": 7 },
  "ui": { "show_notifications": true, "theme": "system" }
}
```

### 7.4 Orchestrator Command Surface (Tauri `invoke` handlers)

| Command | Purpose | Elevation |
|---|---|---|
| `get_status` | free space, last run, next run, quarantine size | no |
| `clean_now(dry_run: bool)` | manual Tier 1 run | per-category |
| `pause_autonomous` / `resume_autonomous` | kill switch | no |
| `get_logs(filter)` | paginated log viewer | no |
| `restore_from_quarantine(path)` | undo single file | no |
| `purge_quarantine` | manual purge | no |
| `debloat_preview(action_id)` | Tier 2 dry-run | no |
| `debloat_apply(action_id)` | Tier 2 execute | yes |
| `debloat_undo(undo_id)` | run undo artifact | yes |
| `repair_run(action_id)` | SFC/DISM/etc. | yes |
| `install_selected(winget_ids)` | bulk install | yes |
| `get_config` / `set_config` | settings | no |

---

## 8. Workflows (End-to-End)

### 8.1 First-Run Flow

1. App launches → registers tray icon, no window.
2. Detects first run (no `config.json`).
3. Shows welcome window: one sentence about what it does + what it will **never** do; "Start dry-run phase" button.
4. `dry_runs_completed = 0`, autonomous disabled.
5. User can trigger "Scan now (dry run)" → shows what would be deleted, sizes, categories.
6. After 3 dry runs, tray menu shows "Enable autonomous cleanup" — requires confirming a checkbox acknowledging conservative behavior.
7. Registers Windows Task Scheduler entry (with user consent) as fallback.

### 8.2 Autonomous Cleanup Run

1. Trigger fires (schedule / low-disk / manual).
2. Pre-flight: kill switch? battery? fullscreen? dry-run phase complete? → abort or proceed.
3. Emit `run_start` log.
4. Load embedded manifest, verify checksum.
5. Iterate enabled categories in order (small → large).
6. Per file: lock check → age check → size cap check → move to quarantine.
7. Per-category summary logged.
8. Post-run: prune quarantine > retention, emit `run_end`, tray notification (suppressed in "quiet hours" 22:00–07:00 unless error).
9. Update next-run time.

### 8.3 Quarantine Restore Flow

1. User opens Logs → selects a run → sees file list.
2. "Restore" on any file → moves back to original path (create dirs if needed).
3. If original path exists, prompt overwrite/rename.
4. Log `quarantine_restore`.

### 8.4 Tier 2 Debloat Flow

1. User opens Debloat window.
2. Selects actions (checkboxes); risk badges shown.
3. "Preview" → generates diff → dialog: "Will remove N apps, change M settings. A restore point will be created. Continue?"
4. On confirm: 
   - Create Restore Point (or write undo.ps1 if SR fails).
   - Elevate if any selected action requires admin.
   - Execute each action via its PS module, streaming output to log.
   - Write undo artifacts.
5. Summary window with per-action result + "Undo all" button.

### 8.5 System Repair Flow

1. User opens Repair tab.
2. Selects one repair (SFC, DISM, WU reset, WMI).
3. Confirmation shows estimated duration and impact (e.g., "WU reset requires reboot").
4. Elevate → run in streaming subprocess → display live output.
5. On completion, offer log export.

---

## 9. Scheduling & Triggers

| Mechanism | Purpose | Details |
|---|---|---|
| **In-process tokio timer** | Primary trigger while app is running (tray resident) | Checked every 60s; fires when due |
| **Windows Task Scheduler** | Fallback if app was not running at scheduled time | Registered on opt-in; points to `tidy-tray.exe --scheduled-run`; uses `--scheduled-run` flag to run headless and exit |
| **Low-disk event** | Reactive cleanup | Polled every 5 min via `GetDiskFreeSpaceExW`; hysteresis (must stay below threshold for 30 min) |
| **Manual tray action** | User-initiated | "Clean now" (default dry-run if not yet enabled) |

**Quiet hours:** default 22:00–07:00. Scheduled runs move to next available window.

**Cooperative cancellation:** every file operation checks an `AtomicBool` cancel flag; kill switch sets it; run aborts within 5 seconds.

---

## 10. Privilege Model

| Operation | Elevation |
|---|---|
| Tier 1 categories with `requires_admin: false` | Runs as current user, no prompt |
| `temp_windows`, `delivery_optimization` | Elevated helper launched **only** for that category, via UAC prompt on first run of that category; user can disable these categories to avoid prompts |
| Tier 2 debloat actions | Elevated per-action via UAC |
| Repair actions | Elevated per-action |
| Winget install | Elevated per-action |

**Design:** no persistent elevated service in v1. Elevation is **per-operation**, driven from the non-elevated tray process via `ShellExecuteEx` with `runas`. Elevated helper is a separate subcommand (`tidy-tray.exe --elevated-task <json>`), receives a signed, short-lived task descriptor, executes it, exits. This avoids a long-running privileged daemon.

> v2 consideration: optional Windows service for fully silent elevated operations. Deferred because it increases attack surface and complexity.

---

## 11. Testing Strategy

### 11.1 Unit Tests (Rust)

- Manifest loader: rejects malformed paths, rejects `**`, rejects drive roots, rejects user-document folders.
- Path template expander: env var substitution, wildcard single-segment matching.
- Age filter, size cap, lock-check mock.
- Quarantine move: atomic rename success, fallback copy+delete, restore round-trip.
- Log writer: valid JSONL, rotation.

### 11.2 Integration Tests (Windows runners)

- Spin up Windows VM (GitHub Actions `windows-latest`), seed synthetic files in each Tier 1 category path, run cleaner, assert outcomes.
- Locked-file simulation: open a file handle, verify skip.
- Kill-switch test: start long run, set cancel, assert <5s halt.
- Quarantine: delete → restore → verify byte-identical.

### 11.3 Safety Property Tests (CI gate)

A dedicated test suite enforces safety invariants:

1. **Manifest boundary test:** iterate every path template in the embedded manifest; assert each expands only to locations under known safe roots (`%TEMP%`, `%LOCALAPPDATA%` subdirs from allowlist, `%SystemRoot%\Temp`, etc.). Fail build if any template can produce a path outside those roots.
2. **No-arbitrary-path test:** grep + AST check ensures `delete::` and `quarantine::move` are only reachable via `cleaner::run_category` — no other call sites.
3. **Registry guard:** static analysis confirms no `winreg` / `RegSetValue` calls in the autonomous cleaner module tree.
4. **Age guard:** assert `min_age_days >= 7` for all `temp_*` categories in manifest (CI reads manifest, not just code).
5. **User-data guard:** blocklist of path fragments (`\Documents`, `\Downloads`, `\Desktop`, `\Pictures`, `\Videos`, `\Music`, `Cookies`, `Login Data`, `Local Storage`, `IndexedDB`, `Session Storage`) — assert none appear in any Tier 1 template.

### 11.4 Manual Test Matrix (release gate)

- Windows 10 22H2, Windows 11 23H2, Windows 11 24H2
- Admin user vs. standard user
- OneDrive-redirected Documents present
- Multiple browser profiles
- Files locked by antivirus / backup tools
- Disk full during quarantine
- Battery-only device
- Fullscreen game running
- Restore-point disabled (SR fallback path)
- Task Scheduler disabled by policy

### 11.5 Chaos / Fuzz

- `proptest` for path expander with random inputs.
- Simulated power loss mid-quarantine (kill process between move steps) → assert no partial deletion (quarantine is atomic rename where possible; incomplete copies are rolled back).

---

## 12. CI/CD Pipeline

### 12.1 `.github/workflows/ci.yml`
- Matrix: `windows-latest`, Rust stable + beta.
- Steps: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo audit`, safety property tests, Tauri build (`cargo tauri build --debug`).
- Artifact: debug binary for smoke test.

### 12.2 `.github/workflows/safety-audit.yml`
- Runs the §11.3 safety tests in isolation.
- Fails the build if any invariant is violated.
- Also runs `cargo deny` on dependencies.

### 12.3 `.github/workflows/release.yml`
- Trigger: tag `v*`.
- Build the release (binaries ship unsigned; no code-signing certificate is used).
- Produce: `TidyTray-v{VERSION}-x64-Setup.exe`, `TidyTray-v{VERSION}-x64-Portable.zip`, `SHA256SUMS.txt`.
- Publish GitHub Release with changelog.
- No winget, MSI or Chocolatey submission (owner rule).

---

## 13. Release & Distribution

| Channel | Method |
|---|---|
| GitHub Releases | Setup.exe, Portable.zip, SHA256SUMS.txt |
| Direct | The same GitHub Release assets; optional opt-in update check (never forced) |

**Auto-update:** updates are offered, never forced. Any in-app update check is opt-in and verifies downloads against the release SHA256SUMS.txt. Users can pin versions.

**Update safety:** new manifest ships with each app version; safety tests run against it in CI. No remote manifest updates in v1 (reduces risk surface; users update via app updater to get new categories).

---

## 14. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Misclassified path deletes user data | Low (with manifest rules) | High | Manifest templates + property tests + 7-day quarantine |
| Browser profile DB touched (cookies/login) | Low | High | Explicit allowlist of cache subdirs only; blocklist fragments in CI |
| Locked-file deletion causes corruption | Low | Medium | Lock check + skip; never force |
| UAC fatigue causes users to disable admin categories | Medium | Low | Admin categories off by default; clear explanation |
| Debloat breaks a required app | Medium | Medium | Risk badges, restore point, undo artifacts, "Caution" apps off by default |
| Restore point fails (SR disabled) | Medium | Medium | Undo script fallback; warn user before Tier 2 action |
| User enables autonomous then blames tool for unrelated issue | Medium | Medium | Clear logs, "last run: X files, Y MB" tray tooltip, support docs |
| Upstream Tidy license incompatible with use | Low | High | Verify license before fork; if incompatible, use as inspiration only, clean-room rewrite |
| Antivirus flags quarantine behavior as ransomware-like | Medium | High | **Submit binary to Microsoft for AV allowlisting early**; keep quarantine human-visible; no obfuscation; open source |
| Task Scheduler disabled by policy | Low | Low | In-process timer is primary; Task Scheduler is fallback only |
| Signing certificate compromise | Very low | High | HSM-backed cert, revocation plan documented |

---

## 15. Milestones / Roadmap

### M0 — Fork & Audit (1 week)
- Verify upstream license; fork; strip non-essential UI.
- Add `docs/SAFETY.md`, `docs/EXCLUDED.md`, `docs/BLUEPRINT.md` (this file).
- Set up CI skeleton.

### M1 — Safety Core (2–3 weeks)
- Manifest schema + embedded loading + checksum validation.
- Scanner with env-var path expansion, age filter, lock check.
- Quarantine (move, restore, prune).
- JSONL logging.
- Safety property tests green.
- Unit + integration tests on Windows runner.

### M2 — Autonomous Loop (1–2 weeks)
- Scheduler (tokio + Task Scheduler registration).
- Pre-flight checks (battery, fullscreen, kill switch).
- Dry-run phase + opt-in flow.
- Tray menu: Clean now, Pause, Settings, Logs.
- Minimal settings UI.
- First runnable alpha.

### M3 — Tier 1 Categories Complete (1–2 weeks)
- All 9 Tier 1 categories implemented, tested on VM matrix.
- Per-run and per-category size caps.
- Low-disk trigger with hysteresis.
- Notifications (quiet hours).

### M4 — Tier 2 Debloat Foundation (2–3 weeks)
- Preview/diff engine.
- Restore point + undo artifact generation.
- Elevated subprocess helper.
- One action group implemented end-to-end (start with Copilot toggle + a few AppX removals) as template.

### M5 — Tier 2 Full Catalog (3–4 weeks)
- AppX catalog (~90 apps, risk-badged).
- Toggles: OneDrive, Edge, Widgets, Recall, telemetry/privacy, ad ID.
- Startup app management.
- Curated Scheduled Task disables.
- Performance tweaks (power plan, background apps).
- Repair actions (SFC, DISM, WU reset, WMI).
- Winget install tab.
- Profiles (save/load action sets).

### M6 — Polish & Release (2 weeks)
- AV allowlisting submission (start at M3!).
- Code signing setup.
- Auto-updater with staged rollout.
- Winget/Chocolatey packaging.
- Docs: user guide, troubleshooting, FAQ, safety explainer.
- Public beta → v1.0.

**Total estimate:** ~14–18 weeks for a small team of 1–2, or ~8–10 weeks for a solo dev working full-time.

---

## 16. Open Questions (Resolve Before M1)

1. **Upstream license** — does Tidy's license permit redistribution of derivative work? Verify before any code reuse.
2. **Code signing** — RESOLVED (2026-11-10): no code signing. Binaries ship unsigned per the owner's standing rule; AV allowlisting submissions use the unsigned release binary.
3. **Manifest update strategy** — v1 ships manifest only with app updates (safe but slow). Consider a v1.1 signed remote manifest with a conservative rollout, gated by safety tests.
4. **Elevated helper protocol** — JSON over stdin/stdout vs. named pipe; pick one and freeze for M4.
5. **Winget dependency** — require winget at install time, or bundle a fallback (Chocolatey / direct downloads)? Recommend: detect winget; disable install tab if absent.
6. **Localization** — English-only v1; strings externalized from day one.
7. **Crash reporting** — v1: none, or opt-in local dump viewer? Recommend local-only for v1.
8. **Windows version support** — drop Windows 10 <1809 or support? Recommend 1809+ only.
9. **UI framework within Tauri** — plain HTML/JS or a small framework? Recommend plain + minimal CSS to keep binary small and attack surface low.
10. **Quarantine encryption** — should quarantined files be encrypted at rest? Recommend no (transparency > security here; users should be able to inspect).

---

## 17. Definition of Done (v1.0)

- [ ] All 9 Tier 1 categories implemented and tested on full manual test matrix.
- [ ] Safety property tests green; CI blocks merges that violate invariants.
- [ ] Dry-run phase + opt-in flow complete.
- [ ] Quarantine restore verified byte-identical across 1000-file sample.
- [ ] Kill switch halts run in <5s in stress test.
- [ ] All Tier 2 actions have preview + restore point / undo artifact.
- [ ] Undo round-trip verified for every Tier 2 action on clean Windows VM.
- [ ] Release binary submitted to Microsoft for AV allowlisting.
- [ ] Documentation complete: SAFETY, EXCLUDED, ARCHITECTURE, MANIFEST_SCHEMA, TROUBLESHOOTING, user guide.
- [ ] Auto-updater operational with staged rollout.
- [ ] Release published in the standard shape (Setup.exe + Portable.zip + SHA256SUMS.txt).
- [ ] No telemetry calls verified by network capture (CI artifact).
- [ ] Public v1.0 release with changelog.

---

## 18. Appendix A — Tier 1 Path Allowlist (Root Anchors)

The safety test in §11.3.1 requires every template path to resolve under one of these root anchors:

- `%TEMP%`
- `%LOCALAPPDATA%\Temp`
- `%LOCALAPPDATA%\D3DSCache`
- `%LOCALAPPDATA%\NVIDIA\DXCache`
- `%LOCALAPPDATA%\AMD\DxCache`
- `%LOCALAPPDATA%\Microsoft\Windows\Explorer` (thumbcache only)
- `%LOCALAPPDATA%\CrashDumps`
- `%LOCALAPPDATA%\{Google\Chrome,Microsoft\Edge}\User Data\*\Cache\Cache_Data`
- `%LOCALAPPDATA%\{Google\Chrome,Microsoft\Edge}\User Data\*\Code Cache`
- `%LOCALAPPDATA%\{Google\Chrome,Microsoft\Edge}\User Data\*\GPUCache`
- `%APPDATA%\Mozilla\Firefox\Profiles\*\cache2`
- `%APPDATA%\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt`
- `%SystemRoot%\Temp` (admin)
- `%ProgramData%\Microsoft\Windows\WER\Temp`
- `%ProgramData%\Microsoft\Windows\DeliveryOptimization\Cache` (admin)

Any new category requires a code review that adds the anchor here **and** the corresponding test.

## 19. Appendix B — Tier 2 Action Schema (Undo Artifact)

Each Tier 2 action produces a JSON artifact:

```jsonc
{
  "undo_id": "2026-01-15T10-32-11_a1b2c3",
  "action_id": "appx.remove.xbox",
  "applied_at": "2026-01-15T10:32:11Z",
  "requires_admin_for_undo": true,
  "undo": {
    "type": "appx_reinstall",
    "commands": [
      "Add-AppxPackage -Register 'C:\\Program Files\\WindowsApps\\Microsoft.XboxApp_...\\AppxManifest.xml' -DisableDevelopmentMode"
    ],
    "fallback": "winget install 9MV0B5HZVK9Z"
  },
  "restore_point_id": "{guid}" 
}
```

`debloat_undo(undo_id)` reads this artifact and executes the commands in an elevated subprocess, logging all output.

---

*End of blueprint. Reference list: [https://github.com/stars/SeanShaffer88/lists/cleanup-debloat](https://github.com/stars/SeanShaffer88/lists/cleanup-debloat).*