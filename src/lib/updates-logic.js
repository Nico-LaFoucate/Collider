// lib/updates-logic.js
//
// Pure helpers for the Adobe app updates UI: no Svelte runes and no Tauri, so `node --test` runs
// them (updates-logic.test.js). Everything here turns Mud Hut's answers into what a view shows; it
// decides nothing about updates itself. Mud Hut's own sentences (blocked_reason, message,
// apps_running) are shown as Mud Hut wrote them.

/** Store key for one app in one prefix. Never the app id alone: two prefixes can hold the same app. */
export const ukey = (prefix, id) => `${prefix}\u0000${id}`;
/** The key of one YEAR of an app in a prefix (`major` = "26" for 26.x): two installed years of one app
 *  each have their own update outcome. */
export const ykey = (prefix, id, major) => `${ukey(prefix, id)}\u0000${major}`;
/** A list key for one of Mud Hut's rows: one app can be installed in two yearly versions. */
export const rowKey = (row) => `${row?.id}\u0000${row?.installed?.dir ?? row?.installed?.year ?? ""}`;

/**
 * "up to 3.9 GB", "up to 187 MB"; null when Mud Hut gave no size. Decimal units, as Adobe and file
 * managers show them.
 *
 * For `update.download_bytes`, which is an UPPER BOUND: the whole build plus every dependency it
 * names, because Adobe's installer needs every package of the build in place (unchanged ones too).
 * Packages already on disk are reused, so the real download can be smaller, never larger. Rounded
 * UP so the figure never understates. `changed_bytes` (only what changed) is NOT a download size
 * and is never shown as one.
 */
export function upToSize(bytes) {
  if (typeof bytes !== "number" || !(bytes > 0)) return null;
  if (bytes > 1e9) return `up to ${trim(Math.ceil(bytes / 1e8) / 10)} GB`;
  if (bytes > 1e6) return `up to ${Math.ceil(bytes / 1e6)} MB`;
  return "up to 1 MB";
}
const trim = (n) => String(n).replace(/\.0$/, "");

/** "A", "A and B", "A, B and C". */
export function joinNames(names) {
  const n = (names ?? []).filter(Boolean);
  if (n.length <= 1) return n[0] ?? "";
  return `${n.slice(0, -1).join(", ")} and ${n[n.length - 1]}`;
}

/** The installed version as Mud Hut reports it. */
export const installedLabel = (row) => row?.installed?.label ?? row?.installed?.version ?? "";
/** A target build as Mud Hut labels it. */
export const targetLabel = (t) => t?.label ?? t?.to ?? "";
/** The major number of a build ("27.0.0.40" -> "27"), for `mudhut install --version MAJOR`. */
export const majorOf = (build) => (typeof build === "string" && /^\d+/.test(build) ? build.match(/^\d+/)[0] : null);

/**
 * The row for one app card. Adobe installs yearly versions side by side; Neutron shows one card
 * per app, Mud Hut reports one row per installed year. Take the row of the card's own install (its
 * folder in the card's exe path, else its year), else the newest installed.
 */
export function pickRow(rows, card) {
  const same = (rows ?? []).filter((r) => r && r.id === card?.id);
  if (same.length <= 1) return same[0];
  const exe = String(card.exe ?? "").replace(/\\/g, "/").toLowerCase();
  const dir = (r) => String(r.installed?.dir ?? "").toLowerCase();
  return same.find((r) => dir(r) && exe.includes(`/${dir(r)}/`))
    ?? same.find((r) => r.installed?.year != null && String(r.installed.year) === String(card.version))
    ?? same.reduce((a, b) => (cmpBuild(b.installed?.version, a.installed?.version) > 0 ? b : a));
}
const cmpBuild = (x, y) => {
  const px = String(x ?? "").split(".").map(Number), py = String(y ?? "").split(".").map(Number);
  for (let i = 0; i < Math.max(px.length, py.length); i++) {
    const d = (px[i] || 0) - (py[i] || 0);
    if (d) return d;
  }
  return 0;
};

/** Is this app one of `list`? Mud Hut names running apps by display name and an update's apps by
 *  id, so both are accepted. */
