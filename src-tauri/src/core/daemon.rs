// core/daemon.rs
//
// Supervises the hwmux finalize daemon (Brief §1.2). Neutron OWNS the daemon
// script + all muxing / symlink / same-filesystem knowledge (Option A); Collider
// only manages its lifecycle: start after a successful launch, stop when the
// Premiere PID exits. All start/stop go through the Neutron CLI wrapper — Collider
// holds no daemon logic of its own.
//
// Reconciled against the shipped CLI: `hwmux start` is idempotent-ish (if already
// running it returns the EXISTING pid rather than starting a second daemon), and
// `hwmux stop` is benign when nothing runs. We track the pid the CLI reports.

use std::path::PathBuf;

pub struct HwmuxDaemon {
    /// PID the CLI reported (whether we started it or it was already running).
    pid: Option<u32>,
    /// The resolved real export dir (documents_real from prefix info).
    watch_dir: PathBuf,
}

impl HwmuxDaemon {
    pub fn new(watch_dir: PathBuf) -> Self {
        Self { pid: None, watch_dir }
    }

    /// Start (or attach to) the daemon watching the resolved real export dir.
    /// Never pass a symlinked prefix path here — inotify would miss events.
    pub fn start(&mut self) -> anyhow::Result<()> {
        let watch = self.watch_dir.to_str().ok_or_else(|| {
            anyhow::anyhow!("watch dir is not valid UTF-8")
        })?;
        let pid = crate::neutron::hwmux_start(watch)?;
        self.pid = Some(pid);
        Ok(())
    }

    /// Stop on app exit. Tie this to the launched Premiere PID exiting.
    /// Benign if the daemon isn't running.
    pub fn stop(&mut self) -> anyhow::Result<()> {
        crate::neutron::hwmux_stop()?;
        self.pid = None;
        Ok(())
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
}
