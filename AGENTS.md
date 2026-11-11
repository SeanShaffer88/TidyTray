# TidyTray — Project Rules

## Project Manifest
- Tier: **t1b** (tray utility) — Adaptive Engineering Contract V4.0
- Stack: Rust + Tauri 2 on the system WebView2 (V4.0 sanctioned alternate), PowerShell 5.1/7 for Tier 2 debloat actions only
- Purpose: Tray-resident, safety-first Windows maintenance: autonomous cache/temp cleanup (Tier 1) plus a manual, previewed, reversible debloat suite (Tier 2)
- Upstream: fork of BeeCaptivate/pc-cleanup-optimizer (MIT); keep attribution and the LICENSE file untouched
- Release shape (every release, no exceptions): `TidyTray-v{VERSION}-x64-Setup.exe` + `TidyTray-v{VERSION}-x64-Portable.zip` + `SHA256SUMS.txt`. No MSI, no ARM64/x86 variants, x64 only
- Distribution: GitHub Releases only. **Never** submit to winget or Chocolatey. Binaries ship **unsigned** — no code signing unless a platform requirement forces it
- Build: `cargo build` in `src-tauri/` · Full gate: `cargo clippy --all-targets -- -D warnings && cargo test` in `src-tauri/` plus the rustfmt check scoped to the safety-core modules (command in `.github/workflows/ci.yml`; upstream modules keep their original style). Local Windows link route on the dev VM (no MSVC): `RUSTFLAGS="-C linker=rust-lld -L $HOME/llvm-mingw/x86_64-w64-mingw32/lib -C link-arg=$HOME/llvm-mingw/x86_64-w64-mingw32/lib/crt2.o" cargo build --target x86_64-pc-windows-gnullvm`, with a working `x86_64-w64-mingw32-windres` on PATH (the llvm-mingw shim in `$HOME/llvm-mingw/bin` execs a missing system llvm-windres — use the standalone llvm-windres from the tools tree) — verify the .exe is fresh and `file` reports PE32+
- Config: `config.json` carries `schema_version` with tested upward migrations only

## Safety invariants (non-negotiable — see docs/SAFETY.md)
1. The autonomous engine acts only on category ids in the build-embedded, checksum-verified manifest (`src-tauri/manifest/cleaner-manifest.json`). It never loads a manifest from disk.
2. The autonomous engine never touches user data, registry, services, scheduled tasks, drivers or policies, and never deletes: it moves files to quarantine (7-day retention, restorable). Permanent deletion exists only in the quarantine retention purge.
3. Locked files are skipped, never forced. Dry-run is the default; the first 3 runs on a machine are dry runs, and autonomous mode starts disabled.
4. Tier 2 actions are manual only: preview → confirmation → restore point or undo artifact → execution → log. An action without a verified undo path does not ship.
5. Tier 3 (docs/EXCLUDED.md) is never implemented: no RAM cleaners, no registry cleaners, no Defender/SmartScreen removal, no permanent Windows Update disabling, no bulk disables.
6. No telemetry, no phone-home.

## Preservation rule (owner, 2026-11-10)
The upstream project's design, structure and operation are preserved. TidyTray
work is **additive only**: new capability lands in new modules/files, upstream
files get only minimal hooks, and existing features are never redesigned,
removed or behaviour-changed. Mechanical lint fixes are the only exception.

## Working rules
- New features are implemented as operations first, then wired to the UI/CLI.
- Every capability in the window is also available from the command line.
- No new external dependency without the owner's explicit approval.
- Safety property tests (`src-tauri/src/safety_tests.rs`) must stay green; a change that weakens an invariant updates docs/SAFETY.md in the same commit or does not land.
- Public docs (README, CHANGELOG, release notes) contain factual product documentation only.
- Known limitations live in docs/PARITY.md · Decision record: docs/BLUEPRINT.md (owner-rules header) · Blueprint: docs/BLUEPRINT.md
