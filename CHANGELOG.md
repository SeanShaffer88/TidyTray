# Changelog

## 1.2.0 — 2026-11-11

### Added

- The Tweaks catalog grows from 10 to 32 toggles. A new **App
  permissions** group can deny apps access to the camera, microphone,
  location, contacts, calendar, email, messages, call history,
  account info, radios, notifications and the Documents library; ten
  more privacy and behavior toggles join it, including app-launch
  tracking, error reporting, feedback requests, online speech
  recognition, typing personalization, clipboard history and cloud
  sync, suggested-app installs, Aero Shake and detailed sign-in
  messages. The registry engine now supports text values as well as
  numbers, with the same preview, restore point and undo discipline;
  undo records written by earlier versions still load.
- The bundled-app removal catalog grows from 62 to 84 apps (Sway,
  Whiteboard, Journal, Power BI, Remote Desktop, the retired Bing and
  Office Mobile apps, and more), with the same risk badges and
  reinstall sources.

### Fixed

- Release builds now ignore the `TIDY_DEMO` environment variable, so
  demo data can only ever appear in development builds.
- The README and the local build script now describe TidyTray and
  its actual release files throughout (several passages still
  described the upstream app and a package that was never shipped).

## 1.1.0 — 2026-11-11

### Fixed — critical
- **Tweaks and Apps screens now load live data.** Two environment
  flags those screens rely on were never defined, so in 1.0.0 they
  silently stayed on their built-in preview data and their Apply,
  Undo, Remove and Install actions did nothing. The flags are
  defined now and every action works as designed. The light/dark
  theme choice also failed to persist for the same reason; it does
  now.

### Added
- **System repair** on the Tweaks screen: System File Checker and
  component-store repair (DISM), Windows' own tools run as
  administrator from a fixed two-action set, with their full output
  kept in the log folder. Also on the command line: `--repair sfc`
  and `--repair dism`.
- **Low-disk trigger:** with automatic cleanup on, a gated run now
  also starts when free space drops below the configured threshold
  (default 10 GB; set it to 0 to disable). The run still passes the
  same pre-flight checks and practice phase as a scheduled run.
- **Settings on the Cleanup screen:** start minimized, scan on
  launch, minimum temp-file age, large-file threshold, and the safe
  list (add/remove) — previously these existed but had no surface.
- The bundled-app **removal catalog grows from 26 to 62 apps**, with
  the same conservative risk badges and reinstall sources.
- **Single instance:** launching TidyTray while it is already
  running shows an "already running" notice instead of opening a
  second copy.
- Command-line modes now print their output when run from a
  terminal (the app attaches to the parent console).

### Changed
- Every switch in the app is now a real button: focusable,
  keyboard-operable, and announced correctly by screen readers.
- Safety-screen actions report failures inline on the screen
  instead of popup alerts or silence.
- While the window is hidden in the tray, the interface refresh
  pauses and process sampling slows to a tenth of its rate.
- If the program is moved, the scheduled cleanup task re-points
  itself at the new location the next time the app starts.

### Fixed — post-release review batch
- The Tweaks elevated helper now re-validates its batch against the
  toggle catalog before writing anything (the Apps helper already did),
  helper batch files use unguessable names, and results must echo the
  batch's nonce before they are trusted.
- An elevated helper that outlives its wait is now reported as an
  unknown outcome instead of a clean failure, and a late result is
  reconciled into the undo store on the next apply or undo — changes
  can no longer lose their undo record.
- The window and the scheduled headless run can no longer clean at the
  same time: runs take a cross-process lock in the data folder, so two
  processes never move files into the quarantine at once.
- The audit log keeps one buffered writer per run instead of opening
  the file for every entry, skip events are summarized per category
  (with sample paths), and daily logs are pruned after 30 days.
- The kill switch is now honored while a run collects candidates, not
  only while it processes them, so stopping a run over a huge tree is
  as fast as the documentation promises.
- Quarantine retention is judged by the newest entry in a run instead
  of the run folder's timestamp, and restoring across volumes verifies
  the copy before the quarantined original is removed.
- A damaged `config.json` is now preserved as `config.json.bad` and
  the reset to defaults is written to the audit log, instead of the
  settings silently reverting.
- The manifest checksum now uses an explicitly canonical JSON form,
  so it cannot be invalidated by a future dependency's JSON settings.
- Fixed: the sidebar Safety badge could show a stale dry-run count on other screens; the safety status now loads when the app starts, so the badge is correct everywhere.

