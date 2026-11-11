<div align="center">

<img src="assets/banner.png" width="100%" alt="TidyTray - free open source PC cleaner and health monitor for Windows 11 and Windows 10">

<h1>TidyTray — Free PC Cleaner &amp; Health Monitor for Windows 11 / 10</h1>

**TidyTray is a free, open source PC cleaner for Windows: free up disk space in 3 seconds, see what is eating your CPU and RAM in plain English, and switch off slow startup programs — a calm, safe CCleaner alternative written in Rust.**

<p>
  <a href="../../releases/latest"><img src="https://img.shields.io/badge/Windows-10%20%7C%2011-34C759?style=flat-square&labelColor=1D1D1F" alt="Windows 10 and 11"></a>
  <a href="../../releases/latest"><img src="https://img.shields.io/badge/release-v1.0.0-34C759?style=flat-square&labelColor=1D1D1F" alt="Latest release v1.0.0"></a>
  <a href="../../releases/latest"><img src="https://img.shields.io/badge/downloads-free-34C759?style=flat-square&labelColor=1D1D1F" alt="Free download"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-6E6E73?style=flat-square&labelColor=1D1D1F" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/Rust-stable-6E6E73?style=flat-square&labelColor=1D1D1F&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Tauri-2-6E6E73?style=flat-square&labelColor=1D1D1F&logo=tauri&logoColor=white" alt="Tauri 2">
</p>

<a class="download-btn-action" href="https://github.com/SeanShaffer88/TidyTray/releases/latest"><img src="assets/download.png" width="300" alt="Download TidyTray for Windows"></a>

<sub>Windows 10 / 11 · x64 · v1.0.0 · free</sub>

<br><br>

<img src="assets/demo-scan.gif" width="880" alt="Tidy Smart Scan finds 12.4 GB of temp files, browser caches and shader caches in under 3 seconds">

</div>

## What TidyTray adds

