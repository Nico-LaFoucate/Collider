// Tests for lib/updates-logic.js. Run: node --test src/lib/
// Fixtures follow CONTRACT.md's check-updates and update shapes.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  ukey, upToSize, joinNames, majorOf, updateAllPlan, cardView, prefixSummary, readyCount,
  dialogModel, dialogAction, outcomeFromAppResult, outcomeFromTerminal, outcomeFromLastUpdate,
  outcomeFromInstall, outcomeNote, SPEECH_LABEL, SPEECH_HELP,
} from "./updates-logic.js";
import { readFileSync } from "node:fs";

// `mudhut --json check-updates --offline` as the finished Mud Hut (build 3c6e464) printed it for
// two lab copies and a bad path (lab paths shortened to /lab).
const REAL = JSON.parse(readFileSync(new URL("./fixtures/check-updates-mudhut-3c6e464.json", import.meta.url), "utf8"));

// The contract's Premiere row (verified point update, a newer unverified build, a new yearly version).
const premiere = () => ({
  id: "premiere", name: "Premiere Pro", sap: "PPRO",
  installed: { version: "26.3.0.12", base: "26.0", year: 2026, managed_by: "hdpim" },
  update: { kind: "in_place", to: "26.5.2.3", label: "26.5.2", verified: true, download_bytes: 187000000,
            can_update: true, blocked_reason: null },
  newer_unverified: { to: "26.6.0.1", label: "26.6" },
  new_major: { year: 2027, to: "27.0.0.40", label: "2027", verified: false },
  related: ["aftereffects", "mediaencoder"],
});
const aftereffects = () => ({
  id: "aftereffects", name: "After Effects", installed: { version: "26.3.0.5", year: 2026, managed_by: "windows-copy" },
  update: { kind: "in_place", to: "26.5.0.9", label: "26.5", verified: true, download_bytes: 2400000000, can_update: true },
  newer_unverified: null, new_major: null, related: ["premiere"],
});
const photoshopUnverified = () => ({
  id: "photoshop", name: "Photoshop", installed: { version: "27.0.1.4", year: 2026, managed_by: "hdpim" },
  update: { kind: "in_place", to: "27.1.0.3", label: "27.1", verified: false, download_bytes: 900000000, can_update: true },
  newer_unverified: null, new_major: null, related: [],
});
const upToDate = () => ({
  id: "illustrator", name: "Illustrator", installed: { version: "30.0.0.1", year: 2026, managed_by: "hdpim" },
  update: null, newer_unverified: null, new_major: null, related: [],
});
const pstate = (apps, extra = {}) => ({ prefix: "/p", ok: true, apps_running: [], update_in_progress: null,
                                        last_update: null, apps, ...extra });

test("keys are per prefix, never the app id alone", () => {
  assert.notEqual(ukey("/home/u/.premiere2025", "premiere"), ukey("/home/u/Adobe2026", "premiere"));
});

test("sizes and names read plainly", () => {
  // An upper bound, rounded UP so it never understates.
  assert.equal(upToSize(187000000), "up to 187 MB");
  assert.equal(upToSize(186100001), "up to 187 MB");
  assert.equal(upToSize(3100000000), "up to 3.1 GB");
  assert.equal(upToSize(3840000000), "up to 3.9 GB", "3.84 GB must not read as 3.8");
  assert.equal(upToSize(2000000000), "up to 2 GB");
  assert.equal(upToSize(null), null);
  assert.equal(joinNames(["Premiere Pro"]), "Premiere Pro");
  assert.equal(joinNames(["Premiere Pro", "After Effects"]), "Premiere Pro and After Effects");
  assert.equal(joinNames(["A", "B", "C"]), "A, B and C");
  assert.equal(majorOf("27.0.0.40"), "27");
});

test("update all covers verified point updates only", () => {
  const blocked = { ...aftereffects(), update: { ...aftereffects().update, can_update: false,
                                                 blocked_reason: "Update Neutron first." } };
  const plan = updateAllPlan([premiere(), blocked, photoshopUnverified(), upToDate()]);
  assert.deepEqual(plan.include.map((r) => r.id), ["premiere"]);
  assert.equal(plan.exclude.length, 2);
  assert.equal(plan.exclude[0].why, "Update Neutron first.");
  assert.equal(plan.exclude[1].unverified, true);
});

