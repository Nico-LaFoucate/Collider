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

import { checkAdobeUpdates, updateAdobeApps, installApp } from "$lib/api.js";
import { ukey, outcomeFromAppResult, outcomeFromTerminal, outcomeFromInstall, targetLabel } from "$lib/updates-logic.js";

export const upd = $state({
  supported: true,     // false: Mud Hut cannot check (unsupported says why)
  unsupported: null,   // "too-old" | "missing"
  checking: false,     // a check is running (any kind)
  refreshing: false,   // a "Check now" is running
  // The last check that was allowed to contact Adobe: { checked_at, online, feed_error,
  // from_cache, verified_list }. Offline reads never replace it.
  lastNetwork: null,
  // When Mud Hut last got Adobe's product list, for "Last checked": its `feed.fetched_at`, which
  // every check reports (an offline read too: it is the time of the list it answered from).
  lastReachedAt: null,
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
    // "Last checked" = when Adobe's list was fetched: Mud Hut's `feed.fetched_at`, never its
    // `checked_at`, which is the time of this run (on a cached tick, not when Adobe was asked).
    // A Mud Hut without `feed` falls back to `checked_at` of a check that reached Adobe.
    const reached = r.feed ? r.feed.fetched_at
      : (!offline && r.online !== false ? r.checked_at : null);
    if (reached && !(Date.parse(upd.lastReachedAt) > Date.parse(reached))) upd.lastReachedAt = reached;
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

// --- running an update -----------------------------------------------------------------------

/** True while this Collider runs any Adobe update or yearly install (Neutron's own update waits). */
export const anyJob = () => Object.keys(upd.jobs).length > 0;

function track(prefix, ev) {
  const j = upd.jobs[prefix];
  if (!j) return;
  if (ev.app) j.app = ev.app;
  if (ev.event === "progress") {
    if (ev.stage) j.stage = ev.stage;
    if (typeof ev.pct === "number") j.pct = ev.pct;
    if (ev.msg) j.msg = ev.msg;
  } else if (ev.event === "note" && ev.msg) {
    j.msg = ev.msg;
  } else if (ev.event === "app_result" && ev.app) {
    record(prefix, ev.app, outcomeFromAppResult(ev));
  }
}

// The target label the user saw goes on a success, so the card can say "Updated to 26.5.2".
let shown = {};
function record(prefix, id, o) {
  if (o.state === "updated" && !o.toLabel) o.toLabel = shown[ukey(prefix, id)] ?? null;
  upd.outcomes[ukey(prefix, id)] = o;
}

/**
 * Run `mudhut update` for the apps the user confirmed in `prefix`. Progress, per-app outcomes and
 * the final answer land in the store; the dialog closes as soon as this starts. Afterwards the
 * prefix is re-read (installed versions changed) and the page reloads its apps.
 *   apps            ids the user confirmed (never "all")
 *   to              the exact build the user saw (one app)
 *   allowUnverified the user chose a build Neutron has not verified
 *   noSnapshot      the Preferences backup setting is off
 *   labels          { id: target label } for the success pills
 */
export async function runUpdate({ prefix, apps, to = null, allowUnverified = false, noSnapshot = false, labels = {} }) {
  if (upd.jobs[prefix] || !apps?.length) return null;
  for (const id of apps) {
    delete upd.outcomes[ukey(prefix, id)];
    shown[ukey(prefix, id)] = labels[id] ?? null;
  }
  upd.jobs[prefix] = { kind: "update", apps: [...apps], app: apps[0], stage: "starting", pct: 0, msg: "", label: null };
  let r = null;
  try {
    r = await updateAdobeApps({ prefix, apps, to, allowUnverified, noSnapshot, onEvent: (ev) => track(prefix, ev) });
  } catch (e) {
    // Mud Hut could not run, or stopped without a word: nothing it reported, so say only that.
    for (const id of apps) {
      if (!upd.outcomes[ukey(prefix, id)]) record(prefix, id, { state: "failed", message: String(e) });
    }
  } finally {
    delete upd.jobs[prefix];
  }
  if (r) settle(prefix, apps, r);
  await check([prefix], { offline: true, partial: true });
  hooks.prefixChanged(prefix);
  return r;
}

// Apps the run did not report on one by one take their outcome from the terminal object: the
// result's `apps` entry when there is one, else the result or error as a whole.
function settle(prefix, apps, r) {
  const perApp = Array.isArray(r.apps) ? r.apps : [];
  for (const id of apps) {
    if (upd.outcomes[ukey(prefix, id)]) continue;
    const entry = perApp.find((a) => a && typeof a === "object" && (a.app === id || a.id === id));
    record(prefix, id, entry ? outcomeFromAppResult(entry) : outcomeFromTerminal(r));
  }
}

/**
 * Install a new yearly version (`mudhut install APP --method download --version MAJOR`), which
 * holds the prefix like an update does, so it is a job of the same kind on the card.
 */
export async function runYearlyInstall({ prefix, app, name = app, version, label, allowUnverified = false,
                                         removePrevious = false, speechPacks = false }) {
  if (upd.jobs[prefix]) return null;
  delete upd.outcomes[ukey(prefix, app)];
  upd.jobs[prefix] = { kind: "install", apps: [app], app, stage: "starting", pct: 0, msg: "", label };
  let r = null;
  try {
    r = await installApp({ app, method: "download", prefix, version, allowUnverified, removePrevious, speechPacks,
                           onEvent: (ev) => track(prefix, ev) });
    upd.outcomes[ukey(prefix, app)] = outcomeFromInstall(r, { label, name });
  } catch (e) {
    upd.outcomes[ukey(prefix, app)] = { state: "failed", message: String(e) };
  } finally {
    delete upd.jobs[prefix];
  }
  await check([prefix], { offline: true, partial: true });
  hooks.prefixChanged(prefix);
  return r;
}

/** The labels to show on success, from the rows the user confirmed. */
export function targetLabels(rows, target = null) {
  const out = {};
  for (const r of rows ?? []) out[r.id] = targetLabel(target ?? r.update);
  return out;
}

export { ukey };