export function listed(list, row) {
  return !!row && (list ?? []).some((x) => x === row.id || x === row.name);
}

/** The name Mud Hut gave an app id in this prefix (falls back to the id). */
export function nameIn(pstate, id) {
  return pstate?.apps?.find((a) => a.id === id || a.name === id)?.name ?? id;
}

/** Mud Hut's update stage tokens in words. An unknown token is shown as it is. */
const STAGES = {
  starting: "Starting", preflight: "Checking", download: "Downloading", snapshot: "Backing up",
  apply: "Installing", provision: "Setting up", verify: "Checking the result",
  rollback: "Putting the backup back",
};
export const stageLabel = (s) => (s ? STAGES[s] ?? s : null);

/** The update running in a prefix: one this Collider started (`job`) first, else one Mud Hut
 *  reports (`update_in_progress`, e.g. left running when Collider was closed). */
export function activeUpdate(job, pstate) {
  if (job) {
    return { ours: true, kind: job.kind, apps: job.apps, app: job.app, stage: job.stage,
             pct: job.pct, msg: job.msg, label: job.label ?? null };
  }
  const u = pstate?.update_in_progress;
  if (u) {
    const apps = u.apps ?? [];
    return { ours: false, kind: "update", apps, app: apps[0] ?? null, stage: u.stage ?? null,
             pct: typeof u.pct === "number" ? u.pct : null, msg: u.msg ?? null,
             startedAt: u.started_at ?? null, log: u.log ?? null, label: null };
  }
  return null;
}

/** What the running job is called in a sentence: "Premiere Pro", or "Photoshop 2027" for an install. */
export function activeName(act, pstate) {
  if (!act) return "";
  return act.label ?? nameIn(pstate, act.app);
}

/** The sentence that locks Launch (and other updates) while a job runs in the prefix. */
export function waitSentence(act, pstate) {
  const what = act.kind === "install" ? "install" : "update";
  return `Wait for the ${activeName(act, pstate)} ${what} to finish.`;
}

/** A verified point update Mud Hut says can run now. */
export function hasVerifiedUpdate(row) {
  const u = row?.update;
  return !!u && u.verified === true && u.can_update !== false;
}

/** What "Update all" in one prefix covers: the verified point updates that can run now. Builds
 *  Neutron has not verified, blocked ones and new yearly versions are never part of it. */
export function updateAllPlan(apps) {
  const include = [], exclude = [];
  for (const row of apps ?? []) {
    const u = row.update;
    if (!u) continue;
    if (u.can_update === false) exclude.push({ row, why: u.blocked_reason ?? null });
    else if (u.verified !== true) exclude.push({ row, why: null, unverified: true });
    else include.push(row);
  }
  return { include, exclude };
}

// --- outcomes --------------------------------------------------------------------------------

/** One app's outcome from Mud Hut's per-app report (`app_result`, or an entry of the terminal
 *  result's `apps`). */
export function outcomeFromAppResult(ev) {
  if (ev?.ok === true) {
    return { state: "updated", from: ev.from ?? null, to: ev.to ?? null, message: ev.message ?? null };
  }
  return { state: ev?.state ?? "failed", message: ev?.message ?? null, log: ev?.log ?? null,
           from: ev?.from ?? null, to: ev?.to ?? null };
}

/** The outcome of a run for an app Mud Hut did not report on one by one, from the terminal object
 *  (`update_adobe_apps` returns it as data: the result, or the error with ok:false). */
export function outcomeFromTerminal(r) {
  if (r?.event === "error") {
    const refused = r.code === "app-running" || r.exit === 3;
    return { state: refused ? "refused" : "failed", message: r.message ?? null, code: r.code ?? null,
             prefixChanged: typeof r.prefix_changed === "boolean" ? r.prefix_changed : null,
             appsRunning: r.apps_running ?? [], log: r.log ?? null };
  }
  if (r?.ok === true) return { state: "updated", message: null };
  const state = r?.exit === 7 ? "rolled_back" : r?.exit === 8 ? "incomplete" : "failed";
  return { state, message: r?.message ?? null, log: r?.log ?? null };
}

