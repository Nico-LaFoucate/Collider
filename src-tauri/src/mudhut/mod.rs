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

/// How a `check-updates` run may get its answer.
#[derive(Debug, Default, Clone)]
pub struct CheckOpts {
    /// `--max-age DUR` (e.g. "24h"): reuse Adobe's feed if Mud Hut fetched it within this long.
    /// The CLI's cache decides whether Adobe is contacted, so a 30-minute tick is a local read
    /// most of the time and a restart does not refetch.
    pub max_age: Option<String>,
    /// `--refresh`: fetch the feed now ("Check now").
    pub refresh: bool,
    /// `--offline`: never touch the network; answer from the cached feed. Used for the startup
    /// read and for following an update that is already running (installed versions and the
    /// update state are re-read from the prefixes either way).
    pub offline: bool,
}

/// `mudhut --json check-updates --prefix P ...`: what can be updated in each prefix.
///
/// Read-only in the engine (it never runs wine or writes into a prefix, and is safe while apps
/// run), so Collider calls it on a timer. The result is returned as Mud Hut wrote it, plus
/// `supported: true`; every sentence in it is Mud Hut's and the page renders them verbatim.
///
/// A Mud Hut without `check-updates` (clap prints "unrecognized subcommand" to stderr, exits 2,
/// and writes nothing to stdout) or no Mud Hut at all is a state to render ("Update Neutron to
/// get Adobe update checks"), not an error: `{ok: true, supported: false, reason}`.
pub fn check_updates(prefixes: &[String], opts: &CheckOpts) -> anyhow::Result<Value> {
    check_updates_with(&mudhut_bin(), prefixes, opts)
}

fn check_args(prefixes: &[String], opts: &CheckOpts) -> Vec<String> {
    let mut args: Vec<String> = vec!["--json".into(), "check-updates".into()];
    for p in prefixes {
        // Exactly as registered: the CLI echoes each prefix string back, and Collider keys its
        // rows on that string (symlinked homes must not be resolved on either side).
        args.push("--prefix".into());
        args.push(p.clone());
    }
    if opts.offline {
        args.push("--offline".into());
    } else {
        if let Some(a) = &opts.max_age {
            args.push("--max-age".into());
            args.push(a.clone());
        }
        if opts.refresh {
            args.push("--refresh".into());
        }
    }
    args
}

fn check_updates_with(bin: &str, prefixes: &[String], opts: &CheckOpts) -> anyhow::Result<Value> {
    let output = match clean_command(bin).args(check_args(prefixes, opts)).output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!({ "ok": true, "supported": false, "reason": "missing" }));
        }
        Err(e) => return Err(anyhow!("failed to run `mudhut check-updates`: {e}")),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout.lines().rev().find(|l| !l.trim().is_empty());
    let Some(mut v) = last.and_then(|l| serde_json::from_str::<Value>(l).ok()) else {
        let err = String::from_utf8_lossy(&output.stderr);
        if err.contains("unrecognized subcommand") {
            return Ok(serde_json::json!({ "ok": true, "supported": false, "reason": "too-old" }));
        }
        let e = err.trim();
        return Err(anyhow!(
            "mudhut check-updates returned non-JSON (exit {}). stderr: {}",
            output.status.code().unwrap_or(-1),
            if e.is_empty() { "<empty>" } else { e }
        ));
    };
    if v.get("event").and_then(|e| e.as_str()) == Some("error") {
        let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
        return Err(anyhow!("{msg}"));
    }
    v["supported"] = Value::Bool(true);
    if output.status.code() != Some(0) {
        v["exit"] = output.status.code().map(Value::from).unwrap_or(Value::Null);
    }
    Ok(v)
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
    opts: &InstallOpts,
    mut on_event: impl FnMut(Value),
) -> anyhow::Result<Value> {
    install_stream_with(&mudhut_bin(), app, method, prefix, source, opts, &mut on_event)
}

