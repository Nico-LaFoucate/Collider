// Tests for lib/updates.svelte.js (the shared store). Run: node --test src/lib/
//
// The store uses Svelte runes, so the test compiles it with the project's own Svelte compiler and
// swaps `$lib/api.js` for a scripted fake that answers like Mud Hut does (CONTRACT.md shapes). The
// compiled copies go under node_modules/.cache, where `svelte/internal/client` resolves.

import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { compileModule } from "svelte/compiler";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const cache = join(here, "../../node_modules/.cache/collider-store-test");
mkdirSync(cache, { recursive: true });

// The fake api: every call is recorded; answers come from `fake.*` set by each test.
writeFileSync(join(cache, "api.js"), `
  export const fake = { calls: [], check: null, update: null, install: null };
  export async function checkAdobeUpdates(prefixes, opts) { fake.calls.push(["check", prefixes, opts]); return fake.check(prefixes, opts); }
  export async function updateAdobeApps(args) { fake.calls.push(["update", args]); return fake.update(args); }
  export async function installApp(args) { fake.calls.push(["install", args]); return fake.install(args); }
`);
writeFileSync(join(cache, "updates-logic.js"), readFileSync(join(here, "updates-logic.js"), "utf8"));
const src = readFileSync(join(here, "updates.svelte.js"), "utf8");
const js = compileModule(src, { filename: "updates.svelte.js", generate: "client" }).js.code
  .replaceAll('"$lib/api.js"', '"./api.js"').replaceAll('"$lib/updates-logic.js"', '"./updates-logic.js"');
writeFileSync(join(cache, "updates.compiled.js"), js);

const store = await import(pathToFileURL(join(cache, "updates.compiled.js")).href);
const { fake } = await import(pathToFileURL(join(cache, "api.js")).href);
const { upd, ukey } = store;

const P = "/home/u/Neutron Prefixes/Adobe 2026";
const Q = "/home/u/.premiere2025";
const row = (id, name, extra = {}) => ({
  id, name, installed: { version: "26.3.0.12", year: 2026, managed_by: "hdpim" },
  update: { kind: "in_place", to: "26.5.2.3", label: "26.5.2", verified: true, download_bytes: 1, can_update: true },
  newer_unverified: null, new_major: null, related: [], ...extra,
});
const result = (prefixes, extra = {}) => ({ event: "result", ok: true, schema: 1, checked_at: "2026-10-16T14:02:11Z",
  online: true, feed_error: null, from_cache: false, supported: true, prefixes, ...extra });
const pobj = (prefix, apps, extra = {}) => ({ prefix, ok: true, apps_running: [], update_in_progress: null,
  last_update: null, apps, ...extra });

beforeEach(() => {
  fake.calls.length = 0;
  upd.supported = true; upd.unsupported = null; upd.failed = null; upd.lastNetwork = null; upd.lastReachedAt = null;
  upd.prefixes = {}; upd.jobs = {}; upd.outcomes = {};
  store.setUpdateHooks({ prefixChanged: () => {} });
});

test("a check keys rows by the prefix string Mud Hut echoes; two prefixes keep the same app apart", async () => {
  fake.check = () => result([pobj(P, [row("premiere", "Premiere Pro")]),
                             pobj(Q, [row("premiere", "Premiere Pro", { update: null })])]);
  await store.check([P, Q]);
  assert.deepEqual(fake.calls[0], ["check", [P, Q], { maxAge: "24h", refresh: false, offline: false }]);
  assert.equal(store.rowFor(P, "premiere").update.to, "26.5.2.3");
  assert.equal(store.rowFor(Q, "premiere").update, null);
  assert.equal(upd.lastReachedAt, "2026-10-16T14:02:11Z");
});

test("Check now passes --refresh; an offline read never sets Last checked", async () => {
  fake.check = (_p, o) => result([pobj(P, [])], o.offline ? { online: false, checked_at: "2099-01-01T00:00:00Z" } : {});
  await store.check([P], { offline: true });
  assert.deepEqual(fake.calls[0][2], { maxAge: null, refresh: false, offline: true });
  assert.equal(upd.lastReachedAt, null);
  assert.equal(upd.lastNetwork, null);
  await store.checkNow([P]);
  assert.deepEqual(fake.calls[1][2], { maxAge: "24h", refresh: true, offline: false });
  assert.equal(upd.lastReachedAt, "2026-10-16T14:02:11Z");
});

