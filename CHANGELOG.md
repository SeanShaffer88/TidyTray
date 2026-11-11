# Changelog

## Unreleased

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