test("card: a verified update shows the pill and an enabled Update button", () => {
  const v = cardView({ row: premiere(), id: "premiere", pstate: pstate([premiere()]) });
  assert.equal(v.pills[0].text, "Update 26.3.0.12 → 26.5.2");
  assert.equal(v.pills[0].cls, "warn");
  assert.equal(v.pills[1].text, "2027 version available");
  assert.deepEqual([v.button.label, v.button.action, v.button.disabled], ["Update", "update", false]);
  assert.equal(v.launchLock, null);
});

test("card: disabled reasons, in order, with Mud Hut's blocked_reason verbatim", () => {
  const row = premiere();
  assert.equal(cardView({ row, pstate: pstate([row]), appRunning: true }).button.reason, "Close Premiere Pro first.");
  assert.equal(cardView({ row, pstate: pstate([row]), neutronBusy: true }).button.reason,
               "Wait for the Neutron update to finish.");
  const blocked = { ...row, update: { ...row.update, can_update: false, blocked_reason: "Update Neutron first: this version needs a newer runtime." } };
  const v = cardView({ row: blocked, pstate: pstate([blocked]) });
  assert.equal(v.button.disabled, true);
  assert.equal(v.button.reason, "Update Neutron first: this version needs a newer runtime.");
});

test("card: an update of another app in the prefix locks Launch and Update", () => {
  const job = { kind: "update", apps: ["aftereffects"], app: "aftereffects", stage: "download", pct: 30, msg: "" };
  const p = pstate([premiere(), aftereffects()]);
  const v = cardView({ row: premiere(), pstate: p, job });
  assert.equal(v.launchLock, "Wait for the After Effects update to finish.");
  assert.equal(v.button.disabled, true);
  assert.equal(v.progress, null);
  const mine = cardView({ row: aftereffects(), pstate: p, job });
  assert.equal(mine.pills[0].text, "Updating…");
  assert.deepEqual(mine.progress, { pct: 30, text: "Downloading" }, "the stage when Mud Hut gave no sentence");
  const said = cardView({ row: aftereffects(), pstate: p, job: { ...job, msg: "Downloading After Effects" } });
  assert.equal(said.progress.text, "Downloading After Effects", "Mud Hut's sentence, not the stage repeated");
  assert.equal(mine.button.label, "Updating…");
});

test("card: an update left running by an earlier Collider (update_in_progress) locks the prefix", () => {
  const p = pstate([premiere()], { update_in_progress: { pid: 4242, apps: ["premiere"], started_at: "2026-10-16T14:03:00Z",
                                                          stage: "apply", log: "/p/.mudhut/update.log" } });
  const v = cardView({ row: premiere(), pstate: p });
  assert.equal(v.launchLock, "Wait for the Premiere Pro update to finish.");
  assert.equal(v.progress.text, "Installing");
  assert.equal(v.progress.pct, null);
});

test("card: an unverified build and a yearly version", () => {
  const v = cardView({ row: photoshopUnverified(), pstate: pstate([photoshopUnverified()]) });
  assert.equal(v.pills[0].text, "27.1 available · not yet verified");
  assert.deepEqual([v.button.label, v.button.action], ["Update…", "unverified"]);
  const yearlyOnly = { ...premiere(), update: null };
  const y = cardView({ row: yearlyOnly, pstate: pstate([yearlyOnly]) });
  assert.deepEqual([y.button.label, y.button.action], ["Install 2027…", "yearly"]);
});