/**
 * The outcome of a yearly install from Mud Hut's terminal result. `removed_previous` lists each
 * older version it was asked to remove: `{version, removed: true}`, or `{version, removed: false,
 * message}` when Adobe's uninstaller failed or the older version was copied without Adobe's records
 * (then it is left in place). A version that stayed is reported; it is not a failed install.
 */
export function outcomeFromInstall(r, { label, name }) {
  if (r?.ok === false) return { state: "failed", message: r.message ?? r.reason ?? `${label} didn't install.` };
  const parts = [];
  let warn = false;
  for (const p of Array.isArray(r?.removed_previous) ? r.removed_previous : []) {
    const v = p?.version ? `${name} ${p.version}` : `The previous ${name}`;
    if (p?.removed === true) parts.push(`Removed ${v}.`);
    else { warn = true; parts.push(`${v} was not removed.${p?.message ? ` ${p.message}` : ""}`); }
  }
  if (r?.exit && (r.message ?? r.reason)) { warn = true; parts.push(r.message ?? r.reason); }
  return { state: "installed", label, message: parts.length ? parts.join(" ") : null, warn };
}

/** The major of the install an outcome is about ("26" from from/to 26.3 -> 26.5), or null when Mud Hut did not say. */
export const outcomeMajor = (o) => majorOf(o?.from ?? o?.to) ?? o?.major ?? null;

/** Can outcome `o` be about `row`'s install? Not when both name a year and they differ. */
export function sameYear(o, row) {
  const a = outcomeMajor(o), b = majorOf(row?.installed?.version);
  return !a || !b || a === b;
}

/**
 * The outcome to show for app `id` in `prefix` (`row`: Mud Hut's row for the install, when known).
 * Outcomes are stored per year (`ykey`) when Mud Hut said which install they are about, else per app
 * (`ukey`, e.g. "Close Premiere Pro first."), which then applies to every installed year.
 */
export function outcomeFor(outcomes, prefix, id, row, rows = null) {
  const m = majorOf(row?.installed?.version);
  const o = (m ? outcomes?.[ykey(prefix, id, m)] : undefined) ?? outcomes?.[ukey(prefix, id)];
  if (o || !rows) return o;
  // A new yearly version installed whose own row is not listed yet (the re-read has not come back, or
  // failed): its "installed" shows on the app rather than nowhere. `rows`: Mud Hut's rows for the prefix.
  const listedMajors = new Set(rows.filter((r) => r?.id === id).map((r) => majorOf(r.installed?.version)));
  const base = `${ukey(prefix, id)}\u0000`;
  for (const [k, v] of Object.entries(outcomes ?? {})) {
    if (k.startsWith(base) && v?.state === "installed" && !listedMajors.has(k.slice(base.length))) return v;
  }
  return undefined;
}

/** An update this Collider did not watch, from Mud Hut's `last_update`. Only a failure is kept: a
 *  success already shows as the new installed version. */
export function outcomeFromLastUpdate(lu) {
  if (!lu || lu.ok !== false) return null;
  return { state: lu.state ?? "failed", message: lu.message ?? null, at: lu.at ?? null, log: lu.log ?? null };
}

const FAILED = new Set(["unchanged", "rolled_back", "incomplete", "failed"]);
export const isFailure = (o) => !!o && FAILED.has(o.state);

/** The sentence(s) a card or row shows for an outcome. `fromLabel` is the version the app had. */
export function outcomeNote(o, name, fromLabel) {
  if (!o) return null;
  const parts = [];
  if (o.state === "unchanged") {
    parts.push(fromLabel ? `${name} wasn't changed and still opens as ${fromLabel}.` : `${name} wasn't changed.`);
  } else if (o.state === "rolled_back") {
    parts.push(`The update didn't finish, so Neutron put back ${name}${fromLabel ? ` ${fromLabel}` : ""}.`);
  } else if (o.state === "incomplete") {
    parts.push(`The update stopped partway, and ${name} may not open until it finishes. Run the update again.`);
  } else if (o.state === "failed" && !o.message) {
    parts.push(`The update of ${name} didn't finish.`);
  }
  if (o.message) parts.push(o.message);
  if ((o.state === "failed" || o.state === "refused") && o.prefixChanged === false) {
    parts.push("Nothing was changed.");
  }
  return parts.length ? parts.join(" ") : null;
}

