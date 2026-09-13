# Neutron CLI Contract (v0)

*The engine's front door. Collider depends on this. This is the contract — implement
in the Neutron repo (LGPL v2.1). For MVP, commands may wrap the existing proven
launcher logic; the contract is the stable surface, internals can evolve.*

> **SHIPPED (v0) — reconciled with COLLIDER_INTEGRATION_HANDOFF.md.** The CLI now
> exists and is tested. Real-world deltas from this idealized contract:
> - `--json` is a **top-level flag, before the subcommand**: `neutron --json prefix info <path>`.
> - Non-zero exits also emit a **JSON error object** on stdout (`error_code` + `reason`) —
>   parse it; surface `reason` to the user.
> - Exit `4` means **dependency missing OR feature unavailable in v0**. `launch --software`
>   returns exit 4 — software mode is an honest v0 gap; Collider must not offer it.
> - `prefix info` and `doctor` carry `"_schema": "provisional-v0"`; their `neutron_stack`
>   and `checks` are **not frozen** until the minimal-stack cleanup (hardening §5.2).
> - `apply-display-fix`, `launch` (GPU), and `hwmux start/stop` are **finalized** — safe to
>   integrate against now.

> ## ⛔ CORRECTIONS — 2026-08-07 (this document has drifted; trust the CLI, not this file)
> - **`hwmux start/stop` NO LONGER EXISTS.** It was removed from the engine in neutron commit
>   `678cb60` ("retire collider-hwmux") — the daemon was a workaround for broken muxing, and the
>   real fix (the ucrtbase `_wstat64` shim) made it unnecessary. Calling it returns an argparse
>   error, not a JSON object. Collider called it until 2026-08-06 and reported every export-app
>   launch as Failed as a result. Do not reintroduce it. The line above marking it "finalized —
>   safe to integrate against" is **wrong** and is left in place only so this correction has
>   something to point at.
> - **New: `teardown --app <id> --prefix <p>`** — closes ONE app, leaving the prefix's other apps
>   running, and sweeps Adobe's shared daemons when the last app exits. Use this on app exit; a
>   bare `kill(pid)` leaks Adobe helpers that wedge the next launch.
> - Current subcommand set: `prefix {info,apply-display-fix,provision}`, `launch`, `apps`,
>   `teardown`, `doctor`, `runtime`.
> - **New: `--progress`** (top-level, with `--json`) — streams newline-delimited
>   `{event:progress|note|error|result}` before the terminal object, the same NDJSON contract Mud
>   Hut uses. Currently implemented for `prefix provision`. ⚠️ **Opt-in on purpose:** Mud Hut runs
>   `neutron prefix provision` with INHERITED stdout, so streaming by default would inject these
>   lines into Mud Hut's own stream and Collider would read neutron's result as Mud Hut's. Without
>   `--progress` the output is byte-compatible with before: exactly one object, no `event` key.
>
> ⚠️ **This file is hand-maintained and nothing enforces it against the CLI.** That is exactly how
> the hwmux drift survived. Verify against `neutron --help` before building on any claim here.

---

## Design rules

1. **Structured output:** every command supports a top-level `--json`. JSON to stdout
   (one object per call, success OR failure), human `[neutron] …` logs to stderr.
   Collider parses stdout only.