test("card: outcomes (rolled back, unchanged, incomplete, refused, updated)", () => {
  const row = premiere();
  const p = pstate([row]);
  const rb = cardView({ row, pstate: p, outcome: { state: "rolled_back", message: "Adobe's installer stopped." } });
  assert.equal(rb.note, "The update didn't finish, so Neutron put back Premiere Pro 26.3.0.12. Adobe's installer stopped.");
  assert.equal(rb.button.label, "Try again");
  const un = cardView({ row, pstate: p, outcome: { state: "unchanged", message: null } });
  assert.equal(un.note, "Premiere Pro wasn't changed and still opens as 26.3.0.12.");
  const inc = cardView({ row, pstate: p, outcome: { state: "incomplete", message: null, log: "/p/.mudhut/update.log" } });
  assert.equal(inc.pills[0].text, "Update incomplete");
  assert.equal(inc.note, "The update stopped partway, and Premiere Pro may not open until it finishes. Run the update again.");
  assert.deepEqual([inc.button.label, inc.button.action], ["Finish update", "finish"]);
  assert.equal(inc.log, "/p/.mudhut/update.log");
  const ref = cardView({ row, pstate: p, outcome: outcomeFromTerminal({ event: "error", code: "app-running", exit: 3,
    prefix_changed: false, apps_running: ["After Effects"], message: "Close After Effects first.", ok: false }) });
  assert.equal(ref.pills[0].text, "Update 26.3.0.12 → 26.5.2", "a refusal gets no failure pill");
  assert.equal(ref.note, "Close After Effects first. Nothing was changed.");
  assert.equal(ref.button.label, "Update");
  const ok = cardView({ row: upToDate(), pstate: pstate([upToDate()]),
                        outcome: { state: "updated", toLabel: "30.1" } });
  assert.equal(ok.pills[0].text, "Updated to 30.1");
  assert.deepEqual(ok.outcomePill, { text: "Updated to 30.1", cls: "ok" }, "the outcome on its own, for the Updates view");
  const yr = cardView({ row: premiere(), pstate: p, outcome: { state: "installed", label: "Premiere Pro 2027", message: null } });
  assert.equal(yr.outcomePill.text, "Premiere Pro 2027 installed");
  assert.equal(ok.button, null);
});

test("card: Mud Hut's last_update shows a failure, never a stale success", () => {
  const row = premiere();
  const failed = pstate([row], { last_update: { ok: false, app: "premiere", state: "rolled_back", message: "m", at: "x" } });
  assert.equal(cardView({ row, pstate: failed }).pills[0].text, "Update put back");
  const okp = pstate([row], { last_update: { ok: true, app: "premiere", at: "x" } });
  assert.equal(cardView({ row, pstate: okp }).pills[0].text, "Update 26.3.0.12 → 26.5.2");
  const other = pstate([row], { last_update: { ok: false, app: "photoshop", state: "incomplete", at: "x" } });
  assert.equal(cardView({ row, pstate: other }).button.action, "update", "another app's failure is not this card's");
});

test("prefix summary line", () => {
  assert.equal(prefixSummary({ pstate: undefined }).text, "Not checked yet");
  assert.equal(prefixSummary({ pstate: { prefix: "/p", ok: false, error: "/p is not a Neutron prefix.", apps: [] } }).text,
               "/p is not a Neutron prefix.", "Mud Hut puts a failed prefix's sentence in `error`");
  assert.equal(prefixSummary({ pstate: pstate([upToDate()]) }).text, "Up to date");
  assert.deepEqual(prefixSummary({ pstate: pstate([premiere(), upToDate()]) }),
                   { dot: "warn", text: "Premiere Pro 26.5.2 is available", action: { kind: "update", id: "premiere" } });
  assert.equal(prefixSummary({ pstate: pstate([premiere(), aftereffects()]) }).text, "2 updates available");
  assert.equal(prefixSummary({ pstate: pstate([photoshopUnverified()]) }).text, "1 update not yet verified by Neutron");
  assert.equal(prefixSummary({ pstate: pstate([premiere()]), outcomes: { premiere: { state: "incomplete" } } }).dot, "bad");
  const job = { kind: "update", apps: ["premiere"], app: "premiere", stage: "download", pct: 42 };
  assert.equal(prefixSummary({ pstate: pstate([premiere()]), job }).text, "Updating Premiere Pro… 42%");
  assert.equal(readyCount({ "/a": pstate([premiere(), aftereffects()]), "/b": pstate([premiere()]) }), 3);
});

