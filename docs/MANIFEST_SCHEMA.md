# Cleaner manifest schema

The Tier 1 categories live in `src-tauri/manifest/cleaner-manifest.json`,
embedded into the program at build time with `include_str!`. The autonomous
engine never loads a manifest from disk at runtime.

## Document shape

```jsonc
{
  "schema_version": 1,
  "manifest_version": "2026.01.1",
  "checksum": "<sha256 hex>",
  "categories": [ /* category objects, see below */ ]
}
```

`checksum` is the SHA-256 of the canonical JSON of the `categories` array:
the parsed value re-serialised with object keys sorted and no whitespace.
The loader recomputes it on every load and refuses to run on a mismatch.

## Category fields

| Field | Meaning |
|---|---|
| `id` | Stable identifier (`temp_user`, `browser_cache_chromium`, …) |
| `display_name` | Human-readable name shown in the UI |
| `paths` | Path templates (rules below) |
| `min_age_days` | Files younger than this are skipped |
| `requires_admin` | Category needs elevation; off by default in config |
| `skip_locked` | Skip files that are in use |
| `max_gb_per_run` | Per-category size cap for one run |
| `enabled_by_default` | Whether the category is in the default config |
| `tier` | Must be `1` |

## Template rules

- A template must start with `${VAR}` where VAR is one of `TEMP`,
  `LOCALAPPDATA`, `APPDATA`, `SystemRoot`, `ProgramData`.
- No `**`, no drive letters, no `..` segments.
- `*` is allowed as a whole path segment (one profile directory, e.g.
  `User Data\*\Cache`) or inside the final file-name segment
  (e.g. `thumbcache_*.db`).
- No template may contain a user-data fragment: `\Documents`, `\Downloads`,
  `\Desktop`, `\Pictures`, `\Videos`, `\Music`, `Cookies`, `Login Data`,
  `Local Storage`, `IndexedDB`, `Session Storage`.
- Categories whose id starts with `temp_` must set `min_age_days >= 7`.
- Category ids are unique; every category has at least one path.

Validation happens in `src-tauri/src/manifest.rs` at load time and is
property-tested in `src-tauri/src/safety_tests.rs`. Adding a category means
adding its root anchor to the allowlist in Appendix A of
[BLUEPRINT.md](BLUEPRINT.md) and to these tests in the same change.
