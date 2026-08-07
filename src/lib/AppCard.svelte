<script>
  // One app widget — a per-app instance of the original Premiere card. Same
  // markup/interactions; self-contained launch state; tinted to the app's brand
  // color via --acrgb. Wired to the generic per-app command surface.
  import { launchApp, isAppAlive, cleanExitApp, forceQuitApp } from "$lib/api.js";
  import { onDestroy } from "svelte";

  // app = { id, name, accent, export, installed }; info = shared prefix info.
  let { app, prefix, info = null } = $props();

  // Adobe-style 2-letter badge; fall back to the first two name letters.
  const BADGES = { premiere: "Pr", photoshop: "Ps", lightroom: "Lr", animate: "An",
                   mediaencoder: "Me", aftereffects: "Ae", illustrator: "Ai" };
  const badge = $derived(BADGES[app.id] ?? app.name.slice(0, 2));

  // "#rrggbb" -> "r, g, b" for the rgba() tints in the scoped CSS.
  function accentRgb(hex) {
    const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(String(hex).trim());
    if (!m) return "154, 92, 245";
    return [1, 2, 3].map((i) => parseInt(m[i], 16)).join(", ");
  }
  const acrgb = $derived(accentRgb(app.accent));

  // --- per-app launch state ---
  let step = $state({ step: "Idle" });
  let busy = $state(false);
  let error = $state(null);
  let menuOpen = $state(false);
  let pollTimer = null;

  const running = $derived(step.step === "Running");
  const failed = $derived(step.step === "Failed");

  const STEP_PROGRESS = {
    Idle: 0, Resolving: 0.2, ApplyingDisplayFix: 0.4,
    LaunchingPremiere: 0.6, Running: 1, Stopped: 0, Failed: 1,
  };
  const progress = $derived(STEP_PROGRESS[step.step] ?? 0);
  const buttonLabel = $derived(busy ? "Launching…" : running ? "Running" : failed ? "Retry" : "Launch");
  const buttonClass = $derived(running ? "running" : failed ? "failed" : "");

  async function onLaunch() {
    busy = true; error = null;
    try {
      step = await launchApp(app.id, prefix, null);
      if (step.step === "Failed") error = `${step.detail.at}: ${step.detail.reason}`;
      else if (step.step === "Running") startPolling();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  // While Running, poll liveness; on exit, auto clean-exit (stop daemon, reset).
  function startPolling() {
    stopPolling();
    pollTimer = setInterval(async () => {
      try {
        if (!(await isAppAlive(app.id))) { stopPolling(); step = await cleanExitApp(app.id); }
      } catch (e) { stopPolling(); error = String(e); }
    }, 1500);
  }
  function stopPolling() { if (pollTimer) { clearInterval(pollTimer); pollTimer = null; } }

  async function onForceQuit() {
    menuOpen = false; busy = true;
    try { stopPolling(); step = await forceQuitApp(app.id); }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }

  // Clicking while Running toggles the force-quit menu (Running is a status).
  function onButtonClick() { if (running) menuOpen = !menuOpen; else onLaunch(); }

  onDestroy(stopPolling);
</script>

<article class="card" style="--acrgb: {acrgb}; --ac: {app.accent};">
  <div class="card-head">
    {#if app.icon}
      <img class="badge-img" src={app.icon} alt={app.name} />
    {:else}
      <div class="badge">{badge}</div>
    {/if}
    <div class="head-text">
      <div class="app-name">{app.name}</div>
      <div class="app-ver">{app.version ? `${app.version} · GPU` : "GPU"}</div>
    </div>
  </div>


  <div class="pills">
    {#if info}
      <span class="pill {info.valid ? 'ok' : 'bad'}">{info.valid ? "Prefix healthy" : "Invalid prefix"}</span>
      <span class="pill {info.display_fix_applied ? 'ok' : 'warn'}">{info.display_fix_applied ? "Display fix" : "Fix pending"}</span>
    {:else}
      <span class="pill">Not checked</span>
    {/if}
    {#if running}
      {#if step.detail?.display}
        <span class="pill ok">{step.detail.display === "wayland" ? "Wayland" : "X11"}</span>
      {/if}
      {#if step.detail && step.detail.neutron_wine === false}
        <span class="pill bad" title="Neutron wine not found — playback/HiDPI fixes are inactive">⚠ Neutron wine missing</span>
      {/if}
    {/if}
  </div>

  {#if error}<div class="card-err">{error}</div>{/if}

  <div class="actions">
    <div class="launch-wrap">
      <button class="primary {buttonClass}" onclick={onButtonClick} disabled={busy}>
        {#if running}<span class="run-dot"></span>{/if}
        <span class="btn-label">{buttonLabel}</span>
        {#if busy}<span class="progress" style="width: {progress * 100}%"></span>{/if}
      </button>
      {#if running && menuOpen}
        <div class="menu">
          <button class="menu-item danger" onclick={onForceQuit}>Force quit {app.name}</button>
        </div>
      {/if}
    </div>
  </div>
</article>

<style>
  /* Copied verbatim from the original Premiere card, with the hardcoded purple
     (rgba(154,92,245,…) / #9a5cf5 / #c9a4ff) replaced by the per-app --acrgb/--ac. */
  /* touch to force HMR CSS re-injection on cold tauri-dev start (dev-only bug) */
  .card { border-radius: 14px; padding: 14px; background: linear-gradient(155deg, rgba(var(--acrgb),0.16), #14141aF5); border: 1px solid color-mix(in srgb, rgb(var(--acrgb)) 30%, #000); }
  .card-head { display: flex; align-items: center; gap: 10px; margin-bottom: 12px; }
  .head-text { flex: 1; min-width: 0; }
  /* Real extracted app logo (lowest-ID exe icon via icoutils); monogram fallback below. */
  .badge-img { width: 38px; height: 38px; border-radius: 9px; object-fit: contain; flex-shrink: 0; }
  /* Authentic Adobe CC app-icon look: dark brand-tinted square + glowing brand monogram. */
  .badge { width: 38px; height: 38px; border-radius: 9px;
    background: linear-gradient(145deg, rgba(var(--acrgb),0.30), #0d0d12);
    border: 1px solid rgba(var(--acrgb),0.55);
    box-shadow: inset 0 0 12px -4px rgba(var(--acrgb),0.55);
    display: flex; align-items: center; justify-content: center;
    color: var(--ac); font-weight: 700; font-size: 15px; letter-spacing: 0.3px; }
  .app-name { font-size: 14px; font-weight: 600; }
  .app-ver { font-size: 10.5px; color: rgba(255,255,255,0.4); }

  .pills { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 10px; }
  .pill { font-size: 10px; padding: 3px 8px; border-radius: 20px; background: rgba(255,255,255,0.06); color: rgba(255,255,255,0.45); border: 1px solid rgba(255,255,255,0.08); }
  .pill.ok { background: rgba(60,200,140,0.12); color: #6fe3b0; border-color: rgba(60,200,140,0.25); }
  .pill.warn { background: rgba(239,159,39,0.12); color: #f2c374; border-color: rgba(239,159,39,0.25); }
  .pill.bad { background: rgba(226,75,74,0.12); color: #ff9b9b; border-color: rgba(226,75,74,0.25); }
  .pill.pulse { animation: pulse 1.6s infinite; }

  .card-err { font-size: 10.5px; color: #ff9b9b; margin-bottom: 10px; word-break: break-word; }

  .actions { display: flex; gap: 8px; }
  .launch-wrap { position: relative; flex: 1; }
  .primary { position: relative; overflow: hidden; width: 100%; padding: 9px 0; border-radius: 9px; border: none; cursor: pointer; font-size: 12.5px; font-weight: 600; color: #fff; background: rgba(var(--acrgb),0.8); display: flex; align-items: center; justify-content: center; gap: 8px; }
  .btn-label { position: relative; z-index: 1; }
  .progress { position: absolute; left: 0; bottom: 0; height: 3px; background: rgba(255,255,255,0.55); border-radius: 0 2px 2px 0; transition: width 0.4s ease; z-index: 0; }
  .primary:disabled { opacity: 0.7; cursor: default; }
  .primary.running { background: rgba(60,200,140,0.22); color: #6fe3b0; cursor: pointer; }
  .primary.failed { background: rgba(239,159,39,0.85); }
  .run-dot { width: 7px; height: 7px; border-radius: 50%; background: #3ce08c; animation: pulse 1.6s infinite; z-index: 1; }

  .menu { position: absolute; left: 0; right: 0; bottom: calc(100% + 6px); background: #1a1a20; border: 1px solid rgba(255,255,255,0.1); border-radius: 9px; padding: 4px; z-index: 10; box-shadow: 0 8px 24px -8px rgba(0,0,0,0.7); }
  .menu-item { width: 100%; padding: 8px 10px; border-radius: 6px; border: none; background: transparent; color: #e8e8ec; font-size: 12px; text-align: left; cursor: pointer; }
  .menu-item:hover { background: rgba(255,255,255,0.06); }
  .menu-item.danger { color: #ff9b9b; }
  .menu-item.danger:hover { background: rgba(226,75,74,0.14); }

  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.35; } }
</style>
