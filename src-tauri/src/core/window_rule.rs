// core/window_rule.rs
//
// Wayland-only workaround: pin Premiere's home/"Welcome" window position via the compositor's
// window-rule mechanism. Wayland forbids a client from positioning its own non-transient
// toplevel, so winewayland's home overlay lands at the screen top, covering the menu bar; the
// compositor CAN place it. The rule itself -- what it matches (app_id containing
// `neutron-premiere-`, empty title) and how it is merged into kwinrulesrc and removed again -- is
// the engine's: `neutron window-rule premiere-home --y N | --off` (DECISIONS C20). Collider owns
// the Preferences toggle and the Y value.
//
// KDE/KWin only; on X11 or another compositor this is a clean no-op, so calling it is always safe.

use crate::core::display;
use crate::core::settings::Settings;

/// Can this session's compositor host the home-window rule? (UI gating + skip logic.)
pub fn supported() -> bool {
    display::is_wayland() && display::desktop_kind() == "kde"
}

/// Apply (or remove, if disabled) the home-window rule for the current settings.
/// Best-effort and idempotent: returns Ok(true) if a rule is in place, Ok(false)
/// if there was nothing to do (X11 / unsupported / disabled).
pub fn apply(settings: &Settings) -> Result<bool, String> {
    if !supported() {
        return Ok(false);
    }
    crate::neutron::home_window_rule(settings.home_window_fix, settings.home_window_y)
        .map(|v| v.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false))
        .map_err(|e| e.to_string())
}
