// core/settings.rs
//
// Persisted Collider settings (~/.config/collider/config.json). Loaded at the
// start of a launch and editable from the Preferences tab. Designed to grow:
// the neutron path, export-folder persistence, GPU mode, etc. land here later.
//
// Robustness contract: load() NEVER panics — a missing or corrupt file yields
// defaults, so a bad config can never wedge the app.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// "auto" => detect the display scale; "manual" => use `scale_value`.
    pub scale_mode: String,
    /// The manual display-scale override (used only when scale_mode == "manual").
    pub scale_value: Option<f64>,

    /// Wayland-only: pin Premiere's home/Welcome window so it stops loading too
    /// high over the menu bar (Wayland forbids client toplevel positioning, so
    /// winewayland forces it to the top). Applied via the compositor's window-rule
    /// mechanism (KWin today). Default on; no-op on X11 / unsupported compositors.
    #[serde(default = "default_home_window_fix")]
    pub home_window_fix: bool,
    /// Logical-pixel Y the home window is pinned to. Setup-specific (scale/monitor),
    /// so it's user-tweakable. Default 82 (verified good at 4K @ 1.7×).
    #[serde(default = "default_home_window_y")]
    pub home_window_y: i32,

    /// Window-decoration color theme: a built-in preset id ("dark" | "light") or
    /// "custom". Drives the .reg written into the prefix on launch (core/theme.rs +
    /// core/decoration.rs). Default "dark" (the Neutron default chrome).
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Custom color overrides (Control Panel color key -> "R G B"), used only when
    /// theme == "custom". Missing keys fall back to the Dark preset.
    #[serde(default)]
    pub custom_colors: Option<BTreeMap<String, String>>,

    /// Window-button (caption) icon set: "none" (Wine's default glyphs), a bundled set
    /// id ("macos" | "win11" | "minimal" | "adobe-flat"), or "custom" (imported). Drives
    /// HKCU\Software\Neutron\Caption + the per-button .ico files copied into the prefix
    /// on launch (core/caption_icons.rs + core/decoration.rs). Default "none".
    #[serde(default = "default_button_icon_set")]
    pub button_icon_set: String,

    /// Known wine prefixes. Collider is explicitly multi-prefix (kickoff brief: "app library,
    /// profiles, prefix registry"; README: run multiple Adobe versions side by side, import an
    /// existing prefix) — a real setup here already has four. Empty on first run; populated by
    /// discovery, by the user adding one, or by a Mud Hut install.
    #[serde(default)]
    pub prefixes: Vec<PrefixEntry>,

    /// Path of the selected entry. Collider passes this EXPLICITLY on every engine call, so the
    /// CLI's own DEFAULT_PREFIX never applies to the GUI — "it needs to launch whatever prefix
    /// you point it at". None = nothing chosen yet, which is what triggers first-run setup.
    #[serde(default)]
    pub selected_prefix: Option<String>,
}

/// One registered prefix. `name` is the user's label — the path alone is a poor identifier
/// when several prefixes are differently named holdovers (`.premiere2025` predates the suite).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixEntry {
    pub name: String,
    pub path: String,
}

fn default_home_window_fix() -> bool { true }
fn default_home_window_y() -> i32 { 82 }
fn default_theme() -> String { "dark".into() }
fn default_button_icon_set() -> String { "none".into() }

impl Default for Settings {
    fn default() -> Self {
        Self {
            scale_mode: "auto".into(),
            scale_value: None,
            home_window_fix: default_home_window_fix(),
            home_window_y: default_home_window_y(),
            theme: default_theme(),
            custom_colors: None,
            button_icon_set: default_button_icon_set(),
            prefixes: Vec::new(),
            selected_prefix: None,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/collider/config.json"))
}

/// Load settings; defaults if the file is absent or unreadable/corrupt (never panics).
pub fn load() -> Settings {
    let Some(p) = config_path() else { return Settings::default() };
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Persist settings, creating ~/.config/collider/ if needed.
pub fn save(s: &Settings) -> std::io::Result<()> {
    let Some(p) = config_path() else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no HOME directory"));
    };
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(s)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&p, json)
}

/// The display scale to pass to the engine: the manual override when set, else
/// the detected primary-monitor scale, else None (engine auto-detects). This is
/// the "auto unless overridden" rule the Preferences scale control drives.
pub fn effective_scale() -> Option<f64> {
    let s = load();
    if s.scale_mode == "manual" {
        if let Some(v) = s.scale_value {
            if v > 0.0 {
                return Some(v);
            }
        }
    }
    crate::core::display::detect_display_scale()
}
