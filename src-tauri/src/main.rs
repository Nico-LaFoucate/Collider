// main.rs — thin binary entry; all logic lives in lib.rs.
//
// Before handing off to the app, we set up the runtime environment so the
// packaged AppImage "just works" on a double-click — no shell env required.
// Two problems this solves, both caused by the AppImage running with a
// stripped-down environment that doesn't match the user's interactive shell:
//
//   1. Rendering: WebKitGTK + NVIDIA + Wayland needs DMA-BUF disabled and
//      X11/XWayland, or the window fails to open (Gdk protocol error). We set
//      these here so the user never types them.
//   2. PATH: the Neutron CLI lives in ~/.local/bin (and friends), which an
//      AppImage's minimal PATH often omits — so `neutron` isn't found and every
//      call fails with "non-JSON on stdout". We prepend the standard user bin
//      dirs so `neutron` resolves however the app was launched.
//
// All of this only SETS values that aren't already present, so a user who
// already configured their environment (or a future Collider setting) wins.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::path::PathBuf;

fn main() {
    // `--version` answers without opening a window (bug reports, `neutron --version`).
    if std::env::args().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("Collider {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    setup_runtime_env();
    collider_lib::run()
}

/// Configure rendering + PATH so the packaged app runs cleanly without the user
/// having to set anything. Only fills in values that are missing.
fn setup_runtime_env() {
    // --- Rendering (NVIDIA / Wayland / WebKitGTK) ---
    // Disable the DMA-BUF renderer that breaks under NVIDIA + Wayland.
    if env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    // Prefer X11/XWayland for GTK, which avoids the Wayland protocol error.
    // Only force this if the user hasn't chosen a backend themselves.
    if env::var_os("GDK_BACKEND").is_none() {
        env::set_var("GDK_BACKEND", "x11");
    }

    // --- PATH (so `neutron` is found under a stripped AppImage env) ---
    augment_path();
}

/// Prepend the standard user/system bin directories to PATH if they're not
/// already present, so the Neutron CLI (typically ~/.local/bin/neutron) resolves
/// even when the AppImage launched with a minimal PATH.
fn augment_path() {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join("bin"));
    }
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/usr/bin"));

    // Existing PATH entries (so we add, never replace).
    let existing = env::var_os("PATH").unwrap_or_default();
    let mut entries: Vec<PathBuf> = env::split_paths(&existing).collect();

    // Prepend any standard dir that's missing, preserving order/uniqueness.
    for d in dirs.into_iter().rev() {
        if !entries.contains(&d) {
            entries.insert(0, d);
        }
    }

    if let Ok(joined) = env::join_paths(entries) {
        env::set_var("PATH", joined);
    }
}
