//! The embedded Tier 1 cleaner manifest: the *only* list of locations the
//! autonomous engine may ever touch.
//!
//! Safety contract (blueprint §§3, 7.1, 11.3):
//! - The manifest is embedded at build time (`include_str!`); the autonomous
//!   engine never loads a manifest from disk.
//! - Its `checksum` field is the SHA-256 of the canonical JSON of the
//!   `categories` block and is verified on every load.
//! - Every path is an explicit `${VAR}` template. No `**`, no drive roots,
//!   no `..`, and `*` only as a whole path segment or inside the final
//!   file-name segment.
//! - No template may name a user-data location (blocklist below).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::sha256::sha256_hex;

pub const MANIFEST_JSON: &str = include_str!("../manifest/cleaner-manifest.json");

/// Environment variables a template may be anchored at. Anything else is
/// rejected — an unknown anchor can never widen the engine's reach.
pub const ALLOWED_VARS: &[&str] = &[
    "TEMP",
    "LOCALAPPDATA",
    "APPDATA",
    "SystemRoot",
    "ProgramData",
];

/// Path fragments that must never appear in a Tier 1 template (blueprint §11.3.5).
pub const USER_DATA_BLOCKLIST: &[&str] = &[
    "\\documents",
    "\\downloads",
    "\\desktop",
    "\\pictures",
    "\\videos",
    "\\music",
    "cookies",
    "login data",
    "local storage",
    "indexeddb",
    "session storage",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Category {
    pub id: String,
    pub display_name: String,
    pub paths: Vec<String>,
    pub min_age_days: u64,
    pub requires_admin: bool,
    pub skip_locked: bool,
    pub max_gb_per_run: u64,
    pub enabled_by_default: bool,
    pub tier: u32,
}

impl Category {
    pub fn max_bytes(&self) -> u64 {
        self.max_gb_per_run.saturating_mul(1024 * 1024 * 1024)
    }

    pub fn min_age(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.min_age_days.saturating_mul(86_400))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub manifest_version: String,
    pub checksum: String,
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Parse(String),
    UnsupportedSchema(u32),
    ChecksumMismatch,
    EmptyCategories,
    DuplicateId(String),
    NotTier1(String),
    EmptyPaths(String),
    BadTemplate {
        category: String,
        template: String,
        why: &'static str,
    },
    BlockedFragment {
        category: String,
        template: String,
    },
    TempAgeTooLow(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "manifest does not parse: {e}"),
            Self::UnsupportedSchema(v) => write!(f, "unsupported manifest schema_version {v}"),
            Self::ChecksumMismatch => write!(f, "manifest checksum mismatch — refusing to run"),
            Self::EmptyCategories => write!(f, "manifest has no categories"),
            Self::DuplicateId(id) => write!(f, "duplicate category id {id}"),
            Self::NotTier1(id) => write!(f, "category {id} is not tier 1"),
            Self::EmptyPaths(id) => write!(f, "category {id} has no paths"),
            Self::BadTemplate {
                category,
                template,
                why,
            } => {
                write!(
                    f,
                    "category {category}: template {template:?} rejected: {why}"
                )
            }
            Self::BlockedFragment { category, template } => {
                write!(
                    f,
                    "category {category}: template {template:?} names a user-data location"
                )
            }
            Self::TempAgeTooLow(id) => {
                write!(f, "category {id}: temp categories need min_age_days >= 7")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

/// Serializes a JSON value canonically: object keys sorted at every
/// depth, no whitespace, scalars in serde_json's normal form. The
/// output matches what `serde_json::to_string` produces for a value
/// held in sorted-key maps — which is how the manifest's stored
/// checksum was authored — without relying on that map behavior.
fn canonical_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = String::from("{");
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                out.push_str(&canonical_json(&map[*k]));
            }
            out.push('}');
            out
        }
        serde_json::Value::Array(items) => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json(item));
            }
            out.push(']');
            out
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

impl Manifest {
    /// Parses and fully validates a manifest document.
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| ManifestError::Parse(e.to_string()))?;
        // Checksum over the canonical form of the categories block:
        // object keys sorted, no whitespace — produced by
        // `canonical_json` below, explicitly, so the checksum's meaning
        // never depends on serde_json's map implementation (a future
        // dependency enabling its `preserve_order` feature would
        // otherwise change the bytes under the stored checksum and
        // disable the engine).
        let categories = value
            .get("categories")
            .ok_or_else(|| ManifestError::Parse("no categories".into()))?;
        let canonical = canonical_json(categories);
        let m: Manifest =
            serde_json::from_value(value).map_err(|e| ManifestError::Parse(e.to_string()))?;
        if m.checksum != sha256_hex(canonical.as_bytes()) {
            return Err(ManifestError::ChecksumMismatch);
        }
        m.validate()?;
        Ok(m)
    }

    /// Loads the build-embedded manifest.
    pub fn embedded() -> Result<Self, ManifestError> {
        Self::parse(MANIFEST_JSON)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != 1 {
            return Err(ManifestError::UnsupportedSchema(self.schema_version));
        }
        if self.categories.is_empty() {
            return Err(ManifestError::EmptyCategories);
        }
        let mut seen = std::collections::HashSet::new();
        for c in &self.categories {
            if !seen.insert(&c.id) {
                return Err(ManifestError::DuplicateId(c.id.clone()));
            }
            if c.tier != 1 {
                return Err(ManifestError::NotTier1(c.id.clone()));
            }
            if c.paths.is_empty() {
                return Err(ManifestError::EmptyPaths(c.id.clone()));
            }
            if c.id.starts_with("temp_") && c.min_age_days < 7 {
                return Err(ManifestError::TempAgeTooLow(c.id.clone()));
            }
            for t in &c.paths {
                validate_template(&c.id, t)?;
            }
        }
        Ok(())
    }

    /// Single-category lookup for the M2 settings UI and tests.
    #[allow(dead_code)]
    pub fn category(&self, id: &str) -> Option<&Category> {
        self.categories.iter().find(|c| c.id == id)
    }
}

