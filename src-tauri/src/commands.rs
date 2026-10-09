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
use std::sync::{Arc, Mutex};
use serde_json::Value;
use crate::core::launch::{LaunchSession, LaunchStep};
use crate::core::settings::Settings;

/// The identity of one supervised session: `(prefix, app_id)`.
///
/// 🚨 2026-09-18: this used to be the app id ALONE, with the prefix passed separately. Two
/// prefixes holding the same app (Premiere 2025 in ~/.premiere2025, Premiere 2026 in
/// ~/Neutron-Prefixes/Adobe2026) therefore shared ONE slot: launching 2026 overwrote 2025's
/// pid and prefix in place, the 2025 card's liveness poll then answered for the 2026 process,
/// and a Force quit from either card tore down whichever prefix had been written LAST. Same bug
/// class as the engine's `_app_running_pid`: per-app where it must be per-app-PER-PREFIX. Two
/// exes at two paths are two programs; Windows does not group them by filename either.
pub type SessionKey = (String, String);

fn session_key(prefix: &str, app_id: &str) -> SessionKey {
    (prefix.to_string(), app_id.to_string())
}

/// App-wide state Tauri manages and injects into commands. One supervised launch
/// session PER (prefix, app id) (several apps, in several prefixes, can run at once),
/// created on demand.
#[derive(Default)]
pub struct AppState {
    /// Arc so a launch can be moved onto a worker thread. See launch_app().
    pub sessions: Arc<Mutex<HashMap<SessionKey, LaunchSession>>>,
}

/// Run a launch OFF the main thread.
///
/// 🚨 WHY THIS IS NOT A PLAIN `#[tauri::command] fn`. In Tauri v2 a SYNCHRONOUS command runs on
/// the MAIN THREAD, so it blocks the event loop for its whole duration. `launch()` shells out to
/// the `neutron` CLI via `Command::output()`, which waits for the child to exit and reads its
/// stdout to EOF — many seconds. The entire Collider window froze for that whole time: no
/// repaint, no input, the "Launching…" button not even animating. That is the freeze the user
/// reported ("Collider freezes when launching an app until the app actually begins launching").
///
/// An `async` command is polled on Tauri's runtime instead, and the blocking work goes to
/// `spawn_blocking`, so the main thread stays free to paint and accept input.
///
/// The session lock is still held for the duration of one launch — that is deliberate and
/// correct, since two concurrent launches of the SAME app must not interleave. It no longer costs
/// anything visible, because the lock is now held on a worker rather than on the main thread.
/// Run any blocking work OFF the main thread.
///
/// 🚨 In Tauri v2 a SYNCHRONOUS `#[tauri::command]` runs on the MAIN THREAD. Every command here
/// either shells out to the `neutron` CLI (`Command::output()` — waits for the child and drains
/// its stdout) or takes the session mutex, which a launch may already hold. Either one blocks the
/// event loop, and the whole window stops painting and accepting input.
///
/// The user hit this twice: first on Launch, and — after I fixed only launch — again on Force
/// quit. Fixing the instance instead of the class just moved the freeze to the next button.
/// Anything that can block belongs here.
async fn off_main<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("task failed: {e}"))?
}

/// Run a closure against the (created-on-demand) session for `(prefix, app_id)`, off the main
/// thread.
async fn with_session_off_main<T, F>(
    sessions: Arc<Mutex<HashMap<SessionKey, LaunchSession>>>,
    prefix: &str,
    app_id: &str,
    f: F,
) -> Result<T, String>
where
    F: FnOnce(&mut LaunchSession) -> T + Send + 'static,
    T: Send + 'static,
{
    let key = session_key(prefix, app_id);
    off_main(move || {
        let mut map = sessions.lock().map_err(|_| "session lock poisoned".to_string())?;
        let s = map.entry(key).or_insert_with(LaunchSession::new);
        Ok(f(s))
    })
    .await
}

async fn launch_off_main(
    sessions: Arc<Mutex<HashMap<SessionKey, LaunchSession>>>,
    app_id: String,
    prefix: String,
    project: Option<String>,
) -> Result<LaunchStep, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut map = sessions.lock().map_err(|_| "session lock poisoned".to_string())?;
        let s = map.entry(session_key(&prefix, &app_id)).or_insert_with(LaunchSession::new);
        Ok(s.launch(&app_id, &prefix, project.as_deref()).clone())
    })
    .await
    .map_err(|e| format!("launch task failed: {e}"))?
}

/// Convert any error into a string the frontend can display.
fn estr<E: std::fmt::Display>(e: E) -> String { e.to_string() }

#[tauri::command]
pub async fn prefix_info(prefix: String) -> Result<Value, String> {
    off_main(move || crate::neutron::prefix_info(Some(&prefix)).map_err(estr)).await
}