test("dialog: one verified update", () => {
  const m = dialogModel({ kind: "update", row: premiere(), related: [aftereffects()] },
                        { prefixName: "Adobe 2026", snapshot: true });
  assert.equal(m.title, "Update Premiere Pro?");
  assert.equal(m.lines[0], "Premiere Pro 26.3.0.12 → 26.5.2, downloaded from Adobe (up to 187 MB).");
  assert.equal(m.lines[1], "Keep every Adobe app in “Adobe 2026” closed until it finishes.");
  assert.equal(m.lines[2], "A backup copy is kept first and put back if the update fails.");
  assert.equal(m.options[0].label, "Also update After Effects (26.3.0.5 → 26.5)");
  assert.equal(m.blockedBy, null);
  const off = dialogModel({ kind: "update", row: premiere() }, { prefixName: "Adobe 2026", snapshot: false });
  assert.match(off.lines[2], /Backups are turned off/);
  const busy = dialogModel({ kind: "update", row: premiere() },
                           { prefixName: "Adobe 2026", appsRunning: ["Premiere Pro", "After Effects"] });
  assert.equal(busy.blockedBy, "Close Premiere Pro and After Effects first. Updating needs every Adobe app in “Adobe 2026” closed.");
});

test("dialog: a Windows copy Mud Hut must register first says so", () => {
  const ae = { ...aftereffects(), update: { ...aftereffects().update, adopt: true } };
  const m = dialogModel({ kind: "update", row: ae }, { prefixName: "Adobe 2026" });
  assert.equal(m.lines[1], "This copy came from Windows, so Mud Hut first registers it with Adobe's installer, then updates it.");
  const plain = dialogModel({ kind: "update", row: aftereffects() }, { prefixName: "Adobe 2026" });
  assert.ok(!plain.lines.some((l) => l.includes("registers")), "only when Mud Hut flags it");
});

test("dialog: update all lists what it covers and what it leaves out", () => {
  const plan = updateAllPlan([premiere(), aftereffects(), photoshopUnverified()]);
  const m = dialogModel({ kind: "all", rows: plan.include, excluded: plan.exclude }, { prefixName: "Adobe 2026" });
  assert.equal(m.title, "Update 2 apps in “Adobe 2026”?");
  assert.deepEqual(m.items, ["Premiere Pro 26.3.0.12 → 26.5.2", "After Effects 26.3.0.5 → 26.5"]);
  assert.equal(m.lines[0], "They update one after another, up to 2.6 GB to download in total.");
  assert.equal(m.lines[1], "Keep every Adobe app in “Adobe 2026” closed until all of them finish.");
  assert.equal(m.lines[3], "Not included: Photoshop 27.1, which Neutron hasn't verified yet. You can update it on its own.");
  assert.equal(m.confirm, "Update all");
});

test("dialog: a build Neutron has not verified", () => {
  const row = premiere();
  const m = dialogModel({ kind: "unverified", row, target: row.newer_unverified }, { prefixName: "Adobe 2026" });
  assert.equal(m.title, "Neutron hasn't verified Premiere Pro 26.6 yet.");
  assert.equal(m.lines[1], "Premiere Pro 26.5.2 is verified, if you'd rather update to that.");
  assert.equal(m.lines[2], "Premiere Pro 26.3.0.12 → 26.6, downloaded from Adobe.");
  assert.deepEqual([m.confirm, m.cancel], ["Update anyway", "Wait"]);
  const ps = photoshopUnverified();
  const p = dialogModel({ kind: "unverified", row: ps, target: ps.update }, { prefixName: "Adobe 2026" });
  assert.equal(p.lines[1], "Once Neutron has verified it, it shows here as a normal update.");
  assert.match(p.lines[2], /up to 900 MB/);
});

