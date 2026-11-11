# Parity — blueprint vs. what is built

Scored from evidence in this repository only. Status key: **Done** (in the
tree and covered by tests), **Partial** (foundation exists, wiring or UI
missing), **Not started**, **Excluded** (Tier 3, will not be built).

## M0 — Fork & audit
| Item | Status | Evidence |
|---|---|---|
| Upstream license verified (MIT) | Done | LICENSE in repo root; fork relationship on GitHub |
| Blueprint filed with owner rules applied | Done | docs/BLUEPRINT.md header |
| SAFETY / EXCLUDED docs | Done | docs/SAFETY.md, docs/EXCLUDED.md |
| CI skeleton | Done | .github/workflows/ci.yml, safety-audit.yml |
| Tier classification recorded | Done | .project-tier (t1b), AGENTS.md manifest |
| Repo topics set | Done | Description + 10 topics set at M0 and re-verified accurate at the v1.0.0 release |
| Strip non-essential upstream UI | Not started | Deliberately deferred: upstream features stay untouched until their Tier mapping is decided per feature |

## M1 — Safety core
| Item | Status | Evidence |
|---|---|---|
| Manifest schema + embedded load + checksum | Done | src-tauri/manifest/cleaner-manifest.json, src/manifest.rs |
| Scanner: template expansion, age filter, caps, link skip | Done | src/cleaner.rs |
| Lock check (skip in-use files) | Done | src/lockcheck.rs (best-effort, documented in SAFETY.md) |
| Quarantine move / restore / retention purge | Done | src/quarantine.rs |
| JSONL audit logging | Done | src/audit.rs |
| Config with schema_version + migration | Done | src/appconfig.rs |
| Safety property tests (§11.3) | Done | src/safety_tests.rs |
| Tauri commands for the safety core | Done | safety_status / safety_clean_now / safety_restore / safety_purge_quarantine in src/commands.rs |
| UI screens for the safety core | Done | Safety screen in the main window (status + dry-run progress, schedule/pause settings, category toggles, quarantine review & restore, recent runs, open-log) in the app's existing style; design approved by the owner from a rendered mockup before building |
| Legacy Smart Clean routed through quarantine | Done (M3) | scanner::clean moves files via Quarantine::move_file (thread-safe index); runs audit-logged, retention purge applied; Cleanup screen copy corrected — a real clean now says files were moved to quarantine, not freed |

## M2–M6 (blueprint roadmap)
| Milestone | Status |
|---|---|
| M2 Autonomous loop (scheduler, pre-flight checks, tray kill switch, dry-run opt-in flow) | Backend complete — Task Scheduler task via schtasks (weekly/daily), battery/fullscreen pre-flight gate, tray Run/Pause items + status line, `--autonomous-run` / `--clean-now` / `--status` CLI, autonomous enable gated on the dry-run phase and installing the task. Safety screen followed in the M2 UI slice (owner-approved design) |
| M3 Tier 1 categories complete on the Windows test matrix | Partial — all 9 categories are in the manifest; field matrix not yet run |
| M4 Tier 2 foundation (preview/diff, restore point, undo artifacts, elevated helper) | Done (M4) | tier2.rs: RegStore abstraction (winreg + in-memory fake), 10-toggle catalog, detect/plan with priors captured, one elevated helper run (--tier2-elevated) covers the restore point + all HKLM writes, undo batches record exact prior values (absent values are deleted on undo); Tweaks screen per the approved design (staging, preview sheet, Recommended preset, Undo, search); CLI --tweaks/--tweak-apply/--tweak-undo expose the same operations |
| M5 Tier 2 full catalog (incl. the 2026-11-10 mining from W10Privacy, ShutUp10++, Winaero Tweaker and XToolbox — see BLUEPRINT.md §2) | Done — Apps screen (owner-approved design): 26-entry removal catalog with Safe/Caution/Advanced badges and per-app reinstall sources, detected against the real installed set; removal via one elevated helper run (restore point first, provisioned copies removed too) with undo records — winget-backed apps reinstall automatically, Store apps are reported for manual reinstall; 21-app winget install catalog with installed detection, two built-in profiles plus user-saved profiles. Honest limits: removal targets the current user plus the provisioned copy; the removal catalog stands at 84 entries (grown in the Tier 2 catalog-depth slice) |
| M6 Polish & release | Done — product renamed to TidyTray everywhere (app name, binary, identifier com.tidytray.desktop, settings folder, UI brand, tray); README and changelog prepared; **v1.0.0 released** from the tag-triggered workflow (run 42976799959) with exactly TidyTray-v1.0.0-x64-Setup.exe + TidyTray-v1.0.0-x64-Portable.zip + SHA256SUMS.txt, re-downloaded and hash-verified; portable zip contains TidyTray.exe + LICENSE |

