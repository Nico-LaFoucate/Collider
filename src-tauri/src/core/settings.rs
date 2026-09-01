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
    /// mechanism (KWin today). No-op on X11 / unsupported compositors.
    ///
    /// 🚨 DEFAULT OFF since 2026-09-01, and it used to default ON. The KWin rule
    /// matches "empty caption + wmclass adobe premiere pro.exe", which we believed
    /// uniquely identified the home overlay. It does not: Premiere's SPLASH also maps
    /// with an empty caption, so a Force rule pinned the splash to (0,82) too. On a
    /// build tester's machine that read as "the splash renders top-left every launch,
    /// starting with the drop", and it was chased through 11.10-69, -69r2, -69r3, -70
    /// and -71 entirely inside Wine. A Force rule overrides client AND compositor
    /// placement, so every Wine-side gate we tested came back null for reasons
    /// unrelated to its merit. `home_window_y` is dev-box-specific as well (82 was
    /// verified at 4K @ 1.7x; the tester runs 2.25).
    ///
    /// Re-enabling needs a predicate that cannot match the splash. Until then the
    /// underlying issue is cosmetic and drag-to-fix, which is what our own June
    /// investigation recommended accepting.
    #[serde(default = "default_home_window_fix")]
    pub home_window_fix: bool,
    /// Logical-pixel Y the home window is pinned to. Setup-specific (scale/monitor),
    /// so it's user-tweakable. Default 82 (verified good at 4K @ 1.7×).
    #[serde(default = "default_home_window_y")]
    pub home_window_y: i32,

    /// Bumped when a stored setting must be forcibly corrected on load. Flipping a
    /// serde default only affects configs MISSING the field, so machines that already
    /// persisted `home_window_fix: true` would keep the harmful rule and the repair
    /// would silently do nothing there -- the exact "fix the source, leave the
    /// artifact" failure this project has hit twice. Migration 1 turns the home-window
    /// fix off once and removes any rule we previously installed.
    #[serde(default)]
    pub settings_migration: u32,

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

fn default_home_window_fix() -> bool { false }
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
            settings_migration: SETTINGS_MIGRATION,
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

/// Current stored-settings migration level. Bump when a persisted value must be
/// corrected on machines that already wrote it.
pub const SETTINGS_MIGRATION: u32 = 1;

/// Correct persisted settings that a changed default cannot reach.
///
/// Migration 1 (2026-09-01): `home_window_fix` used to default ON and installs a KWin
/// rule that Force-pins any empty-captioned Premiere window to (0,82). Premiere's
/// SPLASH is empty-captioned, so the rule mispositions it on every launch. Flipping the
/// serde default only helps configs that never stored the field; every machine that did
/// would keep both the setting and the installed rule. So turn it off AND remove the
/// rule we wrote, then record that we did it.
pub fn migrate() {
    let mut s = load();
    if s.settings_migration >= SETTINGS_MIGRATION {
        return;
    }
    if s.settings_migration < 1 {
        s.home_window_fix = false;
        // Remove the rule itself, not just the setting -- repairing the source and
        // leaving the artifact in place is how this bug survived on a tester's machine.
        let _ = crate::core::window_rule::apply(&s);
    }
    s.settings_migration = SETTINGS_MIGRATION;
    let _ = save(&s);
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