test("offline is a quiet status; the rows stay", async () => {
  fake.check = () => result([pobj(P, [row("premiere", "Premiere Pro")])], { online: false, feed_error: "Couldn't reach Adobe." });
  await store.check([P]);
  assert.equal(upd.lastNetwork.online, false);
  assert.equal(upd.lastNetwork.feed_error, "Couldn't reach Adobe.");
  assert.equal(upd.lastReachedAt, null);
  assert.ok(store.rowFor(P, "premiere"));
  assert.equal(upd.failed, null);
});

test("a Mud Hut too old to check stops the checks; a failed check is kept quietly", async () => {
  fake.check = () => ({ ok: true, supported: false, reason: "too-old" });
  await store.check([P]);
  assert.equal(upd.supported, false);
  assert.equal(upd.unsupported, "too-old");
  await store.check([P]);
  assert.equal(fake.calls.length, 1, "no more checks once unsupported");
  upd.supported = true;
  fake.check = () => { throw new Error("mudhut check-updates returned non-JSON (exit 101)."); };
  await store.check([P]);
  assert.match(upd.failed, /non-JSON/);
});

test("a partial read merges one prefix; a full check replaces the map", async () => {
  fake.check = () => result([pobj(P, []), pobj(Q, [])]);
  await store.check([P, Q]);
  fake.check = () => result([pobj(P, [row("premiere", "Premiere Pro")])]);
  await store.check([P], { offline: true, partial: true });
  assert.ok(upd.prefixes[Q], "a partial read keeps the other prefix");
  await store.check([P]);
  assert.equal(upd.prefixes[Q], undefined, "a full check drops a prefix it no longer reports");
});

test("update: progress and per-app outcomes land in the store, the prefix is re-read, the page told", async () => {
  fake.check = () => result([pobj(P, [row("premiere", "Premiere Pro"), row("aftereffects", "After Effects")])]);
  await store.check([P]);
  const changed = [];
  store.setUpdateHooks({ prefixChanged: (p) => changed.push(p) });
  let seenJob = null;
  fake.update = async ({ onEvent }) => {
    onEvent({ event: "progress", app: "premiere", stage: "download", pct: 42, msg: "Downloading" });
    seenJob = JSON.parse(JSON.stringify(upd.jobs[P]));
    onEvent({ event: "app_result", app: "premiere", ok: true, from: "26.3.0.12", to: "26.5.2.3" });
    onEvent({ event: "app_result", app: "aftereffects", ok: false, state: "rolled_back", message: "Adobe's installer stopped.", log: "/l" });
    return { event: "result", ok: false, prefix: P, apps: [], exit: 1 };
  };
  const r = await store.runUpdate({ prefix: P, apps: ["premiere", "aftereffects"], noSnapshot: true,
                                    labels: { premiere: "26.5.2", aftereffects: "26.5" } });
  assert.equal(r.exit, 1);
  assert.deepEqual(fake.calls[1][1].apps, ["premiere", "aftereffects"]);
  assert.equal(fake.calls[1][1].noSnapshot, true);
  assert.equal(fake.calls[1][1].to, null);
  assert.deepEqual([seenJob.kind, seenJob.app, seenJob.stage, seenJob.pct], ["update", "premiere", "download", 42]);
  assert.equal(upd.jobs[P], undefined, "the job is gone afterwards");
  assert.equal(upd.outcomes[ukey(P, "premiere")].state, "updated");
  assert.equal(upd.outcomes[ukey(P, "premiere")].toLabel, "26.5.2");
  assert.equal(upd.outcomes[ukey(P, "aftereffects")].state, "rolled_back");
  assert.deepEqual(fake.calls[2], ["check", [P], { maxAge: null, refresh: false, offline: true }]);
  assert.ok(changed.includes(P));
});

test("update refused because an app runs: an outcome without failure, Mud Hut's sentence kept", async () => {
  fake.check = () => result([pobj(P, [row("premiere", "Premiere Pro")])]);
  fake.update = async () => ({ event: "error", ok: false, code: "app-running", exit: 3, prefix_changed: false,
                               apps_running: ["Premiere Pro"], message: "Close Premiere Pro first." });
  await store.runUpdate({ prefix: P, apps: ["premiere"], to: "26.5.2.3" });
  assert.equal(fake.calls.find((c) => c[0] === "update")[1].to, "26.5.2.3");
  const o = upd.outcomes[ukey(P, "premiere")];
  assert.equal(o.state, "refused");
  assert.equal(o.message, "Close Premiere Pro first.");
  assert.equal(o.prefixChanged, false);
});

