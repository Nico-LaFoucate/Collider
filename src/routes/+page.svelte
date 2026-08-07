<script>
  import { prefixInfo, doctor, listApps,
           getSettings, setSettings, detectScale, compositorInfo, getThemePresets, getIconSets,
           importIconSet, adobeAuthBegin, adobeAuthPoll,
           mudhutApps, installApp } from "$lib/api.js";
  import AppCard from "$lib/AppCard.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  // --- state ---
  // For v0 the prefix is a single known path; later this comes from a prefix registry.
  let prefix = $state("~/.premiere2025");
  let info = $state(null);        // prefix info result (shared across cards)
  let health = $state(null);      // doctor result
  let error = $state(null);
  let apps = $state([]);          // installed apps from listApps() → one card each
  let refreshing = $state(false); // header Refresh button state

  // --- view switching + Preferences (settings) ---
  let view = $state("apps");                                  // "apps" | "mudhut" | "preferences"

  // --- Mud Hut installer ---
  // Which install method the user picked in the Mud Hut tab. null = show the menu.
  // "windows" (copy) and "offline" need NO Adobe sign-in; only "download" does.
  let mhMethod = $state(null);   // null | "windows" | "offline" | "download"

  // Adobe sign-in (device/QR flow) — only used by the "download" method.
  // phase: idle | starting | waiting | done | expired | error
  let auth = $state({ phase: "idle", url: null, qr: null,
                      requestId: null, deviceId: null, status: null, error: null });
  let authTimer = null;

  // --- Mud Hut install wizard (post-sign-in for download) ---
  let mhCatalog = $state([]);       // installable apps from mudhutApps()
  let mhTarget = $state("~/Adobe");  // install target prefix (editable)
  // phase: pick | installing | done | error
  let mhInstall = $state({ phase: "pick", app: null, name: null, stage: "", pct: 0, msg: "", error: null });

  // --- windows / offline source picker ---
  // The source dir the user points Mud Hut at: a Windows install root (drive_c /
  // mounted C: / copied tree) for "windows", or an offline package dir for
  // "offline". Scanning runs `mudhut apps --source` — the CLI reports which apps
  // are present AND what kind of source it found (source_kind: windows|offline).
  let mhSource = $state("");
  // phase: idle | scanning | done | error
  let mhScan = $state({ phase: "idle", apps: [], kind: null, error: null });
  // Scanned fine, but the source kind doesn't match the chosen method (e.g. the
  // user picked "offline" but pointed at a Windows tree) — installing would fail.
  const mhKindMismatch = $derived(
    mhScan.phase === "done" && mhScan.kind !== null &&
    (mhMethod === "windows" || mhMethod === "offline") && mhScan.kind !== mhMethod);

  function openMudHut() { view = "mudhut"; mhMethod = null; cancelSignIn(); resetInstall(); resetSource(); }
  function backToMethods() { cancelSignIn(); mhMethod = null; resetInstall(); resetSource(); }
  function resetInstall() {
    mhInstall = { phase: "pick", app: null, name: null, stage: "", pct: 0, msg: "", error: null };
  }
  function resetSource() {
    mhSource = "";
    mhScan = { phase: "idle", apps: [], kind: null, error: null };
  }

  async function loadCatalog() {
    try { mhCatalog = (await mudhutApps())?.apps ?? []; }
    catch (e) { mhCatalog = []; }
  }

  async function browseSource() {
    let dir;
    try {
      dir = await open({ directory: true, title: mhMethod === "windows"
        ? "Choose the Windows install root (a drive_c, mounted C:, or copied tree)"
        : "Choose the offline package folder" });
    } catch (_) { return; }
    if (!dir) return;
    mhSource = dir;
    await scanSource();
  }

  async function scanSource() {
    const src = mhSource.trim();
    if (!src) return;
    mhScan = { phase: "scanning", apps: [], kind: null, error: null };
    try {
      const r = await mudhutApps(src);
      mhScan = { phase: "done", apps: r?.apps ?? [], kind: r?.source_kind ?? null, error: null };
    } catch (e) {
      mhScan = { phase: "error", apps: [], kind: null, error: String(e) };
    }
  }

  async function startInstall(app) {
    const method = mhMethod;
    // download resolves from Adobe; windows/offline install from the scanned source.
    const source = method === "download" ? null : mhSource.trim();
    mhInstall = { phase: "installing", app: app.id, name: app.name, stage: "starting", pct: 0, msg: "", error: null };
    try {
      await installApp({
        app: app.id, method, prefix: mhTarget, source,
        onEvent: (ev) => {
          if (ev.event === "progress") {
            if (ev.stage) mhInstall.stage = ev.stage;
            if (typeof ev.pct === "number") mhInstall.pct = ev.pct;
            if (ev.msg) mhInstall.msg = ev.msg;
          } else if (ev.event === "note" && ev.msg) {
            mhInstall.msg = ev.msg;
          }
        },
      });
      mhInstall = { ...mhInstall, phase: "done" };
      prefix = mhTarget;   // point the Apps view at the freshly-installed prefix
      await refresh();     // the new app now shows as a widget
    } catch (e) {
      mhInstall = { ...mhInstall, phase: "error", error: String(e) };
    }
  }

  async function startSignIn() {
    if (authTimer) { clearTimeout(authTimer); authTimer = null; }
    auth = { phase: "starting", url: null, qr: null, requestId: null, deviceId: null, status: null, error: null };
    try {
      const b = await adobeAuthBegin();
      auth = { ...auth, phase: "waiting", url: b.url, qr: b.qr,
               requestId: b.request_id, deviceId: b.device_id, status: "pending" };
      pollSignIn();
    } catch (e) {
      auth = { ...auth, phase: "error", error: String(e) };
    }
  }

  async function pollSignIn() {
    if (auth.phase !== "waiting") return;
    try {
      const r = await adobeAuthPoll(auth.requestId, auth.deviceId);
      auth = { ...auth, status: r.status };
      if (r.status === "complete") { auth = { ...auth, phase: "done" }; loadCatalog(); return; }
      if (r.status === "expired")  { auth = { ...auth, phase: "expired" }; return; }
      authTimer = setTimeout(pollSignIn, ((r.retry_interval ?? 5) * 1000));
    } catch (e) {
      auth = { ...auth, phase: "error", error: String(e) };
    }
  }

  function cancelSignIn() {
    if (authTimer) { clearTimeout(authTimer); authTimer = null; }
    auth = { phase: "idle", url: null, qr: null, requestId: null, deviceId: null, status: null, error: null };
  }
  let settings = $state({ scale_mode: "auto", scale_value: 1.5, home_window_fix: true, home_window_y: 82,
                          theme: "dark", custom_colors: null, button_icon_set: "none" });
  let iconSets = $state([]);                                  // [{ id, label }] from the backend
  let detectedScale = $state(null);                           // live primary-monitor scale, for reference
  let compositor = $state(null);                              // { wayland, desktop, home_rule_supported }

  // --- Appearance / decoration theme ---
  // Theme color groups shown in the Appearance editor. Keys are Control Panel color
  // names (what the wine frame reads); values live in settings.custom_colors as
  // "R G B" strings. Close-hover red is hardcoded in the wine patch (not editable).
  let themePresets = $state([]);            // [{ id, label, colors }] from the backend
  let importing = $state(false);            // caption-icon import in progress
  const COLOR_GROUPS = [
    { label: "Title bar", keys: [
      ["ActiveTitle", "Background"], ["TitleText", "Text"], ["InactiveTitle", "Inactive bg"] ] },
    { label: "Menu bar", keys: [
      ["MenuBar", "Background"], ["MenuText", "Text"], ["MenuHilight", "Highlight"] ] },
    { label: "Buttons", keys: [
      ["ActiveTitle", "Normal bg"], ["ButtonText", "Glyph"],
      ["ButtonHilight", "Min/Max hover"], ["ButtonShadow", "Pressed"] ] },
    { label: "Window", keys: [
      ["Window", "Background"], ["WindowText", "Text"], ["WindowFrame", "Frame edge"] ] },
  ];

  // "R G B" <-> "#rrggbb" so we can use native <input type=color>.
  function rgbToHex(rgb) {
    const p = String(rgb ?? "").trim().split(/\s+/).map(Number);
    if (p.length !== 3 || p.some((n) => Number.isNaN(n))) return "#000000";
    return "#" + p.map((n) => Math.max(0, Math.min(255, n)).toString(16).padStart(2, "0")).join("");
  }
  function hexToRgb(hex) {
    const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(String(hex).trim());
    if (!m) return "0 0 0";
    return [1, 2, 3].map((i) => parseInt(m[i], 16)).join(" ");
  }
  // Relative luminance (0..1) for the contrast guard.
  function luminance(rgb) {
    const [r, g, b] = String(rgb ?? "0 0 0").trim().split(/\s+/).map(Number);
    const f = (c) => { c = (c || 0) / 255; return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
  }
  function contrastRatio(a, b) {
    const la = luminance(a), lb = luminance(b);
    return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
  }

  const dimById = (id) => themePresets.find((p) => p.id === id);

  // The color map currently in effect for the editors: custom overrides merged onto
  // Dark, or the named preset's colors.
  function activeColors() {
    const dark = dimById("dark")?.colors ?? {};
    if (settings.theme === "custom") return { ...dark, ...(settings.custom_colors ?? {}) };
    return dimById(settings.theme)?.colors ?? dark;
  }

  // Switch preset. Choosing "custom" seeds the editable map from whatever's showing.
  function setTheme(id) {
    if (id === "custom" && !settings.custom_colors) settings.custom_colors = { ...activeColors() };
    settings.theme = id;
    saveSettings();
  }
  // Edit one color (auto-switches to custom, seeding from the current colors).
  function setColor(key, hex) {
    if (settings.theme !== "custom") {
      settings.custom_colors = { ...activeColors() };
      settings.theme = "custom";
    }
    settings.custom_colors = { ...settings.custom_colors, [key]: hexToRgb(hex) };
    saveSettings();
  }
  function resetToDark() {
    settings.theme = "dark";
    settings.custom_colors = null;
    saveSettings();
  }

  // Import a custom button-icon set from a folder of close/min/max/restore images.
  async function importIcons() {
    let dir;
    try {
      dir = await open({ directory: true,
                         title: "Choose a folder with close / min / max / restore icons" });
    } catch (_) { return; }
    if (!dir) return;
    importing = true;
    try {
      await importIconSet(dir);
      iconSets = await getIconSets();          // now includes "custom"
      settings.button_icon_set = "custom";
      await saveSettings();
    } catch (e) { error = String(e); }
    importing = false;
  }
  // Title-bar + menu text-on-bg contrast warnings (WCAG-ish; < 4.5 is low).
  const lowContrast = $derived.by(() => {
    const c = activeColors();
    const warns = [];
    if (contrastRatio(c.TitleText, c.ActiveTitle) < 4.5) warns.push("title bar");
    if (contrastRatio(c.MenuText, c.MenuBar) < 4.5) warns.push("menu bar");
    if (contrastRatio(c.ButtonText, c.ActiveTitle) < 3) warns.push("button glyphs");
    return warns;
  });

  onMount(async () => {
    try { settings = await getSettings(); } catch (_) {}
    if (settings.scale_value == null) settings.scale_value = detectedScale ?? 1.5;
    await refresh();                       // load prefix health + the app catalog up front
  });

  async function openPreferences() {
    view = "preferences";
    try { detectedScale = await detectScale(); } catch (_) { detectedScale = null; }
    try { compositor = await compositorInfo(); } catch (_) { compositor = null; }
    try { themePresets = await getThemePresets(); } catch (_) { themePresets = []; }
    try { iconSets = await getIconSets(); } catch (_) { iconSets = []; }
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
        theme: settings.theme,
        custom_colors: settings.custom_colors,
        button_icon_set: settings.button_icon_set,
      });
    } catch (e) { error = String(e); }
  }

  async function refresh() {
    error = null;
    refreshing = true;
    try {
      info = await prefixInfo(prefix);
      health = await doctor(prefix);
      const cat = await listApps(prefix);          // the app catalog + install status
      apps = (cat?.apps ?? []).filter((a) => a.installed);
    } catch (e) {
      error = String(e);
      info = null;
      health = null;
    } finally {
      refreshing = false;
    }
  }