/// The prefix the UI should work in, plus everything first-run setup needs to offer a way
/// forward when there isn't one. `prefix: null` => nothing usable is registered; the UI shows
/// setup rather than an empty library.
#[tauri::command]
pub fn working_prefix() -> Result<Value, String> {
    let selected = crate::core::prefixes::selected();
    Ok(serde_json::json!({
        "prefix": selected,
        "valid": selected.is_some(),
        "default_new": crate::core::prefixes::default_new_prefix_path(),
    }))
}

/// The registry, re-validated, for the Prefixes tab.
#[tauri::command]
pub fn list_prefixes() -> Result<Value, String> {
    Ok(serde_json::json!({ "prefixes": crate::core::prefixes::list() }))
}

/// Prefixes found on disk that aren't registered yet, each with a suggested label.
#[tauri::command]
pub fn discover_prefixes() -> Result<Value, String> {
    let found: Vec<Value> = crate::core::prefixes::discover()
        .into_iter()
        .map(|p| serde_json::json!({ "name": crate::core::prefixes::suggest_name(&p), "path": p }))
        .collect();
    Ok(serde_json::json!({ "found": found }))
}

#[tauri::command]
pub fn add_prefix(path: String, name: Option<String>) -> Result<(), String> {
    crate::core::prefixes::add(&path, name.as_deref())
}

#[tauri::command]
pub fn remove_prefix(path: String) -> Result<(), String> {
    crate::core::prefixes::remove(&path)
}

#[tauri::command]
pub fn rename_prefix(path: String, name: String) -> Result<(), String> {
    crate::core::prefixes::rename(&path, &name)
}

#[tauri::command]
pub fn select_prefix(path: String) -> Result<(), String> {
    crate::core::prefixes::select(&path)
}

/// Make a prefix Neutron-ready (`neutron prefix provision`). Minutes-long — wineboot alone is the
/// bulk of it — so it runs off the UI thread and streams NDJSON progress events back over a
/// Channel, the same way the Mud Hut installer does. Without the stream the user would watch an
/// indeterminate spinner for several minutes on the one operation a first-time tester runs.
/// It CANNOT create a prefix from nothing: the engine rejects anything without a drive_c.
/// Creating one is Mud Hut's job.
#[tauri::command]
pub async fn provision_prefix(
    path: String,
    on_event: tauri::ipc::Channel<Value>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::neutron::provision_stream(&path, |ev| {
            let _ = on_event.send(ev);
        })
        .map_err(estr)
    })
    .await
    .map_err(estr)?
}

#[tauri::command]
pub async fn doctor(prefix: Option<String>) -> Result<Value, String> {
    off_main(move || crate::neutron::doctor(prefix.as_deref()).map_err(estr)).await
}

#[tauri::command]
pub async fn apply_display_fix(prefix: String) -> Result<Value, String> {
    off_main(move || crate::neutron::apply_display_fix(&prefix).map_err(estr)).await
}

/// Fonts row on the Prefixes tab: the engine's verdict, verbatim.
#[tauri::command]
pub async fn fonts_check(prefix: String) -> Result<Value, String> {
    off_main(move || crate::neutron::fonts_check(&prefix).map_err(estr)).await
}

/// The Repair button. Seconds normally; longer when the Microsoft core fonts have to be
/// downloaded first.
#[tauri::command]
pub async fn fonts_repair(prefix: String) -> Result<Value, String> {
    off_main(move || crate::neutron::fonts_repair(&prefix).map_err(estr)).await
}

// ---------------------------------------------------------------------------
// Generic per-app launch surface — Collider's multi-app widgets call these.
// ---------------------------------------------------------------------------

/// The app catalog + install status for a prefix — powers the per-app widgets.
#[tauri::command]
pub async fn list_apps(prefix: String) -> Result<Value, String> {
    off_main(move || crate::neutron::apps(&prefix).map_err(estr)).await
}

/// Run the launch loop for `app_id`. Returns the resulting LaunchStep so the UI
/// can render exactly where it landed (Running, or Failed with the step + reason).
#[tauri::command]
pub async fn launch_app(
    state: tauri::State<'_, AppState>,
    app_id: String,
    prefix: String,
    project: Option<String>,
) -> Result<LaunchStep, String> {
    launch_off_main(state.sessions.clone(), app_id, prefix, project).await
}

/// Adopt an already-running app (started from the application menu, or left over from a previous
/// Collider run) so the GUI can supervise and force-quit it. The frontend calls this when
/// `list_apps` reports `running: true` for an app it has no session for.
#[tauri::command]
pub async fn adopt_app(
    state: tauri::State<'_, AppState>,
    app_id: String,
    prefix: String,
    pid: u32,
) -> Result<LaunchStep, String> {
    let id = app_id.clone();
    let p = prefix.clone();
    with_session_off_main(state.sessions.clone(), &prefix, &app_id,
        move |s| s.adopt(&id, &p, pid).clone()).await
}

