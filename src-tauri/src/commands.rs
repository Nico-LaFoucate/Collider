// commands.rs
//
// The Tauri IPC surface — the functions Svelte can call via invoke(). These are
// thin: they call into core/ and neutron/ and return JSON-serializable results.
// Errors are converted to strings so the frontend gets the engine's `reason`
// (e.g. "Premiere is running; cannot edit prefs") rather than a Rust panic.
//
// Shared mutable state (the active launch session) lives behind a Mutex in
// Tauri's managed state so multiple IPC calls don't race.

use std::sync::Mutex;
use serde_json::Value;
use crate::core::launch::{LaunchSession, LaunchStep};
use crate::core::settings::Settings;

/// App-wide state Tauri manages and injects into commands.
#[derive(Default)]
pub struct AppState {
    pub session: Mutex<LaunchSession>,
}

/// Convert any error into a string the frontend can display.
fn estr<E: std::fmt::Display>(e: E) -> String { e.to_string() }

#[tauri::command]
pub fn prefix_info(prefix: String) -> Result<Value, String> {
    crate::neutron::prefix_info(&prefix).map_err(estr)
}

#[tauri::command]
pub fn doctor(prefix: Option<String>) -> Result<Value, String> {
    crate::neutron::doctor(prefix.as_deref()).map_err(estr)
}

#[tauri::command]
pub fn apply_display_fix(prefix: String) -> Result<Value, String> {
    crate::neutron::apply_display_fix(&prefix).map_err(estr)
}

/// Run the full MVP launch loop. Returns the resulting LaunchStep so the UI can
/// render exactly where it landed (Running, or Failed with the step + reason).
#[tauri::command]
pub fn launch_premiere(
    state: tauri::State<AppState>,
    prefix: String,
    project: Option<String>,
    export_dir: Option<String>,
) -> Result<LaunchStep, String> {
    let mut session = state.session.lock().map_err(|_| "session lock poisoned")?;
    let step = session
        .launch_premiere(&prefix, project.as_deref(), export_dir.as_deref())
        .clone();
    Ok(step)
}

/// Poll whether Premiere is still alive. The frontend calls this on a timer
/// while in the Running state; when it returns false, the frontend calls
/// clean_exit. Cheap (a single kill(pid,0) syscall).
#[tauri::command]
pub fn is_premiere_alive(state: tauri::State<AppState>) -> Result<bool, String> {
    let session = state.session.lock().map_err(|_| "session lock poisoned")?;
    Ok(session.is_premiere_alive())
}

/// Auto-detected clean exit — user closed Premiere normally. Stops the muxer,
/// ensures Premiere is gone, resets to Idle (button returns to "Launch").
#[tauri::command]
pub fn clean_exit(state: tauri::State<AppState>) -> Result<LaunchStep, String> {
    let mut session = state.session.lock().map_err(|_| "session lock poisoned")?;
    Ok(session.clean_exit().clone())
}

/// Manual escape hatch — "Force quit" from the Running button when Premiere hangs.
#[tauri::command]
pub fn force_quit(state: tauri::State<AppState>) -> Result<LaunchStep, String> {
    let mut session = state.session.lock().map_err(|_| "session lock poisoned")?;
    Ok(session.force_quit().clone())
}

/// Poll the current launch step (for the UI's status surface).
#[tauri::command]
pub fn current_step(state: tauri::State<AppState>) -> Result<LaunchStep, String> {
    let session = state.session.lock().map_err(|_| "session lock poisoned")?;
    Ok(session.step.clone())
}

/// Read persisted settings (Preferences). Defaults if no file yet.
#[tauri::command]
pub fn get_settings() -> Result<Settings, String> {
    Ok(crate::core::settings::load())
}

/// Persist settings from the Preferences tab. After saving we (best-effort) push
/// the Wayland home-window rule so a Preferences change takes effect immediately;
/// a rule-write failure never fails the save.
#[tauri::command]
pub fn set_settings(settings: Settings) -> Result<(), String> {
    crate::core::settings::save(&settings).map_err(estr)?;
    let _ = crate::core::window_rule::apply(&settings);
    Ok(())
}

/// Compositor capability for the Preferences UI: whether we're on Wayland, which
/// desktop, and whether the home-window position rule is supported here. Lets the
/// UI show the control only when it can actually do something.
#[tauri::command]
pub fn compositor_info() -> Result<Value, String> {
    Ok(serde_json::json!({
        "wayland": crate::core::display::is_wayland(),
        "desktop": crate::core::display::desktop_kind(),
        "home_rule_supported": crate::core::window_rule::supported(),
    }))
}

/// Detect the primary monitor's display scale (for the Preferences "Auto" readout).
/// None => couldn't detect (engine would auto-detect / fall back at launch).
#[tauri::command]
pub fn detect_scale() -> Result<Option<f64>, String> {
    Ok(crate::core::display::detect_display_scale())
}

/// Built-in decoration themes for the Appearance panel: each entry is
/// { id, label, colors } where colors maps Control Panel color keys to "R G B".
/// The frontend uses these to populate the preset dropdown and to seed the color
/// editors when the user switches to "custom".
#[tauri::command]
pub fn get_theme_presets() -> Result<Value, String> {
    use crate::core::theme;
    let presets: Vec<Value> = theme::PRESET_IDS
        .iter()
        .filter_map(|id| {
            theme::preset(id).map(|colors| {
                serde_json::json!({
                    "id": id,
                    "label": theme::preset_label(id),
                    "colors": colors,
                })
            })
        })
        .collect();
    Ok(Value::Array(presets))
}

/// Available caption-button icon sets for the Appearance panel: [{ id, label }, ...]
/// ("none" = Wine's default glyphs; bundled sets; "custom" if the user imported one).
#[tauri::command]
pub fn get_icon_sets() -> Result<Value, String> {
    use crate::core::caption_icons;
    let mut sets: Vec<Value> = caption_icons::SET_IDS
        .iter()
        .map(|id| serde_json::json!({ "id": id, "label": caption_icons::label(id) }))
        .collect();
    // Offer "custom" only when the user has imported icons.
    if caption_icons::custom_set_dir().map(|d| d.join("close.ico").exists()).unwrap_or(false) {
        sets.push(serde_json::json!({ "id": "custom", "label": caption_icons::label("custom") }));
    }
    Ok(Value::Array(sets))
}

/// Import a custom caption-icon set from a folder containing close/min/max/restore
/// images (.png/.jpg/.bmp/.ico). Re-encodes to .ico under the Collider config; the
/// caller then sets button_icon_set = "custom". Returns the number of buttons imported.
#[tauri::command]
pub fn import_icon_set(dir: String) -> Result<u32, String> {
    crate::core::caption_icons::import_set(&dir)
}