/// Validates one path template against the manifest contract rules.
pub fn validate_template(category: &str, template: &str) -> Result<(), ManifestError> {
    let bad = |why: &'static str| ManifestError::BadTemplate {
        category: category.into(),
        template: template.into(),
        why,
    };
    if template.contains("**") {
        return Err(bad("recursive ** globs are not allowed"));
    }
    if template.len() >= 2 && template.as_bytes()[1] == b':' {
        return Err(bad(
            "drive-letter paths are not allowed; anchor at an environment variable",
        ));
    }
    if template.contains("..") {
        return Err(bad("parent-directory segments are not allowed"));
    }
    // Must open with ${VAR} for an allowlisted variable.
    let Some(rest) = template.strip_prefix("${") else {
        return Err(bad("template must start with ${VAR}"));
    };
    let Some(close) = rest.find('}') else {
        return Err(bad("unclosed ${VAR} anchor"));
    };
    let var = &rest[..close];
    if !ALLOWED_VARS.contains(&var) {
        return Err(bad("environment variable is not on the allowlist"));
    }
    // Segments after the anchor: `*` as a whole segment is fine anywhere;
    // a `*` inside a segment is fine only in the final (file-name) segment.
    let tail = &rest[close + 1..];
    let segments: Vec<&str> = tail.split('\\').filter(|s| !s.is_empty()).collect();
    for (i, seg) in segments.iter().enumerate() {
        if seg.contains('*') && *seg != "*" && i + 1 != segments.len() {
            return Err(bad(
                "wildcards inside a segment are only allowed in the final file-name segment",
            ));
        }
    }
    let lower = template.to_lowercase();
    if USER_DATA_BLOCKLIST.iter().any(|b| lower.contains(b)) {
        return Err(ManifestError::BlockedFragment {
            category: category.into(),
            template: template.into(),
        });
    }
    Ok(())
}

/// Expands a `${VAR}` template against an explicit environment map (tests
/// inject a fake map; production builds one from the real environment).
/// Returns `None` when a variable is missing — a missing variable disables
/// the path instead of widening it.
pub fn expand_template(template: &str, env: &HashMap<String, String>) -> Option<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find('}')?;
        out.push_str(env.get(&after[..end])?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Builds the environment map for template expansion from the real process
/// environment. `SystemRoot` falls back to `WINDIR`, TEMP to the OS temp dir.
pub fn real_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    for var in ALLOWED_VARS {
        if let Ok(v) = std::env::var(var) {
            env.insert(var.to_string(), v);
        }
    }
    if !env.contains_key("SystemRoot") {
        if let Ok(v) = std::env::var("WINDIR") {
            env.insert("SystemRoot".to_string(), v);
        }
    }
    env.entry("TEMP".to_string())
        .or_insert_with(|| std::env::temp_dir().to_string_lossy().into_owned());
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_sorts_keys_at_every_depth() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"b":1,"a":{"d":4,"c":[3,{"z":0,"y":1}]}}"#).unwrap();
        assert_eq!(
            canonical_json(&v),
            r#"{"a":{"c":[3,{"y":1,"z":0}],"d":4},"b":1}"#
        );
    }

    #[test]
    fn embedded_manifest_is_valid() {
        let m = Manifest::embedded().expect("embedded manifest must validate");
        assert_eq!(m.schema_version, 1);
        assert_eq!(m.categories.len(), 9);
        assert!(m.category("temp_user").is_some());
    }

    #[test]
    fn checksum_tampering_is_caught() {
        let tampered = MANIFEST_JSON.replacen("User Temp Files", "User Temp Filez", 1);
        assert_eq!(
            Manifest::parse(&tampered).err(),
            Some(ManifestError::ChecksumMismatch)
        );
    }

    #[test]
    fn template_rules_reject_dangerous_shapes() {
        assert!(validate_template("x", r"${TEMP}\..\Documents").is_err());
        assert!(validate_template("x", r"C:\Temp\**").is_err());
        assert!(validate_template("x", r"${HOME}\Temp").is_err()); // not allowlisted
        assert!(validate_template("x", r"${LOCALAPPDATA}\Google\Chrome\User Data\*\Cache").is_ok());
        assert!(validate_template(
            "x",
            r"${LOCALAPPDATA}\Microsoft\Windows\Explorer\thumbcache_*.db"
        )
        .is_ok());
        // Wildcard inside a non-final segment is rejected.
        assert!(validate_template("x", r"${LOCALAPPDATA}\Prof*le\Cache").is_err());
    }

    #[test]
    fn user_data_fragments_are_blocked() {
        assert!(matches!(
            validate_template("x", r"${LOCALAPPDATA}\App\Cookies"),
            Err(ManifestError::BlockedFragment { .. })
        ));
    }

    #[test]
    fn expansion_uses_only_the_given_map() {
        let mut env = HashMap::new();
        env.insert(
            "TEMP".to_string(),
            r"C:\Users\a\AppData\Local\Temp".to_string(),
        );
        assert_eq!(
            expand_template(r"${TEMP}\x", &env).as_deref(),
            Some(r"C:\Users\a\AppData\Local\Temp\x")
        );
        assert_eq!(expand_template(r"${LOCALAPPDATA}\x", &env), None);
    }
}
