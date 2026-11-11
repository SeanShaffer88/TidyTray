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
| Repo topics set | Not started | Owed via the GitHub web UI (API token here cannot edit About) |
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
| M4 Tier 2 foundation (preview/diff, restore point, undo artifacts, elevated helper) | Not started |
| M5 Tier 2 full catalog (incl. the 2026-11-10 mining from W10Privacy, ShutUp10++, Winaero Tweaker and XToolbox — see BLUEPRINT.md §2) | Not started |
| M6 Polish & release (AV allowlisting, standard release shape, docs) | Not started |

## Known limitations
- Windows runtime behaviour of the safety core is **unverified** until it
  runs on a real Windows machine; the local gate covers logic tests and a
  Windows-target link build, CI covers the full test suite on Windows.
- QUALITY.md scorecard is introduced at the first release gate (M6).
