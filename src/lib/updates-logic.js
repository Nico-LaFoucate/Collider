// lib/updates-logic.js
//
// Pure helpers for the Adobe app updates UI: no Svelte runes and no Tauri, so `node --test` runs
// them (updates-logic.test.js). Everything here turns Mud Hut's answers into what a view shows; it
// decides nothing about updates itself.

/** Store key for one app in one prefix. Never the app id alone: two prefixes can hold the same app. */
export const ukey = (prefix, id) => `${prefix}\u0000${id}`;