/** The short pill for an outcome. */
export function outcomePill(o) {
  switch (o?.state) {
    case "updated": return { text: o.toLabel ? `Updated to ${o.toLabel}` : "Updated", cls: "ok" };
    case "installed": return { text: o.label ? `${o.label} installed` : "Installed", cls: "ok" };
    case "unchanged": return { text: "Not updated", cls: "warn" };
    case "rolled_back": return { text: "Update put back", cls: "warn" };
    case "incomplete": return { text: "Update incomplete", cls: "bad" };
    case "failed": return { text: "Update failed", cls: "warn" };
    default: return null;   // "refused": nothing happened, so no failure styling
  }
}

// --- the app card ----------------------------------------------------------------------------

/**
 * Everything an app card shows about updates.
 *   row         Mud Hut's check row for this app in this prefix (or undefined)
 *   id, name    the card's app (used when Mud Hut has no row for it)
 *   pstate      Mud Hut's prefix object (apps_running, update_in_progress, last_update)
 *   job         the job this Collider runs in the prefix, if any
 *   outcome     how this Collider's last run for the app ended, if any
 *   appRunning  this card's app is running (the card's own, always fresh)
 *   neutronBusy a Neutron update is running
 * Returns { pills, outcomePill, progress, button, launchLock, note, noteCls, log }.
 */
export function cardView({ row, id, name, pstate, job, outcome, appRunning = false, neutronBusy = false }) {
  const view = { pills: [], outcomePill: null, progress: null, button: null, launchLock: null, note: null, noteCls: "", log: null };
  if (!row && !job && !pstate?.update_in_progress && !outcome) return view;
  const appName = row?.name ?? name;
  const act = activeUpdate(job, pstate);

  if (act) {
    view.launchLock = waitSentence(act, pstate);
    if (listed(act.apps, row ?? { id, name: appName })) {
      const installing = act.kind === "install";
      view.pills.push({ text: installing ? "Installing…" : "Updating…", cls: "warn pulse" });
      // Mud Hut's own sentence when it gave one ("Downloading Premiere Pro"), else the stage.
      view.progress = {
        pct: typeof act.pct === "number" ? act.pct : null,
        text: act.msg || stageLabel(act.stage) || "",
      };
      view.button = { label: installing ? "Installing…" : "Updating…", disabled: true, reason: null, action: null };
      return view;
    }
  }

  const o = outcome ?? outcomeFromLastUpdate(listed([pstate?.last_update?.app], row) && sameYear(pstate.last_update, row)
    ? pstate.last_update : null);
  const op = outcomePill(o);
  if (op) view.pills.push(op);
  view.outcomePill = op;   // on its own too, for views that list the update itself in words
  if (o) {
    view.note = outcomeNote(o, appName, installedLabel(row) || o.from);
    view.noteCls = o.state === "incomplete" ? "bad" : isFailure(o) || o.state === "refused" || o.warn ? "warn" : "";
    view.log = isFailure(o) ? o.log ?? null : null;
  }

  const u = row?.update;
  const nm = row?.new_major;
  if (u) {
    view.pills.push(u.verified === true
      ? { text: `Update ${installedLabel(row)} → ${targetLabel(u)}`, cls: "warn" }
      : { text: `${targetLabel(u)} available · not yet verified`, cls: "",
          title: "Neutron hasn't verified this version yet. You can still update to it." });
  }
  if (nm) {
    view.pills.push({ text: `${nm.label ?? nm.year} version available`, cls: "",
                      title: `${appName} ${nm.label ?? nm.year} is a new yearly version.` });
  }

  if (o?.state === "incomplete") {
    view.button = { label: "Finish update", action: "finish" };
  } else if (u) {
    view.button = { label: isFailure(o) ? "Try again" : u.verified === true ? "Update" : "Update…",
                    action: u.verified === true ? "update" : "unverified" };
  } else if (nm) {
    view.button = { label: `Install ${nm.label ?? nm.year}…`, action: "yearly" };
  }

  if (view.button) {
    const reason = act ? view.launchLock
      : neutronBusy ? "Wait for the Neutron update to finish."
      : appRunning ? `Close ${appName} first.`
      : (view.button.action === "update" || view.button.action === "unverified") && u?.can_update === false
        ? (u.blocked_reason ?? "Mud Hut can't update it right now.")
      : null;
    view.button.disabled = !!reason;
    view.button.reason = reason;
  }
  return view;
}

