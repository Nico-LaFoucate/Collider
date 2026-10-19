// core/settings.rs
//
// Persisted Collider settings ($XDG_CONFIG_HOME/collider/config.json, see core/xdg.rs). Loaded at the
// start of a launch and editable from the Preferences tab. Designed to grow:
// the neutron path, export-folder persistence, GPU mode, etc. land here later.
//
// Robustness contract: load() NEVER panics — a missing or corrupt file yields
// defaults, so a bad config can never wedge the app.
//
// Keys this struct no longer has are ignored on load (serde's default; do NOT add
// `deny_unknown_fields`). Collider 0.1.0 wrote `home_window_fix`, `home_window_y` and
// `settings_migration` for the Premiere home-screen position setting, removed in 0.1.1;
// a config that still carries them must load with every other setting intact, or the
// fallback to defaults would forget the user's prefixes. The test below holds that.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// "auto" => detect the display scale; "manual" => use `scale_value`.
    pub scale_mode: String,
    /// The manual display-scale override (used only when scale_mode == "manual").
    pub scale_value: Option<f64>,

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

    /// Ask Mud Hut which Adobe app updates exist: about 45 s after Collider starts, then on a
    /// 30-minute tick that lets Mud Hut's own cache contact Adobe at most once a day. Only while
    /// Collider is open; nothing is ever installed without the user choosing to. Default on
    /// (DECISIONS C70).
    #[serde(default = "default_true")]
    pub adobe_update_check: bool,

    /// Let Mud Hut keep a backup copy of the app before it updates it, so a failed update is put
    /// back automatically. Off passes `--no-snapshot` (the copy can cost disk space where the
    /// filesystem cannot share it). Default on (DECISIONS C70).
    #[serde(default = "default_true")]
    pub adobe_update_snapshot: bool,
}

/// One registered prefix. `name` is the user's label — the path alone is a poor identifier
/// when several prefixes are differently named holdovers (`.premiere2025` predates the suite).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixEntry {
    pub name: String,
    pub path: String,
}

fn default_theme() -> String { "dark".into() }
fn default_button_icon_set() -> String { "none".into() }
fn default_true() -> bool { true }

impl Default for Settings {
    fn default() -> Self {
        Self {
            scale_mode: "auto".into(),
            scale_value: None,
            theme: default_theme(),
            custom_colors: None,
            button_icon_set: default_button_icon_set(),
            prefixes: Vec::new(),
            selected_prefix: None,
            adobe_update_check: true,
            adobe_update_snapshot: true,
        }
    }
}

/// The settings folder as the Preferences page shows it ("~/.config/collider"; a custom
/// XDG_CONFIG_HOME shows its own path). The page used to say ~/.config/collider unconditionally.
pub fn location() -> Option<String> {
    let dir = crate::core::xdg::collider_config_dir()?;
    Some(tilde(&dir, std::env::var_os("HOME").map(PathBuf::from).as_deref()))
}

