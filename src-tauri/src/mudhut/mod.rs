// mudhut/mod.rs
//
// The shell-out boundary to the Mud Hut CLI (the Adobe-app installer engine),
// mirroring neutron/mod.rs. Collider talks to Mud Hut only through here.
//
//   - `--json` is a TOP-LEVEL flag, before the subcommand: `mudhut --json <cmd>`
//   - stdout may be NDJSON (progress `note`s then a terminal object); the LAST
//     non-empty line is the terminal `result` (or `error`) object
//   - an `{"event":"error","message":..}` object (or non-zero exit) is a failure
//
// Every function here has a twin taking the binary explicitly (`*_with`), so the tests can run a
// fake `mudhut` script without changing PATH for the whole test process (tests run in parallel).

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context};
use serde_json::Value;

/// The Mud Hut CLI: `$MUDHUT_BIN` when set (a fake binary for tests and GUI checks, the same
/// convention as `NEUTRON_BIN` in core/decoration.rs), else `mudhut` on PATH (main.rs puts
/// ~/.local/bin on it).
fn mudhut_bin() -> String {
    match std::env::var("MUDHUT_BIN") {
        Ok(p) if !p.is_empty() => p,
        _ => "mudhut".to_string(),
    }
}

/// Build a `mudhut` command with a cleaned environment (same reasoning as the
/// neutron wrapper: a packaged AppImage's PYTHON*/LD_* vars break child tools).
fn clean_command(bin: &str) -> Command {
    let mut cmd = Command::new(bin);
    for var in [
        "PYTHONHOME", "PYTHONPATH", "PYTHONDONTWRITEBYTECODE", "PYTHONNOUSERSITE",
        "LD_LIBRARY_PATH", "LD_PRELOAD", "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0", "GDK_PIXBUF_MODULE_FILE", "GDK_PIXBUF_MODULEDIR",
        // Startup-notification identity (as in neutron/mod.rs). Mud Hut never opens a window, but
        // if anything it starts ever did, it must not show up as Collider.
        "DESKTOP_STARTUP_ID", "XDG_ACTIVATION_TOKEN",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

/// Start a long Mud Hut job (an install, an update) in its own session.
///
/// Closing Collider must not take the job with it. A job in Collider's session gets the hangup
/// when the terminal Collider was started from goes away; in its own session it does not. (The
/// other half, surviving the closed stdout pipe once Collider has exited, is Mud Hut's: its
/// `update` keeps going without a reader, per the update contract.)
fn own_session(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid() is async-signal-safe, which is all pre_exec requires. It can only fail
    // for a process-group leader, which a freshly forked child never is; a failure is ignored
    // rather than failing the spawn.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
}

/// Run a Mud Hut CLI command expecting a terminal JSON object on stdout.
pub fn run_json(args: &[&str]) -> anyhow::Result<Value> {
    run_json_with(&mudhut_bin(), args)
}

fn run_json_with(bin: &str, args: &[&str]) -> anyhow::Result<Value> {
    let output = clean_command(bin)
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

/// What one streamed Mud Hut run produced: its terminal object (if any) and how it exited.
struct Streamed {
    /// The terminal `{"event":"result"}` (the LAST one: an install shells out to the engine for
    /// a dependency and can print an inner result first).
    result: Option<Value>,
    /// The terminal `{"event":"error"}`, kept whole: newer commands put machine fields on it
    /// (`code`, `exit`, `apps_running`, `prefix_changed`).
    error: Option<Value>,
    /// The process exit code; None when it was killed by a signal.
    exit: Option<i32>,
}

/// Run `mudhut --json <args>` and STREAM each NDJSON event to `on_event` as it arrives.
///
/// Bytes, not `lines()`: a child's output can be non-UTF-8 (Wine's `reg` on a French system
/// prints in code page 850), and `lines()` fails on it. That ended the read, closed the pipe, and
/// Mud Hut died on its next write ("Broken pipe") mid-install. Neutron#11. Non-JSON lines are
/// interleaved child output and are skipped.
fn stream_with(
    bin: &str,
    args: &[String],
    detach: bool,
    on_event: &mut dyn FnMut(Value),
) -> anyhow::Result<Streamed> {
    let mut cmd = clean_command(bin);
    cmd.arg("--json").args(args);
    // stdout = the NDJSON stream we parse; stderr = child noise -> inherit (not captured).
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());
    if detach {
        own_session(&mut cmd);
    }
    let mut child = cmd.spawn().context("failed to spawn `mudhut` — is Mud Hut on PATH?")?;
    let stdout = child.stdout.take().context("mudhut produced no stdout")?;

    let mut out = Streamed { result: None, error: None, exit: None };
    read_ndjson(stdout, &mut out, on_event);
    let status = child.wait().context("waiting for mudhut")?;
    out.exit = status.code();
    Ok(out)
}

fn read_ndjson(stdout: impl Read, out: &mut Streamed, on_event: &mut dyn FnMut(Value)) {
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        // A read error (not EOF) ends the stream; the caller still waits for the child, so it is
        // never left behind as a zombie.
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
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
            Some("result") => out.result = Some(v.clone()),
            Some("error") => out.error = Some(v.clone()),
            _ => {}
        }
        on_event(v);
    }
}

