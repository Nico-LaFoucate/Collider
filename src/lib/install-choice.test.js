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
  // Mud Hut's error event (2026-10-24 wording; the match is on `code`).
  const refusal = { event: "error", code: "unverified", exit: 1, prefix_changed: false, to: "27.0.0.40",
                    message: "Premiere Pro 2027 is not yet tested by Neutron. There may be issues we are currently unaware of. To install it anyway, confirm (--allow-unverified)." };
  assert.deepEqual(untestedOnlyChoice(refusal), { tested: null, newest: { label: null } });
  assert.equal(untestedOnlyChoice({ event: "error", code: "network", message: "fetching product feed: connection refused" }), null);
  assert.equal(untestedOnlyChoice(null), null, "an older Mud Hut, or a failure with no error event");
});