TidyTray is a fork of [Tidy](https://github.com/BeeCaptivate/pc-cleanup-optimizer) that keeps the original design and features, and adds a safety layer built on one rule: **everything is reversible**.

- **Safety screen** — optional automatic cleanup that only ever touches regenerable caches and temp files from a built-in, checksum-verified list. Files are *moved to a quarantine folder* (kept 7 days, restorable), never deleted outright; the first three runs are dry runs, and a tray kill switch stops a run in seconds.
- **Smart Clean is reversible too** — the classic one-click cleanup now moves files to the same quarantine instead of deleting them.
- **Tweaks** — manual system tweaks (privacy, debloat, Explorer) that are staged, previewed with exact before → after values, applied behind a System Restore point, and undoable to the exact previous values.
- **Apps** — remove bundled Windows apps from a risk-badged catalog (with each app's reinstall source shown before removal, and an Undo), or install popular apps with winget and reusable profiles.
- **GitHub-style dark theme by default**, with the original light theme kept as an option.

Tier 3 territory stays out on purpose — no registry cleaners, no RAM "optimizers", no Defender or Windows Update removal. See `docs/SAFETY.md` for the full model and `docs/EXCLUDED.md` for what will never be added.

> The screenshots below show the original light theme; the app now starts in the dark theme. Fresh captures are on the way.

## Contents

- [Why Tidy](#why-tidy)
- [Features](#features)
- [Screenshots](#screenshots)
- [Tidy vs. the usual ways](#tidy-vs-the-usual-ways)
- [Download &amp; install](#download--install)
- [Build from source](#build-from-source)
- [System requirements](#system-requirements)
- [FAQ](#faq)
- [Roadmap](#roadmap)
- [License](#license)

## Why Tidy

Your C: drive is full again, the fans spin up for no reason, and Windows takes forever to sign in. The built-in tools are spread across Settings, Task Manager and Disk Cleanup, and most "PC optimizer" apps are noisy, upsell-heavy and delete more than they should.

**Tidy puts PC cleanup and PC health in one quiet window.** One Smart Scan measures temp files, browser caches, the Windows Update cache, crash dumps, NVIDIA / DirectX shader caches and the Recycle Bin in about three seconds. A process view explains in plain English *why* `MsMpEng.exe`, `svchost.exe` or `Memory Compression` is using your CPU or RAM. A startup list shows which apps slow down your boot. Everything runs locally, nothing is uploaded, and a built-in safe-list makes sure your documents, photos and system files are never touched.

## Features

### 1. Three-second Smart Scan

<img src="assets/demo-scan.gif" width="100%" alt="Smart Scan sweeping the storage bar and counting cleanable gigabytes">

A parallel directory walker (plain Rust threads, one per core) sweeps every known junk location at once and adds up the space per category: **temporary files** (only older than 24 h, so running installers are safe), **Chrome / Edge / Brave / Firefox caches**, **Windows Update downloads**, **crash dumps and error reports**, **NVIDIA DXCache, D3DSCache and AMD shader caches**, **thumbnail cache** and the **Recycle Bin**. Junctions and symlinks are never followed.

### 2. One-click cleanup with dry run

<img src="assets/demo-clean.gif" width="100%" alt="Clicking Clean 12.4 GB, categories count down and freed space is shown">

Pick categories in a clear checklist, see how many files and gigabytes each one holds, and press **Clean**. Dry run is on by default: Tidy walks the exact same files and reports what it *would* delete before you ever let it remove anything. Items that cannot be undone (Recycle Bin, update cache) are marked **Review** and unchecked by default.

### 3. Who's eating my PC?

<img src="assets/demo-procs.gif" width="100%" alt="Live process list re-sorting by CPU usage with sparklines and plain-English explanations">

Live CPU and memory per app, grouped like a human would (14 Chrome processes = one Chrome row), with 40-second sparklines and a **plain-English "what is this?"** for the processes people search for most: Antimalware Service Executable, Desktop Window Manager, Service Host, Search Indexer, WMI Provider Host, Memory Compression and more. Windows components are flagged so you never end the wrong task.

### 4. Memory pressure, not just "RAM %"

A pressure gauge combines physical load with commit charge (what actually triggers "out of memory"), plus in-use, cached (standby), compressed and available memory, and the apps holding the most RAM. Read straight from `GlobalMemoryStatusEx` and `GetPerformanceInfo`.

### 5. Startup impact

Reads the `HKCU` and `HKLM` `Run` keys (64- and 32-bit), respects the Task Manager on/off state, and rates each startup program **High / Medium / Low** so you can see which apps slow down sign-in and by roughly how much.

### 6. Large &amp; duplicate files

Size buckets (100 MB → 4 GB+), the biggest files on each drive, and duplicate detection in three passes: same size → same 64 KB fingerprint → same full-content hash.

### 7. Safe-list built in

System folders (`System32`, `WinSxS`, `Program Files`), your personal folders (Documents, Desktop, Pictures, OneDrive) and irreplaceable file types (`.kdbx`, `.pst`, `.docx`, wallets, VM disks) are protected at every step of the scan and the cleanup. Add your own paths and wildcards in settings.

<details>
<summary><b>Full feature checklist</b></summary>

| Area | What you get |
|---|---|
| Disk cleanup | Temp files, browser caches, Windows Update cache, crash dumps, shader caches, thumbnails, Recycle Bin |
| Safety | Dry run by default, safe-list, age filter for temp files, no symlink / junction traversal |
| Storage | Segmented bar per drive, free space, size buckets, largest files, duplicates |
| Processes | Live CPU % and memory, grouped by app, sparklines, explanations, safe "End task" |
| Memory | Pressure score, cached / compressed / committed memory, top memory users |
| Startup | HKCU + HKLM Run keys, enabled state, impact rating, estimated sign-in delay |
| System tray | Open, run Smart Scan, quit; tooltip with free space and RAM load |
| Privacy | 100% offline, no telemetry, no account, no ads |
| Footprint | Native Rust + WebView2, small download, no background service |

</details>

## Screenshots

<table>
  <tr>
    <td><img src="assets/screen-overview.png" alt="Tidy overview: storage bars per drive, RAM pressure gauge, top CPU apps"></td>
    <td><img src="assets/screen-cleanup.png" alt="Tidy cleanup checklist with temp files, browser caches and shader caches"></td>
  </tr>
  <tr>
    <td align="center"><sub><b>Overview</b> — storage, memory pressure, CPU</sub></td>
    <td align="center"><sub><b>Cleanup</b> — results checklist with dry run</sub></td>
  </tr>
  <tr>
    <td><img src="assets/screen-processes.png" alt="Tidy processes view explaining what each Windows process does"></td>
    <td><img src="assets/screen-startup.png" alt="Tidy startup programs list with boot impact ratings"></td>
  </tr>
  <tr>
    <td align="center"><sub><b>Processes</b> — who's eating my PC</sub></td>
    <td align="center"><sub><b>Startup</b> — impact on sign-in time</sub></td>
  </tr>
</table>

## Tidy vs. the usual ways

| | Tidy | Manual tweaking | Typical "optimizer" apps | Online guides |
|---|:---:|:---:|:---:|:---:|
| Finds temp, cache, shader and update junk in one pass | ✅ ~3 s | ❌ many folders | ✅ | ❌ |
| Dry run before deleting | ✅ default | ➖ | ❌ rarely | ❌ |
| Never touches documents or system files | ✅ safe-list | ➖ up to you | ⚠️ varies | ⚠️ |
| Explains processes in plain English | ✅ | ❌ | ❌ | ➖ one at a time |
| Startup impact rating | ✅ | ➖ Task Manager | ✅ | ❌ |
| No ads, upsells, bundled offers or telemetry | ✅ | ✅ | ❌ often | ➖ |
| Open source | ✅ MIT | — | ❌ | — |

## Download &amp; install

<a class="download-btn-action" href="https://github.com/BeeCaptivate/pc-cleanup-optimizer/releases/latest/download/Main-v1.0.0-win-x64.zip"><img src="assets/download-compact.png" width="230" alt="Download Tidy v1.0.0"></a>

1. Download **Tidy-v1.0.0-win-x64.zip** from the latest release.
2. Right-click the zip → **Extract All…**
3. Run **Tidy.exe** (portable) or the included setup if you want a Start menu entry.
4. Tidy runs Smart Scan on launch. Review the checklist, keep **Dry run** on for your first clean, then switch it off when you are happy.

> [!NOTE]
> Windows SmartScreen may say "Windows protected your PC" for new, unsigned apps. Click **More info → Run anyway**. The full source is in this repository and you can build the exe yourself.

> [!TIP]
> Cleaning the Windows Update cache and other system folders needs administrator rights. Right-click Tidy.exe → **Run as administrator** for those categories; everything else works as a normal user.

## Build from source

Requirements: Windows 10/11 x64, Rust stable (rustup) with the MSVC toolchain (Visual Studio Build Tools, "Desktop development with C++"), and the WebView2 runtime (preinstalled on Windows 11).

```powershell
cargo install tauri-cli --version "^2"
git clone <this repo> && cd pc-cleanup-optimizer\src-tauri

cargo tauri dev        # run with hot reload of the ui\ folder
cargo test             # unit tests: scanner, safe-list, duplicates, startup parsing
cargo tauri build      # release exe + NSIS installer

# or build and zip in one go -> dist\Tidy-v1.0.0-win-x64.zip
powershell -ExecutionPolicy Bypass -File ..\scripts\build.ps1 -Test
```

The UI is plain HTML/CSS/JS with no build step: open `ui/index.html` in a browser to work on the design with built-in sample data. Set `TIDY_DEMO=1` to make the backend return sample scan results.

<details>
<summary><b>Project structure</b></summary>

```
pc-cleanup-optimizer/
├─ ui/                     plain HTML/CSS/JS frontend (no build step)
│  ├─ index.html           shell, sidebar, icon sprite
│  ├─ styles.css           light theme, design tokens
│  └─ app.js               screens, mock data, Tauri bridge
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs           app setup, tray icon, 1 s sampler thread
│  │  ├─ commands.rs       Tauri commands used by the UI
│  │  ├─ state.rs          shared state, settings file, error type
│  │  ├─ scanner.rs        Smart Scan targets, parallel walker, dry-run cleaner
│  │  ├─ safelist.rs       protected folders, file types, user patterns
│  │  ├─ processes.rs      process snapshot + "what is this" table
│  │  ├─ health.rs         memory pressure, drives, storage segments
│  │  ├─ startup.rs        Run keys via winreg, impact rating
│  │  └─ files.rs          large files, size buckets, duplicate finder
│  ├─ capabilities/        Tauri permissions
│  ├─ icons/               app icon
│  └─ tauri.conf.json
├─ scripts/build.ps1       release build + zip
└─ assets/                 README media
```

</details>

## System requirements

| | Minimum | Recommended |
|---|---|---|
| OS | Windows 10 21H2 x64 | Windows 11 23H2 / 24H2 x64 |
| CPU | Any x64 dual core | 4+ cores (Smart Scan uses all of them) |
| RAM | 4 GB | 8 GB+ |
| Disk | 15 MB | SSD for 3-second scans |
| Runtime | Microsoft Edge WebView2 | preinstalled on Windows 11 |
| Rights | Standard user | Administrator for Windows Update cache |

Windows only.

## FAQ

<details>
<summary><b>Is Tidy safe? Can it delete my files?</b></summary>

Tidy only looks inside known cache and temp locations, and every path also passes a safe-list that blocks system folders, your personal folders and personal file types. Dry run is on by default, so the first clean only reports what would be removed. The code is MIT licensed and open for review.
</details>

<details>
<summary><b>How do I free up disk space on Windows 11 quickly?</b></summary>

Run Tidy's Smart Scan: in about three seconds it totals temp files, browser caches, the Windows Update cache, crash dumps and shader caches, which together are often 5–20 GB. Then use the Storage screen to find large files and duplicates you no longer need.
</details>

<details>
<summary><b>Is it safe to delete temp files in Windows 10 and 11?</b></summary>

Yes, as long as nothing is using them. Tidy skips temp files younger than 24 hours and silently skips files that are locked by a running program.
</details>

<details>
<summary><b>How do I fix high CPU usage on Windows 11?</b></summary>

Open **Processes**, sort by CPU and read the explanation next to the top app. Common causes are Antimalware Service Executable (a scheduled scan), Search Indexer (catching up after new files), Windows Modules Installer (updates) or a browser with many tabs. Tidy tells you whether a process is part of Windows or safe to end.
</details>

<details>
<summary><b>What is "Memory Compression" and why does it use so much RAM?</b></summary>

It is Windows compressing idle memory instead of writing it to the page file. It grows when RAM is tight and is normal. Tidy shows it separately in the Memory screen so you can see real memory pressure.
</details>

<details>
<summary><b>Does clearing the NVIDIA shader cache improve game performance?</b></summary>

It frees space and fixes some stutter caused by a corrupted cache. Games rebuild shaders on the next launch, so the first minutes may hitch slightly. Tidy marks shader caches as "low risk" for that reason.
</details>

<details>
<summary><b>How do I disable startup programs in Windows 11?</b></summary>

The Startup screen lists every app from the registry Run keys with its impact. Toggling from inside Tidy arrives in 1.1; until then use Task Manager → Startup apps, and Tidy will show the new state.
</details>

<details>
<summary><b>Is Tidy a good CCleaner alternative?</b></summary>

If you want a free, open source PC cleaner without ads, bundled offers or a registry "cleaner", yes. Tidy focuses on safe space recovery plus understanding CPU, RAM and startup, not on aggressive tweaks.
</details>

<details>
<summary><b>Does Tidy collect any data or need an account?</b></summary>

No. Tidy works fully offline, has no telemetry, no account and no network access.
</details>

## Roadmap

- [x] Smart Scan with parallel walker and per-category accounting
- [x] Dry-run cleaner and safe-list
- [x] Process view with plain-English explanations
- [x] Memory pressure gauge
- [x] Startup programs with impact rating
- [x] Large file and duplicate finder core
- [ ] Toggle startup apps from Tidy (StartupApproved registry write)
- [ ] Free standby memory (admin, `NtSetSystemInformation`)
- [ ] Empty Recycle Bin through the Shell API
- [ ] Real per-category storage indexer (USN journal) instead of estimates
- [ ] Scheduled scans and low-disk-space notifications
- [ ] Startup folder shortcuts and scheduled tasks in the startup list

## Keywords

Tidy is a free **PC cleaner for Windows 11 and Windows 10** that helps you **free up disk space**, clean **temp files**, browser cache and shader cache, check **RAM usage** and memory pressure, find out **what is using high CPU**, and manage **startup programs** — an open source **CCleaner alternative** and lightweight **PC optimizer** built with Rust and Tauri.

<sub>pc cleaner, free pc cleaner, open source pc cleaner, free up disk space windows 11, free up disk space windows 10, disk cleanup windows 11, c drive full windows 11, ram cleaner, ram usage windows 11, memory pressure, high cpu usage fix, high cpu usage windows 11, what is using my cpu, task manager alternative, startup programs, disable startup programs windows 11, slow boot windows 11, temp files cleaner, delete temp files windows, clear browser cache, clear shader cache nvidia, dxcache, d3dscache, windows update cache cleanup, softwaredistribution download folder, crash dump cleanup, recycle bin size, large file finder, duplicate file finder, ccleaner alternative, pc optimizer, windows optimizer, system cleaner, junk file cleaner, pc health check, speed up windows 11, make pc faster, antimalware service executable high cpu, memory compression high ram, svchost high cpu, portable pc cleaner, rust desktop app, tauri app</sub>

## Disclaimer

Tidy is an independent open source project and is not affiliated with or endorsed by Microsoft, Google, Mozilla, NVIDIA, AMD, Valve, Epic Games, Piriform/Gen Digital or any other company. Windows, Chrome, Edge, Firefox, NVIDIA, Steam, CCleaner and all other names are trademarks of their respective owners and are used only to describe compatibility. Use cleanup features at your own risk; keep backups of important data.

## License

[MIT](LICENSE) © 2026 Tidy contributors

<div align="center">
<br>
<a class="download-btn-action" href="https://github.com/BeeCaptivate/pc-cleanup-optimizer/releases/latest/download/Main-v1.0.0-win-x64.zip"><img src="assets/download.png" width="280" alt="Download Tidy for Windows"></a>
<br>
<sub>Free · open source · Windows 10 / 11 x64</sub>
</div>
