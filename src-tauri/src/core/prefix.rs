// core/prefix.rs
//
// Prefix detection and path resolution. Delegates entirely to the Neutron CLI
// (`neutron --json prefix info <path>`) — Collider stores the resolved pointers
// but never computes them. `documents_real` is the symlink-resolved target the
// hwmux daemon used to watch; hwmux is gone, so it is informational and may be null.
//
// Struct reconciled against the shipped CLI (COLLIDER_INTEGRATION_HANDOFF.md):
// stable fields are typed directly; the provisional `neutron_stack` is kept as
// a loose Value so a schema change after the minimal-stack cleanup won't break us.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixInfo {
    // --- Stable fields (safe to rely on) ---
    pub valid: bool,
    pub wineprefix: PathBuf,
    pub drive_c: PathBuf,
    pub user: String,
    /// The symlink path inside the prefix. Do NOT watch this with inotify.
    pub documents_symlink: PathBuf,
    /// The resolved real target, or None when the in-prefix Documents link is
    /// missing or points somewhere this user cannot reach (the CLI sends null).
    /// Nothing reads it since hwmux went away; it must never block a launch
    /// (#29: "resolving: invalid type: null, expected path string").
    #[serde(default)]
    pub documents_real: Option<PathBuf>,
    /// Live Debug Database.txt the display fix writes to (CLI globs the
    /// versioned dir and excludes .bak — we don't hardcode the version).
    pub debug_database: Option<PathBuf>,
    pub display_fix_applied: bool,
    /// False when Premiere has no display fix to apply: Premiere 2026 runs on Adobe's own DirectX display, so its
    /// card shows no display-fix pill. A CLI from before beta.34 doesn't send it: true, the card shows the pill as before.
    #[serde(default = "display_fix_needed_default")]
    pub display_fix_needed: bool,

    // --- Provisional (do NOT freeze assumptions on this) ---
    /// dxvk/vkd3d versions are null until the build is pinned (hardening §5.2).
    /// Kept loose on purpose; carries "_schema": "provisional-v0".
    #[serde(default)]
    pub neutron_stack: Option<Value>,
}

fn display_fix_needed_default() -> bool {
    true
}

impl PrefixInfo {
    /// Detect + resolve a prefix via the Neutron CLI. An invalid prefix comes
    /// back as exit 2, surfaced as a NeutronError by the wrapper — callers should
    /// present that to the user rather than treating it as a crash.
    pub fn detect(prefix_path: &PathBuf) -> anyhow::Result<Self> {
        let v = crate::neutron::prefix_info(Some(
            prefix_path.to_str().ok_or_else(|| {
                anyhow::anyhow!("prefix path is not valid UTF-8")
            })?,
        ))?;
        let parsed: PrefixInfo = serde_json::from_value(v)?;
        Ok(parsed)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_documents_real_still_parses() {
        let v = serde_json::json!({
            "valid": true, "wineprefix": "/p", "drive_c": "/p/drive_c", "user": "u",
            "documents_symlink": "/p/drive_c/users/u/Documents", "documents_real": null,
            "debug_database": null, "display_fix_applied": false, "neutron_stack": null,
        });
        let info: PrefixInfo = serde_json::from_value(v).expect("a null documents_real must parse");
        assert!(info.valid && info.documents_real.is_none());
        assert!(info.display_fix_needed, "an older CLI sends no display_fix_needed: keep the pill");
    }

    #[test]
    fn premiere_2026_needs_no_display_fix() {
        let v = serde_json::json!({
            "valid": true, "wineprefix": "/p", "drive_c": "/p/drive_c", "user": "u",
            "documents_symlink": "/p/drive_c/users/u/Documents", "documents_real": null,
            "debug_database": null, "display_fix_applied": true, "display_fix_needed": false,
            "neutron_stack": null,
        });
        let info: PrefixInfo = serde_json::from_value(v).expect("parses");
        assert!(!info.display_fix_needed);
    }
}
