//! Safety property tests (blueprint §11.3) — the CI gate for the autonomous
//! engine's invariants. These tests read the embedded manifest and the
//! engine's own sources and fail the build if an invariant is violated.

use std::collections::HashMap;

use crate::manifest::{expand_template, Manifest, ALLOWED_VARS, USER_DATA_BLOCKLIST};

fn dummy_env() -> HashMap<String, String> {
    HashMap::from([
        ("TEMP".into(), r"C:\Sandbox\Temp".into()),
        ("LOCALAPPDATA".into(), r"C:\Sandbox\Local".into()),
        ("APPDATA".into(), r"C:\Sandbox\Roaming".into()),
        ("SystemRoot".into(), r"C:\Windows".into()),
        ("ProgramData".into(), r"C:\ProgramData".into()),
    ])
}

/// §11.3.1 — every manifest template expands only under a known safe root:
/// the value of one of the allowlisted environment variables.
#[test]
fn manifest_boundary_all_templates_stay_under_safe_roots() {
    let m = Manifest::embedded().unwrap();
    let env = dummy_env();
    let roots: Vec<String> = ALLOWED_VARS
        .iter()
        .map(|v| env[*v].to_lowercase())
        .collect();
    for c in &m.categories {
        for t in &c.paths {
            let expanded = expand_template(t, &env).expect("template must expand under dummy env");
            let lower = expanded.to_lowercase();
            assert!(
                roots
                    .iter()
                    .any(|r| lower == *r || lower.starts_with(&format!("{r}\\"))),
                "template {t} expands outside the safe roots: {expanded}"
            );
        }
    }
}

/// §11.3.5 — no Tier 1 template names a user-data location.
#[test]
fn manifest_names_no_user_data() {
    let m = Manifest::embedded().unwrap();
    for c in &m.categories {
        for t in &c.paths {
            let lower = t.to_lowercase();
            for frag in USER_DATA_BLOCKLIST {
                assert!(
                    !lower.contains(frag),
                    "template {t} contains blocked fragment {frag:?}"
                );
            }
        }
    }
}

/// §11.3.4 — temp categories keep a conservative minimum age.
#[test]
fn temp_categories_keep_conservative_age() {
    let m = Manifest::embedded().unwrap();
    for c in &m.categories {
        if c.id.starts_with("temp_") {
            assert!(
                c.min_age_days >= 7,
                "{} has min_age_days {}",
                c.id,
                c.min_age_days
            );
        }
        assert!(c.tier == 1, "{} is not tier 1", c.id);
        assert!(c.max_gb_per_run >= 1, "{} has no per-run cap", c.id);
    }
}

/// §11.3.2 — the autonomous engine contains no direct deletion and no
/// registry access. Quarantine movement is the only way files leave their
/// location; permanent deletion exists only in quarantine's purge.
#[test]
fn autonomous_engine_has_no_direct_delete_or_registry_access() {
    // Scan production code only: everything before the test module.
    let prod = |src: &'static str| src.split("#[cfg(test)]").next().unwrap_or(src);
    let cleaner = prod(include_str!("cleaner.rs"));
    let manifest = prod(include_str!("manifest.rs"));
    for (name, src) in [("cleaner.rs", cleaner), ("manifest.rs", manifest)] {
        for banned in [
            "remove_file",
            "remove_dir",
            "winreg",
            "RegSetValue",
            "RegDelete",
        ] {
            assert!(!src.contains(banned), "{name} must not contain {banned}");
        }
    }
    // Sanity: the quarantine module is where purge deletion legitimately lives.
    assert!(include_str!("quarantine.rs").contains("purge_expired"));
}

/// The engine's kill switch and dry-run phase are wired to config defaults:
/// autonomous starts off, dry-run comes first.
#[test]
fn engine_defaults_are_safe() {
    let cfg = crate::appconfig::AppConfig::default();
    assert!(!cfg.autonomous.enabled);
    assert!(!cfg.dry_run_phase_complete());
    assert_eq!(cfg.quarantine.retention_days, 7);
}
