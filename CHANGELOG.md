# Changelog

## Unreleased

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
