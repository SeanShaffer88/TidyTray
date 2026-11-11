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
8. **Kill switch.** A run checks a cancellation flag for every file and
   stops within seconds. "Pause autonomous cleanup" in the tray menu uses it.
9. **Everything is logged.** Each run writes append-only JSONL entries
   (run start/end, every move, every skip, restores and purges) to
   `%LOCALAPPDATA%\TidyTray\Logs`.
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
- Every applied or reverted change is written to the audit log.
- Undo restores exact priors, including deleting values that did not
  exist before the change.

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