// --- the Prefixes tab line -------------------------------------------------------------------

/** The one-line Updates summary for a prefix. `outcomeOf(row)`: this Collider's outcome for that row's
 *  install (`outcomes`: { appId: outcome }, the older form, when it is not given). */
export function prefixSummary({ pstate, job, outcomes = {}, outcomeOf = (r) => outcomes[r.id] }) {
  const act = activeUpdate(job, pstate);
  if (act) {
    const pct = typeof act.pct === "number" ? ` ${act.pct}%` : "";
    const what = act.kind === "install" ? "Installing" : "Updating";
    return { dot: "warn", text: `${what} ${activeName(act, pstate)}…${pct}`, action: null };
  }
  if (!pstate) return { dot: "", text: "Not checked yet", action: null };
  if (pstate.ok === false) return { dot: "", text: prefixError(pstate), action: null };
  const apps = pstate.apps ?? [];
  const stuck = apps.find((r) => (outcomeOf(r) ?? outcomeFromLastUpdate(
    listed([pstate.last_update?.app], r) && sameYear(pstate.last_update, r) ? pstate.last_update : null))?.state === "incomplete");
  if (stuck) return { dot: "bad", text: `The ${stuck.name} update stopped partway. Run it again.`,
                      action: { kind: "finish", id: stuck.id } };
  const { include, exclude } = updateAllPlan(apps);
  if (include.length === 1) {
    const r = include[0];
    return { dot: "warn", text: `${r.name} ${targetLabel(r.update)} is available`, action: { kind: "update", id: r.id } };
  }
  if (include.length > 1) return { dot: "warn", text: `${include.length} updates available`, action: { kind: "all" } };
  const unverified = exclude.filter((e) => e.unverified).length;
  if (unverified) {
    return { dot: "", text: unverified === 1 ? "1 update not yet verified by Neutron"
                                             : `${unverified} updates not yet verified by Neutron`,
             action: { kind: "view" } };
  }
  const majors = apps.filter((r) => r.new_major);
  if (majors.length) {
    return { dot: "", text: majors.length === 1
               ? `${majors[0].name} ${majors[0].new_major.label ?? majors[0].new_major.year} is available`
               : `${majors.length} new yearly versions are available`, action: { kind: "view" } };
  }
  if (exclude.length) return { dot: "", text: exclude[0].why ?? "Updates are waiting.", action: { kind: "view" } };
  return { dot: "ok", text: "Up to date", action: null };
}

/** Mud Hut's sentence for a prefix it could not check (`error`; `message` accepted too). */
export const prefixError = (pstate) => pstate?.error ?? pstate?.message ?? "Couldn't check this prefix.";

/** How many verified point updates are ready across the given prefix objects (the nav count). */
export function readyCount(prefixStates) {
  let n = 0;
  for (const p of Object.values(prefixStates ?? {})) n += updateAllPlan(p?.apps).include.length;
  return n;
}

// --- the update dialog -----------------------------------------------------------------------

