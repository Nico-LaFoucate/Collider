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

/** Font verdict for a prefix (`neutron fonts check`): {status: good|warn|bad, summary, checks,
 *  repair, apps_running}. Read-only and fast; the engine never starts wine for it. */
export async function fontsCheck(prefix) {
  return await invoke("fonts_check", { prefix });
}

/** Repair a prefix's fonts (`neutron fonts repair`): {ok, before, steps, after}. Throws with the
 *  engine's reason when an Adobe app is running in the prefix or the runtime would re-stamp it. */
export async function fontsRepair(prefix) {
  return await invoke("fonts_repair", { prefix });
}

/** Apply the DS.DisableDirectXDisplay fix. Idempotent. Throws on exit 3 (Premiere running). */
export async function applyDisplayFix(prefix) {
  return await invoke("apply_display_fix", { prefix });
}

// --- Generic per-app surface (multi-app widgets) -------------------------------
//
// Every session call carries BOTH the app id and the prefix: a session is one app in one prefix.
// (2026-09-18: it was keyed by app id alone, so Premiere 2025 and Premiere 2026 shared a slot and
// one card polled, and force-quit, the other prefix's process.) The old prefix-less
// launchPremiere/isPremiereAlive/cleanExit/forceQuit/currentStep wrappers are gone with it.

/** The app catalog + install status for a prefix.
 *  Returns { apps: [ { id, name, accent, export, installed, exe }, ... ] }. */
export async function listApps(prefix) {
  return await invoke("list_apps", { prefix });
}

/** Launch an app by id. Returns a LaunchStep ({step, detail}). */
export async function launchApp(appId, prefix, project = null) {
  return await invoke("launch_app", { appId, prefix, project });
}

/** Poll whether app `appId` in `prefix` is still running. Called on a timer while Running. */
export async function isAppAlive(appId, prefix) {
  return await invoke("is_app_alive", { appId, prefix });
}

/** Adopt an app that is already running — started from the application menu, or left over from a
 *  previous Collider run. Without this the card would show "Launch" and clicking it would start a
 *  SECOND copy. `neutron apps` reports running/pid per app; this hands that pid to the session so
 *  force-quit and clean-exit work exactly as for an app Collider launched itself. */
export async function adoptApp(appId, prefix, pid) {
  return await invoke("adopt_app", { appId, prefix, pid });
}

/** Auto-detected clean exit for `appId` in `prefix` (user closed it). Resets to idle. */
export async function cleanExitApp(appId, prefix) {
  return await invoke("clean_exit_app", { appId, prefix });
}

/** Manual force-quit for a hung `appId` in `prefix`. */
export async function forceQuitApp(appId, prefix) {
  return await invoke("force_quit_app", { appId, prefix });
}

/** Poll the current launch step for `appId` in `prefix`. */
export async function currentStepApp(appId, prefix) {
  return await invoke("current_step_app", { appId, prefix });
}

/** Read persisted settings (Preferences). Returns the full Settings object, incl.
 *  scale_mode/scale_value, theme/custom_colors and button_icon_set. */
export async function getSettings() {
  return await invoke("get_settings");
}

/** Persist settings from the Preferences tab. `settings` must be the full Settings
 *  object (scale_*, theme, custom_colors, button_icon_set) — omitted fields reset to
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

/** The desktop's display scale as the engine sees it (`neutron display`):
 *  { scale, output, source, dpi, windows_step, warning }. Throws if the CLI could not tell. */
export async function detectScale() {
  return await invoke("detect_scale");
}

// --- Neutron itself -------------------------------------------------------------

/** Every piece's version: { cli, neutron_wine, mudhut, collider, ... }. `cli: null` means the
 *  Neutron CLI is not installed yet. */
export async function versions() {
  return await invoke("versions");
}

/** `neutron setup` (update=false) or `neutron update` (update=true), streaming progress events
 *  to `onEvent` like provisioning. Resolves with the result; rejects with what failed. */
export async function neutronSetup(update, onEvent) {
  const channel = new Channel();
  channel.onmessage = (ev) => onEvent?.(ev);
  return await invoke("neutron_setup", { update, onEvent: channel });
}

/** `neutron uninstall`. Prefixes are deleted only when deletePrefixes is true. */
export async function neutronUninstall(deletePrefixes) {
  return await invoke("neutron_uninstall", { deletePrefixes });
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
