// lib/api.js
//
// The frontend's single boundary to the Rust core. Every invoke() lives here so
// the UI never calls Tauri directly — mirrors the engine/cockpit discipline.
// Each function maps 1:1 to a #[tauri::command] in src-tauri/src/commands.rs.

import { invoke } from "@tauri-apps/api/core";

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
 *  exportDir: where Premiere's exports land (what hwmux watches). Null = default
 *  to the prefix's resolved Documents path. */
export async function launchPremiere(prefix, project = null, exportDir = null) {
  return await invoke("launch_premiere", { prefix, project, exportDir });
}

/** Poll whether Premiere is still running. Called on a timer while Running. */
export async function isPremiereAlive() {
  return await invoke("is_premiere_alive");
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

/** Read persisted settings (Preferences). Returns { scale_mode, scale_value }. */
export async function getSettings() {
  return await invoke("get_settings");
}

/** Persist settings from the Preferences tab. `settings` = { scale_mode, scale_value }. */
export async function setSettings(settings) {
  return await invoke("set_settings", { settings });
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
