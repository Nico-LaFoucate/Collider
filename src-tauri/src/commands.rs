// commands.rs
//
// The Tauri IPC surface — the functions Svelte can call via invoke(). These are
// thin: they call into core/ and neutron/ and return JSON-serializable results.
// Errors are converted to strings so the frontend gets the engine's `reason`
// (e.g. "Premiere is running; cannot edit prefs") rather than a Rust panic.
//
// Shared mutable state (the active launch session) lives behind a Mutex in
// Tauri's managed state so multiple IPC calls don't race.

use std::collections::HashMap;
use std::sync::Mutex;
use serde_json::Value;
use crate::core::launch::{LaunchSession, LaunchStep};
use crate::core::settings::Settings;

/// App-wide state Tauri manages and injects into commands. One supervised launch
/// session PER app id (several apps can run at once), created on demand.
#[derive(Default)]
pub struct AppState {
    pub sessions: Mutex<HashMap<String, LaunchSession>>,
}

/// Convert any error into a string the frontend can display.
fn estr<E: std::fmt::Display>(e: E) -> String { e.to_string() }

/// Run a closure against the (created-on-demand) session for `app_id`.
fn with_session<R>(
    state: &tauri::State<AppState>,
    app_id: &str,
    f: impl FnOnce(&mut LaunchSession) -> R,
) -> Result<R, String> {
    let mut map = state.sessions.lock().map_err(|_| "session lock poisoned")?;
    let s = map.entry(app_id.to_string()).or_insert_with(LaunchSession::new);
    Ok(f(s))
}

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

// ---------------------------------------------------------------------------
// Generic per-app launch surface — Collider's multi-app widgets call these.
// ---------------------------------------------------------------------------

/// The app catalog + install status for a prefix — powers the per-app widgets.
#[tauri::command]
pub fn list_apps(prefix: String) -> Result<Value, String> {
    crate::neutron::apps(&prefix).map_err(estr)
}

/// Run the launch loop for `app_id`. Returns the resulting LaunchStep so the UI
/// can render exactly where it landed (Running, or Failed with the step + reason).
#[tauri::command]
pub fn launch_app(
    state: tauri::State<AppState>,
    app_id: String,
    prefix: String,
    project: Option<String>,
) -> Result<LaunchStep, String> {
    with_session(&state, &app_id, |s| {
        s.launch(&app_id, &prefix, project.as_deref()).clone()
    })
}

/// Poll whether `app_id` is still alive (a single kill(pid,0) syscall).
#[tauri::command]
pub fn is_app_alive(state: tauri::State<AppState>, app_id: String) -> Result<bool, String> {
    with_session(&state, &app_id, |s| s.is_alive())
}

/// Auto-detected clean exit for `app_id` — the user closed it normally.
#[tauri::command]
pub fn clean_exit_app(state: tauri::State<AppState>, app_id: String) -> Result<LaunchStep, String> {
    with_session(&state, &app_id, |s| s.clean_exit().clone())
}

/// Manual "Force quit" for a hung `app_id`.
#[tauri::command]
pub fn force_quit_app(state: tauri::State<AppState>, app_id: String) -> Result<LaunchStep, String> {
    with_session(&state, &app_id, |s| s.force_quit().clone())
}

/// Poll the current launch step for `app_id`.
#[tauri::command]
pub fn current_step_app(state: tauri::State<AppState>, app_id: String) -> Result<LaunchStep, String> {
    with_session(&state, &app_id, |s| s.step.clone())
}

// ---------------------------------------------------------------------------
// Premiere-compat wrappers — the current frontend calls these (no app id) until
// P2 switches to the generic surface. Thin delegates to the "premiere" session.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn launch_premiere(
    state: tauri::State<AppState>,
    prefix: String,
    project: Option<String>,
) -> Result<LaunchStep, String> {
    with_session(&state, "premiere", |s| {
        s.launch("premiere", &prefix, project.as_deref()).clone()
    })
}

#[tauri::command]
pub fn is_premiere_alive(state: tauri::State<AppState>) -> Result<bool, String> {
    with_session(&state, "premiere", |s| s.is_alive())
}

#[tauri::command]
pub fn clean_exit(state: tauri::State<AppState>) -> Result<LaunchStep, String> {
    with_session(&state, "premiere", |s| s.clean_exit().clone())
}

#[tauri::command]
pub fn force_quit(state: tauri::State<AppState>) -> Result<LaunchStep, String> {
    with_session(&state, "premiere", |s| s.force_quit().clone())
}

#[tauri::command]
pub fn current_step(state: tauri::State<AppState>) -> Result<LaunchStep, String> {
    with_session(&state, "premiere", |s| s.step.clone())
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
    let mut ids: Vec<&str> = caption_icons::SET_IDS.to_vec();
    // Offer "custom" only when the user has imported icons.
    if caption_icons::custom_set_dir().map(|d| d.join("close.ico").exists()).unwrap_or(false) {
        ids.push("custom");
    }
    let sets: Vec<Value> = ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "label": caption_icons::label(id),
                "preview": caption_icons::preview_data_uri(id),  // null for "none"
            })
        })
        .collect();
    Ok(Value::Array(sets))
}

/// Import a custom caption-icon set from a folder containing close/min/max/restore
/// images (.png/.jpg/.bmp/.ico). Re-encodes to .ico under the Collider config; the
/// caller then sets button_icon_set = "custom". Returns the number of buttons imported.
#[tauri::command]
pub fn import_icon_set(dir: String) -> Result<u32, String> {
    crate::core::caption_icons::import_set(&dir)
}

// ---------------------------------------------------------------------------
// Mud Hut installer — Adobe sign-in (device/QR flow, driven via `mudhut auth`).
// begin() once to mint the QR+link, then the frontend polls poll() every few
// seconds until status == "complete" (same pattern as the launch/current_step).
// ---------------------------------------------------------------------------

/// Begin Adobe sign-in: returns { url, qr, request_id, device_id }.
#[tauri::command]
pub fn adobe_auth_begin() -> Result<Value, String> {
    crate::mudhut::auth_begin().map_err(estr)
}

/// Poll the sign-in once: returns { status: pending|complete|expired,
/// retry_interval, exchange? }.
#[tauri::command]
pub fn adobe_auth_poll(request_id: String, device_id: String) -> Result<Value, String> {
    crate::mudhut::auth_poll(&request_id, &device_id).map_err(estr)
}

/// The installable-app catalog (`mudhut apps [--source]`). Powers the install
/// wizard's app picker.
#[tauri::command]
pub fn mudhut_apps(source: Option<String>) -> Result<Value, String> {
    crate::mudhut::apps(source.as_deref()).map_err(estr)
}

/// Install an app via Mud Hut, streaming progress to the frontend over `on_event`
/// (each Mud Hut `progress`/`note`/`result`/`error` NDJSON event). Runs on a
/// blocking thread so the (minutes-long) install never freezes the UI. Resolves
/// with the terminal `result` object, or rejects with the error message.
#[tauri::command]
pub async fn install_app(
    app: String,
    method: String,
    prefix: String,
    source: Option<String>,
    on_event: tauri::ipc::Channel<Value>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::mudhut::install_stream(&app, &method, &prefix, source.as_deref(), |ev| {
            let _ = on_event.send(ev);
        })
        .map_err(estr)
    })
    .await
    .map_err(estr)?
}