test("dialog: a yearly version, with remove-previous and (Premiere Pro, Media Encoder) speech packs", () => {
  const m = dialogModel({ kind: "yearly", row: premiere() }, { prefixName: "Adobe 2026" });
  assert.equal(m.title, "Install Premiere Pro 2027?");
  assert.equal(m.lines[1], "It installs next to Premiere Pro 2026, which stays as it is unless you choose to remove it.");
  assert.ok(m.lines.includes("Neutron hasn't verified Premiere Pro 2027 yet. It might not open, or parts of it might not work."));
  assert.deepEqual(m.options.map((o) => [o.key, o.value]), [["removePrevious", false], ["speechPacks", false]]);
  assert.equal(m.options[1].label, SPEECH_LABEL);
  assert.equal(SPEECH_LABEL, "Speech-to-text languages (about 14.5 GB)", "Mud Hut measured 14.49 GB per app");
  const me = { ...upToDate(), id: "mediaencoder", name: "Media Encoder",
               new_major: { year: 2027, to: "27.0.0.12", label: "2027", major: 27, verified: true } };
  const mm = dialogModel({ kind: "yearly", row: me }, { prefixName: "Adobe 2026" });
  const sp = mm.options.find((o) => o.key === "speechPacks");
  assert.deepEqual([sp.label, sp.help, sp.value], [SPEECH_LABEL, SPEECH_HELP, false], "Media Encoder: same wording, off");
  assert.ok(!SPEECH_HELP.includes("Premiere"), "one wording for both apps");
  assert.equal(dialogAction({ kind: "yearly", row: me }, { speechPacks: true }).speechPacks, true);
  assert.equal(m.confirm, "Install 2027");
  const lr = { ...upToDate(), id: "lightroom", name: "Lightroom Classic",
               new_major: { year: 2027, to: "16.0.0.1", label: "2027", verified: true, side_by_side: false } };
  const l = dialogModel({ kind: "yearly", row: lr }, { prefixName: "Adobe 2026" });
  assert.equal(l.lines[1], "It replaces Lightroom Classic 2026.");
  assert.equal(l.options.length, 0, "no remove-previous when it replaces, no speech packs outside Premiere Pro and Media Encoder");
});

test("dialog actions: what runs on confirm", () => {
  const row = premiere();
  assert.deepEqual(dialogAction({ kind: "update", row, related: [aftereffects()] }, {}),
                   { op: "update", apps: ["premiere"], to: "26.5.2.3", allowUnverified: false });
  assert.deepEqual(dialogAction({ kind: "update", row, related: [aftereffects()] }, { "also:aftereffects": true }),
                   { op: "update", apps: ["premiere", "aftereffects"], to: null, allowUnverified: false });
  assert.deepEqual(dialogAction({ kind: "unverified", row, target: row.newer_unverified }),
                   { op: "update", apps: ["premiere"], to: "26.6.0.1", allowUnverified: true });
  assert.deepEqual(dialogAction({ kind: "all", rows: [row, aftereffects()] }),
                   { op: "update", apps: ["premiere", "aftereffects"], to: null, allowUnverified: false });
  assert.deepEqual(dialogAction({ kind: "yearly", row }, { removePrevious: true }),
                   { op: "install", app: "premiere", version: "27", label: "Premiere Pro 2027", allowUnverified: true,
                     removePrevious: true, speechPacks: false });
  // Mud Hut's own `major` wins (Photoshop 2027 is major 28, never the year).
  const ps = { ...photoshopUnverified(), new_major: { year: 2027, to: "28.0.0.50", label: "2027", major: 28, verified: true } };
  assert.equal(dialogAction({ kind: "yearly", row: ps }).version, "28");
  assert.equal(dialogAction({ kind: "yearly", row: ps }).allowUnverified, false,
               "a verified yearly version needs no --allow-unverified");
});

test("outcomes from Mud Hut's update events", () => {
  assert.equal(outcomeFromAppResult({ event: "app_result", app: "premiere", ok: true, from: "a", to: "b" }).state, "updated");
  assert.equal(outcomeFromAppResult({ event: "app_result", app: "premiere", ok: false, state: "rolled_back" }).state, "rolled_back");
  assert.equal(outcomeFromTerminal({ event: "error", code: "app-running", exit: 3, ok: false }).state, "refused");
  assert.equal(outcomeFromTerminal({ event: "error", code: "network", exit: 5, ok: false, prefix_changed: false }).prefixChanged, false);
  assert.equal(outcomeFromTerminal({ event: "result", ok: false, exit: 7 }).state, "rolled_back");
  assert.equal(outcomeFromTerminal({ event: "result", ok: false, exit: 8 }).state, "incomplete");
  assert.equal(outcomeFromTerminal({ event: "result", ok: true }).state, "updated");
  assert.equal(outcomeFromLastUpdate({ ok: true }), null);
  assert.equal(outcomeNote({ state: "failed", message: "Not enough disk space.", prefixChanged: false }, "Photoshop"),
               "Not enough disk space. Nothing was changed.");
});

