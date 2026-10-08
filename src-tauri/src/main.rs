// main.rs — thin binary entry; all logic lives in lib.rs.
//
// Before handing off to the app, we set up the runtime environment so the
// packaged AppImage "just works" on a double-click — no shell env required.
// Three problems this solves, all caused by the AppImage running with an
// environment that doesn't match the user's desktop session:
//
//   1. Display backend: the AppImage's GTK hook (apprun-hooks/linuxdeploy-plugin-gtk.sh)
//      forces GDK_BACKEND=x11 for everyone, so on a Wayland desktop Collider ran
//      through XWayland with the compositor's X11 frame around it. Collider draws
//      its own title bar (decorations are off in tauri.conf.json; the bar lives in
//      +page.svelte), so on Wayland we take the native window back.
//   2. Rendering: WebKitGTK's DMA-BUF renderer does not work on the NVIDIA driver
//      (Collider only renders there with it off). We disable it on NVIDIA so the
//      user never types the variable.
//   3. PATH: the Neutron CLI lives in ~/.local/bin (and friends), which an
//      AppImage's minimal PATH often omits — so `neutron` isn't found and every
//      call fails with "non-JSON on stdout". We prepend the standard user bin
//      dirs so `neutron` resolves however the app was launched.
//
// Values the hook sets for its own reasons (GDK_BACKEND, GTK_THEME) are overridden
// on purpose; everything else only fills in what is missing, so a user who
// configured their environment (or a future Collider setting) wins. The hook's
// own overrides (APPIMAGE_GTK_THEME) and COLLIDER_X11 are the escape hatches.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    // `--version` answers without opening a window (bug reports, `neutron --version`).
    if std::env::args().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("Collider {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    setup_runtime_env();
    collider_lib::run()
}

/// Configure the display backend, rendering and PATH so the packaged app runs
/// cleanly without the user having to set anything. Runs before GTK is
/// initialized: GDK reads these variables when it opens the display.
fn setup_runtime_env() {
    // --- Display backend (Wayland / X11) ---
    // On a Wayland session, run as a native Wayland window. The AppImage hook
    // exported GDK_BACKEND=x11 before we started (Tauri issue 8541: an older
    // WebKitGTK crashed on Wayland), which costs a fractional-scale desktop its
    // crisp window and leaves the compositor's X11 frame around ours. X11
    // desktops keep X11; COLLIDER_X11=1 keeps it on Wayland too.
    let on_wayland = env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty());
    if env_flag("COLLIDER_X11") {
        env::set_var("GDK_BACKEND", "x11");
    } else if on_wayland {
        env::set_var("GDK_BACKEND", "wayland");
    } else if env::var_os("GDK_BACKEND").is_none() {
        env::set_var("GDK_BACKEND", "x11");
    }

    // --- Rendering (NVIDIA / WebKitGTK) ---
    // WebKitGTK's DMA-BUF renderer does not work on the NVIDIA driver; Collider
    // only renders there with it off. Mesa drivers render fine with it, so only
    // NVIDIA gets the fallback.
    if Path::new("/proc/driver/nvidia").exists()
        && env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
    {
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    // --- GTK theme (native popups) ---
    // The hook picks Adwaita:light unless GNOME's gtk-theme setting says "dark",
    // which a KDE desktop never does. Collider is a dark-only UI, and the few
    // widgets WebKitGTK still draws natively (the select list, the file chooser)
    // should not pop up light. APPIMAGE_GTK_THEME is the hook's own override and
    // stays the user's call.
    if env::var_os("APPIMAGE_GTK_THEME").is_none() {
        env::set_var("GTK_THEME", "Adwaita:dark");
    }

    // --- PATH (so `neutron` is found under a stripped AppImage env) ---
    augment_path();
}

/// True when an on/off environment variable is set to anything but "" or "0".
fn env_flag(name: &str) -> bool {
    env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0")
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
