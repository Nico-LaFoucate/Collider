// mudhut/mod.rs
//
// The shell-out boundary to the Mud Hut CLI (the Adobe-app installer engine),
// mirroring neutron/mod.rs. Collider talks to Mud Hut only through here.
//
//   - `--json` is a TOP-LEVEL flag, before the subcommand: `mudhut --json <cmd>`
//   - stdout may be NDJSON (progress `note`s then a terminal object); the LAST
//     non-empty line is the terminal `result` (or `error`) object
//   - an `{"event":"error","message":..}` object (or non-zero exit) is a failure

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

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

/// The installable-app catalog (`mudhut apps [--source]`). Returns
/// `{ apps: [ { id, name, sap, present, dir } ] }`.
pub fn apps(source: Option<&str>) -> anyhow::Result<Value> {
    match source {
        Some(s) => run_json(&["apps", "--source", s]),
        None => run_json(&["apps"]),
    }
}

/// Install an app, STREAMING each NDJSON event to `on_event` as it arrives (Mud
/// Hut's `install` emits many `progress`/`note` lines before the terminal object).
/// Returns the terminal `result` object, or an error (from an `error` event or a
/// non-zero exit). Non-JSON lines (interleaved child-process output) are ignored.
pub fn install_stream(
    app: &str,
    method: &str,
    prefix: &str,
    source: Option<&str>,
    mut on_event: impl FnMut(Value),
) -> anyhow::Result<Value> {
    let mut cmd = clean_command();
    cmd.arg("--json")
        .arg("install")
        .arg(app)
        .arg("--method")
        .arg(method)
        .arg("--prefix")
        .arg(prefix);
    if let Some(s) = source {
        cmd.arg("--source").arg(s);
    }
    // stdout = the NDJSON stream we parse; stderr = child noise -> inherit (not captured).
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("failed to spawn `mudhut` — is Mud Hut on PATH?")?;
    let stdout = child.stdout.take().context("mudhut produced no stdout")?;

    let mut terminal: Option<Value> = None;
    let mut error_msg: Option<String> = None;
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    loop {
        // Bytes, not `lines()`: a child's output can be non-UTF-8 (Wine's `reg` on a French
        // system prints in code page 850), and `lines()` fails on it. That ended the read, closed
        // the pipe, and Mud Hut died on its next write ("Broken pipe") mid-install. Neutron#11.
        buf.clear();
        if reader.read_until(b'\n', &mut buf).context("reading mudhut output")? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&buf);
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(t) else {
            continue; // interleaved wine output — skip
        };
        match v.get("event").and_then(|e| e.as_str()) {
            Some("result") => terminal = Some(v.clone()),
            Some("error") => {
                error_msg = Some(
                    v.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error").to_string(),
                );
            }
            _ => {}
        }
        on_event(v);
    }

    let status = child.wait().context("waiting for mudhut install")?;
    if let Some(msg) = error_msg {
        return Err(anyhow!("{msg}"));
    }
    if !status.success() {
        return Err(anyhow!("mudhut install exited with code {}", status.code().unwrap_or(-1)));
    }
    terminal.ok_or_else(|| anyhow!("mudhut install finished without a result"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A fake `mudhut` on PATH whose stdout carries the line Wine's `reg add` prints on a French
    /// system (GitHub Neutron#11): Windows code page 850, not UTF-8. Collider must skip it like
    /// any other non-JSON line and keep reading to the terminal `result`.
    #[test]
    fn install_stream_survives_non_utf8_child_output() {
        let dir = std::env::temp_dir().join(format!("collider-fake-mudhut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("mudhut");
        std::fs::write(
            &fake,
            "#!/bin/sh\n\
             echo '{\"event\":\"note\",\"message\":\"preparing the prefix\"}'\n\
             printf 'reg\\377: L'\"'\"'op\\202ration s'\"'\"'est termin\\202e avec succ\\212s\\r\\n'\n\
             echo '{\"event\":\"progress\",\"pct\":50}'\n\
             echo '{\"event\":\"result\",\"apps\":[\"lightroom\"]}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default());
        std::env::set_var("PATH", path);

        let mut seen = Vec::new();
        let r = install_stream("lightroom", "iso", "/nonexistent", None, |v| {
            seen.push(v["event"].as_str().unwrap_or("").to_string())
        });
        std::fs::remove_dir_all(&dir).ok();
        let result = r.expect("install_stream must not fail on a non-UTF-8 line");
        assert_eq!(result["event"], "result");
        assert_eq!(seen, ["note", "progress", "result"]);
    }
}
