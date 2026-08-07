// lib/api.js
//
// The frontend's single boundary to the Rust core. Every invoke() lives here so
// the UI never calls Tauri directly — mirrors the engine/cockpit discipline.
// Each function maps 1:1 to a #[tauri::command] in src-tauri/src/commands.rs.

import { invoke, Channel } from "@tauri-apps/api/core";

/** The prefix the UI should work in: the user's saved choice, else the ENGINE's default.
 *  Returns { prefix, valid, saved, home }. `valid:false` => nothing usable there yet, so the
 *  UI offers first-run setup instead of rendering an empty library. Never hardcode a prefix
 *  path in the frontend — the engine owns the default. */
export async function workingPrefix() {
  return await invoke("working_prefix");
}

// --- prefix registry -------------------------------------------------------------------
// Collider is multi-prefix by design (kickoff brief: "prefix registry"; README: run multiple
// Adobe versions side by side). Prefixes carry user-chosen NAMES — the path is a poor label
// when they're differently-named holdovers. The selected path is passed explicitly on every
// engine call, so Neutron's own DEFAULT_PREFIX never applies here.

/** The registry, re-validated. Returns { prefixes: [{name, path, valid, selected, apps}] }. */
export async function listPrefixes() {
  return await invoke("list_prefixes");
}

/** Prefixes on disk that aren't registered yet: { found: [{name, path}] }. */
export async function discoverPrefixes() {
  return await invoke("discover_prefixes");
}

export async function addPrefix(path, name = null) {
  return await invoke("add_prefix", { path, name });
}

export async function removePrefix(path) {
  return await invoke("remove_prefix", { path });
}

export async function renamePrefix(path, name) {
  return await invoke("rename_prefix", { path, name });
}

export async function selectPrefix(path) {
  return await invoke("select_prefix", { path });
}

/** Make a prefix Neutron-ready. Minutes-long (wineboot is the bulk), so it STREAMS progress:
 *  onEvent receives {event:"progress", stage, pct, msg} lines, then the terminal
 *  {event:"result", ...}. Cannot create a prefix from nothing — that's Mud Hut's job. */
export async function provisionPrefix(path, onEvent) {
  const channel = new Channel();
  channel.onmessage = (ev) => onEvent?.(ev);
  return await invoke("provision_prefix", { path, onEvent: channel });
}

/** Resolve a prefix's paths. Returns the prefix info object (documents_real, etc). */
export async function prefixInfo(prefix) {
  return await invoke("prefix_info", { prefix });
}

/** Health check. Pass a prefix for prefix-specific checks, or omit for system-only. */
export async function doctor(prefix = null) {
  return await invoke("doctor", { prefix });
}

/** Apply the DS.DisableDirectXDisplay fix. Idempotent. Throws on exit 3 (Premiere running). */
export async function applyDisplayFix(prefix) {
  return await invoke("apply_display_fix", { prefix });
}

/** Run the full MVP launch loop. Returns a LaunchStep ({step, detail}).
 *  (the export-dir argument is gone — the hwmux daemon it fed was retired from the engine)
 *  to the prefix's resolved Documents path. */
export async function launchPremiere(prefix, project = null) {
  return await invoke("launch_premiere", { prefix, project });
}

/** Poll whether Premiere is still running. Called on a timer while Running. */
export async function isPremiereAlive() {
  return await invoke("is_premiere_alive");
}

// --- Generic per-app surface (multi-app widgets) -------------------------------

/** The app catalog + install status for a prefix.
 *  Returns { apps: [ { id, name, accent, export, installed, exe }, ... ] }. */
export async function listApps(prefix) {
  return await invoke("list_apps", { prefix });
}

/** Launch an app by id. Returns a LaunchStep ({step, detail}). */
export async function launchApp(appId, prefix, project = null) {
  return await invoke("launch_app", { appId, prefix, project });
}

/** Poll whether app `appId` is still running. Called on a timer while Running. */
export async function isAppAlive(appId) {
  return await invoke("is_app_alive", { appId });
}

