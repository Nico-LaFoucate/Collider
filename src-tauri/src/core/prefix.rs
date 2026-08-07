// core/prefix.rs
//
// Prefix detection and path resolution. Delegates entirely to the Neutron CLI
// (`neutron --json prefix info <path>`) — Collider stores the resolved pointers
// but never computes them. This is where the §1.2 symlink lesson is enforced:
// `documents_real` is the symlink-resolved target, and it (not the symlinked
// prefix path) is what the hwmux daemon must watch.
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
    /// The resolved real target. Watch THIS for hwmux; carry verbatim into
    /// `hwmux start --watch`. See handoff "path-resolution flow".
    pub documents_real: PathBuf,
    /// Live Debug Database.txt the display fix writes to (CLI globs the
    /// versioned dir and excludes .bak — we don't hardcode the version).
    pub debug_database: Option<PathBuf>,
    pub display_fix_applied: bool,

    // --- Provisional (do NOT freeze assumptions on this) ---
    /// dxvk/vkd3d versions are null until the build is pinned (hardening §5.2).
    /// Kept loose on purpose; carries "_schema": "provisional-v0".
    #[serde(default)]
    pub neutron_stack: Option<Value>,
}

impl PrefixInfo {
    /// Detect + resolve a prefix via the Neutron CLI. An invalid prefix comes
    /// back as exit 2, surfaced as a NeutronError by the wrapper — callers should
    /// present that to the user rather than treating it as a crash.
    pub fn detect(prefix_path: &PathBuf) -> anyhow::Result<Self> {
        let v = crate::neutron::prefix_info(
            prefix_path.to_str().ok_or_else(|| {
                anyhow::anyhow!("prefix path is not valid UTF-8")
            })?,
        )?;
        let parsed: PrefixInfo = serde_json::from_value(v)?;
        Ok(parsed)
    }

}
