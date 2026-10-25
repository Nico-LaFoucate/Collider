import { test } from "node:test";
import assert from "node:assert/strict";
import { versionChoice, untestedOnlyChoice } from "./install-choice.js";

// Mud Hut 0.3.6's `install premiere --method download --dry-run --json` result (live feed, 2026-10-24).
const PREMIERE = { event: "result", ok: true, dry_run: true, app: "premiere", product_version: "26.3.0.93",
                   label: "26.3", verified: true, newest: "26.5.2.5", newest_label: "26.5.2" };

test("a newer untested build is offered beside the tested one", () => {
  assert.deepEqual(versionChoice(PREMIERE), { tested: { label: "26.3" }, newest: { label: "26.5.2" } });
});

test("nothing to choose when the tested build is Adobe's newest", () => {
  assert.equal(versionChoice({ ...PREMIERE, newest: null, newest_label: null }), null);
});

test("nothing to choose for an app Neutron has never tested (experimental)", () => {
  assert.equal(versionChoice({ ...PREMIERE, verified: false, newest: null }), null);
});

test("a plan from an older Mud Hut (no verified field) installs as before", () => {
  assert.equal(versionChoice({ ok: true, dry_run: true, product_version: "26.5.2.5" }), null);
});

test("Mud Hut's refusal when nothing tested is offered leaves only the untested build", () => {
  assert.deepEqual(untestedOnlyChoice("Neutron hasn't verified Premiere Pro 2027 yet. It may not open, or parts of it may not work. To install it anyway, confirm (--allow-unverified)."),
                   { tested: null, newest: { label: null } });
  assert.equal(untestedOnlyChoice("fetching product feed: connection refused"), null);
});
