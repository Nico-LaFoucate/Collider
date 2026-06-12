// neutron/mod.rs
//
// The shell-out boundary to the Neutron CLI. This is the ONLY place Collider
// talks to the engine (ADR-003). Everything goes through here so the contract
// stays in one spot and is easy to mock in tests.
//
// Reconciled against the shipped CLI (COLLIDER_INTEGRATION_HANDOFF.md):
//   - `--json` is a TOP-LEVEL flag, before the subcommand: `neutron --json <cmd>`
//   - stdout = exactly one JSON object (parse); stderr = `[neutron] ...` logs (ignore)
//   - non-zero exits ALSO return a JSON error object with `error_code` + `reason`
//   - software mode is an honest v0 gap (exit 4); we do not offer it

use std::process::Command;
use serde_json::Value;
use anyhow::{Context, anyhow};

/// A Neutron CLI failure carrying the engine's structured error detail.
#[derive(Debug)]
pub struct NeutronError {
    pub code: i32,
    pub reason: String,
}
impl std::fmt::Display for NeutronError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "neutron error {}: {}", self.code, self.reason)
    }
}
impl std::error::Error for NeutronError {}

/// Run a Neutron CLI command expecting JSON on stdout. Prepends the top-level
/// `--json` flag. On non-zero exit, parses the error object so the UI can show
/// the engine's own `reason` (e.g. "Premiere is running; cannot edit prefs")
/// rather than a generic message.
pub fn run_json(args: &[&str]) -> anyhow::Result<Value> {
    let output = Command::new("neutron")
        .arg("--json")
        .args(args)
        .output()
        .context("failed to spawn `neutron` — is the Neutron CLI installed and on PATH?")?;

    // The CLI emits a JSON object on stdout for BOTH success and failure.
    let json: Value = serde_json::from_slice(&output.stdout)
        .context("neutron returned non-JSON on stdout")?;

    match output.status.code() {
        Some(0) => Ok(json),
        Some(code) => {
            // Exit codes (stable, finalized in the handoff):
            //   1 generic | 2 bad prefix | 3 Premiere running
            //   4 dependency missing OR feature not available in v0
            let reason = json
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("unknown neutron failure")
                .to_string();
            Err(NeutronError { code, reason }.into())
        }
        None => Err(anyhow!("neutron terminated by signal")),
    }
}

/// Launch Premiere (GPU is the only mode in v0) and return its PID so the
/// daemon lifecycle can be tied to it. We never pass `--software` — it's an
/// honest v0 gap that returns exit 4 (see handoff "Things Collider should NOT do").
pub fn launch_premiere(prefix: &str, project: Option<&str>) -> anyhow::Result<u32> {
    let mut args = vec!["launch", "premiere", "--prefix", prefix];
    if let Some(p) = project {
        // Project paths are Windows-style inside the prefix, e.g.
        // C:\users\nuck\Documents\test.prproj — Collider passes them through verbatim.
        args.push("--project");
        args.push(p);
    }
    let v = run_json(&args)?;
    pid_from(&v)
}

/// Resolve a prefix's paths. `documents_real` (symlink-resolved) is the value
/// Collider carries verbatim into `hwmux start --watch` — Collider never resolves
/// the symlink itself (the §1.2 lesson lives engine-side).
pub fn prefix_info(prefix: &str) -> anyhow::Result<Value> {
    run_json(&["prefix", "info", prefix])
}

/// Apply the DS.DisableDirectXDisplay fix. Idempotent. Exit 3 if Premiere runs.
pub fn apply_display_fix(prefix: &str) -> anyhow::Result<Value> {
    run_json(&["prefix", "apply-display-fix", prefix])
}

/// Health surface. Render `checks` generically in the UI — do NOT hardcode the
/// list (it's provisional until the minimal-stack cleanup lands).
pub fn doctor(prefix: Option<&str>) -> anyhow::Result<Value> {
    match prefix {
        Some(p) => run_json(&["doctor", "--prefix", p]),
        None => run_json(&["doctor"]),
    }
}

/// Start the hwmux daemon. `watch` MUST be the documents_real value from
/// prefix_info — never a guess. Returns the daemon PID to supervise.
pub fn hwmux_start(watch: &str) -> anyhow::Result<u32> {
    let v = run_json(&["hwmux", "start", "--watch", watch])?;
    pid_from(&v)
}

/// Stop the hwmux daemon. Benign if nothing is running.
pub fn hwmux_stop() -> anyhow::Result<Value> {
    run_json(&["hwmux", "stop"])
}

fn pid_from(v: &Value) -> anyhow::Result<u32> {
    v.get("pid")
        .and_then(|p| p.as_u64())
        .map(|p| p as u32)
        .ok_or_else(|| anyhow!("neutron did not return a pid"))
}
