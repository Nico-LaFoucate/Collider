// core/display.rs
//
// Desktop facts for UI gating (Wayland? which desktop?) and the display-scale readout. Scale
// detection itself is the engine's (`neutron display`); at launch Collider passes `--scale` only
// for a manual override, and the engine detects otherwise.

use std::process::Command;

pub(crate) enum Desktop { Kde, Gnome, Wlroots, Unknown }

/// True when running in a Wayland session (the only place the home-window rule applies).
pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").map(|s| s == "wayland").unwrap_or(false)
}

/// Stable string for the current desktop ("kde" | "gnome" | "wlroots" | "unknown"),
/// for UI gating and per-compositor rule handling.
pub fn desktop_kind() -> &'static str {
    match detect_desktop() {
        Desktop::Kde => "kde",
        Desktop::Gnome => "gnome",
        Desktop::Wlroots => "wlroots",
        Desktop::Unknown => "unknown",
    }
}

pub(crate) fn detect_desktop() -> Desktop {
    let de = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
        .unwrap_or_default()
        .to_lowercase();
    if de.contains("kde") || de.contains("plasma") { Desktop::Kde }
    else if de.contains("gnome") { Desktop::Gnome }
    else if de.contains("sway") || de.contains("hyprland") || de.contains("wlroots") { Desktop::Wlroots }
    else { Desktop::Unknown }
}

/// Spawn a system tool with the AppImage's Python/linker env stripped — matches
/// the caution in `neutron/mod.rs::clean_command`. (kscreen-doctor is a system
/// binary, so this is belt-and-suspenders, but consistent with the project.)
pub(crate) fn clean_command(bin: &str) -> Command {
    let mut cmd = Command::new(bin);
    for var in [
        "PYTHONHOME", "PYTHONPATH", "PYTHONDONTWRITEBYTECODE", "PYTHONNOUSERSITE",
        "LD_LIBRARY_PATH", "LD_PRELOAD",
        "GST_PLUGIN_SYSTEM_PATH", "GST_PLUGIN_SYSTEM_PATH_1_0",
        "GDK_PIXBUF_MODULE_FILE", "GDK_PIXBUF_MODULEDIR",
    ] {
        cmd.env_remove(var);
    }
    cmd
}