test("yearly install outcome: removed_previous, as Mud Hut reports it", () => {
  const ok = outcomeFromInstall({ event: "result", ok: true, version: "27.0.0.40",
                                  removed_previous: [{ version: "26.3.0.12", removed: true }] },
                                { label: "Premiere Pro 2027", name: "Premiere Pro" });
  assert.deepEqual(ok, { state: "installed", label: "Premiere Pro 2027", message: "Removed Premiere Pro 26.3.0.12.", warn: false });
  const kept = outcomeFromInstall({ event: "result", ok: true, removed_previous: [{ version: "25.3.0.084", removed: false,
    dir: "C:/x", message: "This older version was copied without Adobe's records, so Adobe's uninstaller cannot remove it. It was left in place." }] },
    { label: "Premiere Pro 2026", name: "Premiere Pro" });
  assert.equal(kept.state, "installed", "a version left in place is not a failed install");
  assert.equal(kept.warn, true);
  assert.equal(kept.message, "Premiere Pro 25.3.0.084 was not removed. This older version was copied without Adobe's records, so Adobe's uninstaller cannot remove it. It was left in place.");
  assert.equal(outcomeFromInstall({ event: "result", ok: true }, { label: "X", name: "X" }).message, null);
  assert.equal(outcomeFromInstall({ event: "result", ok: false, message: "m" }, { label: "X", name: "X" }).state, "failed");
  const card = cardView({ row: upToDate(), pstate: pstate([upToDate()]), outcome: kept });
  assert.equal(card.noteCls, "warn");
});

test("the real Mud Hut output (3c6e464) renders: sizes are upper bounds, never changed_bytes", () => {
  const [ae, lrc, bad] = REAL.prefixes;
  const prem = ae.apps.find((a) => a.id === "premiere");
  assert.equal(prem.update.verified, false);
  const card = cardView({ row: prem, pstate: ae });
  assert.equal(card.pills[0].text, "26.5.2 available · not yet verified");
  const unv = dialogModel({ kind: "unverified", row: prem, target: prem.update }, { prefixName: "AE2026" });
  assert.ok(unv.lines.includes("Premiere Pro 26.3 → 26.5.2, downloaded from Adobe (up to 4 GB)."), unv.lines.join("|"));
  const lr = ae.apps.find((a) => a.id === "lightroom");
  const one = dialogModel({ kind: "update", row: lr }, { prefixName: "AE2026" });
  assert.equal(one.lines[0], "Lightroom Classic 15.4.1 → 15.6, downloaded from Adobe (up to 3.1 GB).");
  // changed_bytes is never shown as a size: premiere's 3.19 GB would read "3.2 GB".
  assert.ok(!JSON.stringify([unv, one]).includes("3.2 GB"));
  assert.deepEqual(updateAllPlan(ae.apps).include.map((r) => r.id), ["lightroom"]);
  assert.equal(prefixSummary({ pstate: ae }).text, "Lightroom Classic 15.6 is available");
  assert.equal(outcomeFromLastUpdate(ae.last_update), null, "a successful last_update is not an outcome to show");
  // The copy without Adobe's records: adopt, and a new yearly version with Mud Hut's major.
  const lp = lrc.apps.find((a) => a.id === "premiere");
  assert.equal(lp.update.adopt, true);
  assert.ok(dialogModel({ kind: "update", row: lp }, { prefixName: "LrcRelease" }).lines.some((l) => l.includes("registers it")));
  assert.deepEqual(dialogAction({ kind: "yearly", row: lp }, {}),
                   { op: "install", app: "premiere", version: "26", label: "Premiere Pro 2026", allowUnverified: true,
                     removePrevious: false, speechPacks: false });
  assert.equal(prefixSummary({ pstate: bad }).text, "This is not a prefix (it has no drive_c folder).");
});
