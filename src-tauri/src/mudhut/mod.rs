// mudhut/mod.rs
//
// The shell-out boundary to the Mud Hut CLI (the Adobe-app installer engine),
// mirroring neutron/mod.rs. Collider talks to Mud Hut only through here.
//
//   - `--json` is a TOP-LEVEL flag, before the subcommand: `mudhut --json <cmd>`
//   - stdout may be NDJSON (progress `note`s then a terminal object); the LAST
//     non-empty line is the terminal `result` (or `error`) object
//   - an `{"event":"error","message":..}` object (or non-zero exit) is a failure

use std::process::Command;

use anyhow::{anyhow, Context};
use serde_json::Value;

/// Build a `mudhut` command with a cleaned environment (same reasoning as the
/// neutron wrapper: a packaged AppImage's PYTHON*/LD_* vars break child tools).
fn clean_command() -> Command {
    let mut cmd = Command::new("mudhut");
    for var in [
        "PYTHONHOME", "PYTHONPATH", "PYTHONDONTWRITEBYTECODE", "PYTHONNOUSERSITE",
        "LD_LIBRARY_PATH", "LD_PRELOAD", "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0", "GDK_PIXBUF_MODULE_FILE", "GDK_PIXBUF_MODULEDIR",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

/// Run a Mud Hut CLI command expecting a terminal JSON object on stdout.
pub fn run_json(args: &[&str]) -> anyhow::Result<Value> {
    let output = clean_command()
        .arg("--json")
        .args(args)
        .output()
        .context("failed to spawn `mudhut` — is Mud Hut installed and on PATH?")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout.lines().rev().find(|l| !l.trim().is_empty());
    let json: Value = match last.and_then(|l| serde_json::from_str(l).ok()) {
        Some(v) => v,
        None => {
            let code = output.status.code().unwrap_or(-1);
            let err = String::from_utf8_lossy(&output.stderr);
            let e = err.trim();
            return Err(anyhow!(
                "mudhut returned non-JSON (exit {code}). stderr: {}",
                if e.is_empty() { "<empty>" } else { e }
            ));
        }
    };

    if json.get("event").and_then(|e| e.as_str()) == Some("error") {
        let msg = json.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
        return Err(anyhow!("{msg}"));
    }
    Ok(json)
}

/// Begin Adobe sign-in: mint the QR + login link.
/// Returns `{ event:"result", url, qr, request_id, device_id }`.
pub fn auth_begin() -> anyhow::Result<Value> {
    run_json(&["auth", "begin"])
}

/// Poll the sign-in once; runs the token exchange when the user has signed in.
/// Returns `{ event:"result", status, retry_interval, exchange? }`.
pub fn auth_poll(request_id: &str, device_id: &str) -> anyhow::Result<Value> {
    run_json(&["auth", "poll", "--request-id", request_id, "--device-id", device_id])
}
