// core/launch.rs
//
// The launch loop, generalized to ANY app in the Neutron catalog. Collider keeps
// one supervised session PER app (several apps can run at once). The flow:
//   1. prefix info         -> health gate
//   2. apply-display-fix   -> ensure DS.DisableDirectXDisplay (idempotent)
//   2b. decoration         -> dark caption + KWin home-position script (best-effort)
//   3. neutron launch <app> -> get the PID to supervise
//   ... app runs ...  4. on exit -> `neutron teardown --app <id>`
//
// NOTE (2026-08-06): step 4 used to be `hwmux start`, a daemon that finalized Premiere's exports.
// That daemon was a WORKAROUND for broken muxing and was retired from the engine when the native
// ucrtbase muxing fix landed (neutron commit 678cb60 "retire collider-hwmux"). Collider was never
// updated, so `neutron hwmux` returned an argparse error and every export-app launch reported
// Failed even though the app came up fine. The step is gone.

use serde::Serialize;
use crate::core::prefix::PrefixInfo;

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
    Running { premiere_pid: u32, display: String, neutron_wine: bool },
    Failed { at: String, reason: String },
    Stopped,
}

/// One supervised app session: the app id, its PID, and the prefix it runs in
/// (kept so exit can ask the engine for an app-scoped teardown).
pub struct LaunchSession {
    pub step: LaunchStep,
    pub app_id: String,
    pub pid: Option<u32>,
    prefix: String,
}

impl LaunchSession {
    pub fn new() -> Self {
        Self { step: LaunchStep::Idle, app_id: String::new(), pid: None, prefix: String::new() }
    }

    /// Run the launch loop for `app_id`. Each failure records WHICH step failed and
    /// the engine's own reason so the UI can show it.
    pub fn launch(
        &mut self,
        app_id: &str,
        prefix: &str,
        project: Option<&str>,
    ) -> &LaunchStep {
        self.app_id = app_id.to_string();
        self.prefix = prefix.to_string();

        // 1. Resolve paths — also the prefix health gate (exit 2 = invalid).
        self.step = LaunchStep::Resolving;
        // The result is only a health gate now — documents_real was the hwmux watch target.
        let _info: PrefixInfo = match PrefixInfo::detect(&prefix.into()) {
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

        self.step = LaunchStep::Running { premiere_pid: pid, display, neutron_wine };
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
        // Ask the ENGINE to tear this app down, app-scoped. A bare kill_group(pid) — which is all
        // this used to do — leaves Adobe's per-app helpers behind, and those orphans accumulate
        // across launches and wedge the NEXT app (Lightroom deadlocks on ntdll's loader_section
        // behind them). The engine kills only what belongs to THIS app, so the other apps sharing
        // the prefix keep running, and it sweeps the shared daemons itself when the last one exits.
        if !self.prefix.is_empty() {
            if let Err(e) = crate::neutron::teardown_app(&self.app_id, &self.prefix) {
                eprintln!("[collider] teardown of {} failed: {e}", self.app_id);
            }
        }
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