function closedLine(prefixName, all) {
  return `Keep every Adobe app in “${prefixName}” closed until ${all ? "all of them finish" : "it finishes"}.`;
}
function snapshotLine(snapshot) {
  return snapshot
    ? "A backup copy is kept first and put back if the update fails."
    : "Backups are turned off in Preferences, so a failed update can't be put back automatically.";
}
// Mud Hut flags a Windows copy that Adobe's installer has no record of (`update.adopt`); it
// registers the copy first, so the run takes longer than a plain update.
const ADOPT_LINE = "This copy came from Windows, so Mud Hut first registers it with Adobe's installer, then updates it.";
// Speech-to-text language packs: optional, off by default, as in Creative Cloud (DECISIONS C70).
// Premiere Pro and Media Encoder ship the same 16 packs and Mud Hut's --speech-packs applies to
// both; Mud Hut measured 14.49 GB per app (Premiere Pro 26.5.2: 3.88 GB without, 18.36 GB with).
// One wording for both apps, so the help line names neither.
export const SPEECH_APPS = ["premiere", "mediaencoder"];
export const SPEECH_LABEL = "Speech-to-text languages (about 14.5 GB)";
export const SPEECH_HELP = "They turn speech into text for captions. Creative Cloud doesn't install them by default either.";

/**
 * The text of the one update dialog, for each request:
 *   { kind: "update", row, related: [rows] }  one app, a verified point update
 *   { kind: "all", rows }                     every verified point update in the prefix
 *   { kind: "unverified", row, target }       a build Neutron has not verified ("Update anyway")
 *   { kind: "yearly", row }                   a new yearly version, installed next to this one
 *   { kind: "finish", row }                   run an update that stopped partway again
 * ctx = { prefixName, snapshot (the backup setting), appsRunning (fresh from Mud Hut) }.
 * Returns { title, lines, items, options, confirm, cancel, blockedBy }.
 */
export function dialogModel(req, { prefixName, snapshot = true, appsRunning = [] }) {
  const m = { title: "", lines: [], items: [], options: [], confirm: "Update", cancel: "Not now", blockedBy: null };
  const row = req.row;
  const name = row?.name ?? "";
  const verb = req.kind === "yearly" ? "Installing" : "Updating";
  if (appsRunning?.length) {
    m.blockedBy = `Close ${joinNames(appsRunning)} first. ${verb} needs every Adobe app in “${prefixName}” closed.`;
  }

  if (req.kind === "update") {
    const u = row.update;
    const size = upToSize(u?.download_bytes);
    m.title = `Update ${name}?`;
    m.lines.push(`${name} ${installedLabel(row)} → ${targetLabel(u)}, downloaded from Adobe${size ? ` (${size})` : ""}.`);
    if (u?.adopt === true) m.lines.push(ADOPT_LINE);
    m.lines.push(closedLine(prefixName, false));
    m.lines.push(snapshotLine(snapshot));
    for (const r of req.related ?? []) {
      m.options.push({ key: `also:${r.id}`, label: `Also update ${r.name} (${installedLabel(r)} → ${targetLabel(r.update)})`,
                       value: false });
    }
  } else if (req.kind === "all") {
    const rows = req.rows ?? [];
    const sizes = rows.map((r) => r.update?.download_bytes);
    // Each figure is that app's upper bound (apps can share components, so the sum overstates
    // further); still an honest "up to".
    const total = sizes.every((b) => typeof b === "number" && b > 0) ? upToSize(sizes.reduce((a, b) => a + b, 0)) : null;
    m.title = `Update ${rows.length} apps in “${prefixName}”?`;
    m.items = rows.map((r) => `${r.name} ${installedLabel(r)} → ${targetLabel(r.update)}`);
    m.lines.push(`They update one after another${total ? `, ${total} to download in total` : ""}.`);
    m.lines.push(closedLine(prefixName, true));
    m.lines.push(snapshotLine(snapshot));
    for (const e of req.excluded ?? []) {
      m.lines.push(e.unverified
        ? `Not included: ${e.row.name} ${targetLabel(e.row.update)}, which Neutron hasn't verified yet. You can update it on its own.`
        : `Not included: ${e.row.name}.${e.why ? ` ${e.why}` : ""}`);
    }
    m.confirm = "Update all";
  } else if (req.kind === "unverified") {
    const t = req.target;
    // Mud Hut sizes only its recommended target (`update`); a newer build gets no figure.
    const size = t === row.update ? upToSize(row.update?.download_bytes) : null;
    m.title = `Neutron hasn't verified ${name} ${targetLabel(t)} yet.`;
    m.lines.push("It might not open, or parts of it might not work, until Neutron is updated for it.");
    if (row.update?.verified === true && row.update.to !== t.to) {
      m.lines.push(`${name} ${targetLabel(row.update)} is verified, if you'd rather update to that.`);
    } else {
      m.lines.push("Once Neutron has verified it, it shows here as a normal update.");
    }
    m.lines.push(`${name} ${installedLabel(row)} → ${targetLabel(t)}, downloaded from Adobe${size ? ` (${size})` : ""}.`);
    if (row.update?.adopt === true) m.lines.push(ADOPT_LINE);
    m.lines.push(closedLine(prefixName, false));
    m.lines.push(snapshotLine(snapshot));
    m.confirm = "Update anyway";
    m.cancel = "Wait";
  } else if (req.kind === "yearly") {
    const nm = row.new_major;
    const year = nm.label ?? String(nm.year);
    const have = row.installed?.year ? String(row.installed.year) : installedLabel(row);
    m.title = `Install ${name} ${year}?`;
    m.lines.push("This is a new yearly version. It downloads from Adobe as a full install.");
    if (nm.side_by_side === false) {
      m.lines.push(`It replaces ${name} ${have}.`);
    } else {
      m.lines.push(`It installs next to ${name} ${have}, which stays as it is unless you choose to remove it.`);
      m.options.push({ key: "removePrevious", label: `Remove ${name} ${have} after ${year} installs`, value: false });
    }
    m.lines.push(`Projects saved in ${name} ${year} might not open in ${have}.`);
    if (nm.verified !== true) {
      m.lines.push(`Neutron hasn't verified ${name} ${year} yet. It might not open, or parts of it might not work.`);
    }
    m.lines.push(`Keep every Adobe app in “${prefixName}” closed until it finishes.`);
    if (SPEECH_APPS.includes(row.id)) {
      m.options.push({ key: "speechPacks", label: SPEECH_LABEL, help: SPEECH_HELP, value: false });
    }
    m.confirm = `Install ${year}`;
  } else if (req.kind === "finish") {
    m.title = `Finish updating ${name}?`;
    m.lines.push(`The last update stopped partway. Mud Hut runs it again${row.update ? ` (${targetLabel(row.update)})` : ""}.`);
    m.lines.push(closedLine(prefixName, false));
    m.lines.push(snapshotLine(snapshot));
    m.confirm = "Finish update";
  }
  return m;
}

