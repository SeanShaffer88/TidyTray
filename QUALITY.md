# TidyTray — Quality Scorecard

Scored from evidence, per the project's governing quality rules.
Scale: each dimension 1–5; the overall is the mean converted to a
percentage. Gate: overall ≥ 85 and no dimension below 3.

**Current score: 83 / 100** (2026-11-11, v1.1.0) — below the 85 gate.
The gap is verification debt, itemized under Follow-ups; the v1.1.0
release ships on the owner's direction with the gap recorded here
rather than the scores adjusted.

| Dimension | Score | Evidence |
|---|---|---|
| Correctness | 4 | Core flows (scan, Safety runs, quarantine, preflight skips) field-verified on a real machine at v1.0.0. The v1.0.0 Tweaks/Apps live-data defect (undefined environment flags) was found by inspection and fixed in v1.1.0; those screens' live behavior awaits the v1.1.0 field pass. |
| Safety & security | 5 | Autonomous cleanup acts only on a build-embedded checksummed manifest; quarantine-only, never deletes; Tier 2 elevated helpers re-validate their batches against the catalog; nonce-paired helper results; cross-process run lock; no telemetry. All covered by property tests (`safety_tests.rs`) and unit tests. |
| Reliability | 4 | Every Tier 2 change carries an undo artifact; helper timeouts reconcile late results into the undo stores; a damaged config is preserved as `config.json.bad` and the reset is audit-logged; quarantine restores are size-verified. |
| Performance | 4 | Audit logging is buffered with per-category skip summaries; process sampling and UI refresh slow down while the window is hidden; memory use is bounded by design (resident tray utility). No formal benchmark suite. |
| UX & accessibility | 4 | All switches are real buttons (`role="switch"`, keyboard-operable, visible focus); failures surface inline on the screen that caused them; plain-language copy reviewed against the blueprint's honesty rules. Settings and the safe list gained a surface in v1.1.0. |
| Maintainability | 4 | Upstream structure preserved; new capability lands as new modules with thin hooks; docs (SAFETY, PARITY, BLUEPRINT) updated in the same changes as the code they describe. |
| Test & verification evidence | 4 | Logic test suite runs in CI plus a Windows build-and-test job; release assets are re-downloaded and hash-verified. Windows runtime evidence still comes only from field passes — see Follow-ups. |

## Known limitations

- Windows runtime behavior is verified by field passes on the owner's
  machine, not by this scorecard: the installer run, a real
  (non-dry) cleanup + restore, tweak apply/undo against the live
  registry, app removal/undo, winget installs, autonomous scheduled
  firing, and the v1.1.0 additions (repair runs, low-disk trigger,
  single-instance notice) are Unverified until exercised there.
- The removal catalog (62 entries) grows only as fast as entries can
  be verified; runtime/framework packages (VCLibs, UI.Xaml, Store
  itself is Advanced-only) are deliberately excluded or badged.

## Follow-ups that raise the score

1. v1.1.0 field pass covering Tweaks/Apps live data, one tweak
   apply + undo, one app removal + undo, one repair run, and the
   theme-persistence fix → Correctness 5, Test evidence 5 (≈ 91).
2. Installer-run verification on a clean profile → Reliability 5.
