<script>
  import { prefixInfo, doctor, launchPremiere, isPremiereAlive, cleanExit, forceQuit,
           getSettings, setSettings, detectScale, compositorInfo } from "$lib/api.js";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onDestroy, onMount } from "svelte";

  // --- MVP state ---
  // For v0 the prefix is a single known path; later this comes from a prefix registry.
  let prefix = $state("~/.premiere2025");
  let info = $state(null);        // prefix info result
  let health = $state(null);      // doctor result
  let step = $state({ step: "Idle" });
  let busy = $state(false);
  let error = $state(null);
  let menuOpen = $state(false);   // force-quit dropdown visibility
  let exportDir = $state(null);   // muxer watch target; null = use resolved default

  let pollTimer = null;           // liveness poll handle

  // --- view switching + Preferences (settings) ---
  let view = $state("apps");                                  // "apps" | "preferences"
  let settings = $state({ scale_mode: "auto", scale_value: 1.5, home_window_fix: true, home_window_y: 82 });
  let detectedScale = $state(null);                           // live primary-monitor scale, for reference
  let compositor = $state(null);                              // { wayland, desktop, home_rule_supported }

  onMount(async () => {
    try { settings = await getSettings(); } catch (_) {}
    if (settings.scale_value == null) settings.scale_value = detectedScale ?? 1.5;
  });

  async function openPreferences() {
    view = "preferences";
    try { detectedScale = await detectScale(); } catch (_) { detectedScale = null; }
    try { compositor = await compositorInfo(); } catch (_) { compositor = null; }
    if (settings.scale_mode === "manual" && settings.scale_value == null)
      settings.scale_value = detectedScale ?? 1.5;
  }

  // Persist on any change. "auto unless overridden": engine uses scale_value only
  // when scale_mode === "manual". set_settings also (re)writes the Wayland
  // home-window compositor rule, so toggling it takes effect immediately.
  async function saveSettings() {
    try {
      await setSettings({
        scale_mode: settings.scale_mode,
        scale_value: settings.scale_value,
        home_window_fix: settings.home_window_fix,
        home_window_y: settings.home_window_y,
      });
    } catch (e) { error = String(e); }
  }

  async function refresh() {
    error = null;
    try {
      info = await prefixInfo(prefix);
      health = await doctor(prefix);
      // Default the export folder to the prefix's resolved Documents path, but
      // only if the user hasn't already chosen an override.
      if (!exportDir && info?.documents_real) {
        exportDir = info.documents_real;
      }
    } catch (e) {
      error = String(e);
      info = null;
      health = null;
    }
  }

  // Open the native folder picker to set where Premiere exports land — this is
  // what hwmux watches. Defaults to the current export dir.
  async function pickExportDir() {
    try {
      const chosen = await open({
        directory: true,
        multiple: false,
        defaultPath: exportDir ?? undefined,
        title: "Choose Premiere's export folder",
      });
      if (chosen) exportDir = chosen;
    } catch (e) {
      error = String(e);
    }
  }

  async function onLaunch() {
    busy = true;
    error = null;
    try {
      step = await launchPremiere(prefix, null, exportDir);
      if (step.step === "Failed") {
        error = `${step.detail.at}: ${step.detail.reason}`;
      } else if (step.step === "Running") {
        startPolling();
      }
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  // While Running, poll Premiere's liveness. When it exits (closed normally by
  // the user), auto-run the clean exit: stop muxer, confirm Premiere gone, reset
  // the button to Launch. This is the "close it the normal way" behavior.
  function startPolling() {
    stopPolling();
    pollTimer = setInterval(async () => {
      try {
        const alive = await isPremiereAlive();
        if (!alive) {
          stopPolling();
          step = await cleanExit();
        }
      } catch (e) {
        // If the poll itself errors, stop polling rather than spin forever.
        stopPolling();
        error = String(e);
      }
    }, 1500);
  }

  function stopPolling() {
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  }

  // The manual escape hatch from the Running button's dropdown — for a hung app.
  async function onForceQuit() {
    menuOpen = false;
    busy = true;
    try {
      stopPolling();
      step = await forceQuit();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  // Clicking the button while Running toggles the force-quit menu (it's not a
  // stop button anymore — Running is a status, with force-quit tucked behind it).
  function onButtonClick() {
    if (running) {
      menuOpen = !menuOpen;
    } else {
      onLaunch();
    }
  }

  onDestroy(stopPolling);

  const running = $derived(step.step === "Running");
  const failed = $derived(step.step === "Failed");

  // Map each launch step to a progress fraction (0..1) for the thin bar.
  const STEP_PROGRESS = {
    Idle: 0,
    Resolving: 0.2,
    ApplyingDisplayFix: 0.4,
    LaunchingPremiere: 0.6,
    StartingDaemon: 0.8,
    Running: 1,
    Stopped: 0,
    Failed: 1,
  };
  const progress = $derived(STEP_PROGRESS[step.step] ?? 0);

  // The button's own label carries the state. When running, it's a STATUS
  // ("Running"), not a stop action — clicking it opens the force-quit menu.
  const buttonLabel = $derived(
    busy ? "Launching…" :
    running ? "Running" :
    failed ? "Retry" :
    "Launch"
  );
  // Visual variant for the button color.
  const buttonClass = $derived(
    running ? "running" :
    failed ? "failed" :
    ""
  );

</script>

<div class="app">
  <aside>
    <div class="brand">
      <div class="logo">⚛</div>
      <div>
        <div class="brand-name">Collider</div>
        <div class="brand-by">by Nico LaFoucate</div>
      </div>
    </div>
    <nav>
      <div class="nav-section">Library</div>
      <div class="nav-item" class:active={view === "apps"} role="button" tabindex="0"
           onclick={() => (view = "apps")}>All apps</div>
      <div class="nav-item disabled" title="Coming soon">Installed</div>
      <div class="nav-section">Tools</div>
      <div class="nav-item disabled" title="Coming soon">Prefixes</div>
      <div class="nav-item" class:active={view === "preferences"} role="button" tabindex="0"
           onclick={openPreferences}>Preferences</div>
    </nav>

    <div class="sys">
      <div class="nav-section">System</div>
      {#if health}
        <div class="sys-list">
          {#each health.checks as c}
            <div class="sys-row" title={c.detail ?? ""}>
              <span class="dot {c.ok ? 'ok' : 'bad'}"></span>
              <span class="sys-name">{c.name}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="sys-empty">Not checked — hit Refresh</div>
      {/if}
    </div>

    <div class="studio">Smack Studio</div>
  </aside>

  <main>
  {#if view === "apps"}
    <header>
      <div>
        <div class="title">All apps</div>
        <div class="subtitle">
          Shared Neutron prefix · {prefix}
          {#if info}· {info.valid ? "healthy" : "invalid"}{/if}
        </div>
      </div>
      <button class="ghost" onclick={refresh} disabled={busy}>↻ Refresh</button>
    </header>

    <div class="scroll">
    {#if error}
      <div class="banner err">{error}</div>
    {/if}

    <section class="grid">
      <!-- The one MVP card: Premiere, wired to the real launch loop. -->
      <article class="card premiere">
        <div class="card-head">
          <div class="badge">Pr</div>
          <div class="head-text">
            <div class="app-name">Premiere Pro</div>
            <div class="app-ver">v25.3.0 · GPU</div>
          </div>
          <button
            class="export-btn"
            onclick={pickExportDir}
            disabled={running || busy}
            title={exportDir ? `Export folder: ${exportDir}` : "Set Premiere's export folder"}
            aria-label="Set export folder"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M4 4h5l2 2h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/>
            </svg>
          </button>
        </div>

        {#if exportDir}
          <div class="export-path" title={exportDir}>
            <span class="export-label">Exports →</span>
            <span class="export-val">{exportDir}</span>
          </div>
        {/if}

        <div class="pills">
          {#if info}
            <span class="pill {info.valid ? 'ok' : 'bad'}">
              {info.valid ? "Prefix healthy" : "Invalid prefix"}
            </span>
            <span class="pill {info.display_fix_applied ? 'ok' : 'warn'}">
              {info.display_fix_applied ? "Display fix" : "Fix pending"}
            </span>
          {:else}
            <span class="pill">Not checked</span>
          {/if}
          {#if running}
            <span class="pill ok pulse">hwmux watching</span>
            {#if step.detail?.display}
              <span class="pill ok">{step.detail.display === "wayland" ? "Wayland" : "X11"}</span>
            {/if}
            {#if step.detail && step.detail.neutron_wine === false}
              <span class="pill bad" title="Neutron wine not found — playback/HiDPI fixes are inactive">⚠ Neutron wine missing</span>
            {/if}
          {/if}
        </div>

        <div class="actions">
          <div class="launch-wrap">
            <button
              class="primary {buttonClass}"
              onclick={onButtonClick}
              disabled={busy}
            >
              {#if running}<span class="run-dot"></span>{/if}
              <span class="btn-label">{buttonLabel}</span>
              {#if busy}
                <span class="progress" style="width: {progress * 100}%"></span>
              {/if}
            </button>

            {#if running && menuOpen}
              <div class="menu">
                <button class="menu-item danger" onclick={onForceQuit}>
                  Force quit Premiere
                </button>
              </div>
            {/if}
          </div>
        </div>
      </article>
    </section>
    </div>
  {:else if view === "preferences"}
    <header>
      <div>
        <div class="title">Preferences</div>
        <div class="subtitle">Collider settings · stored in ~/.config/collider</div>
      </div>
    </header>
    <div class="scroll">
      <section class="prefs">
        <div class="pref-group">
          <div class="pref-label">Display scale</div>
          <div class="pref-desc">
            How crisp the app UI renders on HiDPI displays. <b>Auto</b> uses your primary
            monitor's scale; <b>Manual</b> overrides it. Applied at launch
            (LogPixels = 96 × scale), engine-side.
          </div>
          <div class="pref-row">
            <label class="radio">
              <input type="radio" name="scalemode" value="auto"
                     checked={settings.scale_mode === "auto"}
                     onchange={() => { settings.scale_mode = "auto"; saveSettings(); }} />
              Auto{#if detectedScale} <span class="muted">(detected: {detectedScale.toFixed(2)}×)</span>{/if}
            </label>
            <label class="radio">
              <input type="radio" name="scalemode" value="manual"
                     checked={settings.scale_mode === "manual"}
                     onchange={() => { settings.scale_mode = "manual";
                       if (settings.scale_value == null) settings.scale_value = detectedScale ?? 1.5;
                       saveSettings(); }} />
              Manual
            </label>
            {#if settings.scale_mode === "manual"}
              <input class="scale-input" type="number" min="0.5" max="3" step="0.05"
                     bind:value={settings.scale_value} onchange={saveSettings} />
            {/if}
          </div>
        </div>

        {#if compositor?.wayland}
        <div class="pref-group">
          <div class="pref-label">Premiere home-screen position (Wayland)</div>
          <div class="pref-desc">
            On Wayland, Premiere's home/Welcome screen loads too high and covers the menu
            bar (Wayland doesn't let apps position their own windows). This pins it back into
            place via a compositor window rule.
            {#if !compositor.home_rule_supported}
              <b>Not supported on your compositor ({compositor.desktop}) yet</b> — use the X11
              display mode if you need it.
            {/if}
          </div>
          <div class="pref-row">
            <label class="radio">
              <input type="checkbox" disabled={!compositor.home_rule_supported}
                     bind:checked={settings.home_window_fix} onchange={saveSettings} />
              Fix home-screen position
            </label>
            {#if settings.home_window_fix && compositor.home_rule_supported}
              <label class="muted" style="display:flex;align-items:center;gap:6px;">
                Y offset
                <input class="scale-input" type="number" min="0" max="600" step="1"
                       bind:value={settings.home_window_y} onchange={saveSettings} />
                px
              </label>
            {/if}
          </div>
          {#if settings.home_window_fix && compositor.home_rule_supported}
            <div class="pref-desc muted">
              Tweak the Y offset if it still doesn't sit right (it depends on your resolution
              and scale). Lower = higher on screen.
            </div>
          {/if}
        </div>
        {/if}
      </section>
    </div>
  {/if}

    <footer>
      <span class="dot ok"></span>
      Ready · Neutron CLI connected
    </footer>
  </main>
</div>

<style>
  .app { display: flex; height: 100vh; overflow: hidden; font-family: system-ui, sans-serif; color: #e8e8ec; }

  aside { width: 200px; flex-shrink: 0; padding: 20px 14px; border-right: 1px solid rgba(255,255,255,0.06); display: flex; flex-direction: column; height: 100vh; overflow: hidden; }
  .brand { display: flex; align-items: center; gap: 9px; padding: 0 6px 22px; }
  .logo { font-size: 22px; }
  .brand-name { font-size: 17px; font-weight: 700; }
  .brand-by { font-size: 9.5px; color: rgba(255,255,255,0.35); }
  .nav-section { flex-shrink: 0; font-size: 9.5px; text-transform: uppercase; letter-spacing: 1.2px; color: rgba(255,255,255,0.3); padding: 14px 8px 6px; }
  .nav-item { padding: 7px 10px; border-radius: 8px; font-size: 13px; color: rgba(255,255,255,0.6); cursor: pointer; }
  .nav-item.active { color: #fff; background: rgba(154,92,245,0.16); }
  .nav-item.disabled { color: rgba(255,255,255,0.25); cursor: default; }
  .nav-item.disabled:hover { background: transparent; }
  .studio { flex-shrink: 0; padding: 10px 8px 0; font-size: 10px; color: rgba(255,255,255,0.25); }

  main { flex: 1; min-width: 0; display: flex; flex-direction: column; height: 100vh; }
  header { flex-shrink: 0; padding: 16px 22px; border-bottom: 1px solid rgba(255,255,255,0.06); display: flex; align-items: center; justify-content: space-between; }
  .scroll { flex: 1; overflow-y: auto; min-height: 0; }
  .title { font-size: 16px; font-weight: 600; }
  .subtitle { font-size: 11.5px; color: rgba(255,255,255,0.4); margin-top: 2px; }

  .banner { margin: 14px 22px 0; padding: 10px 14px; border-radius: 8px; font-size: 12.5px; }
  .banner.err { background: rgba(226,75,74,0.14); color: #ff9b9b; border: 1px solid rgba(226,75,74,0.3); }

  .grid { padding: 22px; display: grid; grid-template-columns: repeat(auto-fill, 250px); gap: 14px; justify-content: start; }
  .card { border-radius: 14px; padding: 14px; background: linear-gradient(155deg, #2a1a3ef0, #14141aF5); border: 1px solid rgba(255,255,255,0.07); }
  .card-head { display: flex; align-items: center; gap: 10px; margin-bottom: 12px; }
  .head-text { flex: 1; min-width: 0; }
  .badge { width: 34px; height: 34px; border-radius: 9px; background: rgba(154,92,245,0.12); border: 1px solid rgba(154,92,245,0.33); display: flex; align-items: center; justify-content: center; color: #c9a4ff; font-weight: 600; font-size: 14px; }
  .app-name { font-size: 14px; font-weight: 600; }
  .app-ver { font-size: 10.5px; color: rgba(255,255,255,0.4); }
  .export-btn { flex-shrink: 0; width: 30px; height: 30px; display: flex; align-items: center; justify-content: center; border-radius: 8px; border: 1px solid rgba(255,255,255,0.1); background: rgba(255,255,255,0.04); color: rgba(255,255,255,0.55); cursor: pointer; }
  .export-btn:hover:not(:disabled) { background: rgba(154,92,245,0.16); color: #c9a4ff; border-color: rgba(154,92,245,0.33); }
  .export-btn:disabled { opacity: 0.4; cursor: default; }
  .export-path { display: flex; gap: 6px; align-items: baseline; margin-bottom: 10px; font-size: 10.5px; }
  .export-label { color: rgba(255,255,255,0.35); flex-shrink: 0; }
  .export-val { color: rgba(255,255,255,0.55); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; direction: rtl; text-align: left; }

  .pills { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 10px; }
  .pill { font-size: 10px; padding: 3px 8px; border-radius: 20px; background: rgba(255,255,255,0.06); color: rgba(255,255,255,0.45); border: 1px solid rgba(255,255,255,0.08); }
  .pill.ok { background: rgba(60,200,140,0.12); color: #6fe3b0; border-color: rgba(60,200,140,0.25); }
  .pill.warn { background: rgba(239,159,39,0.12); color: #f2c374; border-color: rgba(239,159,39,0.25); }
  .pill.bad { background: rgba(226,75,74,0.12); color: #ff9b9b; border-color: rgba(226,75,74,0.25); }
  .pill.pulse { animation: pulse 1.6s infinite; }

  .actions { display: flex; gap: 8px; }
  .launch-wrap { position: relative; flex: 1; }
  .primary { position: relative; overflow: hidden; width: 100%; padding: 9px 0; border-radius: 9px; border: none; cursor: pointer; font-size: 12.5px; font-weight: 600; color: #fff; background: rgba(154,92,245,0.8); display: flex; align-items: center; justify-content: center; gap: 8px; }
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

  .ghost { padding: 7px 12px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.1); background: rgba(255,255,255,0.04); color: #ccc; font-size: 12px; cursor: pointer; }

  .sys { margin-top: 18px; display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .sys-list { display: flex; flex-direction: column; gap: 2px; overflow-y: auto; min-height: 0; }
  .sys-row { display: flex; align-items: center; gap: 8px; padding: 4px 10px; border-radius: 6px; font-size: 11.5px; flex-shrink: 0; }
  .sys-row:hover { background: rgba(255,255,255,0.04); }
  .sys-name { color: rgba(255,255,255,0.6); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sys-empty { padding: 4px 10px; font-size: 11px; color: rgba(255,255,255,0.3); }
  .dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; background: rgba(255,255,255,0.3); }
  .dot.ok { background: #3ce08c; }
  .dot.bad { background: #e24b4a; }

  footer { flex-shrink: 0; padding: 10px 22px; border-top: 1px solid rgba(255,255,255,0.06); display: flex; align-items: center; gap: 8px; font-size: 11.5px; color: rgba(255,255,255,0.4); }

  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.35; } }

  /* Preferences view */
  .prefs { padding: 22px; max-width: 560px; }
  .pref-group { background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.07); border-radius: 12px; padding: 16px 18px; }
  .pref-label { font-size: 14px; font-weight: 600; color: #e8e8ec; margin-bottom: 4px; }
  .pref-desc { font-size: 11.5px; color: rgba(255,255,255,0.45); line-height: 1.5; margin-bottom: 14px; }
  .pref-row { display: flex; align-items: center; gap: 18px; flex-wrap: wrap; }
  .radio { display: flex; align-items: center; gap: 7px; font-size: 12.5px; color: rgba(255,255,255,0.8); cursor: pointer; }
  .radio input { accent-color: #9a5cf5; }
  .muted { color: rgba(255,255,255,0.4); }
  .scale-input { width: 84px; padding: 6px 8px; border-radius: 7px; border: 1px solid rgba(255,255,255,0.14); background: rgba(255,255,255,0.05); color: #e8e8ec; font-size: 12.5px; }
</style>