/// The install choices the user can make beyond app, method and target.
#[derive(Debug, Default, Clone)]
pub struct InstallOpts {
    /// `--version MAJOR`: a specific yearly version (e.g. the 2027 apps) instead of the newest.
    pub version: Option<String>,
    /// `--allow-unverified`: the user chose Adobe's newest build (of `version`, if set) knowing
    /// Neutron has not tested it (his call, 2026-10-24). Without it Mud Hut installs the newest
    /// TESTED build, and refuses (`code: "unverified"`) a chosen yearly version with none.
    pub allow_unverified: bool,
    /// `--dry-run`: only the plan (which build, tested or not, Adobe's newest); nothing is
    /// downloaded or installed.
    pub dry_run: bool,
    /// `--speech-packs`: also install Premiere Pro's speech-to-text languages (about 13.8 GB).
    /// Off unless the user asks, as in Creative Cloud (DECISIONS C70).
    pub speech_packs: bool,
    /// `--remove-previous`: remove the previous yearly version once the new one is installed.
    pub remove_previous: bool,
}

fn install_stream_with(
    bin: &str,
    app: &str,
    method: &str,
    prefix: &str,
    source: Option<&str>,
    opts: &InstallOpts,
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
    if let Some(v) = &opts.version {
        args.push("--version".into());
        args.push(v.clone());
    }
    if opts.allow_unverified {
        args.push("--allow-unverified".into());
    }
    if opts.speech_packs {
        args.push("--speech-packs".into());
    }
    if opts.remove_previous {
        args.push("--remove-previous".into());
    }
    if opts.dry_run {
        args.push("--dry-run".into());
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

/// The choices behind one `mudhut update` run, all made by the user in the update dialog.
#[derive(Debug, Default, Clone)]
pub struct UpdateOpts {
    /// `--to BUILD`: the exact build the user saw and chose (one app only).
    pub to: Option<String>,
    /// `--allow-unverified`: the user chose a build Neutron has not verified, after the dialog
    /// told them so.
    pub allow_unverified: bool,
    /// `--no-snapshot`: the user turned off "Keep a backup copy before updating".
    pub no_snapshot: bool,
}

fn update_args(prefix: &str, apps: &[String], opts: &UpdateOpts) -> Vec<String> {
    let mut args: Vec<String> = vec!["update".into(), "--prefix".into(), prefix.into()];
    args.extend(apps.iter().cloned());
    if let Some(t) = &opts.to {
        args.push("--to".into());
        args.push(t.clone());
    }
    if opts.allow_unverified {
        args.push("--allow-unverified".into());
    }
    if opts.no_snapshot {
        args.push("--no-snapshot".into());
    }
    args
}

/// `mudhut --json update --prefix P APP ...`, STREAMED: every `progress` (with its `app`), `note`
/// and per-app `app_result` goes to `on_event` as it arrives.
///
/// Every outcome Mud Hut reports comes back as DATA, so the page can tell them apart:
/// - the terminal `result` (`{ok, prefix, apps}`), even on a non-zero exit (one app of several
///   failed: exit 1; failed and put back: 7; failed and could not be put back: 8), with `exit`
///   added when non-zero;
/// - the terminal `error` (refused because an app is running: exit 3; network: 5; disk: 6; ...)
///   with `ok:false` and `exit` added, keeping Mud Hut's `code`, `message`, `apps_running` and
///   `prefix_changed`, so "nothing happened, close Premiere Pro first" is not shown as a failure.
///
/// Only a run that produced neither (Mud Hut could not start, or died without a word) is an Err.
///
/// It runs in its own session, and Mud Hut's update keeps going if Collider is closed (it writes
/// its events to `<prefix>/.mudhut/update.log` and its state for `check-updates` to report).
/// Collider never passes `--all`: it lists the apps the user confirmed.
pub fn update_stream(
    prefix: &str,
    apps: &[String],
    opts: &UpdateOpts,
    mut on_event: impl FnMut(Value),
) -> anyhow::Result<Value> {
    update_stream_with(&mudhut_bin(), prefix, apps, opts, &mut on_event)
}

fn update_stream_with(
    bin: &str,
    prefix: &str,
    apps: &[String],
    opts: &UpdateOpts,
    on_event: &mut dyn FnMut(Value),
) -> anyhow::Result<Value> {
    if apps.is_empty() {
        return Err(anyhow!("no apps to update"));
    }
    let s = stream_with(bin, &update_args(prefix, apps, opts), true, on_event)?;
    let exit = s.exit.map(Value::from).unwrap_or(Value::Null);
    if let Some(mut r) = s.result {
        if s.exit != Some(0) {
            r["exit"] = exit;
        }
        return Ok(r);
    }
    if let Some(mut e) = s.error {
        e["ok"] = Value::Bool(false);
        if e.get("exit").is_none() {
            e["exit"] = exit;
        }
        return Ok(e);
    }
    if s.exit != Some(0) {
        return Err(anyhow!("mudhut update exited with {} and reported no result", exit_label(s.exit)));
    }
    Err(anyhow!("mudhut update finished without a result"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Create the executable `path` holding `content`, written by a child `sh` rather than by
    /// this process.
    ///
    /// The tests run on parallel threads, and each one forks to start its own fake. A fork that
    /// lands while this process has `path` open for writing hands the child a copy of that
    /// descriptor (O_CLOEXEC only closes it at the child's exec), and until the child gets there,
    /// executing `path` fails with ETXTBSY, "Text file busy". Writing a temp name and renaming it
    /// into place would not help: the leaked descriptor follows the inode, not the name. The
    /// `sh` below holds the only writable descriptor `path` ever has, nothing else can inherit it,
    /// and it has exited by the time this returns.
    pub(crate) fn write_executable(path: &Path, content: &str) {
        let st = Command::new("/bin/sh")
            .args(["-c", "printf '%s' \"$1\" > \"$0\""])
            .arg(path)
            .arg(content)
            .status()
            .unwrap();
        assert!(st.success(), "could not write {}", path.display());
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

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
    fn fake_dir() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "collider-fake-mudhut-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    /// The fake in `dir`, written once, whole (see `write_executable`).
    fn fake_in(dir: PathBuf, script: &str) -> Fake {
        let path = dir.join("mudhut");
        let args = dir.join("args");
        write_executable(
            &path,
            &format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{script}", args.display()),
        );
        Fake { bin: path.to_string_lossy().into_owned(), dir }
    }
    pub(crate) fn fake(script: &str) -> Fake {
        fake_in(fake_dir(), script)
    }
    /// A fake that prints `stdout` verbatim (from a data file, so JSON needs no shell quoting),
    /// writes `stderr` to stderr, and exits with `exit`.
    pub(crate) fn fake_output(stdout: &str, stderr: &str, exit: i32) -> Fake {
        let dir = fake_dir();
        std::fs::write(dir.join("out"), stdout).unwrap();
        std::fs::write(dir.join("err"), stderr).unwrap();
        let script = format!("cat '{0}/out'\ncat '{0}/err' >&2\nexit {exit}\n", dir.display());
        fake_in(dir, &script)
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
        let r = install_stream_with(&f.bin, "lightroom", "iso", "/nonexistent", None, &InstallOpts::default(), &mut |v| {
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
        let r = install_stream_with(&f.bin, "premiere", "download", "/p", None, &InstallOpts::default(), &mut |_| {})
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
        let e = install_stream_with(&f.bin, "premiere", "download", "/p", None, &InstallOpts::default(), &mut |_| {})
            .expect_err("an error event is a failure");
        assert_eq!(e.to_string(), "Close Premiere Pro first.");
    }

    #[test]
    fn install_stream_nonzero_exit_without_result_is_an_error() {
        let f = fake("echo 'wine noise'\nexit 1\n");
        let e = install_stream_with(&f.bin, "premiere", "download", "/p", None, &InstallOpts::default(), &mut |_| {})
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
        let r = install_stream_with(&f.bin, "premiere", "download", "/p", None, &InstallOpts::default(), &mut |_| {}).unwrap();
        assert_eq!(r["pid"], r["sid"], "the install must lead its own session: {r}");
        let ours = std::fs::read_to_string("/proc/self/stat").unwrap();
        let our_sid: i64 = ours.rsplit(')').next().unwrap().split_whitespace().nth(3).unwrap().parse().unwrap();
        assert_ne!(r["sid"].as_i64().unwrap(), our_sid, "and not Collider's session");
    }

    #[test]
    fn install_argv_is_exact() {
        let f = fake("echo '{\"event\":\"result\",\"ok\":true}'\n");
        install_stream_with(&f.bin, "photoshop", "offline", "/home/u/My Prefix", Some("/m/disc.iso"), &InstallOpts::default(), &mut |_| {})
            .unwrap();
        assert_eq!(
            argv(&f),
            ["--json", "install", "photoshop", "--method", "offline", "--prefix", "/home/u/My Prefix",
             "--source", "/m/disc.iso"]
        );
    }

    #[test]
    fn a_plan_is_a_dry_run_with_the_same_choices() {
        let f = fake("echo '{\"event\":\"result\",\"ok\":true,\"dry_run\":true}'\n");
        let o = InstallOpts { allow_unverified: true, dry_run: true, ..InstallOpts::default() };
        install_stream_with(&f.bin, "premiere", "download", "/p", None, &o, &mut |_| {}).unwrap();
        assert_eq!(argv(&f), ["--json", "install", "premiere", "--method", "download", "--prefix", "/p",
                              "--allow-unverified", "--dry-run"]);
    }

    /// The `check-updates` terminal result exactly as CONTRACT.md writes it (one prefix,
    /// Premiere with a verified in-place update, a newer unverified build and a new yearly
    /// version), on one line as Mud Hut prints it.
    pub(crate) const CONTRACT_CHECK: &str = r#"{"event":"result","ok":true,"schema":1,"checked_at":"2026-10-16T14:02:11Z","online":true,"feed_error":null,"from_cache":false,"next_check_after":"2026-10-17T14:02:11Z","verified_list":{"source":"network","serial":1,"signature":"valid"},"prefixes":[{"prefix":"/home/u/Neutron Prefixes/Adobe 2026","ok":true,"apps_running":["Premiere Pro"],"update_in_progress":null,"last_update":null,"apps":[{"id":"premiere","name":"Premiere Pro","sap":"PPRO","installed":{"version":"26.3.0.12","base":"26.0","year":2026,"managed_by":"hdpim"},"update":{"kind":"in_place","to":"26.5.2.3","label":"26.5.2","verified":true,"download_bytes":187000000,"can_update":true,"blocked_reason":null},"newer_unverified":{"to":"26.6.0.1","label":"26.6"},"new_major":{"year":2027,"to":"27.0.0.40","label":"2027","verified":false},"related":["aftereffects","mediaencoder"]}]}]}"#;

    fn opts(max_age: Option<&str>, refresh: bool, offline: bool) -> CheckOpts {
        CheckOpts { max_age: max_age.map(str::to_string), refresh, offline }
    }

    #[test]
    fn check_passes_the_contract_shape_through_verbatim() {
        let f = fake_output(&format!("{CONTRACT_CHECK}\n"), "", 0);
        let p = "/home/u/Neutron Prefixes/Adobe 2026".to_string();
        let v = check_updates_with(&f.bin, std::slice::from_ref(&p), &opts(Some("24h"), false, false)).unwrap();
        assert_eq!(v["supported"], true);
        assert!(v.get("exit").is_none());
        assert_eq!(v["online"], true);
        let pre = &v["prefixes"][0];
        assert_eq!(pre["prefix"], p.as_str(), "the prefix comes back exactly as passed");
        assert_eq!(pre["apps_running"][0], "Premiere Pro");
        let app = &pre["apps"][0];
        assert_eq!(app["id"], "premiere");
        assert_eq!(app["installed"]["managed_by"], "hdpim");
        assert_eq!(app["update"]["to"], "26.5.2.3");
        assert_eq!(app["update"]["verified"], true);
        assert_eq!(app["update"]["download_bytes"], 187000000u64);
        assert_eq!(app["newer_unverified"]["label"], "26.6");
        assert_eq!(app["new_major"]["year"], 2027);
        assert_eq!(app["related"][1], "mediaencoder");
    }

    #[test]
    fn check_argv_for_a_tick_a_refresh_and_an_offline_read() {
        let f = fake_output(&format!("{CONTRACT_CHECK}\n"), "", 0);
        let ps = ["/a b/one".to_string(), "/home/u/two".to_string()];
        check_updates_with(&f.bin, &ps, &opts(Some("24h"), false, false)).unwrap();
        assert_eq!(argv(&f), ["--json", "check-updates", "--prefix", "/a b/one", "--prefix", "/home/u/two",
                              "--max-age", "24h"]);
        check_updates_with(&f.bin, &ps[..1], &opts(Some("24h"), true, false)).unwrap();
        assert_eq!(argv(&f), ["--json", "check-updates", "--prefix", "/a b/one", "--max-age", "24h", "--refresh"]);
        check_updates_with(&f.bin, &ps[..1], &opts(Some("24h"), true, true)).unwrap();
        assert_eq!(argv(&f), ["--json", "check-updates", "--prefix", "/a b/one", "--offline"],
                   "an offline read never asks for a refresh");
    }

    #[test]
    fn check_up_to_date() {
        let f = fake_output(
            r#"{"event":"result","ok":true,"schema":1,"checked_at":"2026-10-16T14:02:11Z","online":true,"feed_error":null,"from_cache":true,"prefixes":[{"prefix":"/p","ok":true,"apps_running":[],"update_in_progress":null,"last_update":null,"apps":[{"id":"photoshop","name":"Photoshop","sap":"PHSP","installed":{"version":"27.0.1.4","base":"27.0","year":2026,"managed_by":"windows-copy"},"update":null,"newer_unverified":null,"new_major":null,"related":[]}]}]}
"#, "", 0);
        let v = check_updates_with(&f.bin, &["/p".into()], &opts(Some("24h"), false, false)).unwrap();
        let app = &v["prefixes"][0]["apps"][0];
        assert!(app["update"].is_null());
        assert_eq!(app["installed"]["managed_by"], "windows-copy");
    }

    /// Offline is a status: exit 0, `online:false`, the cached rows still there.
    #[test]
    fn check_offline_is_status_not_error() {
        let f = fake_output(
            r#"{"event":"result","ok":true,"schema":1,"checked_at":"2026-10-14T09:00:00Z","online":false,"feed_error":"Couldn't reach Adobe.","from_cache":true,"prefixes":[]}
"#, "", 0);
        let v = check_updates_with(&f.bin, &["/p".into()], &opts(Some("24h"), false, false)).unwrap();
        assert_eq!(v["online"], false);
        assert_eq!(v["supported"], true);
    }

    /// A Mud Hut from before `check-updates` (0.2.x): clap's message on stderr, exit 2, nothing on
    /// stdout. Exactly what `mudhut --json check` printed in report D (F31).
    #[test]
    fn check_on_a_mudhut_too_old_is_unsupported() {
        let f = fake_output(
            "",
            "error: unrecognized subcommand 'check-updates'\n\nUsage: mudhut [OPTIONS] <COMMAND>\n\nFor more information, try '--help'.\n",
            2,
        );
        let v = check_updates_with(&f.bin, &["/p".into()], &opts(Some("24h"), false, false)).unwrap();
        assert_eq!(v, serde_json::json!({ "ok": true, "supported": false, "reason": "too-old" }));
    }

    #[test]
    fn check_without_mudhut_is_unsupported() {
        let v = check_updates_with("/nonexistent/mudhut", &["/p".into()], &opts(None, false, false)).unwrap();
        assert_eq!(v["supported"], false);
        assert_eq!(v["reason"], "missing");
    }

    #[test]
    fn check_error_event_is_an_error_with_its_message() {
        let f = fake_output(
            r#"{"event":"error","code":"bad-prefix","exit":2,"message":"/p is not a Neutron prefix."}
"#, "", 2);
        let e = check_updates_with(&f.bin, &["/p".into()], &opts(None, false, false)).unwrap_err();
        assert_eq!(e.to_string(), "/p is not a Neutron prefix.");
    }

    #[test]
    fn check_garbage_is_an_error_naming_stderr() {
        let f = fake_output("not json\n", "thread 'main' panicked", 101);
        let e = check_updates_with(&f.bin, &["/p".into()], &opts(None, false, false)).unwrap_err();
        let s = e.to_string();
        assert!(s.contains("exit 101") && s.contains("panicked"), "{s}");
    }

    /// A result that comes with a non-zero exit is still the answer; the code rides along.
    #[test]
    fn check_result_on_a_nonzero_exit_keeps_the_code() {
        let f = fake_output(&format!("{CONTRACT_CHECK}\n"), "", 5);
        let v = check_updates_with(&f.bin, &["/p".into()], &opts(None, false, false)).unwrap();
        assert_eq!(v["exit"], 5);
        assert_eq!(v["supported"], true);
    }

    // --- update ----------------------------------------------------------------------------

    fn upd_opts(to: Option<&str>, allow_unverified: bool, no_snapshot: bool) -> UpdateOpts {
        UpdateOpts { to: to.map(str::to_string), allow_unverified, no_snapshot }
    }
    fn run_update(f: &Fake, apps: &[&str], o: &UpdateOpts) -> (anyhow::Result<Value>, Vec<Value>) {
        let apps: Vec<String> = apps.iter().map(|a| a.to_string()).collect();
        let mut seen = Vec::new();
        let r = update_stream_with(&f.bin, "/home/u/Neutron Prefixes/Adobe 2026", &apps, o, &mut |v| seen.push(v));
        (r, seen)
    }

    #[test]
    fn update_argv_is_exact() {
        let f = fake_output("{\"event\":\"result\",\"ok\":true,\"prefix\":\"p\",\"apps\":[]}\n", "", 0);
        run_update(&f, &["premiere"], &upd_opts(Some("26.6.0.1"), true, true)).0.unwrap();
        assert_eq!(argv(&f), ["--json", "update", "--prefix", "/home/u/Neutron Prefixes/Adobe 2026", "premiere",
                              "--to", "26.6.0.1", "--allow-unverified", "--no-snapshot"]);
        run_update(&f, &["premiere", "aftereffects"], &upd_opts(None, false, false)).0.unwrap();
        assert_eq!(argv(&f), ["--json", "update", "--prefix", "/home/u/Neutron Prefixes/Adobe 2026", "premiere",
                              "aftereffects"], "no --all, no --to, no flags the user did not choose");
    }

    #[test]
    fn update_with_no_apps_is_refused_without_running_mudhut() {
        let f = fake_output("", "", 0);
        let e = run_update(&f, &[], &UpdateOpts::default()).0.unwrap_err();
        assert_eq!(e.to_string(), "no apps to update");
        assert!(argv(&f).is_empty(), "mudhut must not have run");
    }

    #[test]
    fn update_success_streams_progress_then_app_result_then_result() {
        let f = fake_output(concat!(
            r#"{"event":"progress","app":"premiere","stage":"preflight","pct":0,"msg":"Checking"}"#, "\n",
            r#"{"event":"progress","app":"premiere","stage":"download","pct":42,"msg":"Downloading"}"#, "\n",
            "wine: some noise\n",
            r#"{"event":"progress","app":"premiere","stage":"snapshot","pct":60,"msg":"Backing up"}"#, "\n",
            r#"{"event":"app_result","app":"premiere","ok":true,"from":"26.3.0.12","to":"26.5.2.3"}"#, "\n",
            r#"{"event":"result","ok":true,"prefix":"/home/u/Neutron Prefixes/Adobe 2026","apps":[{"app":"premiere","ok":true,"from":"26.3.0.12","to":"26.5.2.3"}]}"#, "\n"),
            "", 0);
        let (r, seen) = run_update(&f, &["premiere"], &upd_opts(Some("26.5.2.3"), false, false));
        let r = r.unwrap();
        assert_eq!(r["ok"], true);
        assert!(r.get("exit").is_none());
        let kinds: Vec<&str> = seen.iter().map(|v| v["event"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["progress", "progress", "progress", "app_result", "result"]);
        assert_eq!(seen[1]["app"], "premiere");
        assert_eq!(seen[1]["pct"], 42);
        assert_eq!(seen[3]["to"], "26.5.2.3");
    }

    /// Update all with one app failed: the terminal result still arrives (exit 1) and is returned,
    /// so the page can report every app.
    #[test]
    fn update_all_with_one_failure_returns_the_result_and_exit_1() {
        let f = fake_output(concat!(
            r#"{"event":"app_result","app":"premiere","ok":true,"from":"26.3.0.12","to":"26.5.2.3"}"#, "\n",
            r#"{"event":"app_result","app":"aftereffects","ok":false,"state":"unchanged","message":"Adobe's download stopped.","log":"/p/.mudhut/update.log"}"#, "\n",
            r#"{"event":"result","ok":false,"prefix":"/p","apps":[{"app":"premiere","ok":true},{"app":"aftereffects","ok":false,"state":"unchanged"}]}"#, "\n"),
            "", 1);
        let (r, seen) = run_update(&f, &["premiere", "aftereffects"], &UpdateOpts::default());
        let r = r.expect("a result on exit 1 is data");
        assert_eq!(r["exit"], 1);
        assert_eq!(r["apps"][1]["state"], "unchanged");
        assert_eq!(seen.iter().filter(|v| v["event"] == "app_result").count(), 2);
    }

    #[test]
    fn update_refused_while_an_app_runs_is_data_not_failure() {
        let f = fake_output(
            r#"{"event":"error","code":"app-running","exit":3,"prefix_changed":false,"apps_running":["Premiere Pro","After Effects"],"message":"Close Premiere Pro and After Effects first."}
"#, "", 3);
        let r = run_update(&f, &["premiere"], &UpdateOpts::default()).0.unwrap();
        assert_eq!(r["ok"], false);
        assert_eq!(r["code"], "app-running");
        assert_eq!(r["exit"], 3);
        assert_eq!(r["prefix_changed"], false);
        assert_eq!(r["apps_running"][1], "After Effects");
    }

    /// Exits 4, 5 and 6 come as terminal error events; an event without an `exit` field gets the
    /// process's.
    #[test]
    fn update_error_exits_4_5_6_keep_code_and_exit() {
        for (code, exit) in [("needs-runtime", 4), ("network", 5), ("snapshot-space", 6)] {
            let f = fake_output(
                &format!("{{\"event\":\"error\",\"code\":\"{code}\",\"prefix_changed\":false,\"message\":\"m\"}}\n"),
                "", exit);
            let r = run_update(&f, &["photoshop"], &UpdateOpts::default()).0.unwrap();
            assert_eq!(r["code"], code);
            assert_eq!(r["exit"], exit);
            assert_eq!(r["ok"], false);
        }
    }

    #[test]
    fn update_rolled_back_exit_7_and_rollback_failed_exit_8() {
        for (state, exit) in [("rolled_back", 7), ("incomplete", 8)] {
            let f = fake_output(&format!(concat!(
                r#"{{"event":"progress","app":"photoshop","stage":"rollback","pct":95,"msg":"Putting back"}}"#, "\n",
                r#"{{"event":"app_result","app":"photoshop","ok":false,"state":"{0}","message":"m","log":"/p/.mudhut/update.log"}}"#, "\n",
                r#"{{"event":"result","ok":false,"prefix":"/p","apps":[{{"app":"photoshop","ok":false,"state":"{0}"}}]}}"#, "\n"),
                state), "", exit);
            let (r, seen) = run_update(&f, &["photoshop"], &UpdateOpts::default());
            let r = r.unwrap();
            assert_eq!(r["exit"], exit);
            assert_eq!(seen[1]["state"], state);
        }
    }

    #[test]
    fn update_survives_non_utf8_lines() {
        let f = fake(
            "printf 'reg\\377: termin\\202e\\r\\n'\n\
             echo '{\"event\":\"progress\",\"app\":\"premiere\",\"stage\":\"apply\",\"pct\":70,\"msg\":\"x\"}'\n\
             echo '{\"event\":\"result\",\"ok\":true,\"prefix\":\"p\",\"apps\":[]}'\n",
        );
        let (r, seen) = run_update(&f, &["premiere"], &UpdateOpts::default());
        assert_eq!(r.unwrap()["ok"], true);
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn update_without_a_terminal_object_is_an_error() {
        let f = fake_output(r#"{"event":"progress","app":"premiere","stage":"apply","pct":70,"msg":"x"}
"#, "", 1);
        let e = run_update(&f, &["premiere"], &UpdateOpts::default()).0.unwrap_err();
        assert!(e.to_string().contains("code 1"), "{e}");
    }

    #[test]
    fn update_runs_in_its_own_session() {
        let f = fake(
            "sid=$(cut -d' ' -f6 /proc/$$/stat)\n\
             echo \"{\\\"event\\\":\\\"result\\\",\\\"ok\\\":true,\\\"pid\\\":$$,\\\"sid\\\":$sid}\"\n",
        );
        let r = run_update(&f, &["premiere"], &UpdateOpts::default()).0.unwrap();
        assert_eq!(r["pid"], r["sid"], "{r}");
    }

    #[test]
    fn install_argv_with_yearly_and_speech_options() {
        let f = fake_output("{\"event\":\"result\",\"ok\":true}\n", "", 0);
        let o = InstallOpts { version: Some("27".into()), allow_unverified: true, speech_packs: true, remove_previous: true, dry_run: false };
        install_stream_with(&f.bin, "premiere", "download", "/p", None, &o, &mut |_| {}).unwrap();
        assert_eq!(argv(&f), ["--json", "install", "premiere", "--method", "download", "--prefix", "/p",
                              "--version", "27", "--allow-unverified", "--speech-packs", "--remove-previous"]);
        install_stream_with(&f.bin, "premiere", "download", "/p", None, &InstallOpts::default(), &mut |_| {}).unwrap();
        assert_eq!(argv(&f), ["--json", "install", "premiere", "--method", "download", "--prefix", "/p"],
                   "speech packs are off unless asked for");
    }

    /// What the finished Mud Hut (build 3c6e464) printed for `check-updates --offline` on two lab
    /// copies and a bad path (lab paths shortened to /lab), shared with the page's tests.
    const REAL_CHECK: &str = include_str!("../../../src/lib/fixtures/check-updates-mudhut-3c6e464.json");

    #[test]
    fn check_passes_the_real_mudhut_output_through() {
        let one_line: Value = serde_json::from_str(REAL_CHECK).unwrap();
        let f = fake_output(&format!("{one_line}\n"), "", 0);
        let ps: Vec<String> = ["/lab/cr/AE2026", "/lab/cr/LrcRelease", "/nonexistent/x"].map(String::from).to_vec();
        let v = check_updates_with(&f.bin, &ps, &opts(None, false, true)).unwrap();
        assert_eq!(v["supported"], true);
        assert_eq!(v["feed"]["fetched_at"], "2026-10-17T02:15:08Z");
        let prem = &v["prefixes"][0]["apps"][1];
        assert_eq!(prem["id"], "premiere");
        assert_eq!(prem["update"]["download_bytes"], 3955197627u64);
        assert_eq!(prem["update"]["changed_bytes"], 3188608272u64);
        assert_eq!(v["prefixes"][1]["apps"][0]["new_major"]["major"], 26);
        assert_eq!(v["prefixes"][2]["ok"], false);
        assert_eq!(v["prefixes"][2]["error"], "This is not a prefix (it has no drive_c folder).");
    }

    /// Collider's exact yearly-install argv against the REAL Mud Hut, with `--dry-run` appended
    /// by the wrapper, so nothing is downloaded or installed (the download path resolves the build
    /// from Adobe's feed, then returns before the lock, the busy check, staging and the installer).
    /// Run in the lab only:
    ///   COLLIDER_REAL_MUDHUT_WRAPPER=<script that sources the lab env and runs
    ///   `exec "$MUDHUT" "$@" --dry-run`> cargo test -- --ignored real_mudhut
    #[test]
    #[ignore]
    fn real_mudhut_takes_the_yearly_install_argv() {
        let bin = std::env::var("COLLIDER_REAL_MUDHUT_WRAPPER").expect("COLLIDER_REAL_MUDHUT_WRAPPER");
        let prefix = std::env::temp_dir().join("collider-real-mudhut-dry-run-prefix");
        let prefix = prefix.to_string_lossy();
        // Audition 25 is not verified (DRAFT list): with the user's confirmation it resolves.
        let o = InstallOpts { version: Some("25".into()), allow_unverified: true, speech_packs: false, remove_previous: true, dry_run: false };
        let mut seen = Vec::new();
        let r = install_stream_with(&bin, "audition", "download", &prefix, None, &o, &mut |v| seen.push(v))
            .expect("the real Mud Hut must accept Collider's argv");
        println!("result: {r}");
        assert_eq!(r["ok"], true);
        assert_eq!(r["dry_run"], true);
        assert!(r["product_version"].as_str().unwrap().starts_with("25."), "{r}");
        assert!(!std::path::Path::new(prefix.as_ref()).exists(), "a dry run must not create the prefix");
        // Without the confirmation Mud Hut refuses, and Collider reports its sentence.
        let o = InstallOpts { allow_unverified: false, ..o };
        let e = install_stream_with(&bin, "audition", "download", &prefix, None, &o, &mut |_| {}).unwrap_err();
        println!("without --allow-unverified: {e}");
        assert!(e.to_string().contains("is not yet tested by Neutron"), "{e}");
    }

    #[test]
    fn run_json_with_reads_the_last_line() {
        let f = fake("echo '{\"event\":\"note\",\"msg\":\"a\"}'\necho '{\"event\":\"result\",\"apps\":[]}'\n");
        let v = run_json_with(&f.bin, &["apps"]).unwrap();
        assert_eq!(v["event"], "result");
    }
}