/// Poll whether `app_id` in `prefix` is still alive (a single kill(pid,0) syscall).
#[tauri::command]
pub async fn is_app_alive(
    state: tauri::State<'_, AppState>,
    app_id: String,
    prefix: String,
) -> Result<bool, String> {
    with_session_off_main(state.sessions.clone(), &prefix, &app_id, |s| s.is_alive()).await
}

/// Auto-detected clean exit for `app_id` in `prefix` — the user closed it normally.
#[tauri::command]
pub async fn clean_exit_app(
    state: tauri::State<'_, AppState>,
    app_id: String,
    prefix: String,
) -> Result<LaunchStep, String> {
    with_session_off_main(state.sessions.clone(), &prefix, &app_id, |s| s.clean_exit().clone()).await
}

/// Manual "Force quit" for a hung `app_id` in `prefix`.
#[tauri::command]
pub async fn force_quit_app(
    state: tauri::State<'_, AppState>,
    app_id: String,
    prefix: String,
) -> Result<LaunchStep, String> {
    with_session_off_main(state.sessions.clone(), &prefix, &app_id, |s| s.force_quit().clone()).await
}

/// Poll the current launch step for `app_id` in `prefix`.
#[tauri::command]
pub async fn current_step_app(
    state: tauri::State<'_, AppState>,
    app_id: String,
    prefix: String,
) -> Result<LaunchStep, String> {
    with_session_off_main(state.sessions.clone(), &prefix, &app_id, |s| s.step.clone()).await
}

// The Premiere-compat wrappers (`launch_premiere`, `is_premiere_alive`, `clean_exit`,
// `force_quit`, `current_step`) that used to live here were removed 2026-09-18. Nothing in the
// frontend called them any more (AppCard.svelte uses the generic surface), and each one was a
// hard-wired global "premiere" session with no prefix at all -- the exact shape of the bug the
// SessionKey change fixes. Dead code that embeds a known bug is not worth keeping for "compat".

/// Read persisted settings (Preferences). Defaults if no file yet.
#[tauri::command]
pub fn get_settings() -> Result<Settings, String> {
    Ok(crate::core::settings::load())
}

/// Persist settings from the Preferences tab.
#[tauri::command]
pub fn set_settings(mut settings: Settings) -> Result<(), String> {
    // The Preferences UI sends an explicit field list that does NOT include the prefix registry —
    // it owns display/theme settings; prefixes belong to the Prefixes tab. Because save() writes
    // the whole struct, the omitted fields would deserialize to empty and WIPE every registered
    // prefix the moment the user touched a preference. Carry the stored values forward.
    let stored = crate::core::settings::load();
    if settings.prefixes.is_empty() {
        settings.prefixes = stored.prefixes;
    }
    if settings.selected_prefix.is_none() {
        settings.selected_prefix = stored.selected_prefix;
    }
    crate::core::settings::save(&settings).map_err(estr)?;
    Ok(())
}

/// The desktop's display scale as the engine sees it (`neutron display`), for the Preferences
/// "Auto" readout: `{scale, output, source, dpi, windows_step, warning}`.
#[tauri::command]
pub async fn detect_scale() -> Result<Value, String> {
    off_main(|| crate::neutron::display().map_err(estr)).await
}

// ---------------------------------------------------------------------------
// Neutron itself: set up / update / uninstall / versions (the engine does the work).
// ---------------------------------------------------------------------------

/// `neutron setup` (or `neutron update`), streaming progress over `on_event`. Minutes: it
/// downloads neutron-wine, Microsoft's components, Mud Hut and Adobe's ACCCx. May show the
/// desktop's password dialog once, to turn on ntsync.
#[tauri::command]
pub async fn neutron_setup(update: bool, on_event: tauri::ipc::Channel<Value>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::neutron::setup_stream(update, |ev| {
            let _ = on_event.send(ev);
        })
        .map_err(estr)
    })
    .await
    .map_err(estr)?
}

/// `neutron uninstall`; the confirmation (and the separate prefixes question) is the UI's.
#[tauri::command]
pub async fn neutron_uninstall(delete_prefixes: bool) -> Result<Value, String> {
    off_main(move || crate::neutron::uninstall(delete_prefixes).map_err(estr)).await
}

/// Every piece's version, plus Collider's own. `cli: null` means the CLI is not installed.
#[tauri::command]
pub async fn versions() -> Result<Value, String> {
    off_main(|| {
        let mut v = crate::neutron::version().unwrap_or_else(|_| serde_json::json!({ "cli": null }));
        v["collider"] = serde_json::json!(env!("CARGO_PKG_VERSION"));
        Ok(v)
    })
    .await
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
// Mud Hut installer. No sign-in anywhere: Adobe's feed and CDN are public, and licensing happens
// inside the app on first launch.
// ---------------------------------------------------------------------------

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