/** Auto-detected clean exit for `appId` (user closed it). Resets to idle. */
export async function cleanExitApp(appId) {
  return await invoke("clean_exit_app", { appId });
}

/** Manual force-quit for a hung `appId`. */
export async function forceQuitApp(appId) {
  return await invoke("force_quit_app", { appId });
}

/** Poll the current launch step for `appId`. */
export async function currentStepApp(appId) {
  return await invoke("current_step_app", { appId });
}

/** Auto-detected clean exit — user closed Premiere. Stops muxer, resets to idle. */
export async function cleanExit() {
  return await invoke("clean_exit");
}

/** Manual force-quit from the Running button's dropdown (for a hung Premiere). */
export async function forceQuit() {
  return await invoke("force_quit");
}

/** Poll the current launch step for the status surface. */
export async function currentStep() {
  return await invoke("current_step");
}

/** Read persisted settings (Preferences). Returns the full Settings object, incl.
 *  scale_mode/scale_value, home_window_*, and theme/custom_colors. */
export async function getSettings() {
  return await invoke("get_settings");
}

/** Persist settings from the Preferences tab. `settings` must be the full Settings
 *  object (scale_*, home_window_*, theme, custom_colors) — omitted fields reset to
 *  their Rust defaults, so always send the whole thing. */
export async function setSettings(settings) {
  return await invoke("set_settings", { settings });
}

/** Built-in decoration themes for the Appearance panel.
 *  Returns [{ id, label, colors: { "<ColorKey>": "R G B", ... } }, ...]. */
export async function getThemePresets() {
  return await invoke("get_theme_presets");
}

/** Caption-button icon sets for the Appearance panel. Returns [{ id, label }, ...]
 *  ("none" = default glyphs; bundled sets; "custom" only if the user imported one). */
export async function getIconSets() {
  return await invoke("get_icon_sets");
}

/** Import a custom caption-icon set from a folder (close/min/max/restore .png/.ico/...).
 *  Returns the number of buttons imported. Caller then sets button_icon_set = "custom". */
export async function importIconSet(dir) {
  return await invoke("import_icon_set", { dir });
}

/** Detect the primary monitor's display scale (for the Preferences readout). May be null. */
export async function detectScale() {
  return await invoke("detect_scale");
}

/** Compositor capability: { wayland, desktop, home_rule_supported }. Used to gate the
 *  Wayland home-window-position control in Preferences. */
export async function compositorInfo() {
  return await invoke("compositor_info");
}

// --- Mud Hut installer: Adobe sign-in (device/QR flow) -------------------------

/** Begin Adobe sign-in. Returns { url, qr, request_id, device_id }.
 *  `qr` is base64 PNG (prefix with "data:image/png;base64,"). */
export async function adobeAuthBegin() {
  return await invoke("adobe_auth_begin");
}

/** Poll the sign-in once. Returns { status: "pending"|"complete"|"expired",
 *  retry_interval, exchange? }. Frontend calls this every retry_interval seconds
 *  until status !== "pending". */
export async function adobeAuthPoll(requestId, deviceId) {
  return await invoke("adobe_auth_poll", { requestId, deviceId });
}

// --- Mud Hut installer: app catalog + streaming install ------------------------

/** The installable-app catalog (`mudhut apps`). Returns { apps: [ { id, name, sap,
 *  present, dir } ] }. Pass a `source` dir to also mark what's present there. */
export async function mudhutApps(source = null) {
  return await invoke("mudhut_apps", { source });
}

/** Install an app via Mud Hut, streaming progress. `onEvent(ev)` receives each
 *  event live: { event:"progress", stage, pct, msg } / { event:"note", msg } /
 *  the terminal { event:"result", ... }. Resolves with the terminal result, or
 *  rejects with the error message. */
export async function installApp({ app, method, prefix, source = null, onEvent }) {
  const channel = new Channel();
  channel.onmessage = (ev) => onEvent?.(ev);
  return await invoke("install_app", { app, method, prefix, source, onEvent: channel });
}
