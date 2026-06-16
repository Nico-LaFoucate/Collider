// core/display.rs
//
// Display-scale detection for HiDPI. Collider DETECTS the primary monitor's
// scale and passes it to the engine via `neutron launch ... --scale <S>`; the
// engine does the LogPixels math + registry/env (engine/cockpit split). This is
// the cockpit half. DE-aware: KDE works now; GNOME/wlroots are clearly-marked
// stubs to fill in later. None => caller omits --scale and the engine
// auto-detects / falls back.

use std::process::Command;
use serde_json::Value;

enum Desktop { Kde, Gnome, Wlroots, Unknown }

fn detect_desktop() -> Desktop {
    let de = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
        .unwrap_or_default()
        .to_lowercase();
    if de.contains("kde") || de.contains("plasma") { Desktop::Kde }
    else if de.contains("gnome") { Desktop::Gnome }
    else if de.contains("sway") || de.contains("hyprland") || de.contains("wlroots") { Desktop::Wlroots }
    else { Desktop::Unknown }
}

/// Detect the PRIMARY monitor's display scale. DE-aware; KDE implemented.
/// `None` => the engine auto-detects / falls back (so this is a safe enhancement).
pub fn detect_display_scale() -> Option<f64> {
    match detect_desktop() {
        Desktop::Kde => kde_scale(),
        Desktop::Gnome => None,    // TODO: gsettings text-scaling-factor / Mutter D-Bus DisplayConfig
        Desktop::Wlroots => None,  // TODO: `swaymsg -t get_outputs` / `hyprctl -j monitors`
        Desktop::Unknown => None,
    }
}

/// KDE: parse `kscreen-doctor --json`. Primary = the enabled output with
/// priority == 1; fallback = the max scale among enabled outputs; else None.
fn kde_scale() -> Option<f64> {
    let out = clean_command("kscreen-doctor").arg("--json").output().ok()?;
    if !out.status.success() { return None; }
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let outputs = v.get("outputs")?.as_array()?;
    let enabled: Vec<&Value> = outputs
        .iter()
        .filter(|o| o.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false))
        .collect();

    // Primary monitor: priority == 1.
    if let Some(p) = enabled
        .iter()
        .find(|o| o.get("priority").and_then(|x| x.as_i64()) == Some(1))
    {
        if let Some(s) = p.get("scale").and_then(|x| x.as_f64()) {
            return Some(s);
        }
    }
    // Fallback: the largest scale among enabled outputs (favor the HiDPI one).
    enabled
        .iter()
        .filter_map(|o| o.get("scale").and_then(|x| x.as_f64()))
        .fold(None, |acc, s| Some(acc.map_or(s, |a: f64| a.max(s))))
}

/// Spawn a system tool with the AppImage's Python/linker env stripped — matches
/// the caution in `neutron/mod.rs::clean_command`. (kscreen-doctor is a system
/// binary, so this is belt-and-suspenders, but consistent with the project.)
fn clean_command(bin: &str) -> Command {
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