fn tilde(dir: &std::path::Path, home: Option<&std::path::Path>) -> String {
    match home.and_then(|h| dir.strip_prefix(h).ok()) {
        Some(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => dir.display().to_string(),
    }
}

fn config_path() -> Option<PathBuf> {
    Some(crate::core::xdg::collider_config_dir()?.join("config.json"))
}

/// Load settings; defaults if the file is absent or unreadable/corrupt (never panics).
pub fn load() -> Settings {
    let Some(p) = config_path() else { return Settings::default() };
    match std::fs::read_to_string(&p) {
        Ok(s) => parse(&s),
        Err(_) => Settings::default(),
    }
}

/// The stored JSON as Settings; defaults if it does not parse.
fn parse(json: &str) -> Settings {
    serde_json::from_str(json).unwrap_or_default()
}

/// Persist settings, creating the settings folder if needed.
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

/// Apply a save from the Preferences tab onto the stored settings.
///
/// The page sends the fields it owns. It used to send an explicit list, and `set_settings` wrote
/// the whole struct, so any setting missing from that list (every new one, until someone
/// remembered the list) was reset to its default the moment the user touched any preference.
/// Merging onto what is stored makes an omitted field keep its value instead.
///
/// `prefixes` and `selected_prefix` belong to the prefix registry (core/prefixes.rs), never to
/// Preferences: a page's copy of them can be stale, so they are ignored here.
pub fn merge_patch(stored: &Settings, patch: &serde_json::Value) -> Result<Settings, String> {
    let mut base = serde_json::to_value(stored).map_err(|e| e.to_string())?;
    let (Some(b), Some(p)) = (base.as_object_mut(), patch.as_object()) else {
        return Err("settings must be a JSON object".into());
    };
    for (k, v) in p {
        if k == "prefixes" || k == "selected_prefix" {
            continue;
        }
        b.insert(k.clone(), v.clone());
    }
    serde_json::from_value(base).map_err(|e| format!("invalid settings: {e}"))
}

/// The display scale to pass to the engine: the manual override when set, else None, and
/// `neutron launch` detects the desktop's scale itself (the one detector, DECISIONS C4b).
pub fn effective_scale() -> Option<f64> {
    let s = load();
    if s.scale_mode == "manual" {
        if let Some(v) = s.scale_value {
            if v > 0.0 {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_shows_home_as_tilde() {
        let home = std::path::Path::new("/home/u");
        assert_eq!(tilde(std::path::Path::new("/home/u/.config/collider"), Some(home)), "~/.config/collider");
        assert_eq!(tilde(std::path::Path::new("/srv/cfg/collider"), Some(home)), "/srv/cfg/collider");
        assert_eq!(tilde(std::path::Path::new("/home/user2/collider"), Some(home)), "/home/user2/collider");
    }

    // A config written by Collider 0.1.0, which still had the Premiere home-screen position
    // setting. Its keys are gone from Settings; the file must still load as the user's settings,
    // not fall back to defaults (which would drop the registered prefixes).
    const CONFIG_0_1_0: &str = r#"{
  "scale_mode": "manual",
  "scale_value": 1.75,
  "home_window_fix": true,
  "home_window_y": 82,
  "settings_migration": 2,
  "theme": "light",
  "custom_colors": null,
  "button_icon_set": "win11",
  "prefixes": [
    { "name": "Adobe 2025", "path": "/data/prefixes/adobe-2025" }
  ],
  "selected_prefix": "/data/prefixes/adobe-2025"
}"#;

    #[test]
    fn config_with_removed_home_screen_keys_loads() {
        let s: Settings = serde_json::from_str(CONFIG_0_1_0)
            .expect("a 0.1.0 config with the removed home-screen keys must deserialize");
        assert_eq!(s.scale_mode, "manual");
        assert_eq!(s.scale_value, Some(1.75));
        assert_eq!(s.theme, "light");
        assert_eq!(s.button_icon_set, "win11");
        assert_eq!(s.prefixes.len(), 1);
        assert_eq!(s.prefixes[0].name, "Adobe 2025");
        assert_eq!(s.selected_prefix.as_deref(), Some("/data/prefixes/adobe-2025"));

        // A config from before the Adobe update settings gets their defaults (both on).
        assert!(s.adobe_update_check);
        assert!(s.adobe_update_snapshot);

        // load() goes through parse(), which must keep the same settings rather than defaults.
        let p = parse(CONFIG_0_1_0);
        assert_eq!(p.prefixes.len(), 1);
        assert_eq!(p.theme, "light");

        // Saving writes the current keys only, so the old ones drop out of the file.
        let out = serde_json::to_string(&s).unwrap();
        for gone in ["home_window_fix", "home_window_y", "settings_migration"] {
            assert!(!out.contains(gone), "{gone} should not be written any more");
        }
    }

    /// The Preferences save trap: a save that does not mention a setting must keep it, not
    /// reset it to its default.
    #[test]
    fn a_preferences_save_keeps_what_it_does_not_mention() {
        let stored = parse(CONFIG_0_1_0);
        let patch = serde_json::json!({ "theme": "dark" });
        let s = merge_patch(&stored, &patch).unwrap();
        assert_eq!(s.theme, "dark", "the field it sent changes");
        assert_eq!(s.button_icon_set, "win11", "a field it left out keeps its stored value");
        assert_eq!(s.scale_mode, "manual");
        assert_eq!(s.scale_value, Some(1.75));
        assert_eq!(s.prefixes.len(), 1);
    }

    /// The page's copy of the registry can be stale (a prefix added since it loaded settings), so
    /// a save never writes it, empty or not.
    #[test]
    fn a_preferences_save_never_touches_the_prefix_registry() {
        let stored = parse(CONFIG_0_1_0);
        for patch in [
            serde_json::json!({ "prefixes": [], "selected_prefix": null }),
            serde_json::json!({ "prefixes": [{ "name": "Old", "path": "/old" }], "selected_prefix": "/old" }),
        ] {
            let s = merge_patch(&stored, &patch).unwrap();
            assert_eq!(s.prefixes.len(), 1);
            assert_eq!(s.prefixes[0].path, "/data/prefixes/adobe-2025");
            assert_eq!(s.selected_prefix.as_deref(), Some("/data/prefixes/adobe-2025"));
        }
    }

    /// The update settings are the first ones added since the trap was fixed: turning one off
    /// must survive a later save of an unrelated preference.
    #[test]
    fn update_settings_survive_an_unrelated_save() {
        let stored = merge_patch(&parse(CONFIG_0_1_0),
                                 &serde_json::json!({ "adobe_update_snapshot": false })).unwrap();
        assert!(!stored.adobe_update_snapshot);
        let s = merge_patch(&stored, &serde_json::json!({ "theme": "dark", "scale_mode": "auto" })).unwrap();
        assert!(!s.adobe_update_snapshot, "an unrelated save must not reset it to on");
        assert!(s.adobe_update_check);
    }

    #[test]
    fn a_save_with_a_wrong_type_is_refused() {
        let stored = parse(CONFIG_0_1_0);
        assert!(merge_patch(&stored, &serde_json::json!({ "scale_value": "big" })).is_err());
        assert!(merge_patch(&stored, &serde_json::json!(["not", "an", "object"])).is_err());
    }
}
