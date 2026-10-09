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
//   - `hwmux` is GONE (engine commit 678cb60) — the muxing workaround it drove was replaced by a
//     real fix in Wine. Do not reintroduce a call to it.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
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
/// Build a `neutron` command with a CLEANED environment.
///
/// When Collider runs as a packaged AppImage, the bundle sets variables that
/// point Python and the dynamic linker at the AppImage's own internals
/// (PYTHONHOME, PYTHONPATH, LD_LIBRARY_PATH, ...). `neutron` is a Python script,
/// so an inherited PYTHONHOME makes its interpreter look for the standard
/// library in the wrong place and die at startup with "No module named
/// 'encodings'". We remove these so the child uses the SYSTEM Python and
/// libraries — exactly as it would if the user ran `neutron` in their shell.
fn clean_command() -> Command {
    clean_command_for("neutron")
}

fn clean_command_for(bin: &str) -> Command {
    let mut cmd = Command::new(bin);
    for var in [
        "PYTHONHOME",
        "PYTHONPATH",
        "PYTHONDONTWRITEBYTECODE",
        "PYTHONNOUSERSITE",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0",
        "GDK_PIXBUF_MODULE_FILE",
        "GDK_PIXBUF_MODULEDIR",
        // NEUTRON: startup-notification identity. Without these the launched Adobe app inherits
        // COLLIDER's activation token, so the compositor attributes the new window to Collider and
        // it shows Collider's icon in the dock. The window should stand on its own identity (Wine
        // advertises the lowercased exe basename as the Wayland app_id) and be matched to its own
        // .desktop file, not inherit ours.
        "DESKTOP_STARTUP_ID",
        "XDG_ACTIVATION_TOKEN",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

pub fn run_json(args: &[&str]) -> anyhow::Result<Value> {
    run_json_inner(args, true)
}

/// Like `run_json`, but a non-zero exit is NOT automatically a failure.
///
/// Some commands report STATUS through the exit code rather than failure. `doctor` returns 1 for
/// "unhealthy" — a health check that ran fine and found problems — and its payload carries
/// `healthy` + `checks`. Treating that as a command failure is what blanked Collider's ENTIRE
/// Apps view for build testers on 2026-08-08: `refresh()` threw at the doctor call, so
/// `listApps` never ran and the library rendered empty behind an opaque
/// "neutron error 1: unknown neutron failure".
///
/// A payload carrying a structured error (`reason`) is still surfaced as an error.
fn run_json_status(args: &[&str]) -> anyhow::Result<Value> {
    run_json_inner(args, false)
}

fn run_json_inner(args: &[&str], nonzero_is_failure: bool) -> anyhow::Result<Value> {
    let output = clean_command()
        .arg("--json")
        .args(args)
        .output()
        .context("failed to spawn `neutron` — is the Neutron CLI installed and on PATH?")?;

    // The CLI emits a JSON object on stdout for BOTH success and failure.
    // If parsing fails, surface exactly what we got — the exit code, the raw
    // stdout, and the stderr — so a failure is diagnosable instead of opaque.
    let json: Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => {
            let code = output.status.code().unwrap_or(-1);
            let out = String::from_utf8_lossy(&output.stdout);
            let err = String::from_utf8_lossy(&output.stderr);
            let out_snip = out.trim();
            let err_snip = err.trim();
            return Err(anyhow!(
                "neutron returned non-JSON (exit {code}). stdout: {}. stderr: {}",
                if out_snip.is_empty() { "<empty>" } else { out_snip },
                if err_snip.is_empty() { "<empty>" } else { err_snip },
            ));
        }
    };

    match output.status.code() {
        Some(0) => Ok(json),
        Some(code) => {
            // Exit codes (stable, finalized in the handoff):
            //   1 generic | 2 bad prefix | 3 Premiere running
            //   4 dependency missing OR feature not available in v0
            let reason = json.get("reason").and_then(|r| r.as_str());
            // A status-carrying exit code with a real payload is not a failure — see
            // run_json_status.
            if !nonzero_is_failure && reason.is_none() {
                return Ok(json);
            }
            Err(NeutronError {
                code,
                reason: reason.unwrap_or("unknown neutron failure").to_string(),
            }
            .into())
        }
        None => Err(anyhow!("neutron terminated by signal")),
    }
}

/// Result of a launch: the PID to supervise, plus the engine's reported display
/// path (`wayland`/`x11`) and whether the patched Neutron wine was used. The
/// latter two are advisory (for UI badges / a misconfig warning).
#[derive(Debug, Clone)]
pub struct LaunchResult {
    pub pid: u32,
    pub display: String,
    pub neutron_wine: bool,
}

/// Launch any app in the catalog by id (GPU is the only mode in v0) and return
/// its PID so the session (and, for export apps, the daemon) can be tied to it.
/// `scale` (the primary monitor's display scale, detected cockpit-side) is passed
/// to the engine, which turns it into the right LogPixels; None => engine
/// auto-detects. We never pass `--software` — an honest v0 gap (exit 4).
pub fn launch_app(
    app_id: &str,
    prefix: &str,
    project: Option<&str>,
    scale: Option<f64>,
) -> anyhow::Result<LaunchResult> {
    let scale_str: String;
    let mut args = vec!["launch", app_id, "--prefix", prefix];
    if let Some(p) = project {
        // Project paths are Windows-style inside the prefix, e.g.
        // C:\users\nuck\Documents\test.prproj — Collider passes them through verbatim.
        args.push("--project");
        args.push(p);
    }
    if let Some(s) = scale {
        scale_str = format!("{s}");
        args.push("--scale");
        args.push(&scale_str);
    }
    let v = run_json(&args)?;
    Ok(LaunchResult {
        pid: pid_from(&v)?,
        display: v.get("display").and_then(|d| d.as_str()).unwrap_or("unknown").to_string(),
        neutron_wine: v.get("neutron_wine").and_then(|n| n.as_bool()).unwrap_or(false),
    })
}

/// The app catalog + install status for a prefix (`neutron apps`). Returns the
/// raw JSON object `{ "apps": [ { id, name, accent, export, installed, exe }, … ] }`
/// that powers Collider's per-app widget grid. Pure detection — launches nothing.
pub fn apps(prefix: &str) -> anyhow::Result<Value> {
    run_json(&["apps", "--prefix", prefix])
}

/// Resolve a prefix's paths. Pass None to let the ENGINE resolve its own default
/// ($HOME/.premiere2025) — every prefix argument in the CLI is `nargs="?"` with that
/// default, so Collider never has to know the path. Do not hardcode a default here:
/// a second copy of the engine's default is a source of truth that drifts.
///
/// On a path with no drive_c the CLI still returns JSON — `{valid:false, reason,
/// error_code:2}` — and exits 0, so callers get a structured answer, not an error.
pub fn prefix_info(prefix: Option<&str>) -> anyhow::Result<Value> {
    match prefix {
        Some(p) => run_json(&["prefix", "info", p]),
        None => run_json(&["prefix", "info"]),
    }
}

/// Apply the DS.DisableDirectXDisplay fix. Idempotent. Exit 3 if Premiere runs.
pub fn apply_display_fix(prefix: &str) -> anyhow::Result<Value> {
    run_json(&["prefix", "apply-display-fix", prefix])
}

/// Health surface. Render `checks` generically in the UI — do NOT hardcode the
/// list (it's provisional until the minimal-stack cleanup lands).
pub fn doctor(prefix: Option<&str>) -> anyhow::Result<Value> {
    // run_json_STATUS: doctor exits 1 to mean "unhealthy", which is information to render, not a
    // failure to propagate.
    match prefix {
        Some(p) => run_json_status(&["doctor", "--prefix", p]),
        None => run_json_status(&["doctor"]),
    }
}

/// Font verdict for a prefix (`neutron fonts check`). Read-only in the engine — it never starts
/// wine — so it is safe to run for every registered prefix on the Prefixes tab. Returns the raw
/// object `{status: good|warn|bad, summary, checks, repair, apps_running, note?}`; render
/// `summary` and colour by `status`. Exit 1 means "bad", which is a verdict, not a failure —
/// hence run_json_status, the same convention as doctor.
pub fn fonts_check(prefix: &str) -> anyhow::Result<Value> {
    run_json_status(&["fonts", "check", "--prefix", prefix])
}

/// Repair a prefix's fonts (`neutron fonts repair`): restore genuine Microsoft core fonts,
/// register everything staged in windows/Fonts, invalidate CoolType's cache, re-verify. The
/// engine refuses (exit 3, `reason`) while an Adobe app is running in the prefix, and (exit 1)
/// through a runtime that would re-stamp it — both surface as the engine's own reason. Collider
/// owns none of this logic; it is a button.
pub fn fonts_repair(prefix: &str) -> anyhow::Result<Value> {
    run_json(&["fonts", "repair", "--prefix", prefix])
}

/// Provision a prefix, streaming progress.
///
/// `neutron --json --progress prefix provision <p>` emits newline-delimited events
/// ({event:progress|note|error|result}) — the same NDJSON contract Mud Hut uses, so this reader is
/// the twin of mudhut::install_stream. The terminal object is tagged `event:"result"`.
///
/// ⚠️ `--progress` is OPT-IN in the engine and must stay that way: Mud Hut shells out to
/// `neutron prefix provision` with INHERITED stdout, so streaming by default would inject these
/// lines into Mud Hut's own event stream and this app would read our result as Mud Hut's.
pub fn provision_stream(prefix: &str, on_event: impl FnMut(Value)) -> anyhow::Result<Value> {
    run_stream(&["prefix", "provision", prefix], on_event)
}

/// `neutron setup` / `neutron update`, streaming progress: the same NDJSON contract as provision.
/// Setup may show the desktop's password dialog (pkexec) once, to turn on ntsync. When the CLI
/// is not installed yet (a first run from the Collider AppImage alone), it is fetched first.
pub fn setup_stream(update: bool, on_event: impl FnMut(Value)) -> anyhow::Result<Value> {
    let bin = if update { "neutron".to_string() } else { bootstrap_cli()? };
    run_stream_with(&bin, &[if update { "update" } else { "setup" }], on_event)
}

/// The CLI to run setup with: `neutron` on PATH, else the CLI from the neutron repo's latest
/// GitHub release, downloaded to ~/.local/share/neutron/bin/neutron and checked against that
/// release's SHA256SUMS. `neutron setup` then links it onto PATH.
fn bootstrap_cli() -> anyhow::Result<String> {
    let on_path = Command::new("sh").args(["-c", "command -v neutron"]).output()
        .map(|o| o.status.success()).unwrap_or(false);
    if on_path {
        return Ok("neutron".into());
    }
    let home = std::env::var("HOME").context("no HOME")?;
    let dir = std::path::Path::new(&home).join(".local/share/neutron/bin");
    std::fs::create_dir_all(&dir)?;
    let base = "https://github.com/Nico-LaFoucate/neutron/releases/latest/download";
    let part = dir.join("neutron.part");
    let get = |url: &str, out: &std::path::Path| -> anyhow::Result<()> {
        let st = Command::new("curl").args(["-fsSL", "--retry", "3", "-o"]).arg(out).arg(url)
            .status().context("curl is needed to download the Neutron CLI")?;
        if !st.success() { return Err(anyhow!("could not download {url}")); }
        Ok(())
    };
    get(&format!("{base}/neutron"), &part)?;
    let sums = dir.join("SHA256SUMS.part");
    get(&format!("{base}/SHA256SUMS"), &sums)?;
    let want = std::fs::read_to_string(&sums)?.lines()
        .filter_map(|l| l.split_once(char::is_whitespace))
        .find(|(_, n)| n.trim().trim_start_matches('*') == "neutron")
        .map(|(h, _)| h.to_lowercase())
        .context("the release lists no checksum for the CLI")?;
    let _ = std::fs::remove_file(&sums);
    let got = Command::new("sha256sum").arg(&part).output()?;
    let got = String::from_utf8_lossy(&got.stdout).split_whitespace().next().unwrap_or("").to_string();
    if got != want {
        let _ = std::fs::remove_file(&part);
        return Err(anyhow!("the downloaded Neutron CLI failed its checksum"));
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
    let dest = dir.join("neutron");
    std::fs::rename(&part, &dest)?;
    Ok(dest.to_string_lossy().into_owned())
}

/// `neutron uninstall --yes [--delete-prefixes]`. The confirmation is Collider's dialog.
pub fn uninstall(delete_prefixes: bool) -> anyhow::Result<Value> {
    let mut args = vec!["uninstall", "--yes"];
    if delete_prefixes { args.push("--delete-prefixes"); }
    run_json(&args)
}

/// Every piece's version (`neutron --version`), for Preferences and bug reports.
pub fn version() -> anyhow::Result<Value> {
    run_json(&["--version"])
}

/// The desktop's display scale as the engine sees it (`neutron display`): `{scale, output,
/// source, dpi, windows_step, warning}`. Collider has no detection of its own (DECISIONS C4b).
pub fn display() -> anyhow::Result<Value> {
    run_json(&["display"])
}

/// Install a window-button icon set (a folder of close/min/max/restore .ico) into a prefix, or
/// turn custom icons off with `dir = None`.
pub fn caption_icons(prefix: &str, dir: Option<(&str, &str)>) -> anyhow::Result<Value> {
    match dir {
        Some((path, name)) => run_json(&["theme", "icons", "--prefix", prefix, "--from", path,
                                         "--name", name]),
        None => run_json(&["theme", "icons", "--prefix", prefix, "--off"]),
    }
}

/// Run `neutron --json --progress <args>` and stream its NDJSON events to `on_event`. Returns the
/// terminal `result` object.
fn run_stream(args: &[&str], on_event: impl FnMut(Value)) -> anyhow::Result<Value> {
    run_stream_with("neutron", args, on_event)
}

fn run_stream_with(bin: &str, args: &[&str], mut on_event: impl FnMut(Value)) -> anyhow::Result<Value> {
    let mut cmd = clean_command_for(bin);
    cmd.arg("--json").arg("--progress").args(args);
    // stdout = the NDJSON stream; stderr = wineboot/child noise -> inherit, never parsed.
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("failed to spawn `neutron`")?;
    let stdout = child.stdout.take().context("neutron produced no stdout")?;

    let mut terminal: Option<Value> = None;
    let mut error_msg: Option<String> = None;
    for line in BufReader::new(stdout).lines() {
        let line = line.context("reading neutron output")?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(t) else {
            continue; // interleaved child output — not ours to interpret
        };
        match v.get("event").and_then(|e| e.as_str()) {
            Some("result") => terminal = Some(v.clone()),
            Some("error") => {
                error_msg = Some(
                    v.get("reason").or_else(|| v.get("message"))
                        .and_then(|m| m.as_str()).unwrap_or("unknown error").to_string(),
                );
            }
            // A command that stops before it starts (e.g. a missing prerequisite) prints a plain
            // `{ok: false, reason}` object rather than a tagged event.
            None if v.get("ok") == Some(&Value::Bool(false)) => {
                if let Some(r) = v.get("reason").and_then(|m| m.as_str()) {
                    error_msg = Some(r.to_string());
                }
            }
            _ => {}
        }
        on_event(v);
    }

    let status = child.wait().context("waiting for neutron")?;
    if let Some(msg) = error_msg {
        return Err(anyhow!("{msg}"));
    }
    if !status.success() {
        // A failed run still ends with a result object naming what failed.
        if let Some(t) = &terminal {
            let failed: Vec<String> = t.get("steps").and_then(|s| s.as_array())
                .map(|steps| steps.iter()
                    .filter(|st| st.get("ok") == Some(&Value::Bool(false)))
                    .map(|st| format!("{}: {}",
                        st.get("step").and_then(|v| v.as_str()).unwrap_or("?"),
                        st.get("detail").and_then(|v| v.as_str()).unwrap_or("failed")))
                    .collect())
                .unwrap_or_default();
            if !failed.is_empty() {
                return Err(anyhow!("{}", failed.join("; ")));
            }
            if let Some(r) = t.get("reason").and_then(|v| v.as_str()) {
                return Err(anyhow!("{r}"));
            }
        }
        return Err(anyhow!("neutron {} exited with code {}", args.join(" "),
                           status.code().unwrap_or(-1)));
    }
    terminal.ok_or_else(|| anyhow!("neutron {} finished without a result", args.join(" ")))
}

/// Close ONE app cleanly, leaving every other app in the prefix running.
///
/// All seven apps share a single wine prefix, and closing/reopening apps mid-project is normal —
/// so a full-prefix teardown (wineserver is per-prefix, so `-k` takes down EVERYTHING) is the
/// wrong tool here. The engine kills only processes under this app's install directory, and when
/// the last app exits it sweeps Adobe's shared daemons itself.
///
/// This matters beyond tidiness: orphaned Adobe daemons accumulate across launches and wedge the
/// NEXT app (Lightroom deadlocks on ntdll's loader_section behind them). Collider used to only
/// `kill_group(pid)`, which left them behind.
pub fn teardown_app(app_id: &str, prefix: &str, force: bool) -> anyhow::Result<Value> {
    // `force` maps to the engine's --force, which SKIPS the graceful WM_CLOSE step.
    //
    // ⏱ That step waits up to --grace seconds (default 25) for the app to close itself and write
    // its preferences. Correct for a normal exit; wrong for a FORCE QUIT, where we have already
    // SIGTERMed the process group and are now waiting 25 s for a graceful close that can never
    // arrive. That is the "hangs on the quitting phase for too long" the user reported.
    //
    // The trade is the engine's own: --force means the app loses unsaved preferences. That is
    // precisely what the user asked for by choosing Force quit, and it is NOT applied to the
    // clean-exit path, which still gives the app its full grace period to save.
    let mut args = vec!["teardown", "--app", app_id, "--prefix", prefix];
    if force { args.push("--force"); }
    run_json(&args)
}

fn pid_from(v: &Value) -> anyhow::Result<u32> {
    v.get("pid")
        .and_then(|p| p.as_u64())
        .map(|p| p as u32)
        .ok_or_else(|| anyhow!("neutron did not return a pid"))
}