</script>

<div class="app">
  <aside>
    <div class="brand">
      <img class="logo" src="/collider-logo.png" alt="" />
      <div>
        <div class="brand-name">Collider</div>
        <div class="brand-by">by Nico LaFoucate</div>
      </div>
    </div>
    <nav>
      <div class="nav-section">Library</div>
      <div class="nav-item" class:active={view === "apps"} role="button" tabindex="0"
           onclick={() => (view = "apps")}>All apps</div>
      <div class="nav-section">Tools</div>
      <div class="nav-item" class:active={view === "mudhut"} role="button" tabindex="0"
           onclick={openMudHut}>Mud Hut</div>
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
      <button class="ghost" onclick={refresh} disabled={refreshing}>↻ Refresh</button>
    </header>

    <div class="scroll">
    {#if error}
      <div class="banner err">{error}</div>
    {/if}

    <section class="grid">
      <!-- One widget per installed app, tinted to its brand color. -->
      {#each apps as app (app.id)}
        <AppCard {app} {prefix} {info} />
      {/each}
      {#if apps.length === 0}
        <div class="empty-apps">No apps detected in this prefix — hit ↻ Refresh.</div>
      {/if}
    </section>
    </div>
  {:else if view === "mudhut"}
    <header>
      <div>
        <div class="title">Mud Hut</div>
        <div class="subtitle">
          {#if mhMethod === null}Install Adobe apps · choose how to install
          {:else if mhMethod === "download"}Download from Adobe
          {:else if mhMethod === "windows"}Copy from a Windows install
          {:else}Install from an offline package{/if}
        </div>
      </div>
    </header>
    <div class="scroll">
      {#if mhMethod === null}
        <!-- Method picker. Copy + Offline need NO Adobe sign-in; only Download does. -->
        <section class="mh-methods">
          <button class="method-card" onclick={() => (mhMethod = "windows")}>
            <div class="method-h">Copy from an existing Windows install</div>
            <div class="method-d">Point Mud Hut at a licensed Windows Adobe install and copy it in. <b>No Adobe sign-in needed.</b></div>
          </button>
          <button class="method-card" onclick={() => (mhMethod = "offline")}>
            <div class="method-h">Install from an offline package</div>
            <div class="method-d">Use a pre-downloaded Adobe offline installer / package. <b>No Adobe sign-in needed.</b></div>
          </button>
          <button class="method-card" onclick={() => { mhMethod = "download"; loadCatalog(); }}>
            <div class="method-h">Download from Adobe</div>
            <div class="method-d">Fetch genuine app files straight from Adobe. <b>No sign-in to install</b> — you activate once on first launch, inside the app.</div>
          </button>
        </section>
      {:else}
        <!-- download / windows / offline — the install phases (progress / done /
             error) are shared; only the pick step differs per method.
             No sign-in gate anywhere: the download feed + CDN are public and HDPIM
             decrypts without entitlement; windows/offline are local-only. Licensing
             is a one-time sign-in INSIDE the app on first launch (NGL writes opm.db
             itself) — not here. -->
        <section class="mudhut">
          <button class="link-back" onclick={backToMethods}>← Back to install options</button>
          {#if mhInstall.phase === "installing"}
            <div class="signin-card">
              <div class="signin-h">Installing {mhInstall.name}…</div>
              <div class="install-stage">{mhInstall.stage || "starting"}</div>
              <div class="progress"><div class="bar" style="width: {mhInstall.pct}%"></div></div>
              <div class="install-msg">{mhInstall.msg}</div>
              <div class="signin-d">
                {#if mhMethod === "download"}Downloading + installing genuine {mhInstall.name} from Adobe.
                {:else if mhMethod === "windows"}Copying {mhInstall.name} from your Windows install and provisioning the prefix.
                {:else}Installing {mhInstall.name} from the offline package.{/if}
                This takes a while — safe to leave running.
              </div>
            </div>
          {:else if mhInstall.phase === "done"}
            <div class="signin-card">
              <div class="signin-h">✓ {mhInstall.name} installed</div>
              <div class="signin-d">Installed into {mhTarget}. It now appears in <b>Apps</b> — launch it there and sign in once, inside the app, to finish Adobe activation (licensing).</div>
              <button class="primary" onclick={() => { view = "apps"; }}>Go to Apps</button>
              <button class="ghost" onclick={resetInstall}>Install another</button>
            </div>
          {:else if mhInstall.phase === "error"}
            <div class="signin-card">
              <div class="banner err">{mhInstall.error}</div>
              <button class="primary" onclick={resetInstall}>Back to apps</button>
            </div>
          {:else if mhMethod === "download"}
            <div class="signin-card">
              <div class="signin-h">Choose an app to install</div>
              <div class="signin-d">Genuine Adobe files download straight from Adobe — no sign-in needed to install. You activate (sign in) once on first launch, inside the app.</div>
              <div class="mh-target">
                <label>Install into</label>
                <input bind:value={mhTarget} spellcheck="false" />
              </div>
              <div class="mh-applist">
                {#each mhCatalog as a}
                  <button class="mh-app" onclick={() => startInstall(a)}>
                    <span class="mh-app-badge">{a.sap}</span>
                    <span class="mh-app-name">{a.name}</span>
                    <span class="mh-app-go">Install →</span>
                  </button>
                {/each}
                {#if mhCatalog.length === 0}<div class="signin-d">Loading catalog…</div>{/if}
              </div>
            </div>
          {:else}
            <!-- windows / offline — source picker, then the apps found in it. -->
            <div class="signin-card">
              <div class="signin-h">
                {mhMethod === "windows" ? "Choose the Windows install" : "Choose the offline package"}
              </div>
              <div class="signin-d">
                {#if mhMethod === "windows"}
                  Point at a Windows install root — a Wine <b>drive_c</b>, a mounted Windows
                  <b>C:</b> drive, or a copied tree (anything containing
                  <b>Program&nbsp;Files/Adobe</b>). No Adobe sign-in needed.
                {:else}
                  Point at an offline package folder — the layout <b>mudhut download</b> stages
                  (per-app folders with <b>Application.json</b> + payload files). Extract an
                  ISO first. No Adobe sign-in needed.
                {/if}
              </div>
              <div class="mh-target">
                <label>{mhMethod === "windows" ? "Windows install root" : "Package folder"}</label>
                <div class="mh-source-row">
                  <input bind:value={mhSource} spellcheck="false"
                         placeholder={mhMethod === "windows" ? "/path/to/drive_c or mounted C:" : "/path/to/package/products"}
                         onkeydown={(e) => { if (e.key === "Enter") scanSource(); }} />
                  <button class="ghost" onclick={browseSource}>Browse…</button>
                  <button class="ghost" onclick={scanSource}
                          disabled={!mhSource.trim() || mhScan.phase === "scanning"}>
                    {mhScan.phase === "scanning" ? "Scanning…" : "Scan"}
                  </button>
                </div>
              </div>
              {#if mhScan.phase === "error"}
                <div class="banner err mh-banner">{mhScan.error}</div>
              {:else if mhScan.phase === "done"}
                {#if mhKindMismatch}
                  <div class="banner err mh-banner">
                    {mhScan.kind === "windows"
                      ? "That folder is a Windows install, not an offline package — go back and pick “Copy from an existing Windows install”."
                      : "That folder is an offline package, not a Windows install — go back and pick “Install from an offline package”."}
                  </div>
                {:else}
                  <div class="mh-target">
                    <label>Install into</label>
                    <input bind:value={mhTarget} spellcheck="false" />
                  </div>
                  <div class="mh-applist">
                    {#each mhScan.apps as a}
                      {#if a.present}
                        <button class="mh-app" onclick={() => startInstall(a)}>
                          <span class="mh-app-badge">{a.sap}</span>
                          <span class="mh-app-name">{a.name}</span>
                          <span class="mh-app-go">Install →</span>
                        </button>
                      {:else}
                        <div class="mh-app absent">
                          <span class="mh-app-badge">{a.sap}</span>
                          <span class="mh-app-name">{a.name}</span>
                          <span class="mh-app-go">not found</span>
                        </div>
                      {/if}
                    {/each}
                    {#if !mhScan.apps.some((a) => a.present)}
                      <div class="signin-d">No installable Adobe apps found in this source.</div>
                    {/if}
                  </div>
                {/if}
              {/if}
            </div>
          {/if}
        </section>
      {/if}
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

        <div class="pref-group">
          <div class="pref-label">Window decoration theme</div>
          <div class="pref-desc">
            Colors for Premiere's title bar, menu bar and window buttons (Neutron draws
            its own frame). Pick a preset or edit any color to make a custom theme.
            Applied to the prefix on launch — <b>restart Premiere to see changes</b>.
          </div>
          <div class="pref-row">
            {#each [...themePresets, { id: "custom", label: "Custom" }] as p}
              <label class="radio">
                <input type="radio" name="theme" value={p.id}
                       checked={settings.theme === p.id}
                       onchange={() => setTheme(p.id)} />
                {p.label}
              </label>
            {/each}
          </div>

          <div class="theme-editor">
            {#each COLOR_GROUPS as group}
              <div class="theme-col">
                <div class="theme-col-label">{group.label}</div>
                {#each group.keys as [key, label]}
                  <label class="swatch-row">
                    <input type="color" class="swatch"
                           value={rgbToHex(activeColors()[key])}
                           oninput={(e) => setColor(key, e.currentTarget.value)} />
                    <span>{label}</span>
                  </label>
                {/each}
              </div>
            {/each}
          </div>

          {#if lowContrast.length}
            <div class="pref-desc warn">
              ⚠ Low contrast in the {lowContrast.join(" and ")} — text may be hard to read.
            </div>
          {/if}

          <div class="theme-col-label" style="margin-top:18px;">Window buttons (_ □ X)</div>
          <div class="icon-grid">
            {#each iconSets as s}
              <button class="icon-card" class:sel={settings.button_icon_set === s.id}
                      onclick={() => { settings.button_icon_set = s.id; saveSettings(); }}>
                {#if s.preview}
                  <img class="icon-thumb" src={s.preview} alt={s.label} />
                {:else}
                  <span class="icon-thumb placeholder">_&nbsp;□&nbsp;✕</span>
                {/if}
                <span class="icon-card-label">{s.label}</span>
              </button>
            {/each}
            <button class="icon-card" onclick={importIcons} disabled={importing}>
              <span class="icon-thumb placeholder">{importing ? "…" : "+"}</span>
              <span class="icon-card-label">{importing ? "Importing…" : "Import…"}</span>
            </button>
          </div>
          <div class="pref-desc muted">
            Pick a style or import your own — a folder with <b>close</b>, <b>min</b>,
            <b>max</b>, <b>restore</b> images (.png or .ico). Restart Premiere to apply;
            the close button still highlights red on hover.
          </div>

          <div class="pref-row" style="margin-top:12px;">
            <button class="ghost-btn" onclick={resetToDark} disabled={settings.theme === "dark"}>
              Reset to Dark
            </button>
            <span class="muted">Close-button hover stays red on every theme (by design).</span>
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
  .logo { width: 26px; height: 26px; display: block; flex-shrink: 0; }
  .brand-name { font-size: 17px; font-weight: 700; }
  .brand-by { font-size: 9.5px; color: rgba(255,255,255,0.35); }
  .nav-section { flex-shrink: 0; font-size: 9.5px; text-transform: uppercase; letter-spacing: 1.2px; color: rgba(255,255,255,0.3); padding: 14px 8px 6px; }
  .nav-item { padding: 7px 10px; border-radius: 8px; font-size: 13px; color: rgba(255,255,255,0.6); cursor: pointer; }
  .nav-item.active { color: #fff; background: rgba(154,92,245,0.16); }
  .nav-item.disabled { color: rgba(255,255,255,0.25); cursor: default; }
  .nav-item.disabled:hover { background: transparent; }

  main { flex: 1; min-width: 0; display: flex; flex-direction: column; height: 100vh; }
  header { flex-shrink: 0; padding: 16px 22px; border-bottom: 1px solid rgba(255,255,255,0.06); display: flex; align-items: center; justify-content: space-between; }
  .scroll { flex: 1; overflow-y: auto; min-height: 0; }
  .title { font-size: 16px; font-weight: 600; }
  .subtitle { font-size: 11.5px; color: rgba(255,255,255,0.4); margin-top: 2px; }

  .banner { margin: 14px 22px 0; padding: 10px 14px; border-radius: 8px; font-size: 12.5px; }
  .banner.err { background: rgba(226,75,74,0.14); color: #ff9b9b; border: 1px solid rgba(226,75,74,0.3); }

  .grid { padding: 22px; display: grid; grid-template-columns: repeat(auto-fill, 250px); gap: 14px; justify-content: start; }
  .empty-apps { padding: 22px; color: rgba(255,255,255,0.4); font-size: 12.5px; }
  /* The app-card styles (card / badge / pills / launch button / force-quit menu)
     now live in src/lib/AppCard.svelte, tinted per app. */

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
  .prefs { padding: 22px; max-width: 560px; display: flex; flex-direction: column; gap: 16px; }
  .pref-group { background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.07); border-radius: 12px; padding: 16px 18px; }
  .pref-label { font-size: 14px; font-weight: 600; color: #e8e8ec; margin-bottom: 4px; }
  .pref-desc { font-size: 11.5px; color: rgba(255,255,255,0.45); line-height: 1.5; margin-bottom: 14px; }
  .pref-row { display: flex; align-items: center; gap: 18px; flex-wrap: wrap; }
  .radio { display: flex; align-items: center; gap: 7px; font-size: 12.5px; color: rgba(255,255,255,0.8); cursor: pointer; }
  .radio input { accent-color: #9a5cf5; }
  .muted { color: rgba(255,255,255,0.4); }
  .scale-input { width: 84px; padding: 6px 8px; border-radius: 7px; border: 1px solid rgba(255,255,255,0.14); background: rgba(255,255,255,0.05); color: #e8e8ec; font-size: 12.5px; }
  /* Appearance / theme editor */
  .theme-editor { display: grid; grid-template-columns: repeat(2, 1fr); gap: 14px 22px; margin-top: 14px; }
  .theme-col-label { font-size: 10px; text-transform: uppercase; letter-spacing: 0.8px; color: rgba(255,255,255,0.35); margin-bottom: 7px; }
  .swatch-row { display: flex; align-items: center; gap: 9px; font-size: 12px; color: rgba(255,255,255,0.75); margin-bottom: 6px; cursor: pointer; }
  .swatch { width: 26px; height: 18px; padding: 0; border: 1px solid rgba(255,255,255,0.18); border-radius: 5px; background: none; cursor: pointer; }
  .swatch::-webkit-color-swatch { border: none; border-radius: 4px; }
  .swatch::-webkit-color-swatch-wrapper { padding: 0; }
  .warn { color: #f0b84a; }
  .ghost-btn { padding: 6px 12px; border-radius: 7px; border: 1px solid rgba(255,255,255,0.16); background: rgba(255,255,255,0.05); color: #e8e8ec; font-size: 12px; cursor: pointer; }
  .ghost-btn:disabled { opacity: 0.4; cursor: default; }
  .icon-grid { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 10px; }
  .icon-card { display: flex; flex-direction: column; align-items: center; gap: 6px; width: 88px; padding: 8px 6px; border-radius: 9px; border: 1px solid rgba(255,255,255,0.10); background: rgba(255,255,255,0.03); color: rgba(255,255,255,0.7); cursor: pointer; transition: border-color .12s, background .12s; }
  .icon-card:hover { border-color: rgba(255,255,255,0.22); background: rgba(255,255,255,0.06); }
  .icon-card.sel { border-color: #9a5cf5; background: rgba(154,92,245,0.12); color: #e8e8ec; }
  .icon-card:disabled { opacity: 0.6; cursor: default; }
  .icon-thumb { height: 22px; width: auto; border-radius: 4px; background: #2b2b2b; image-rendering: auto; }
  .icon-thumb.placeholder { display: flex; align-items: center; justify-content: center; width: 66px; height: 22px; font-size: 12px; color: rgba(255,255,255,0.55); letter-spacing: 1px; }
  .icon-card-label { font-size: 11px; text-align: center; line-height: 1.2; }

  /* Mud Hut — install method picker */
  .mh-methods { padding: 22px; display: flex; flex-direction: column; gap: 12px; max-width: 560px; }
  .method-card { text-align: left; padding: 16px 18px; border-radius: 12px; cursor: pointer;
    background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08); color: inherit;
    display: flex; flex-direction: column; gap: 5px; transition: background 0.12s, border-color 0.12s; }
  .method-card:hover { background: rgba(255,255,255,0.06); border-color: rgba(49,168,255,0.4); }
  .method-h { font-size: 14px; font-weight: 600; color: #f0f0f2; }
  .method-d { font-size: 12.5px; color: rgba(255,255,255,0.6); line-height: 1.5; }
  .method-d b { color: #8fd0ff; font-weight: 600; }
  .link-back { align-self: flex-start; margin: 4px 0 14px; background: none; border: none;
    color: rgba(255,255,255,0.6); font-size: 12.5px; cursor: pointer; padding: 0; }
  .link-back:hover { color: #cfcfd4; }

  /* Mud Hut — Adobe sign-in */
  .mudhut { padding: 22px; }
  .signin-card { max-width: 420px; margin: 8px auto; padding: 24px; border-radius: 12px;
    background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08);
    display: flex; flex-direction: column; align-items: center; gap: 14px; text-align: center; }
  .signin-h { font-size: 15px; font-weight: 600; color: #f0f0f2; }
  .signin-d { font-size: 12.5px; color: rgba(255,255,255,0.6); line-height: 1.5; }
  .primary { padding: 9px 18px; border-radius: 9px; border: 1px solid rgba(49,168,255,0.4);
    background: rgba(49,168,255,0.16); color: #8fd0ff; font-size: 13px; font-weight: 600; cursor: pointer; }
  .primary:hover { background: rgba(49,168,255,0.24); }

  /* --- Mud Hut install wizard --- */
  .install-stage { font-size: 12px; text-transform: capitalize; color: #8fd0ff; font-weight: 600; }
  .install-msg { font-size: 11.5px; color: rgba(255,255,255,0.5); min-height: 1.2em;
    max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .progress { width: 100%; height: 8px; border-radius: 6px; overflow: hidden;
    background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.08); }
  .progress .bar { height: 100%; background: linear-gradient(90deg, #2f7fd6, #31a8ff);
    transition: width 0.3s ease; }
  .mh-target { display: flex; flex-direction: column; gap: 5px; width: 100%; text-align: left; }
  .mh-target label { font-size: 11px; color: rgba(255,255,255,0.5); }
  .mh-target input { width: 100%; box-sizing: border-box; padding: 7px 10px; border-radius: 8px;
    border: 1px solid rgba(255,255,255,0.1); background: rgba(0,0,0,0.25); color: #cfcfd4; font-size: 12px; }
  /* windows / offline — source picker row + apps-not-in-source rows */
  .mh-source-row { display: flex; gap: 6px; width: 100%; }
  .mh-source-row input { flex: 1; min-width: 0; box-sizing: border-box; padding: 7px 10px;
    border-radius: 8px; border: 1px solid rgba(255,255,255,0.1); background: rgba(0,0,0,0.25);
    color: #cfcfd4; font-size: 12px; }
  .mh-source-row .ghost { flex-shrink: 0; }
  .mh-source-row .ghost:disabled { opacity: 0.45; cursor: default; }
  .mh-banner { width: 100%; box-sizing: border-box; margin: 0; text-align: left; }
  .mh-app.absent { opacity: 0.4; cursor: default; }
  .mh-app.absent:hover { background: rgba(255,255,255,0.03); border-color: rgba(255,255,255,0.08); }
  .mh-applist { display: flex; flex-direction: column; gap: 6px; width: 100%; }
  .mh-app { display: flex; align-items: center; gap: 10px; width: 100%; text-align: left;
    padding: 9px 12px; border-radius: 9px; cursor: pointer;
    background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08); color: #e8e8ea; }
  .mh-app:hover { background: rgba(49,168,255,0.1); border-color: rgba(49,168,255,0.4); }
  .mh-app-badge { font-size: 10px; font-weight: 700; letter-spacing: 0.5px; padding: 2px 6px;
    border-radius: 5px; background: rgba(49,168,255,0.18); color: #8fd0ff; }
  .mh-app-name { flex: 1; font-size: 13px; }
  .mh-app-go { font-size: 11.5px; color: rgba(255,255,255,0.4); }
  .mh-app:hover .mh-app-go { color: #8fd0ff; }
</style>
