// core/window_rule.rs
//
// Wayland-only workaround: pin Premiere's home/"Welcome" window position via the
// compositor's window-rule mechanism. Wayland forbids a client from positioning
// its own non-transient toplevel, so winewayland forces the home overlay to the
// screen top, covering the menu bar. We can't fix that in the wine build (it'd
// need a Wayland protocol extension), but the compositor CAN place the window —
// so Collider writes a window rule that matches the home overlay and forces its Y.
//
// The home overlay is uniquely identified by an EMPTY window title (the main frame's
// title is "Adobe Premiere Pro 2025"); both share app_id `adobe premiere pro.exe`
// (winewayland sets app_id = lowercased exe basename).
//
// KDE/KWin is implemented (via kwriteconfig6 + a KWin reconfigure, merging into
// kwinrulesrc rather than clobbering other rules). sway/hyprland are clearly-marked
// stubs; GNOME/Mutter has no equivalent (recommend the X11 fallback there). On X11
// or an unsupported compositor this is a clean no-op, so calling it is always safe.

use crate::core::display;
use crate::core::settings::Settings;

/// Stable kwinrulesrc group id for our rule (so we update, never duplicate).
const RULE_ID: &str = "neutron-premiere-home";

/// Every key kde_write() writes. kde_remove() deletes exactly these, because dropping the id
/// from the [General] list leaves the stanza on disk fully populated -- and on a machine that
/// never hand-edited it, still reading `positionrule=2` (Force), one `rules=` edit away from
/// live again. A stale group of this name also collides with a future re-enable, which would
/// write fresh keys over a stanza nobody has audited. Reported by the build tester 2026-09-01
/// after observing the migration leave an armed orphan behind.
const RULE_KEYS: &[&str] = &[
    "Description", "wmclass", "wmclasscomplete", "wmclassmatch",
    "title", "titlematch", "position", "positionrule",
];
const APP_ID: &str = "adobe premiere pro.exe";

/// Can this session's compositor host the home-window rule? (UI gating + skip logic.)
pub fn supported() -> bool {
    display::is_wayland() && display::desktop_kind() == "kde"
}

/// Apply (or remove, if disabled) the home-window rule for the current settings.
/// Best-effort and idempotent: returns Ok(true) if a rule was written, Ok(false)
/// if there was nothing to do (X11 / unsupported / disabled-and-absent).
pub fn apply(settings: &Settings) -> Result<bool, String> {
    if !display::is_wayland() {
        return Ok(false);
    }
    match display::desktop_kind() {
        "kde" => {
            if settings.home_window_fix {
                kde_write(settings.home_window_y)?;
                Ok(true)
            } else {
                kde_remove()?;
                Ok(false)
            }
        }
        // TODO sway: `swaymsg for_window [app_id="adobe premiere pro.exe" title="^$"] move position 0 <y>`
        // TODO hyprland: `hyprctl keyword windowrulev2 "move 0 <y>, class:^(adobe premiere pro.exe)$, title:^$"`
        // GNOME/Mutter: no equivalent — X11 fallback is the answer there.
        _ => Ok(false),
    }
}

// ---- KDE / KWin -----------------------------------------------------------

fn kde_write(y: i32) -> Result<(), String> {
    // Rule group: match app_id substring + exact-empty title (= the home overlay),
    // force position to (0, y). positionrule=2 is "Force" (verified on Plasma 6).
    let pos = format!("0,{}", y);
    let pairs: &[(&str, &str)] = &[
        ("Description", "Neutron: Premiere home screen position"),
        ("wmclass", APP_ID),
        ("wmclasscomplete", "false"),
        ("wmclassmatch", "2"), // 2 = substring
        ("title", ""),
        ("titlematch", "1"),   // 1 = exact (empty title = the home overlay)
        ("position", &pos),
        ("positionrule", "2"), // 2 = Force
    ];
    for (k, v) in pairs {
        kwrite(RULE_ID, k, v)?;
    }
    ensure_in_rule_list()?;
    reconfigure()
}

fn kde_remove() -> Result<(), String> {
    // Drop our id from the [General] rules list, THEN delete the stanza's keys. Deregistering
    // alone leaves the group on disk with positionrule=2 intact; KConfig drops a group once its
    // last key is gone, and kwriteconfig has no --delete-group. We don't error if it wasn't there.
    let ids: Vec<String> = read_rule_list().into_iter().filter(|x| x != RULE_ID).collect();
    write_rule_list(&ids)?;
    for k in RULE_KEYS {
        let _ = display::clean_command(kwriteconfig())
            .args(["--file", "kwinrulesrc", "--group", RULE_ID, "--key", k, "--delete"])
            .status();
    }
    reconfigure()
}

/// Read the [General] `rules` list (comma-separated rule group ids).
fn read_rule_list() -> Vec<String> {
    let out = display::clean_command(kreadconfig())
        .args(["--file", "kwinrulesrc", "--group", "General", "--key", "rules"])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn write_rule_list(ids: &[String]) -> Result<(), String> {
    kwrite_general("rules", &ids.join(","))?;
    kwrite_general("count", &ids.len().to_string())
}

/// Make sure our RULE_ID is in the [General] rules list (+ updated count), without
/// disturbing any rules the user already has.
fn ensure_in_rule_list() -> Result<(), String> {
    let mut ids = read_rule_list();
    if !ids.iter().any(|x| x == RULE_ID) {
        ids.push(RULE_ID.to_string());
        write_rule_list(&ids)?;
    }
    Ok(())
}

fn kwrite(group: &str, key: &str, value: &str) -> Result<(), String> {
    run_ok(
        display::clean_command(kwriteconfig())
            .args(["--file", "kwinrulesrc", "--group", group, "--key", key, value]),
    )
}

fn kwrite_general(key: &str, value: &str) -> Result<(), String> {
    kwrite("General", key, value)
}

fn reconfigure() -> Result<(), String> {
    run_ok(display::clean_command("gdbus").args([
        "call", "--session",
        "--dest", "org.kde.KWin",
        "--object-path", "/KWin",
        "--method", "org.kde.KWin.reconfigure",
    ]))
}

fn run_ok(cmd: &mut std::process::Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("spawn failed: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Plasma 6 ships kwriteconfig6/kreadconfig6; fall back to the unsuffixed names.
fn kwriteconfig() -> &'static str { bin_for("kwriteconfig6", "kwriteconfig") }
fn kreadconfig() -> &'static str { bin_for("kreadconfig6", "kreadconfig") }

fn bin_for(primary: &'static str, fallback: &'static str) -> &'static str {
    if which(primary) { primary } else { fallback }
}

fn which(bin: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v {bin}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