## Post-release review batches B & C (2026-11-11, v1.1.0)

- **Critical fix:** the Tweaks and Apps screens referenced two
  environment flags (`IN_TAURI` / `hasTauri`) that were never defined,
  so in v1.0.0 those screens silently stayed on their preview data and
  their actions did nothing; the theme choice also failed to persist.
  The flags are now defined and every call site works as designed.
- **Batch B:** all switches are real buttons (`role="switch"`,
  keyboard-operable, focus-visible); Safety-screen actions report
  failures inline instead of alert boxes or silence; UI refresh and
  the process sampler slow down while the window is hidden in the
  tray; the scheduled task re-points itself if the executable moved;
  settings that previously had no surface (start minimized, scan on
  launch, temp-file age, large-file threshold, the safe list) are
  editable on the Cleanup screen; headless CLI modes attach to the
  parent console so their output is visible from a terminal.
- **Batch C:** the low-disk trigger is implemented (above);
  single-instance is enforced with a hand-rolled named mutex (no new
  dependency); the removal catalog grows from 26 to 62 verified
  entries with conservative risk badges; system repair (SFC and DISM,
  §6.2) is implemented as a fixed two-action catalog with the same
  validated elevated-helper discipline as the other Tier 2 surfaces;
  QUALITY.md is adopted (repo root) and scored from evidence.

## Post-release review fixes (2026-11-11)
A senior-review pass over the v1.0.0 tree produced a batch of fixes with no
intended behaviour change (full report kept with the project records):
the Tier 2 elevated helper now re-validates its batch against the toggle
catalog (as the apps helper already did); elevated-helper timeouts are
reported distinctly and late results are reconciled into the undo store;
cleaning runs take a cross-process lock so the GUI and the scheduled
headless run cannot move files into the quarantine at the same time; the
audit log keeps one buffered writer per run, summarizes per-file skip
events per category, and prunes daily logs after 30 days; the kill switch
is honored during candidate collection; quarantine retention is judged by
the newest entry timestamp and restore verifies cross-volume copies;
helper batch files use unguessable names with nonce-paired results; a
corrupt config is preserved as `config.json.bad` and the reset is
audit-logged; the manifest checksum uses explicit canonical JSON.

## Known limitations
- Windows runtime behaviour was **field-verified on 2026-11-11** (owner
  captures of v1.0.0 on a real Windows 11 machine: launch, all nine
  screens, Smart Scan on real data, the dry-run phase with recorded
  practice runs, a scheduled run correctly skipped for a fullscreen app,
  a populated quarantine, applied tweaks, AppX detection). Still
  unverified: the installer run itself, a real (non-dry) clean and
  quarantine restore, tweak undo, app removal and its undo, winget
  installs from the catalog, and autonomous scheduled firing.
- ~~The blueprint's low-disk run trigger (§8) is not implemented~~ —
  implemented in v1.1.0 (see the Post-release review section below).
  Implement-or-remove is an open decision (BLUEPRINT.md §16 note).
- QUALITY.md scorecard was planned for the first release gate (M6) and
  has not been introduced yet.

## Tier 2 catalog depth (2026-11-11, post-v1.1.0)

- **String-valued registry ops:** the Tier 2 engine now handles REG_SZ
  values alongside DWORDs (`OpSpec`/`OpVal`), through detect, plan,
  the validated elevated helper, execute and undo. Undo files written
  by earlier versions (bare DWORD numbers) still parse.
- **Tweaks catalog 10 → 32:** a new **App permissions** group (12
  toggles denying camera, microphone, location, contacts, calendar,
  email, messaging, call history, account info, radios, notifications
  and the Documents library through the CapabilityAccessManager
  consent store) plus ten DWORD toggles from the approved mining list:
  app-launch tracking, error reporting, feedback requests, online
  speech recognition, typing & inking personalization, clipboard
  history, clipboard cloud sync, suggested/sponsored apps, Aero
  Shake, and detailed sign-in messages.
- **Removal catalog 62 → 84:** 22 more verified Store packages with
  conservative badges (retired Bing and Office Mobile apps, Sway,
  Whiteboard, Journal, Power BI, Remote Desktop, and more).
- **Hygiene:** `TIDY_DEMO` is honored only in debug builds, so a stray
  environment variable can never make a shipped build show demo data;
  the README and `scripts/build.ps1` now describe TidyTray and its
  actual release assets throughout.