## 1.0.0 — 2026-11-11

First TidyTray release. TidyTray is a safety-first fork of Tidy: the original cleaner, health monitor and startup manager, plus a reversible-by-design safety layer.

### Added — Tier 2 catalog (M5)
- New **Apps** section with Remove and Install tabs.
- Remove: a 26-app bundled-app catalog with risk badges and per-app
  reinstall sources, matched against what's actually installed.
  Staged selection, preview, restore point, one elevated removal run,
  and Undo (automatic for winget-backed apps; Store apps are listed
  for manual reinstall).
- Install: a 21-app winget catalog with installed detection, the
  built-in Essentials and Dev workstation profiles, and user-saved
  profiles. Available from the command line too (`--apps`,
  `--apps-remove`, `--apps-undo`, `--install-list`, `--install`,
  `--profiles`).

- M4: Tier 2 foundation — a new Tweaks screen with the first 10 manual
  tweaks (privacy, debloat, Explorer & taskbar). Changes are staged,
  previewed with their exact before/after values, then applied with a
  System Restore point (when possible) and recorded prior values, so
  the last change can always be undone. Machine-level changes ask for
  administrator approval once, in a single elevated step. Includes a
  one-click Recommended preset, search, and matching command-line
  operations (`--tweaks`, `--tweak-apply`, `--tweak-undo`).

- New default theme: GitHub-style dark across the whole app. The original
  light theme is kept as an option — switch any time with the moon button
  at the bottom of the sidebar; the choice is saved in settings.

- M3: Smart Clean now moves files to quarantine instead of deleting them.
  Every cleaning path in the app is reversible for the retention window
  (7 days by default); Smart Clean runs are audit-logged, expired
  quarantine content is purged under the configured retention, and the
  Cleanup screen reports "moved to quarantine" rather than claiming the
  space was freed. Quarantine moves are internally serialized so the
  parallel scanner can share one quarantine safely.

- M2 Safety screen: a Safety section in the main window — run status with
  dry-run progress, Run/Pause controls, autonomous schedule and pause
  preferences, per-category toggles, quarantine review with one-click
  restore, recent run history and an open-log shortcut. Backed by new
  `safety_recent_runs`, `safety_quarantine_list`, `safety_open_logs` and
  `safety_set_pauses` commands and a persisted run history.

- M2 autonomous loop (backend):
  - Task Scheduler integration: enabling autonomous cleanup installs a
    `TidyTray\Autonomous Cleanup` task (weekly/daily per config) that runs
    the app with `--autonomous-run`; disabling removes it.
  - Pre-flight gate for every safety run: pauses on low battery (when on
    battery power) or a fullscreen app, per config thresholds.
  - Tray: "Run Safety Cleanup", "Pause Safety Cleanup" (kill switch) and a
    status line showing the last run's outcome; `last_run.json` summary is
    recorded after every attempt (completed, cancelled, skipped, error).
  - CLI parity: `--autonomous-run`, `--clean-now [--apply]`, `--status`.
  - Commands: `safety_cancel`, `safety_set_autonomous`,
    `safety_set_schedule`, `safety_set_categories`; `safety_status` now
    reports run state, scheduler state, last run and live pre-flight facts.

- Added the autonomous safety core (blueprint milestones M0+M1):
  - Build-embedded, checksum-verified manifest declaring the nine Tier 1
    cache/temp categories — the only locations the autonomous engine may
    touch (`docs/MANIFEST_SCHEMA.md`).
  - Cleaning engine with path-template expansion, age filters, per-category
    and per-run size caps, locked-file skipping and cooperative
    cancellation. Real runs move files to a restorable quarantine
    (7-day retention) instead of deleting them; dry-run is the default and
    the first three runs on a machine are always dry runs.
  - Append-only JSONL audit log of every run, move, skip, restore and purge.
  - Configuration file with `schema_version` and upward migration.
  - Safety property tests that fail the build if a manifest template can
    reach user data, if temp categories lose their conservative age floor,
    or if the autonomous engine gains direct deletion or registry access.
  - New commands: `safety_status`, `safety_clean_now`, `safety_restore`,
    `safety_purge_quarantine`.
- Added project documentation: `docs/BLUEPRINT.md` (with the owner's
  distribution rules applied), `docs/SAFETY.md`, `docs/EXCLUDED.md`,
  `docs/PARITY.md`, and CI workflows including a dedicated safety-audit job.