2. **Exit codes are meaningful:** `0` success, `1` generic failure, `2` prefix not
   found / invalid, `3` Premiere running (can't edit prefs), `4` dependency missing
   **or feature unavailable in v0**.
3. **Idempotent where possible:** `apply-display-fix` on an already-fixed prefix is a
   no-op success, not an error.
4. **No interactivity:** never prompt. Collider drives this non-interactively.

---

## Commands

### `neutron prefix info <path> [--json]`
Inspect a prefix. Resolves and reports paths Collider needs.
```json
{
  "valid": true,
  "wineprefix": "/home/nico/.premiere2025",
  "drive_c": "/home/nico/.premiere2025/drive_c",
  "user": "nico",
  "documents_symlink": "/home/nico/.premiere2025/drive_c/users/nico/Documents",
  "documents_real": "/home/nico/Documents",
  "display_fix_applied": true,
  "neutron_stack": { "wine": "wine-tkg-...", "dxvk": "...", "vkd3d": "..." }
}
```
`documents_real` is the symlink-resolved target — the value the hwmux watch path must use.

### `neutron prefix apply-display-fix <path> [--json]`
Write `DS.DisableDirectXDisplay=true` into `Debug Database.txt` (CRLF, tab-separated,
key/current/default). Idempotent. Exit `3` if Premiere is running.
```json
{ "changed": true, "already_applied": false, "premiere_running": false }
```

### `neutron launch <app> --prefix <path> [--json] [--gpu|--software]`
Launch an app through the Neutron stack with correct env + DLL overrides. This folds in
the current `~/.local/bin/premiere` logic. `<app>` = `premiere` for MVP. Returns the
spawned PID so Collider can supervise / tie daemon lifecycle to it.
```json
{ "launched": true, "app": "premiere", "pid": 48213, "gpu_mode": "gpu" }
```

### `neutron fonts check --prefix <path> [--json]` (added 2026-09-13)
Font verdict for a prefix. Read-only — never starts wine — so Collider runs it for every
registered prefix when the Prefixes tab opens. Exit 0 for `good`/`warn`, exit 1 for `bad`
(a verdict, not a failure: use `run_json_status`, like doctor).
```json
{ "ok": true, "prefix": "...", "status": "good|warn|bad",
  "summary": "one plain-English sentence Collider shows",
  "checks": [ { "name": "roman-default|cooltype-cache|ms-core-fonts|replacements|registered",
                "status": "good|warn|bad", "detail": "..." } ],
  "repair": [ "what `fonts repair` would do, in order" ],
  "apps_running": [], "note": "only when a wine session is live in the prefix" }
```

### `neutron fonts repair --prefix <path> [--json]` (added 2026-09-13)
Restores genuine Microsoft core fonts (from this machine, else `winetricks corefonts`),
restores the `HKCU\\Software\\Wine\\Fonts\\Replacements` map (Segoe UI and 15 others -> Adobe
Clean, from the runtime's `fonts.json`), registers everything staged in `windows/Fonts`,
invalidates CoolType's cache, waits for the registry to reach disk, re-verifies. Exit 3 + `reason` while an Adobe app is running in the
prefix; exit 1 + `reason` when the resolved runtime would re-stamp the prefix, or when the
prefix is still `bad` afterwards.
```json
{ "ok": true, "prefix": "...", "before": { "status": "bad", "summary": "..." },
  "steps": [ { "step": "roman_default|ms_core_fonts|font_replacements|font_register|cooltype_cache_invalidate", "ok": true } ],
  "after": { "status": "good", "summary": "...", "checks": [ ... ] } }
```

### `neutron doctor [--prefix <path>] [--json]`
Health check: stack present? DLL overrides set? deps (ffmpeg, inotify-tools) installed?
Powers Collider's "is the prefix healthy" status surface.
```json
{
  "healthy": true,
  "checks": [
    { "name": "wine-tkg", "ok": true },
    { "name": "ffmpeg", "ok": true },
    { "name": "inotify-tools", "ok": true },
    { "name": "display_fix", "ok": true }
  ]
}
```

---

## The hwmux daemon — ownership (RESOLVED → Option A)

**Decision:** Neutron owns the daemon (script, muxing logic, AND the config/target
knowledge). Collider supervises the process lifecycle only.

Flow today (daemon does not yet self-detect its target):
  1. Collider calls `neutron prefix info --json`, reads `documents_real`
     (the symlink-resolved watch path — Neutron computed it, Collider didn't).
  2. Collider carries that value into `neutron hwmux start --watch <documents_real>`.
  3. Collider supervises the process (restart on crash, stop on app exit) but owns
     none of the muxing/symlink/same-filesystem logic — that's all engine-side.

Collider never *computes* the target — it only *carries* the value Neutron gave it
from one call to the next. No symlink logic leaks into the cockpit.

Future upgrade path: when the daemon learns to auto-detect its target, `neutron hwmux
start` drops the `--watch` flag and resolves internally. Collider's supervision code
barely changes — it just stops passing the flag.

This is the original Option A below, now confirmed.

---

### Original options (kept for context)

The hwmux daemon (`collider-hwmux.sh`) is engine knowledge (it encodes the symlink and
same-filesystem lessons) but Collider supervises its lifecycle. Two options:

- **A:** Neutron owns the daemon script; Collider starts/stops it via
  `neutron hwmux start --watch <real-dir>` / `neutron hwmux stop`. Keeps all
  engine knowledge in Neutron. **(Recommended — consistent with the split.)**
- **B:** Collider bundles and runs the daemon directly. Faster for MVP but splits
  systems knowledge across both repos.

Recommendation: **A.** Neutron owns the script and exposes start/stop; Collider's Rust
core supervises the process (restart on crash, stop on app exit) but doesn't own the
muxing logic. Resolves the §1.2 lessons in one place.
