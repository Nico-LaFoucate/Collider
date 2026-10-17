// lib/updates.svelte.js
//
// One store for Adobe app updates, shared by every view that shows them (app cards, the Apps
// header, the Mud Hut "Update installed apps" view, the Prefixes tab, Preferences), so the state is
// the same wherever the user looks.
//
// Collider is the GUI for Mud Hut here and never the authority: whether an update exists, whether
// Neutron has verified it, whether it can run now and how it is applied are all Mud Hut's answers
// (`mudhut check-updates`, `mudhut update`). Collider decides only WHEN to ask and WHAT the user
// clicked, and shows Mud Hut's sentences as Mud Hut wrote them.
//
// Rows are keyed per (prefix, app id), never by app id alone: two prefixes can hold the same app
// (the 2026-09-18 session bug class).

import { checkAdobeUpdates } from "$lib/api.js";
import { ukey } from "$lib/updates-logic.js";

export const upd = $state({
  supported: true,     // false: Mud Hut cannot check (unsupported says why)
  unsupported: null,   // "too-old" | "missing"
  checking: false,     // a check is running (any kind)
  refreshing: false,   // a "Check now" is running
  // The last check that was allowed to contact Adobe: { checked_at, online, feed_error,
  // from_cache, verified_list }. Offline reads never replace it, so "Last checked" means Adobe.
  lastNetwork: null,
  failed: null,        // a check that failed outright: its message (shown quietly, never as the Apps banner)
  prefixes: {},        // prefix -> Mud Hut's prefix object { ok, apps_running, update_in_progress, last_update, apps }
  jobs: {},            // prefix -> a job THIS Collider started (see runUpdate/runYearlyInstall)
  outcomes: {},        // ukey(prefix, id) -> how this Collider's last run for that app ended
});

// The page tells the store what to re-read when a prefix's apps changed (an update finished).
let hooks = { prefixChanged: (_prefix) => {} };
export function setUpdateHooks(h) { hooks = { ...hooks, ...h }; }

// --- the check -------------------------------------------------------------------------------

let inflight = null;

/**
 * Ask Mud Hut what can be updated in `prefixes` (the registered, valid ones, passed explicitly).
 * - default: a timer tick; `--max-age 24h`, so Mud Hut's cache decides whether Adobe is contacted
 * - refresh: "Check now" (`--refresh`)
 * - offline: a local read (`--offline`): installed versions, running apps and update state are
 *   re-read from the prefixes, Adobe is not contacted
 * - partial: merge these prefixes' answers instead of replacing the whole map (following one prefix)
 * - skipIfBusy: a timer tick does nothing while another check runs; a click waits for it
 */
export async function check(prefixes, { refresh = false, offline = false, partial = false, skipIfBusy = false } = {}) {
  if (!upd.supported || !prefixes?.length) return;
  while (inflight) {
    if (skipIfBusy) return;
    try { await inflight; } catch (_) {}
  }
  inflight = runCheck(prefixes, { refresh, offline, partial });
  try { await inflight; } finally { inflight = null; }
}

async function runCheck(prefixes, { refresh, offline, partial }) {
  upd.checking = true;
  if (refresh) upd.refreshing = true;
  try {
    const r = await checkAdobeUpdates(prefixes, { maxAge: offline ? null : "24h", refresh, offline });
    if (r?.supported === false) {
      upd.supported = false;
      upd.unsupported = r.reason ?? "too-old";
      stopUpdateChecks();
      return;
    }
    upd.failed = null;
    if (!offline) {
      upd.lastNetwork = {
        checked_at: r.checked_at ?? null, online: r.online !== false, feed_error: r.feed_error ?? null,
        from_cache: !!r.from_cache, verified_list: r.verified_list ?? null,
      };
    }
    const before = upd.prefixes;
    const next = partial ? { ...before } : {};
    for (const p of r.prefixes ?? []) next[p.prefix] = p;
    upd.prefixes = next;
    noticeFinished(before, next);
    syncFollowing();
  } catch (e) {
    upd.failed = String(e);
  } finally {
    upd.checking = false;
    if (refresh) upd.refreshing = false;
  }
}

// An update this Collider did not start (one left running when Collider was closed) finished:
// its outcome comes from Mud Hut's `last_update`, and the page re-reads that prefix's apps.
function noticeFinished(before, next) {
  for (const [prefix, old] of Object.entries(before)) {
    if (!old?.update_in_progress) continue;
    const now = next[prefix];
    if (now === undefined || now.update_in_progress) continue;
    if (!upd.jobs[prefix]) hooks.prefixChanged(prefix);
  }
}

// --- following an update this Collider did not start -----------------------------------------

let followTimer = null;
let followed = [];

/** Prefixes with an update in progress that this Collider is not running itself. */
export function followedPrefixes() {
  return Object.entries(upd.prefixes)
    .filter(([p, s]) => s?.update_in_progress && !upd.jobs[p])
    .map(([p]) => p);
}

// While one runs, re-read just those prefixes every 10 s (offline: Adobe is not contacted).
function syncFollowing() {
  followed = followedPrefixes();
  if (followed.length && !followTimer) {
    followTimer = setInterval(() => {
      if (followed.length) check(followed, { offline: true, partial: true, skipIfBusy: true });
    }, 10_000);
  } else if (!followed.length && followTimer) {
    clearInterval(followTimer);
    followTimer = null;
  }
}

// --- the schedule ----------------------------------------------------------------------------

let firstTimer = null, tickTimer = null;

/**
 * Start the checks for this Collider run: a local read right away (badges, and an update left
 * running by a previous Collider shows at once), the first check that may contact Adobe about
 * 45 s after startup, then a tick every 30 minutes. Each tick passes `--max-age 24h`, so Adobe is
 * contacted at most about once a day and a laptop that slept through the day checks on its next
 * tick. Only while Collider is open (DECISIONS C70); `enabled()` is read on every tick, so turning
 * the setting off takes effect without a restart.
 */
export function startUpdateChecks(getPrefixes, enabled) {
  stopUpdateChecks();
  check(getPrefixes(), { offline: true });
  const tick = () => { if (enabled()) check(getPrefixes(), { skipIfBusy: true }); };
  firstTimer = setTimeout(tick, 45_000);
  tickTimer = setInterval(tick, 30 * 60_000);
}

export function stopUpdateChecks() {
  clearTimeout(firstTimer);
  clearInterval(tickTimer);
  firstTimer = tickTimer = null;
}

/** "Check now": contact Adobe now, for every registered prefix. */
export function checkNow(prefixes) {
  return check(prefixes, { refresh: true });
}

/** The row Mud Hut reported for `id` in `prefix`, or undefined. */
export function rowFor(prefix, id) {
  return upd.prefixes[prefix]?.apps?.find((a) => a.id === id);
}

export { ukey };
