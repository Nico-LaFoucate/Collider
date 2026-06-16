// core/launch.rs
//
// The MVP launch loop, encoded as an explicit sequence so the UI can show where
// it is and recover cleanly if a step fails. This is the heart of Collider:
// everything we did by hand across the Neutron investigation, in order, done right.
//
// The sequence (from COLLIDER_INTEGRATION_HANDOFF.md "path-resolution flow"):
//   1. prefix info        -> read documents_real (the hwmux watch target)
//   2. apply-display-fix  -> ensure DS.DisableDirectXDisplay (idempotent)
//   3. launch premiere    -> get the Premiere PID to supervise
//   4. hwmux start        -> watch documents_real; get the daemon PID
//   ... Premiere runs ...
//   5. on Premiere exit   -> hwmux stop
//
// Collider never computes paths: documents_real flows from step 1 into step 4.

use serde::Serialize;
use crate::core::prefix::PrefixInfo;
use crate::core::daemon::HwmuxDaemon;

/// Where the launch loop is. The UI renders this directly as status.
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

/// One supervised Premiere session. Holds the PIDs and the daemon handle so the
/// UI can poll status and so teardown stops the daemon when Premiere exits.
pub struct LaunchSession {
    pub step: LaunchStep,
    pub premiere_pid: Option<u32>,
    daemon: Option<HwmuxDaemon>,
}

impl LaunchSession {
    pub fn new() -> Self {
        Self { step: LaunchStep::Idle, premiere_pid: None, daemon: None }
    }

    /// Run the full MVP launch loop for Premiere. Each failure records WHICH step
    /// failed and the engine's own reason, so the UI can show "close Premiere
    /// first" (exit 3) rather than a generic error.
    ///
    /// `export_dir`: where Premiere's exports actually land, which is what hwmux
    /// must watch. If None, we default to the prefix's resolved Documents path
    /// (`documents_real`). A creator who exports elsewhere passes their real
    /// export folder here so the muxer watches the right place (Brief §4 / §1.2).
    pub fn launch_premiere(
        &mut self,
        prefix: &str,
        project: Option<&str>,
        export_dir: Option<&str>,
    ) -> &LaunchStep {
        // 1. Resolve paths — this is also our prefix health gate (exit 2 = invalid).
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

        // 2b. Best-effort: install the Wayland home-window position rule before the
        //     window appears, so the home screen opens in the right place. Purely
        //     cosmetic and compositor-side — never block or fail the launch over it.
        let _ = crate::core::window_rule::apply(&crate::core::settings::load());

        // 3. Launch Premiere (GPU only in v0) — get the PID to supervise.
        //    Resolve the display scale cockpit-side (primary monitor) and pass it;
        //    the engine turns it into LogPixels. None => engine auto-detects.
        //    (Layer 3 will let a manual Preferences override win here.)
        self.step = LaunchStep::LaunchingPremiere;
        let scale = crate::core::settings::effective_scale();
        let result = match crate::neutron::launch_premiere(prefix, project, scale) {
            Ok(r) => r,
            Err(e) => return self.fail("launch", &e.to_string()),
        };
        let premiere_pid = result.pid;
        let display = result.display;
        let neutron_wine = result.neutron_wine;
        self.premiere_pid = Some(premiere_pid);

        // 4. Start hwmux watching the export dir. Override if the user set one,
        //    otherwise the resolved Documents path. Either way it's a real
        //    filesystem path Neutron resolved or the user chose — never a guess.
        self.step = LaunchStep::StartingDaemon;
        let watch_dir: std::path::PathBuf = match export_dir {
            Some(d) => d.into(),
            None => info.documents_real.clone(),
        };
        let mut daemon = HwmuxDaemon::new(watch_dir);
        if let Err(e) = daemon.start() {
            // Premiere is up but the daemon failed — surface it, but don't kill
            // Premiere; exports just won't auto-mux until the daemon is retried.
            return self.fail("daemon", &e.to_string());
        }
        let daemon_pid = daemon.pid().unwrap_or(0);
        self.daemon = Some(daemon);

        self.step = LaunchStep::Running { premiere_pid, daemon_pid, display, neutron_wine };
        &self.step
    }

    /// Is the Premiere process still alive? Uses kill(pid, 0) — sends no signal,
    /// just checks existence. Linux/Unix only (fine: target is CachyOS).
    /// Returns false if there's no tracked PID.
    pub fn is_premiere_alive(&self) -> bool {
        match self.premiere_pid {
            Some(pid) => pid_alive(pid),
            None => false,
        }
    }

    /// The auto-detected clean exit: the user closed Premiere normally and the
    /// frontend's liveness poll noticed. Stop the muxer, make sure Premiere is
    /// really gone, and reset to Idle so the button returns to "Launch".
    pub fn clean_exit(&mut self) -> &LaunchStep {
        if let Some(d) = self.daemon.as_mut() {
            let _ = d.stop();
        }
        self.daemon = None;
        // Belt-and-suspenders: if the process somehow lingers, signal its group.
        if let Some(pid) = self.premiere_pid {
            if pid_alive(pid) {
                let _ = kill_group(pid);
            }
        }
        self.premiere_pid = None;
        self.step = LaunchStep::Idle;
        &self.step
    }

    /// The manual escape hatch: Premiere is hung and the user picked "Force quit"
    /// from the Running button. Kill Premiere's process group, then clean up.
    pub fn force_quit(&mut self) -> &LaunchStep {
        if let Some(pid) = self.premiere_pid {
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
/// Premiere process tree spawned detached via start_new_session — go down too.
fn kill_group(pid: u32) -> std::io::Result<()> {
    let r = unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGTERM) };
    if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

impl Default for LaunchSession {
    fn default() -> Self { Self::new() }
}
