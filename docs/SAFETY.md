# Safety model

TidyTray's promise is narrow on purpose: **autonomous cleanup can only touch
regenerable caches and temporary files. Everything that changes system state
is manual, previewed, logged, and reversible.**

## The three tiers

- **Tier 1 — autonomous-safe.** Nine cache/temp categories declared in a
  manifest that is embedded in the program at build time and verified with a
  checksum on every load. The engine can only act on those categories.
- **Tier 2 — manual opt-in.** Debloat, privacy and repair actions. Each one
  needs an explicit click, shows a preview of what will change, creates a
  System Restore point (or a generated undo script), and writes an undo
  artifact and a log entry.
- **Tier 3 — excluded.** Never implemented. See [EXCLUDED.md](EXCLUDED.md).

## Invariants enforced by the Tier 1 engine

1. **No arbitrary paths.** Actions are chosen by category id from the
   embedded manifest only. Path templates must start at an allowlisted
   environment variable, may not contain `**`, drive letters or `..`, and
   may use `*` only as a whole path segment or inside the final file-name
   segment.
2. **No user data.** Templates may never name documents, downloads, desktop,
   pictures, videos, music, browser cookies, login data, local storage,
   IndexedDB or session storage. A blocklist is checked when the manifest
   loads and again in CI.
3. **No system changes.** The autonomous engine contains no registry,
   service, scheduled-task, driver or policy code — verified by a static
   source check in the test suite.
4. **Quarantine, never delete.** Files are moved to
   `%LOCALAPPDATA%\TidyTray\Quarantine\<run-id>` and kept for 7 days
   (configurable). Any file can be restored from the log view. Permanent
   deletion happens only when expired quarantine content is purged.
5. **Locked files are skipped.** Files in use are never forced, moved with
   shadow-copy tricks, or queued for reboot deletion.
6. **Conservative ages.** Temp files must be at least 7 days old (Windows
   temp 30), browser caches 14, crash dumps and delivery-optimization files
   30. Per-category and per-run size caps bound every run.
7. **Dry-run first.** The first 3 runs on any machine are dry runs that only
   report. Autonomous mode starts disabled and needs an explicit opt-in.
8. **Kill switch.** A run checks a cancellation flag while collecting
   candidates as well as for every file it processes, and stops within
   seconds. "Pause autonomous cleanup" in the tray menu uses it.
9. **Everything is logged.** Each run writes append-only JSONL entries
   (run start/end, every move, restores and purges) to
   `%LOCALAPPDATA%\TidyTray\Logs`. Files that are skipped (too new,
   over a size cap, in use) are logged as a per-category summary with a
   few sample paths, so a run over tens of thousands of files stays
   readable. Daily log files are kept for 30 days.
11. **One run at a time — across processes.** A lock file in the data
    folder stops the window and the scheduled headless run (separate
    processes) from ever moving files into the quarantine at the same
    time; the second run reports that a run is already in progress.
12. **Damaged config is visible.** If `config.json` cannot be parsed,
    the original is kept as `config.json.bad`, the reset to safe
    defaults is written to the audit log, and the app continues with
    autonomous mode off and the dry-run phase on.
10. **No telemetry.** Nothing is uploaded, ever. Crash reporting is
    local-only.

## How the invariants are tested

`src-tauri/src/safety_tests.rs` runs in CI on every change:

- every manifest template expands only under a known safe root;
- no template names a user-data location;
- temp categories keep a minimum age of 7 days;
- the autonomous engine's production sources contain no direct file
  deletion and no registry calls;
- configuration defaults keep autonomous mode off and the dry-run phase on.

## Tier 2 (manual tweaks)

Tier 2 changes system state, so it follows a stricter, fully manual
discipline — the autonomous engine never touches it:

- Changes are **staged** in the UI first; nothing is written until the
  user reviews an exact preview (current value → new value, in plain
  words) and confirms.
- Before anything is written, the prior value of every registry value
  is recorded. Applying creates a System Restore point when possible
  *and* stores those priors as an undo batch, so Undo works even when
  no restore point could be created (for example, System Restore off).
- Machine-level writes and the restore point happen in a single
  elevated helper run of the app itself; if the user declines the
  Windows prompt, those changes are reported as failed — never silently
  retried, never partially claimed.
- The helper **re-validates its batch against the toggle catalog**
  before writing anything: it can only ever touch the registry values
  the catalog owns, and an apply batch can only write the values the
  planner can produce for them. Batch files have unguessable names and
  the result must echo the batch's nonce before the parent trusts it.
- If a helper outlives the parent's wait, the timeout is reported as
  an unknown outcome — not a clean failure — and a late result is
  reconciled into the undo store on the next apply or undo, so changes
  never lose their undo record.
- Every applied or reverted change is written to the audit log.
- Undo restores exact priors, including deleting values that did not
  exist before the change.

## Tier 2 (app removal and installs)

- The **removal catalog is fixed in the build** (26 bundled apps with
  Safe/Caution/Advanced badges). Windows-critical packages and core
  utilities are never listed. Callers — UI, CLI, or the elevated helper
  itself — can only ever remove catalog packages: the helper re-checks
  membership before running anything.
- Removal is staged and previewed; the preview shows each app's
  reinstall source *before* removal. One elevated run creates a restore
  point and removes the packages (current user + provisioned copy).
  Every removal is audit-logged and recorded; **Undo** reinstalls
  winget-backed apps automatically and names the Store apps to
  reinstall manually — it never pretends a manual step was automated.
- Installs run **winget with catalog ids only** (21 apps). Profile ids
  are validated against the catalog when a profile is saved. If winget
  is unavailable, the Install tab explains instead of failing.

## Honest limits

- The pre-move "file is in use" check is best-effort. If a move fails, the
  file is skipped and the failure is logged — it is never retried with force.
- The legacy Smart Clean from the upstream project is fully manual and
  dry-run-by-default, with its own safe-list. Since M3 it also moves files
  into quarantine instead of deleting them, and its runs are audit-logged;
  the quarantine guarantee now covers every cleaning path in the app.
  (Its Recycle Bin category remains a no-op during cleaning, as upstream:
  emptying the Recycle Bin is Explorer's job.)
- Wording in the UI says "conservative", "regenerable" and "reversible" —
  never "100% safe" or "guaranteed".
