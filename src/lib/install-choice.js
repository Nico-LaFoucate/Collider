// The version choice of a fresh download install. His call (2026-10-24): "users should still be able
// to download an untested version if they choose to do so as long as they're aware that it's
// untested". Mud Hut installs the newest build Neutron has tested; its plan (`install --dry-run`)
// also names Adobe's newest build when that is another one, which the user may choose instead.

export const UNTESTED_HELP = "Not yet tested by Neutron. There may be issues we are currently unaware of.";

/** Mud Hut's plan -> null when there is nothing to choose (the planned build is Adobe's newest, or
 *  Neutron has never tested the app, as with the experimental ones), else
 *  { tested: { label }, newest: { label } }. */
export function versionChoice(plan) {
  if (!plan || plan.ok === false || plan.verified !== true || !plan.newest) return null;
  return {
    tested: { label: plan.label ?? plan.product_version },
    newest: { label: plan.newest_label ?? plan.newest },
  };
}

/** A plan that failed because Adobe offers no build Neutron has tested (Mud Hut's error event with
 *  `code: "unverified"`; matched by code, so rewording the sentence cannot break it)
 *  -> { tested: null, newest: { label: null } }: the only choice is the untested build.
 *  Any other failure -> null (shown as an error). */
export function untestedOnlyChoice(error) {
  return error?.code === "unverified" ? { tested: null, newest: { label: null } } : null;
}