test("update rolled back (exit 7) or not put back (exit 8) without per-app events", async () => {
  fake.check = () => result([pobj(P, [row("photoshop", "Photoshop")])]);
  for (const [exit, state] of [[7, "rolled_back"], [8, "incomplete"]]) {
    fake.update = async () => ({ event: "result", ok: false, prefix: P, apps: [], exit });
    await store.runUpdate({ prefix: P, apps: ["photoshop"] });
    assert.equal(upd.outcomes[ukey(P, "photoshop")].state, state);
  }
});

test("Mud Hut could not run at all: a failure with the reason, and the job is cleared", async () => {
  fake.check = () => result([pobj(P, [row("premiere", "Premiere Pro")])]);
  fake.update = async () => { throw new Error("failed to spawn `mudhut`"); };
  await store.runUpdate({ prefix: P, apps: ["premiere"] });
  assert.equal(upd.outcomes[ukey(P, "premiere")].state, "failed");
  assert.match(upd.outcomes[ukey(P, "premiere")].message, /spawn/);
  assert.equal(store.anyJob(), false);
});

test("one job per prefix; a second prefix can run at the same time", { timeout: 5000 }, async () => {
  fake.check = () => result([]);
  let release;
  fake.update = () => new Promise((r) => { release = r; });
  const first = store.runUpdate({ prefix: P, apps: ["premiere"] });
  assert.equal(await store.runUpdate({ prefix: P, apps: ["aftereffects"] }), null, "the prefix is held");
  assert.equal(fake.calls.filter((c) => c[0] === "update").length, 1);
  const releaseP = release;
  const second = store.runUpdate({ prefix: Q, apps: ["premiere"] });
  assert.equal(fake.calls.filter((c) => c[0] === "update").length, 2, "another prefix is not held");
  assert.deepEqual(Object.keys(upd.jobs).sort(), [Q, P].sort());
  assert.equal(store.anyJob(), true);
  releaseP({ event: "result", ok: true, prefix: P, apps: [] });
  release({ event: "result", ok: true, prefix: Q, apps: [] });
  await first; await second;
  assert.equal(store.anyJob(), false);
  assert.equal(upd.outcomes[ukey(P, "premiere")].state, "updated");
  assert.equal(upd.outcomes[ukey(Q, "premiere")].state, "updated");
});

test("yearly install: an install job, version and choices passed, outcome installed", async () => {
  fake.check = () => result([]);
  fake.install = async ({ onEvent }) => { onEvent({ event: "progress", stage: "download", pct: 10, msg: "x" });
                                          return { event: "result", ok: true }; };
  await store.runYearlyInstall({ prefix: P, app: "premiere", version: "27", label: "Premiere Pro 2027",
                                 removePrevious: true, speechPacks: false });
  const args = fake.calls.find((c) => c[0] === "install")[1];
  assert.deepEqual([args.app, args.method, args.prefix, args.version, args.removePrevious, args.speechPacks],
                   ["premiere", "download", P, "27", true, false]);
  assert.deepEqual(upd.outcomes[ukey(P, "premiere")], { state: "installed", label: "Premiere Pro 2027", message: null });
});

test("an update left running by an earlier Collider: followed until it finishes, then the page is told", async () => {
  const changed = [];
  store.setUpdateHooks({ prefixChanged: (p) => changed.push(p) });
  const running = { pid: 4242, apps: ["premiere"], started_at: "2026-10-16T14:03:00Z", stage: "apply", log: "/p/.mudhut/update.log" };
  fake.check = () => result([pobj(P, [row("premiere", "Premiere Pro")], { update_in_progress: running })]);
  await store.check([P], { offline: true });
  assert.deepEqual(store.followedPrefixes(), [P]);
  fake.check = () => result([pobj(P, [row("premiere", "Premiere Pro", { update: null })],
    { last_update: { ok: false, app: "premiere", state: "rolled_back", message: "m", at: "x" } })]);
  await store.check([P], { offline: true, partial: true });
  assert.deepEqual(store.followedPrefixes(), []);
  assert.deepEqual(changed, [P]);
  store.stopUpdateChecks();
});
