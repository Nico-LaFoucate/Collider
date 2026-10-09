<script>
  import { prefixInfo, doctor, listApps,
           getSettings, setSettings, detectScale, getThemePresets, getIconSets,
           importIconSet, versions, neutronSetup, neutronUninstall,
           mudhutApps, installApp, workingPrefix,
           listPrefixes, discoverPrefixes, addPrefix, removePrefix, renamePrefix,
           selectPrefix, provisionPrefix, fontsCheck, fontsRepair } from "$lib/api.js";
  import AppCard from "$lib/AppCard.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  // --- window chrome ---
  // Collider draws its own title bar (DECISIONS C34, option A): the window is undecorated, the
  // empty strip along the top drags it (double-click maximizes, via data-tauri-drag-region), and
  // the three buttons at its right are ours. `maximized` picks the restore glyph and hides the
  // resize handles while there is nothing to resize.
  const win = getCurrentWindow();
  let maximized = $state(false);
  onMount(() => {
    let unlisten = null;
    const sync = () => win.isMaximized().then((m) => (maximized = m)).catch(() => {});
    sync();
    win.onResized(sync).then((u) => (unlisten = u)).catch(() => {});
    return () => { if (unlisten) unlisten(); };
  });
  // An undecorated GTK window has no resize border of its own (tao's edge hit-test sits on the GTK
  // window, under the webview), so thin handles along the page edge start the compositor's resize
  // for the matching direction. [css class, Tauri ResizeDirection]
  const EDGES = [
    ["n", "North"], ["s", "South"], ["e", "East"], ["w", "West"],
    ["nw", "NorthWest"], ["ne", "NorthEast"], ["sw", "SouthWest"], ["se", "SouthEast"],
  ];
  function resizeFrom(direction) {
    return (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      win.startResizeDragging(direction).catch(() => {});
    };
  }

  // --- state ---
  // The working prefix is RESOLVED AT STARTUP, never hardcoded: the user's saved choice if
  // they have one, else whatever the engine's default resolves to ($HOME/.premiere2025).
  // This was previously a literal "~/.premiere2025", which meant that on anyone
  // else's machine Collider opened against a nonexistent path and showed an empty library
  // with no way to fix it. null = not resolved yet.
  let prefix = $state(null);
  let prefixValid = $state(true);   // false => show first-run setup instead of an empty library
  let defaultNewPrefix = $state(""); // where a NEW prefix should go, per the backend

  // --- prefix registry (Prefixes tab) ---
  let prefixes = $state([]);         // [{ name, path, valid, selected, apps }]
  let discovered = $state([]);       // [{ name, path }] found on disk, not registered
  let renaming = $state(null);       // path currently being renamed
  let renameText = $state("");
  let provisioning = $state(null);   // path currently provisioning
  let provProgress = $state({ pct: 0, msg: "" });
  // Fonts row per prefix: path -> the engine's verdict ({status, summary, checks, ...}) or
  // null while unknown. Filled by loadFonts() after the registry loads; the engine's check is
  // read-only and never starts wine, so running it for every prefix on tab open is free.
  let fonts = $state({});
  let repairingFonts = $state(null);   // path whose fonts are being repaired
  let prefixMsg = $state(null);
  let info = $state(null);        // prefix info result (shared across cards)
  let health = $state(null);      // doctor result
  let error = $state(null);
  let apps = $state([]);          // installed apps from listApps() → one card each
  let refreshing = $state(false); // header Refresh button state

  // --- view switching + Preferences (settings) ---
  let view = $state("apps");                    // "apps" | "prefixes" | "mudhut" | "preferences"

  // --- Mud Hut installer ---
  // Which install method the user picked in the Mud Hut tab. null = show the menu. No method
  // needs an Adobe sign-in; licensing happens inside the app on first launch.
  let mhMethod = $state(null);   // null | "windows" | "offline" | "download"

  // --- Mud Hut install wizard ---
  let mhCatalog = $state([]);       // installable apps from mudhutApps()
  let mhTarget = $state("");  // install target prefix (editable); seeded from $HOME at startup
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

  function openMudHut() { view = "mudhut"; mhMethod = null; resetInstall(); resetSource(); }
  function backToMethods() { mhMethod = null; resetInstall(); resetSource(); }
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

  // Offline only: pick a disc image directly. Mud Hut mounts it read-only for the
  // scan and again for the install, and unmounts afterwards — nothing is extracted
  // and nothing is copied, so there is no reason to make the user do it by hand.
  async function browseSourceIso() {
    let file;
    try {
      file = await open({ directory: false, multiple: false, title: "Choose an Adobe disc image",
                          filters: [{ name: "Disc image", extensions: ["iso", "img"] }] });
    } catch (_) { return; }
    if (!file) return;
    mhSource = typeof file === "string" ? file : file?.path ?? "";
    if (!mhSource) return;
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
      // Point the Apps view at the freshly-installed prefix AND persist it — an install into a
      // non-default location used to be forgotten on restart, leaving the library empty.
      await adoptPrefix(mhTarget);
    } catch (e) {
      mhInstall = { ...mhInstall, phase: "error", error: String(e) };
    }
  }

  let settings = $state({ scale_mode: "auto", scale_value: 1.5,
                          theme: "dark", custom_colors: null, button_icon_set: "none" });
  let iconSets = $state([]);                                  // [{ id, label }] from the backend
  let detectedScale = $state(null);                           // `neutron display`: { scale, dpi, windows_step, warning, ... }
  // The Windows scale steps (DECISIONS C21): Adobe's UI is laid out for these, and anything in
  // between renders slightly off.
  const SCALE_STEPS = [1, 1.25, 1.5, 1.75, 2, 2.25, 2.5, 3];
  const pct = (s) => Math.round(s * 100) + "%";

  // --- Neutron itself: versions, set up, update, uninstall ---
  let vers = $state(null);              // { cli, neutron_wine, mudhut, collider }
  let neutronReady = $derived(!!(vers?.cli && vers?.neutron_wine));
  // phase: idle | running | done | error; kind: setup | update
  let nrun = $state({ phase: "idle", kind: null, stage: "", msg: "", error: null });
  let confirmUninstall = $state(false);
  let uninstallPrefixes = $state(false);
  let uninstalled = $state(null);       // the uninstall result, once it has run

  async function loadVersions() {
    try { vers = await versions(); } catch (_) { vers = { cli: null }; }
  }

  // Set up (first run) or Update: the engine does the work and streams its progress.
  async function runNeutron(kind) {
    nrun = { phase: "running", kind, stage: "starting", msg: "", error: null };
    try {
      await neutronSetup(kind === "update", (ev) => {
        if (ev.event === "progress") {
          if (ev.stage) nrun.stage = ev.stage;
          if (ev.msg) nrun.msg = ev.msg;
        } else if (ev.event === "note" && ev.msg) {
          nrun.msg = ev.msg;
        }
      });
      nrun = { ...nrun, phase: "done" };
      await loadVersions();
      await loadPrefixes();
      if (prefixValid) await refresh();
    } catch (e) {
      nrun = { ...nrun, phase: "error", error: String(e) };
    }
  }

  async function runUninstall() {
    confirmUninstall = false;
    try { uninstalled = await neutronUninstall(uninstallPrefixes); }
    catch (e) { error = String(e); }
    await loadVersions();
  }

  // --- Appearance / decoration theme ---
  // Theme color groups shown in the Appearance editor. Keys are Control Panel color
  // names (what the wine frame reads); values live in settings.custom_colors as
  // "R G B" strings. Close-hover red is hardcoded in the wine patch (not editable).
  let themePresets = $state([]);            // [{ id, label, colors }] from the backend
  let importing = $state(false);            // caption-icon import in progress
  const COLOR_GROUPS = [
    // The caption buttons (_ □ X) follow this group: their glyph is TitleText and their
    // hover/pressed shades are derived from ActiveTitle by the wine patch. They used to have
    // their own swatches wired to the classic ButtonText/ButtonHilight/ButtonShadow, which is
    // why those had to be dark -- and that darkened COLOR_BTNFACE for every app, making
    // Lightroom's About labels black on near-black. Those keys now stay at Windows' values.
    { label: "Title bar & buttons", keys: [
      ["ActiveTitle", "Background"], ["TitleText", "Text / glyph"], ["InactiveTitle", "Inactive bg"] ] },
    { label: "Menu bar", keys: [
      ["MenuBar", "Background"], ["MenuText", "Text"], ["MenuHilight", "Highlight"] ] },
    { label: "Window", keys: [
      ["Window", "Background"], ["WindowText", "Text"], ["WindowFrame", "Frame edge"] ] },
    // Message boxes, common dialogs, and any control an app doesn't draw itself. Since the
    // chrome moved off this group (neutron-wine zzzzzzzzzp/q/r) these are safe to theme freely.
    { label: "Dialogs & controls", keys: [
      ["ButtonFace", "Background"], ["ButtonText", "Text"], ["ButtonShadow", "Edge"] ] },
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
    if (contrastRatio(c.ButtonText, c.ButtonFace) < 4.5) warns.push("dialogs");
    return warns;
  });

  onMount(async () => {
    try { settings = await getSettings(); } catch (_) {}
    await loadVersions();

    // Resolve WHERE we're working before asking anything about it.
    try {
      const wp = await workingPrefix();
      prefix = wp.prefix;
      prefixValid = wp.valid;
      defaultNewPrefix = wp.default_new ?? "";
      if (!mhTarget) mhTarget = defaultNewPrefix;
      await loadPrefixes();
    } catch (e) {
      error = String(e);
      prefixValid = false;
    }

    // Only load the library if there's a prefix to load it from; otherwise the first-run
    // panel takes over and a failed refresh would just add a confusing error on top of it.
    if (prefixValid) await refresh();
  });

  // --- prefix registry actions ---------------------------------------------------------
  async function loadPrefixes() {
    try {
      prefixes = (await listPrefixes())?.prefixes ?? [];
      discovered = (await discoverPrefixes())?.found ?? [];
    } catch (e) { prefixMsg = String(e); }
    loadFonts();   // not awaited: the rows render first, the verdicts fill in
  }

  // One `neutron fonts check` per valid prefix. Advisory, like doctor: a failure to check must
  // never take the Prefixes tab down with it, so a throw leaves that row's verdict unknown.
  async function loadFonts() {
    for (const p of prefixes) {
      if (!p.valid) continue;
      try { fonts[p.path] = await fontsCheck(p.path); }
      catch (e) { fonts[p.path] = null; }
    }
  }

  function fontsDot(v) {
    return v?.status === "good" ? "ok" : v?.status === "warn" ? "warn" : "bad";
  }

  // The per-check details, one per line, for the row's tooltip.
  function fontsTitle(v) {
    if (!v) return "";
    const lines = (v.checks ?? []).map((c) => `${c.status.toUpperCase()}  ${c.name}: ${c.detail}`);
    if (v.note) lines.push(`note: ${v.note}`);
    return lines.join("\n");
  }

  // Repair = `neutron fonts repair`. Seconds normally; up to a minute when the Microsoft core
  // fonts have to be fetched. The engine refuses while an Adobe app runs in that prefix, and
  // its reason is what we show.
  async function runFontsRepair(path) {
    repairingFonts = path; prefixMsg = null;
    try {
      const r = await fontsRepair(path);
      fonts[path] = r.after ?? (await fontsCheck(path));
      prefixMsg = r.ok
        ? `Fonts repaired in ${path} — apps read fonts at startup, so restart any that are open`
        : (r.reason ?? "Fonts are still not right after the repair");
      if (path === prefix) await refresh();   // the sidebar doctor lines include the font checks
    } catch (e) { prefixMsg = String(e); }
    finally { repairingFonts = null; }
  }

  async function recheckFonts(path) {
    try { fonts[path] = await fontsCheck(path); }
    catch (e) { prefixMsg = String(e); }
  }

  function openPrefixes() { view = "prefixes"; prefixMsg = null; loadPrefixes(); }

  // Register `dir` and, if nothing was selected, start using it. Used by the first-run panel,
  // the Prefixes tab, and after a Mud Hut install — which can land a prefix somewhere other
  // than the default, a pointer that used to be held in memory only and lost on restart.
  async function adoptPrefix(dir, name = null) {
    prefixMsg = null; error = null;
    try {
      await addPrefix(dir, name);
      await useprefix(dir);
    } catch (e) { prefixMsg = String(e); error = String(e); }
  }

  async function chooseExistingPrefix() {
    let dir;
    try {
      dir = await open({ directory: true, title: "Choose an existing Neutron wine prefix" });
    } catch (_) { return; }
    if (dir) await adoptPrefix(dir);
  }

  // Switch the whole app over to `path`.
  async function useprefix(path) {
    try {
      await selectPrefix(path);
      prefix = path;
      prefixValid = true;
      await loadPrefixes();
      await refresh();
    } catch (e) { prefixMsg = String(e); }
  }

  // Forget a prefix. Registry entry only — never the 100 GB of Adobe installs on disk.
  async function forgetPrefix(path) {
    try {
      await removePrefix(path);
      await loadPrefixes();
      const wp = await workingPrefix();
      prefix = wp.prefix; prefixValid = wp.valid;
      if (prefixValid) await refresh();
    } catch (e) { prefixMsg = String(e); }
  }

  function startRename(p) { renaming = p.path; renameText = p.name; }
  async function commitRename() {
    const path = renaming, name = renameText.trim();
    renaming = null;
    if (!path || !name) return;
    try { await renamePrefix(path, name); await loadPrefixes(); }
    catch (e) { prefixMsg = String(e); }
  }

  // Minutes-long — wineboot is the bulk of it — so show the engine's own progress stream rather
  // than a spinner that's indistinguishable from a hang.
  async function runProvision(path) {
    provisioning = path; prefixMsg = null;
    provProgress = { pct: 0, msg: "starting…" };
    try {
      await provisionPrefix(path, (ev) => {
        if (ev.event === "progress") {
          if (typeof ev.pct === "number") provProgress.pct = ev.pct;
          if (ev.msg) provProgress.msg = ev.msg;
        } else if (ev.event === "note" && ev.msg) {
          provProgress.msg = ev.msg;
        }
      });
      prefixMsg = `Provisioned ${path}`;
      await loadPrefixes();
      if (path === prefix) await refresh();
    } catch (e) { prefixMsg = String(e); }
    finally { provisioning = null; provProgress = { pct: 0, msg: "" }; }
  }

  async function openPreferences() {
    view = "preferences";
    loadVersions();
    try { detectedScale = await detectScale(); } catch (_) { detectedScale = null; }
    try { themePresets = await getThemePresets(); } catch (_) { themePresets = []; }
    try { iconSets = await getIconSets(); } catch (_) { iconSets = []; }
  }

  // The scale dropdown: "auto" or one of the steps. A manual value from an older Collider that is
  // not a step stays selectable rather than silently changing.
  function setScale(v) {
    if (v === "auto") settings.scale_mode = "auto";
    else { settings.scale_mode = "manual"; settings.scale_value = Number(v); }
    saveSettings();
  }

  // Persist on any change. "auto unless overridden": engine uses scale_value only
  // when scale_mode === "manual".
  async function saveSettings() {
    try {
      await setSettings({
        scale_mode: settings.scale_mode,
        scale_value: settings.scale_value,
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
      // Health is ADVISORY — never let it block the library. A build tester's Apps view rendered
      // completely empty because doctor threw here and listApps below never ran (2026-08-08).
      // Whatever is wrong with a prefix, the user should still see and be able to launch the apps
      // that are installed in it.
      try { health = await doctor(prefix); } catch (e) { health = null; }
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
  <!-- Title strip: not a bar of its own, just the empty top of the page. The bare attribute makes
       the strip itself (not its children) the drag region; the buttons are drawn here so they
       match the rest of the UI. -->
  <div class="titlebar" data-tauri-drag-region>
    <div class="win-controls">
      <button class="win-btn" title="Minimize" aria-label="Minimize" onclick={() => win.minimize()}>
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5.5h8" /></svg>
      </button>
      <button class="win-btn" title={maximized ? "Restore" : "Maximize"}
              aria-label={maximized ? "Restore" : "Maximize"} onclick={() => win.toggleMaximize()}>
        {#if maximized}
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M3.5 3.5v-2h5v5h-2" /><rect x="1.5" y="3.5" width="5" height="5" /></svg>
        {:else}
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="1.5" y="1.5" width="7" height="7" /></svg>
        {/if}
      </button>
      <button class="win-btn close" title="Close" aria-label="Close" onclick={() => win.close()}>
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M2 2l6 6M8 2l-6 6" /></svg>
      </button>
    </div>
  </div>
  {#if !maximized}
    {#each EDGES as [cls, dir]}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resize-handle {cls}" onmousedown={resizeFrom(dir)}></div>
    {/each}
  {/if}

  <div class="shell">
  <aside>
    <div class="brand">
      <div class="logo" aria-hidden="true"></div>
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
      <div class="nav-item" class:active={view === "prefixes"} role="button" tabindex="0"
           onclick={openPrefixes}>Prefixes</div>
      <div class="nav-item" class:active={view === "preferences"} role="button" tabindex="0"
           onclick={openPreferences}>Preferences</div>
    </nav>

    <div class="sys">
      <div class="nav-section">System</div>
      {#if health}
        <div class="sys-list">
          {#each health.checks as c}
            <div class="sys-row" title={c.detail ?? ""}>
              <span class="dot {!c.ok ? 'bad' : c.warning ? 'warn' : 'ok'}"></span>
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
          {#if prefixValid}
            {prefixes.find((p) => p.selected)?.name ?? "Neutron prefix"} · {prefix}
            {#if info}· {info.valid ? "healthy" : "invalid"}{/if}
          {:else}
            No prefix selected
          {/if}
        </div>
      </div>
      <button class="ghost" onclick={refresh} disabled={refreshing}>↻ Refresh</button>
    </header>

    <div class="scroll">
    {#if error}
      <div class="banner err">{error}</div>
    {/if}

    {#if vers && !neutronReady}
      <!-- First run: nothing set up yet. `neutron setup` downloads everything; the CLI itself is
           fetched first when it is missing (the Collider AppImage on its own). -->
      <section class="setup">
        <div class="setup-title">Set up Neutron</div>
        <div class="setup-body">
          Neutron downloads its Wine runtime and the Mud Hut installer from GitHub, Microsoft's
          Visual C++ runtimes, GDI+ and core fonts from Microsoft, and Adobe's Creative Cloud
          package from Adobe. You may be asked for your password once, to turn on ntsync, which
          makes the apps much faster.
        </div>
        {#if nrun.phase === "running"}
          <div class="install-stage">{nrun.stage}</div>
          <div class="install-msg">{nrun.msg}</div>
        {:else if nrun.phase === "error"}
          <div class="banner err">{nrun.error}</div>
        {/if}
        <div class="setup-actions">
          <button class="primary" onclick={() => runNeutron("setup")} disabled={nrun.phase === "running"}>
            {nrun.phase === "running" ? "Setting up…" : nrun.phase === "error" ? "Try again" : "Set up"}
          </button>
        </div>
      </section>
    {:else if !prefixValid}
      <!-- First run, or every registered prefix has gone missing. Showing an empty library here
           would be a dead end: it reads as "no apps installed" when the real problem is that
           Collider doesn't know where to look. -->
      <section class="setup">
        <div class="setup-title">No Neutron prefix yet</div>
        <div class="setup-body">
          Collider needs a wine prefix with Adobe apps in it. Point it at one you already have,
          or let Mud Hut build one at <code>{defaultNewPrefix}</code>.
        </div>
        <div class="setup-actions">
          <button class="primary" onclick={chooseExistingPrefix}>Choose existing…</button>
          <button class="ghost" onclick={openMudHut}>Set one up with Mud Hut</button>
        </div>
        {#if discovered.length}
          <div class="setup-found">
            <div class="setup-found-label">Found on this machine:</div>
            {#each discovered as d (d.path)}
              <button class="found-row" onclick={() => adoptPrefix(d.path, d.name)}>
                <span class="found-name">{d.name}</span>
                <span class="found-path">{d.path}</span>
              </button>
            {/each}
          </div>
        {/if}
      </section>
    {:else}
    <section class="grid">
      <!-- One widget per installed app, tinted to its brand color. -->
      {#each apps as app (app.id)}
        <AppCard {app} {prefix} {info} />
      {/each}
      {#if apps.length === 0}
        <div class="empty-apps">No apps detected in this prefix — hit ↻ Refresh.</div>
      {/if}
    </section>
    {/if}
    </div>
  {:else if view === "prefixes"}
    <header>
      <div>
        <div class="title">Prefixes</div>
        <div class="subtitle">
          Every prefix Collider knows about. The selected one is what the Apps tab launches into.
        </div>
      </div>
      <button class="ghost" onclick={chooseExistingPrefix}>+ Add existing…</button>
    </header>

    <div class="scroll">
    {#if prefixMsg}
      <div class="banner">{prefixMsg}</div>
    {/if}

    <section class="plist">
      {#each prefixes as p (p.path)}
        <div class="prow" class:sel={p.selected} class:bad={!p.valid}>
          <div class="pmain">
            {#if renaming === p.path}
              <!-- svelte-ignore a11y_autofocus -->
              <input class="pname-edit" bind:value={renameText} autofocus
                     onblur={commitRename}
                     onkeydown={(e) => { if (e.key === "Enter") commitRename();
                                         if (e.key === "Escape") renaming = null; }} />
            {:else}
              <div class="pname">
                {p.name}
                {#if p.selected}<span class="pill ok">in use</span>{/if}
                {#if !p.valid}<span class="pill bad">missing</span>{/if}
              </div>
            {/if}
            <div class="ppath">{p.path}</div>
            {#if provisioning === p.path}
              <div class="prov">
                <div class="prov-bar"><div class="prov-fill" style="width: {provProgress.pct}%"></div></div>
                <div class="prov-msg">{provProgress.pct}% · {provProgress.msg}</div>
              </div>
            {/if}
            <div class="papps">
              {#if p.apps.length}{p.apps.length} app{p.apps.length === 1 ? "" : "s"} ·
                {p.apps.join(", ")}
              {:else}no Adobe apps detected{/if}
            </div>
            {#if p.valid}
              <div class="pfonts" title={fontsTitle(fonts[p.path])}>
                <span class="dot {fonts[p.path] ? fontsDot(fonts[p.path]) : ''}"></span>
                <span class="pfonts-label">Fonts</span>
                <span class="pfonts-msg">
                  {#if fonts[p.path] === undefined}checking…
                  {:else if fonts[p.path] === null}could not check — see the Neutron CLI (`neutron fonts check`)
                  {:else}{fonts[p.path].summary}{/if}
                </span>
              </div>
            {/if}
          </div>
          <div class="pacts">
            {#if !p.selected && p.valid}
              <button class="primary sm" onclick={() => useprefix(p.path)}>Use</button>
            {/if}
            <button class="ghost sm" onclick={() => startRename(p)}>Rename</button>
            <button class="ghost sm" disabled={provisioning !== null || !p.valid}
                    title="Run the Neutron provisioning recipe (wineboot, registry, fonts, DXVK). Takes a few minutes."
                    onclick={() => runProvision(p.path)}>
              {provisioning === p.path ? "Provisioning…" : "Provision"}
            </button>
            {#if p.valid && fonts[p.path] && fonts[p.path].status !== "good"}
              <button class="ghost sm" disabled={repairingFonts !== null || provisioning !== null
                                                 || (fonts[p.path].apps_running?.length ?? 0) > 0}
                      title={(fonts[p.path].apps_running?.length ?? 0) > 0
                               ? `Close ${fonts[p.path].apps_running.join(", ")} first — apps read fonts at startup`
                               : "Repair: " + (fonts[p.path].repair ?? []).join("; ") + ". Then re-verify."}
                      onclick={() => runFontsRepair(p.path)}>
                {repairingFonts === p.path ? "Repairing…" : "Repair fonts"}
              </button>
            {:else if p.valid}
              <button class="ghost sm" disabled={repairingFonts !== null || provisioning !== null}
                      title="Re-run the font verification for this prefix (read-only)"
                      onclick={() => recheckFonts(p.path)}>Check fonts</button>
            {/if}
            <button class="ghost sm danger" onclick={() => forgetPrefix(p.path)}
                    title="Remove from this list. Does NOT delete the prefix on disk.">Forget</button>
          </div>
        </div>
      {/each}

      {#if prefixes.length === 0}
        <div class="empty-apps">No prefixes registered yet.</div>
      {/if}

      {#if discovered.length}
        <div class="disc">
          <div class="disc-label">Found on this machine, not yet added</div>
          {#each discovered as d (d.path)}
            <div class="prow ghost-row">
              <div class="pmain">
                <div class="pname">{d.name}</div>
                <div class="ppath">{d.path}</div>
              </div>
              <div class="pacts">
                <button class="primary sm" onclick={() => adoptPrefix(d.path, d.name)}>Add</button>
              </div>
            </div>
          {/each}
        </div>
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
        <!-- Method picker. None of the three needs an Adobe sign-in. -->
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
            <div class="method-d">Fetch genuine app files straight from Adobe. <b>No sign-in to install</b> — you sign in inside the app, the same as on Windows.</div>
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
              <div class="signin-d">Installed into {mhTarget}. It now appears in <b>Apps</b> — launch it there and sign in inside the app, the same as on Windows.</div>
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
              <div class="signin-d">Genuine Adobe files download straight from Adobe — no sign-in needed to install. You sign in inside the app, the same as on Windows.</div>
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
                  Point at an offline package — either a folder in the layout
                  <b>mudhut download</b> stages (per-app folders with <b>Application.json</b> +
                  payload files), or an <b>.iso</b> disc image, which is mounted and installed
                  from directly. No extracting, no copying, no Adobe sign-in needed.
                {/if}
              </div>
              <div class="mh-target">
                <label>{mhMethod === "windows" ? "Windows install root" : "Package folder or .iso"}</label>
                <div class="mh-source-row">
                  <input bind:value={mhSource} spellcheck="false"
                         placeholder={mhMethod === "windows" ? "/path/to/drive_c or mounted C:" : "/path/to/package/products or /path/to/disc.iso"}
                         onkeydown={(e) => { if (e.key === "Enter") scanSource(); }} />
                  <button class="ghost" onclick={browseSource}>Browse…</button>
                  {#if mhMethod === "offline"}
                    <button class="ghost" onclick={browseSourceIso}>Choose ISO…</button>
                  {/if}
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
            How large the app UI renders. <b>Auto</b> matches your desktop's scale. Best results:
            set the same scale in your desktop's display settings.
          </div>
          <div class="pref-row">
            <select class="scale-select"
                    value={settings.scale_mode === "manual" && settings.scale_value ? String(settings.scale_value) : "auto"}
                    onchange={(e) => setScale(e.currentTarget.value)}>
              <option value="auto">Auto (match desktop){detectedScale?.scale ? ` — ${pct(detectedScale.scale)}` : ""}</option>
              {#each SCALE_STEPS as st}
                <option value={String(st)}>{pct(st)}</option>
              {/each}
              {#if settings.scale_mode === "manual" && settings.scale_value && !SCALE_STEPS.includes(settings.scale_value)}
                <option value={String(settings.scale_value)}>{pct(settings.scale_value)} (not a Windows step)</option>
              {/if}
            </select>
          </div>
          {#if detectedScale?.warning && settings.scale_mode !== "manual"}
            <div class="pref-desc warn">⚠ {detectedScale.warning}</div>
          {/if}
        </div>

        <div class="pref-group">
          <div class="pref-label">Window decoration theme</div>
          <div class="pref-desc">
            Colors for the title bar, menu bar and window buttons Neutron draws around
            every app in the prefix, and for the dialogs Wine draws. Pick a preset or edit
            any color to make a custom theme.
            Applied to the prefix on launch — <b>restart the app to see changes</b>.
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
            <b>max</b>, <b>restore</b> images (.png or .ico). Restart the app to apply;
            the close button still highlights red on hover.
          </div>

          <div class="pref-row" style="margin-top:12px;">
            <button class="ghost-btn" onclick={resetToDark} disabled={settings.theme === "dark"}>
              Reset to Dark
            </button>
            <span class="muted">Close-button hover stays red on every theme (by design).</span>
          </div>
        </div>

        <div class="pref-group">
          <div class="pref-label">Neutron</div>
          <div class="pref-desc">
            {#if vers?.cli}
              CLI {vers.cli} · neutron-wine {vers.neutron_wine ?? "not installed"} ·
              Mud Hut {vers.mudhut ?? "not installed"} · Collider {vers.collider}
            {:else}
              The Neutron CLI is not installed. Use <b>Set up</b> on the Apps page.
            {/if}
          </div>
          {#if nrun.phase === "running" && nrun.kind === "update"}
            <div class="pref-desc">Updating · {nrun.stage} {nrun.msg}</div>
          {:else if nrun.phase === "done" && nrun.kind === "update"}
            <div class="pref-desc">Up to date. Prefixes were moved to the newest runtime.</div>
          {:else if nrun.phase === "error" && nrun.kind === "update"}
            <div class="pref-desc warn">⚠ {nrun.error}</div>
          {/if}
          {#if uninstalled}
            <div class="pref-desc">Neutron was uninstalled ({uninstalled.removed} items removed).
              {#if uninstalled.prefixes_not_removed?.length}Your prefixes were kept.{/if}</div>
          {/if}
          <div class="pref-row">
            <button class="ghost-btn" onclick={() => runNeutron("update")}
                    disabled={!vers?.cli || nrun.phase === "running"}>
              {nrun.phase === "running" && nrun.kind === "update" ? "Updating…" : "Update"}
            </button>
            <button class="ghost-btn" onclick={() => { confirmUninstall = true; uninstallPrefixes = false; }}
                    disabled={!vers?.cli || nrun.phase === "running"}>Uninstall…</button>
          </div>
          {#if confirmUninstall}
            <div class="confirm">
              <div>This removes Neutron's runtime, downloads, logs, menu entries and icons, file
                associations, the KWin script and rule, the neutron command, Mud Hut, and Collider
                itself with its settings. ntsync stays on.</div>
              <label class="radio">
                <input type="checkbox" bind:checked={uninstallPrefixes} />
                Also delete my prefixes (the Adobe apps and their settings)
              </label>
              <div class="pref-row">
                <button class="ghost-btn danger" onclick={runUninstall}>Uninstall</button>
                <button class="ghost-btn" onclick={() => (confirmUninstall = false)}>Cancel</button>
              </div>
            </div>
          {/if}
        </div>
      </section>
    </div>
  {/if}

    <footer>
      <span class="dot ok"></span>
      Ready · Neutron CLI connected
    </footer>
  </main>
  </div>
</div>

<style>
  .app { display: flex; flex-direction: column; height: 100vh; overflow: hidden; font-family: system-ui, sans-serif; color: #e8e8ec; }
  .shell { display: flex; flex: 1; min-height: 0; }

  /* Title strip + window controls (DECISIONS C34). The strip is the page's own background, no
     border, no title; the buttons follow .ghost (dim glyphs, 6px radius) and .dot.bad for close. */
  .titlebar { flex-shrink: 0; height: 36px; display: flex; align-items: center; justify-content: flex-end; padding: 0 8px; }
  .win-controls { display: flex; gap: 4px; }
  .win-btn { width: 28px; height: 24px; padding: 0; border: none; border-radius: 6px; background: transparent; color: rgba(255,255,255,0.6); display: flex; align-items: center; justify-content: center; cursor: default; transition: background .12s, color .12s; }
  .win-btn:hover { background: rgba(255,255,255,0.08); color: rgba(255,255,255,0.85); }
  .win-btn:active { background: rgba(255,255,255,0.14); }
  .win-btn.close { background: rgba(226,75,74,0.18); color: #ff9b9b; }
  .win-btn.close:hover { background: rgba(226,75,74,0.32); color: #ffb3b3; }
  .win-btn.close:active { background: rgba(226,75,74,0.45); }
  .win-btn svg { fill: none; stroke: currentColor; stroke-width: 1; }
  /* Resize handles along the window edge; the corners win over the sides. */
  .resize-handle { position: fixed; z-index: 100; }
  .resize-handle.n { top: 0; left: 8px; right: 8px; height: 4px; cursor: n-resize; }
  .resize-handle.s { bottom: 0; left: 8px; right: 8px; height: 4px; cursor: s-resize; }
  .resize-handle.e { right: 0; top: 8px; bottom: 8px; width: 4px; cursor: e-resize; }
  .resize-handle.w { left: 0; top: 8px; bottom: 8px; width: 4px; cursor: w-resize; }
  .resize-handle.nw { top: 0; left: 0; width: 8px; height: 8px; cursor: nw-resize; }
  .resize-handle.ne { top: 0; right: 0; width: 8px; height: 8px; cursor: ne-resize; }
  .resize-handle.sw { bottom: 0; left: 0; width: 8px; height: 8px; cursor: sw-resize; }
  .resize-handle.se { bottom: 0; right: 0; width: 8px; height: 8px; cursor: se-resize; }

  aside { width: 200px; flex-shrink: 0; padding: 20px 14px; border-right: 1px solid rgba(255,255,255,0.06); display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
  .brand { display: flex; align-items: center; gap: 9px; padding: 0 6px 22px; }
  /* One copy per scale, each the exact pixel size it is drawn at. CSS picks it, not srcset:
     WebKit re-checks resolution media queries when the window moves to a monitor with another
     scale, but keeps the srcset choice it made at first paint. */
  .logo { width: 44px; height: 44px; display: block; flex-shrink: 0; background: url(/collider-logo-44.png) 0 0 / 44px 44px no-repeat; }
  @media (-webkit-min-device-pixel-ratio: 1.5), (min-resolution: 1.5dppx) { .logo { background-image: url(/collider-logo-88.png); } }
  @media (-webkit-min-device-pixel-ratio: 2.5), (min-resolution: 2.5dppx) { .logo { background-image: url(/collider-logo-132.png); } }
  @media (-webkit-min-device-pixel-ratio: 3.5), (min-resolution: 3.5dppx) { .logo { background-image: url(/collider-logo-176.png); } }
  .brand-name { font-size: 17px; font-weight: 700; }
  .brand-by { font-size: 9.5px; color: rgba(255,255,255,0.35); }
  .nav-section { flex-shrink: 0; font-size: 9.5px; text-transform: uppercase; letter-spacing: 1.2px; color: rgba(255,255,255,0.3); padding: 14px 8px 6px; }
  .nav-item { padding: 7px 10px; border-radius: 8px; font-size: 13px; color: rgba(255,255,255,0.6); cursor: pointer; }
  .nav-item.active { color: #fff; background: rgba(154,92,245,0.16); }
  .nav-item.disabled { color: rgba(255,255,255,0.25); cursor: default; }
  .nav-item.disabled:hover { background: transparent; }

  main { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
  header { flex-shrink: 0; padding: 16px 22px; border-bottom: 1px solid rgba(255,255,255,0.06); display: flex; align-items: center; justify-content: space-between; }
  .scroll { flex: 1; overflow-y: auto; min-height: 0; }
  .title { font-size: 16px; font-weight: 600; }
  .subtitle { font-size: 11.5px; color: rgba(255,255,255,0.4); margin-top: 2px; }

  /* appearance:none: WebKitGTK otherwise draws the closed select with the desktop's GTK theme and
     ignores this background, so a light theme put our light text on white (unreadable). */
  .scale-select { appearance: none; -webkit-appearance: none; color-scheme: dark; background: rgba(255,255,255,0.06) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='rgba(255,255,255,0.6)' stroke-width='1.5'/%3E%3C/svg%3E") no-repeat right 9px center; color: inherit; border: 1px solid rgba(255,255,255,0.12); border-radius: 6px; padding: 5px 26px 5px 8px; font-size: 12.5px; }
  .scale-select option { background: #1c1c1f; color: #e6e6e6; }
  .confirm { margin-top: 10px; padding: 12px 14px; border-radius: 8px; background: rgba(226,75,74,0.08); border: 1px solid rgba(226,75,74,0.25); font-size: 12.5px; display: flex; flex-direction: column; gap: 8px; }
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
  .dot.warn { background: #f0b232; }

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
  /* --- Prefixes tab ------------------------------------------------------------------ */
  .plist { display: flex; flex-direction: column; gap: 10px; }
  .prow { display: flex; align-items: center; gap: 14px; padding: 13px 15px; border-radius: 11px;
          border: 1px solid rgba(255,255,255,0.08); background: rgba(255,255,255,0.03); }
  .prow.sel { border-color: rgba(255,255,255,0.28); background: rgba(255,255,255,0.06); }
  .prow.bad { opacity: 0.6; }
  .prow.ghost-row { background: transparent; border-style: dashed; }
  .pmain { flex: 1; min-width: 0; }
  .pname { font-size: 13.5px; font-weight: 600; display: flex; align-items: center; gap: 8px; }
  .pname-edit { font-size: 13.5px; font-weight: 600; background: rgba(0,0,0,0.35); color: inherit;
                border: 1px solid rgba(255,255,255,0.25); border-radius: 6px; padding: 3px 7px; width: 260px; }
  .ppath { font-size: 11px; color: rgba(255,255,255,0.45); margin-top: 2px;
           overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .papps { font-size: 10.5px; color: rgba(255,255,255,0.32); margin-top: 3px;
           overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pfonts { display: flex; align-items: center; gap: 7px; margin-top: 5px; font-size: 10.5px;
            color: rgba(255,255,255,0.55); min-width: 0; }
  .pfonts-label { color: rgba(255,255,255,0.32); font-size: 9.5px; letter-spacing: 0.05em;
                  text-transform: uppercase; flex-shrink: 0; }
  .pfonts-msg { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pacts { display: flex; gap: 6px; flex-shrink: 0; }
  .pacts button { white-space: nowrap; }   /* five buttons on a row must not wrap "Repair fonts" */
  button.sm { padding: 5px 10px; font-size: 11.5px; }
  button.danger:hover:not(:disabled) { color: #ff6b6b; border-color: rgba(255,107,107,0.4); }
  .disc { margin-top: 18px; }
  .disc-label { font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.07em;
                color: rgba(255,255,255,0.3); margin-bottom: 8px; }

  /* --- first-run setup panel --------------------------------------------------------- */
  .setup { max-width: 560px; padding: 26px; border-radius: 14px;
           border: 1px solid rgba(255,255,255,0.1); background: rgba(255,255,255,0.03); }
  .setup-title { font-size: 16px; font-weight: 700; margin-bottom: 8px; }
  .setup-body { font-size: 12.5px; color: rgba(255,255,255,0.55); line-height: 1.55; }
  .setup-body code { background: rgba(0,0,0,0.35); padding: 1px 6px; border-radius: 5px; font-size: 11.5px; }
  .setup-actions { display: flex; gap: 9px; margin-top: 18px; }
  .setup-found { margin-top: 22px; border-top: 1px solid rgba(255,255,255,0.07); padding-top: 15px; }
  .setup-found-label { font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.07em;
                       color: rgba(255,255,255,0.3); margin-bottom: 8px; }
  .found-row { display: flex; flex-direction: column; align-items: flex-start; gap: 2px; width: 100%;
               text-align: left; padding: 9px 12px; margin-bottom: 6px; border-radius: 9px;
               border: 1px solid rgba(255,255,255,0.08); background: rgba(255,255,255,0.02);
               color: inherit; cursor: pointer; }
  .found-row:hover { background: rgba(255,255,255,0.06); }
  .found-name { font-size: 12.5px; font-weight: 600; }
  .found-path { font-size: 10.5px; color: rgba(255,255,255,0.42); }

  .prov { margin-top: 7px; }
  .prov-bar { height: 4px; border-radius: 3px; background: rgba(255,255,255,0.1); overflow: hidden; }
  .prov-fill { height: 100%; background: rgba(255,255,255,0.6); transition: width 0.35s ease; }
  .prov-msg { font-size: 10.5px; color: rgba(255,255,255,0.5); margin-top: 4px;
              overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
