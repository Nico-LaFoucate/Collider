// core/launch.rs
//
// The launch loop, generalized to ANY app in the Neutron catalog. Collider keeps
// one supervised session PER app (several apps can run at once), each with its
// own status + optional export daemon. The flow:
//   1. prefix info         -> health gate + documents_real (the export watch target)
//   2. apply-display-fix   -> ensure DS.DisableDirectXDisplay (idempotent)
//   2b. decoration         -> dark caption + KWin home-position script (best-effort)
//   3. neutron launch <app> -> get the PID to supervise
//   4. (export apps only)  -> hwmux start on the export dir; get the daemon PID
//   ... app runs ...  5. on exit -> hwmux stop
//
// Collider never computes paths: documents_real flows from step 1 into step 4.

use serde::Serialize;
use crate::core::prefix::PrefixInfo;
use crate::core::daemon::HwmuxDaemon;

/// Whether an app has a hardware-export pipeline (→ Collider runs the hwmux
/// daemon while it's up). Mirrors the `export` flag in the CLI's APP_PROFILES;
/// kept here so a launch doesn't need an extra catalog round-trip.
pub fn app_has_export(app_id: &str) -> bool {
    matches!(app_id, "premiere" | "mediaencoder")
}

/// Where the launch loop is. The UI renders this directly as status.
/// (Variant/field names are still Premiere-flavored from the MVP; the frontend
/// maps them to a progress bar. P2 renames them generically when it touches the UI.)
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "step", content = "detail")]
pub enum LaunchStep {
    Idle,
    Resolving,                 // prefix info
    ApplyingDisplayFix,
    LaunchingPremiere,
    StartingDaemon,
    Running { premiere_pid: u32, daemon_pid: u32, display: String, neutron_wine: bool },
    Failed { at: String, reason: String },
    Stopped,
}

/// One supervised app session: the app id, its PID, and (for export apps) the
/// hwmux daemon handle so teardown stops the daemon when the app exits.
pub struct LaunchSession {
    pub step: LaunchStep,
    pub app_id: String,
    pub pid: Option<u32>,
    daemon: Option<HwmuxDaemon>,
}

impl LaunchSession {
    pub fn new() -> Self {
        Self { step: LaunchStep::Idle, app_id: String::new(), pid: None, daemon: None }
    }

    /// Run the launch loop for `app_id`. `has_export` gates the hwmux daemon.
    /// `export_dir`: where exports land (the daemon's watch target); None => the
    /// prefix's resolved Documents path (`documents_real`). Each failure records
    /// WHICH step failed and the engine's own reason so the UI can show it.
    pub fn launch(
        &mut self,
        app_id: &str,
        prefix: &str,
        project: Option<&str>,
        export_dir: Option<&str>,
        has_export: bool,
    ) -> &LaunchStep {
        self.app_id = app_id.to_string();

        // 1. Resolve paths — also the prefix health gate (exit 2 = invalid).
        self.step = LaunchStep::Resolving;
        let info: PrefixInfo = match PrefixInfo::detect(&prefix.into()) {
            Ok(i) if i.valid => i,
            Ok(_) => return self.fail("resolving", "prefix is not valid"),
            Err(e) => return self.fail("resolving", &e.to_string()),
        };

        // 2. Display fix — idempotent; exit 3 if Premiere is already running.
        self.step = LaunchStep::ApplyingDisplayFix;
        if let Err(e) = crate::neutron::apply_display_fix(prefix) {
            return self.fail("display-fix", &e.to_string());
        }

        // 2b. Best-effort decoration (dark caption + home-position script). Cosmetic;
        //     never block a launch on it.
        let _ = crate::core::decoration::apply(prefix);

        // 3. Launch the app — get the PID to supervise. Scale resolved cockpit-side
        //    (primary monitor); None => the engine auto-detects.
        self.step = LaunchStep::LaunchingPremiere;
        let scale = crate::core::settings::effective_scale();
        let result = match crate::neutron::launch_app(app_id, prefix, project, scale) {
            Ok(r) => r,
            Err(e) => return self.fail("launch", &e.to_string()),
        };
        let pid = result.pid;
        let display = result.display;
        let neutron_wine = result.neutron_wine;
        self.pid = Some(pid);

        // 4. Export apps (Premiere / Media Encoder): start hwmux watching the export
        //    dir — the user's override, else the resolved Documents path. Non-export
        //    apps skip this entirely (daemon_pid = 0).
        let daemon_pid = if has_export {
            self.step = LaunchStep::StartingDaemon;
            let watch_dir: std::path::PathBuf = match export_dir {
                Some(d) => d.into(),
                None => info.documents_real.clone(),
            };
            let mut daemon = HwmuxDaemon::new(watch_dir);
            if let Err(e) = daemon.start() {
                // App is up but the daemon failed — surface it; don't kill the app.
                return self.fail("daemon", &e.to_string());
            }
            let dpid = daemon.pid().unwrap_or(0);
            self.daemon = Some(daemon);
            dpid
        } else {
            0
        };

        self.step = LaunchStep::Running { premiere_pid: pid, daemon_pid, display, neutron_wine };
        &self.step
    }

    /// Is the supervised process still alive? kill(pid, 0) — sends no signal, just
    /// checks existence. Returns false if there's no tracked PID.
    pub fn is_alive(&self) -> bool {
        match self.pid {
            Some(pid) => pid_alive(pid),
            None => false,
        }
    }

    /// The auto-detected clean exit: the user closed the app normally and the
    /// frontend's liveness poll noticed. Stop the muxer, make sure the app is
    /// really gone, and reset to Idle so the button returns to "Launch".
    pub fn clean_exit(&mut self) -> &LaunchStep {
        if let Some(d) = self.daemon.as_mut() {
            let _ = d.stop();
        }
        self.daemon = None;
        // Belt-and-suspenders: if the process somehow lingers, signal its group.
        if let Some(pid) = self.pid {
            if pid_alive(pid) {
                let _ = kill_group(pid);
            }
        }
        self.pid = None;
        self.step = LaunchStep::Idle;
        &self.step
    }

    /// The manual escape hatch: the app is hung and the user picked "Force quit".
    /// Kill the app's process group, then clean up.
    pub fn force_quit(&mut self) -> &LaunchStep {
        if let Some(pid) = self.pid {
            let _ = kill_group(pid);
        }
        self.clean_exit()
    }

    fn fail(&mut self, at: &str, reason: &str) -> &LaunchStep {
        self.step = LaunchStep::Failed { at: at.to_string(), reason: reason.to_string() };
        &self.step
    }
}

/// kill(pid, 0): no signal sent, just checks whether the process exists.
fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Kill the whole process group (negative PID) with SIGTERM so children — the
/// app process tree spawned detached via start_new_session — go down too.
fn kill_group(pid: u32) -> std::io::Result<()> {
    let r = unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGTERM) };
    if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

impl Default for LaunchSession {
    fn default() -> Self { Self::new() }
}