fn exit_label(exit: Option<i32>) -> String {
    match exit {
        Some(c) => format!("code {c}"),
        None => "a signal".to_string(),
    }
}

/// Install an app, STREAMING each NDJSON event to `on_event` as it arrives (Mud
/// Hut's `install` emits many `progress`/`note` lines before the terminal object).
///
/// Returns the terminal `result` object, or an error (from an `error` event, or a non-zero exit
/// with no result). A run that reached its `result` reports through it EVEN ON A NON-ZERO EXIT:
/// the exit code is added as `exit`, and the page reads `ok`/`message` from it. (This used to
/// return "exited with code N" and drop the result, so the user never saw what Mud Hut said.)
/// Non-JSON lines (interleaved child-process output) are ignored.
pub fn install_stream(
    app: &str,
    method: &str,
    prefix: &str,
    source: Option<&str>,
    mut on_event: impl FnMut(Value),
) -> anyhow::Result<Value> {
    install_stream_with(&mudhut_bin(), app, method, prefix, source, &mut on_event)
}

fn install_stream_with(
    bin: &str,
    app: &str,
    method: &str,
    prefix: &str,
    source: Option<&str>,
    on_event: &mut dyn FnMut(Value),
) -> anyhow::Result<Value> {
    let mut args: Vec<String> = vec![
        "install".into(), app.into(),
        "--method".into(), method.into(),
        "--prefix".into(), prefix.into(),
    ];
    if let Some(s) = source {
        args.push("--source".into());
        args.push(s.into());
    }
    let s = stream_with(bin, &args, true, on_event)?;
    if let Some(e) = s.error {
        let msg = e.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
        return Err(anyhow!("{msg}"));
    }
    if let Some(mut r) = s.result {
        if s.exit != Some(0) {
            r["exit"] = s.exit.map(Value::from).unwrap_or(Value::Null);
        }
        return Ok(r);
    }
    if s.exit != Some(0) {
        return Err(anyhow!("mudhut install exited with {} and reported no result", exit_label(s.exit)));
    }
    Err(anyhow!("mudhut install finished without a result"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fake `mudhut` (a /bin/sh script) in its own temp dir. It records its argv, one argument
    /// per line, in `<dir>/args` before running `script`. Dropping it removes the dir.
    pub(crate) struct Fake {
        pub dir: PathBuf,
        pub bin: String,
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }
    pub(crate) fn fake(script: &str) -> Fake {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "collider-fake-mudhut-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mudhut");
        let args = dir.join("args");
        std::fs::write(
            &path,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{script}", args.display()),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Fake { bin: path.to_string_lossy().into_owned(), dir }
    }
    /// The argv the fake was called with.
    pub(crate) fn argv(f: &Fake) -> Vec<String> {
        std::fs::read_to_string(f.dir.join("args"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// A fake `mudhut` whose stdout carries the line Wine's `reg add` prints on a French
    /// system (GitHub Neutron#11): Windows code page 850, not UTF-8. Collider must skip it like
    /// any other non-JSON line and keep reading to the terminal `result`.
    #[test]
    fn install_stream_survives_non_utf8_child_output() {
        let f = fake(
            "echo '{\"event\":\"note\",\"message\":\"preparing the prefix\"}'\n\
             printf 'reg\\377: L'\"'\"'op\\202ration s'\"'\"'est termin\\202e avec succ\\212s\\r\\n'\n\
             echo '{\"event\":\"progress\",\"pct\":50}'\n\
             echo '{\"event\":\"result\",\"apps\":[\"lightroom\"]}'\n",
        );
        let mut seen = Vec::new();
        let r = install_stream_with(&f.bin, "lightroom", "iso", "/nonexistent", None, &mut |v| {
            seen.push(v["event"].as_str().unwrap_or("").to_string())
        });
        let result = r.expect("install_stream must not fail on a non-UTF-8 line");
        assert_eq!(result["event"], "result");
        assert!(result.get("exit").is_none(), "a clean exit adds no `exit` field");
        assert_eq!(seen, ["note", "progress", "result"]);
    }

    /// The bug: a run that printed its terminal `result` and then exited non-zero came back as
    /// "exited with code 1", and what Mud Hut said was lost. The result must come back, with the
    /// exit code on it.
    #[test]
    fn install_stream_keeps_the_result_on_a_nonzero_exit() {
        let f = fake(
            "echo '{\"event\":\"progress\",\"stage\":\"apply\",\"pct\":90,\"msg\":\"x\"}'\n\
             echo '{\"event\":\"result\",\"ok\":false,\"message\":\"Premiere Pro installed, but the 2025 version could not be removed.\"}'\n\
             exit 1\n",
        );
        let r = install_stream_with(&f.bin, "premiere", "download", "/p", None, &mut |_| {})
            .expect("a result on a non-zero exit is data, not an error");
        assert_eq!(r["ok"], false);
        assert_eq!(r["exit"], 1);
        assert!(r["message"].as_str().unwrap().contains("could not be removed"));
    }

    #[test]
    fn install_stream_error_event_is_the_message() {
        let f = fake(
            "echo '{\"event\":\"error\",\"code\":\"app-running\",\"exit\":3,\"message\":\"Close Premiere Pro first.\",\"apps_running\":[\"Premiere Pro\"]}'\n\
             exit 3\n",
        );
        let e = install_stream_with(&f.bin, "premiere", "download", "/p", None, &mut |_| {})
            .expect_err("an error event is a failure");
        assert_eq!(e.to_string(), "Close Premiere Pro first.");
    }

    #[test]
    fn install_stream_nonzero_exit_without_result_is_an_error() {
        let f = fake("echo 'wine noise'\nexit 1\n");
        let e = install_stream_with(&f.bin, "premiere", "download", "/p", None, &mut |_| {})
            .expect_err("no terminal object and exit 1");
        assert!(e.to_string().contains("code 1"), "{e}");
    }

    /// Long jobs run in their own session, so the hangup of the terminal Collider was started
    /// from does not reach them. The fake reports its pid and its session id (field 6 of
    /// /proc/<pid>/stat); after setsid() they are equal.
    #[test]
    fn install_runs_in_its_own_session() {
        let f = fake(
            "sid=$(cut -d' ' -f6 /proc/$$/stat)\n\
             echo \"{\\\"event\\\":\\\"result\\\",\\\"pid\\\":$$,\\\"sid\\\":$sid}\"\n",
        );
        let r = install_stream_with(&f.bin, "premiere", "download", "/p", None, &mut |_| {}).unwrap();
        assert_eq!(r["pid"], r["sid"], "the install must lead its own session: {r}");
        let ours = std::fs::read_to_string("/proc/self/stat").unwrap();
        let our_sid: i64 = ours.rsplit(')').next().unwrap().split_whitespace().nth(3).unwrap().parse().unwrap();
        assert_ne!(r["sid"].as_i64().unwrap(), our_sid, "and not Collider's session");
    }

    #[test]
    fn install_argv_is_exact() {
        let f = fake("echo '{\"event\":\"result\",\"ok\":true}'\n");
        install_stream_with(&f.bin, "photoshop", "offline", "/home/u/My Prefix", Some("/m/disc.iso"), &mut |_| {})
            .unwrap();
        assert_eq!(
            argv(&f),
            ["--json", "install", "photoshop", "--method", "offline", "--prefix", "/home/u/My Prefix",
             "--source", "/m/disc.iso"]
        );
    }

    #[test]
    fn run_json_with_reads_the_last_line() {
        let f = fake("echo '{\"event\":\"note\",\"msg\":\"a\"}'\necho '{\"event\":\"result\",\"apps\":[]}'\n");
        let v = run_json_with(&f.bin, &["apps"]).unwrap();
        assert_eq!(v["event"], "result");
    }
}