/** What to run when the user confirms the dialog. `choices` = { optionKey: bool }.
 *  -> { op: "update", apps, to, allowUnverified } | { op: "install", app, version, removePrevious, speechPacks } */
export function dialogAction(req, choices = {}) {
  const row = req.row;
  if (req.kind === "update") {
    const also = (req.related ?? []).filter((r) => choices[`also:${r.id}`]).map((r) => r.id);
    const apps = [row.id, ...also];
    // `--to` pins the build the user saw; it names one app's build, so only for one app.
    return { op: "update", apps, to: apps.length === 1 ? row.update?.to ?? null : null, allowUnverified: false };
  }
  if (req.kind === "all") return { op: "update", apps: [...new Set((req.rows ?? []).map((r) => r.id))], to: null, allowUnverified: false };
  if (req.kind === "unverified") return { op: "update", apps: [row.id], to: req.target?.to ?? null, allowUnverified: true };
  if (req.kind === "finish") return { op: "update", apps: [row.id], to: null, allowUnverified: false };
  if (req.kind === "yearly") {
    const nm = row.new_major;
    // `--version` takes the MAJOR number (Photoshop 2027 is 28), never the year: Mud Hut's own
    // `major`, else the leading number of the build it named.
    const major = nm?.major ?? majorOf(nm?.to);
    // The dialog tells the user when the version is not verified; confirming it is the "one
    // confirmation" (DECISIONS C70), so it passes --allow-unverified. Mud Hut still installs a
    // verified build of that version when one exists.
    return { op: "install", app: row.id, version: major === null || major === undefined ? null : String(major),
             label: `${row.name} ${nm?.label ?? nm?.year}`, allowUnverified: nm?.verified !== true,
             removePrevious: !!choices.removePrevious, speechPacks: !!choices.speechPacks };
  }
  return null;
}
