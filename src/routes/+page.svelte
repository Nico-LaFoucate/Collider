<script>
  import { prefixInfo, doctor, listApps,
           getSettings, setSettings, settingsLocation, detectScale, getThemePresets, getIconSets,
           importIconSet, versions, neutronSetup, neutronUninstall, appimagePath,
           mudhutApps, installApp, workingPrefix,
           listPrefixes, discoverPrefixes, addPrefix, removePrefix, renamePrefix,
           selectPrefix, provisionPrefix, fontsCheck, fontsRepair } from "$lib/api.js";
  import AppCard from "$lib/AppCard.svelte";
  import { versionChoice, untestedOnlyChoice, UNTESTED_HELP } from "$lib/install-choice.js";
  import UpdateDialog from "$lib/UpdateDialog.svelte";
  import ReportDialog from "$lib/ReportDialog.svelte";
  import PluginsPanel from "$lib/PluginsPanel.svelte";
  import SupportPanel from "$lib/SupportPanel.svelte";
  import { upd, startUpdateChecks, stopUpdateChecks, setUpdateHooks, check, checkNow,
           runUpdate, runYearlyInstall, targetLabels, anyJob, outcomeFor } from "$lib/updates.svelte.js";
  import { updateAllPlan, hasVerifiedUpdate, prefixSummary, readyCount, activeUpdate, waitSentence,
           cardView, joinNames, installedLabel, targetLabel, upToSize, downloadDetail, prefixError,
           SPEECH_APPS, SPEECH_LABEL, SPEECH_HELP, rowKey } from "$lib/updates-logic.js";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, onDestroy } from "svelte";

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

  // --- close guard ---
  // Closing the window ends Collider, and with it the reading end of every running job's output.
  // `neutron` stops at its next write when that happens (it treats a closed pipe as "the reader
  // left", bin/neutron main()), so a Neutron update or a provision stops partway. Ask first.
  // Every way of closing (our button, the compositor's close, Alt+F4) arrives here: with a
  // listener registered, Tauri holds the close and this handler decides (`destroy()` needs
  // core:window:allow-destroy, granted in capabilities/default.json).
  let closeGuard = $state(null);   // null, or the busyJobs() list shown in the dialog
  onMount(() => {
    let unlisten = null;
    win.onCloseRequested((e) => {
      const jobs = busyJobs();
      if (jobs.length) { e.preventDefault(); closeGuard = jobs; }
    }).then((u) => (unlisten = u)).catch(() => {});
    return () => { if (unlisten) unlisten(); };
  });
  function closeAnyway() { closeGuard = null; win.destroy().catch(() => {}); }

  // What would be cut off by closing now, one plain sentence each.
  function busyJobs() {
    const jobs = [];
    for (const [p, j] of Object.entries(upd.jobs)) {
      const pname = prefixes.find((x) => x.path === p)?.name ?? p;
      if (j.kind === "install") {
        jobs.push(`${j.label} is installing in “${pname}”. If you close Collider now, the install can ` +
                  `stop partway, and you would need to start it again.`);
      } else {
        const names = joinNames(j.apps.map((id) => upd.prefixes[p]?.apps?.find((a) => a.id === id)?.name ?? id));
        jobs.push(`${names} ${j.apps.length === 1 ? "is" : "are"} updating in “${pname}”. If you close ` +
                  `Collider, the update keeps going, and Collider shows it again when you open it.`);
      }
    }
    if (mhInstall.phase === "installing") {
      jobs.push(`${mhInstall.name} is installing. If you close Collider now, the install can stop ` +
                `partway, and you would need to start it again.`);
    }
    if (nrun.phase === "running") {
      jobs.push(nrun.kind === "update"
        ? "Neutron is updating. If you close Collider now, the update stops partway."
        : "Neutron is being set up. If you close Collider now, setup stops partway.");
    }
    if (provisioning) {
      const name = prefixes.find((p) => p.path === provisioning)?.name ?? provisioning;
      jobs.push(`“${name}” is being provisioned. If you close Collider now, provisioning stops ` +
                `partway; run Provision again afterward.`);
    }
    return jobs;
  }
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
  let view = $state("apps");                    // "apps" | "plugins" | "prefixes" | "mudhut" | "preferences" | "support"

  // --- Mud Hut installer ---
  // Which install method the user picked in the Mud Hut tab. null = show the menu. No method
  // needs an Adobe sign-in; licensing happens inside the app on first launch.
  let mhMethod = $state(null);   // null | "windows" | "offline" | "download"

  // --- Mud Hut install wizard ---
  let mhCatalog = $state([]);       // installable apps from mudhutApps()
  // The catalog request: phase loading | done | error. Without it a failed `mudhut apps` left
  // "Loading catalog…" on screen for good.
  let mhCatalogLoad = $state({ phase: "loading", error: null });
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

  function openMudHut() { view = "mudhut"; mhMethod = null; mhConfirm = null; mhPlanning = null; resetInstall(); resetSource(); }
  function backToMethods() { mhMethod = null; mhConfirm = null; mhPlanning = null; resetInstall(); resetSource(); }
  function resetInstall() {
    mhInstall = { phase: "pick", app: null, name: null, stage: "", pct: 0, msg: "", error: null };
  }
  function resetSource() {
    mhSource = "";
    mhScan = { phase: "idle", apps: [], kind: null, error: null };
  }

  async function loadCatalog() {
    mhCatalogLoad = { phase: "loading", error: null };
    try {
      mhCatalog = (await mudhutApps())?.apps ?? [];
      mhCatalogLoad = { phase: "done", error: null };
    } catch (e) {
      mhCatalog = [];
      mhCatalogLoad = { phase: "error", error: String(e) };
    }
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

  // The speech-to-text languages (Premiere Pro and Media Encoder, about 14.5 GB) are a choice,
  // off by default as in Creative Cloud (DECISIONS C70), so those downloads stop at a confirm
  // step first.
  // A download install also offers a choice of version when Adobe has a newer build than the newest
  // one Neutron has tested (his call, 2026-10-24): the tested one is the default, the newer one is
  // the user's informed choice. Mud Hut's plan (a dry run, nothing downloaded) says which applies.
  let mhConfirm = $state(null);      // the catalog app awaiting a choice, with `choice` (install-choice.js)
  let mhSpeech = $state(false);
  let mhVersion = $state("tested");  // "tested" | "newest"
  let mhPlanning = $state(null);     // the app whose plan is being fetched
  async function pickInstall(app) {
    if (mhMethod !== "download") { startInstall(app); return; }
    mhPlanning = app;
    let choice = null;
    let refusal = null;   // Mud Hut's error event, if the plan fails
    try {
      choice = versionChoice(await installApp({ app: app.id, method: "download", prefix: mhTarget, dryRun: true,
                                                onEvent: (ev) => { if (ev.event === "error") refusal = ev; } }));
    } catch {
      // Only Mud Hut's "nothing tested is offered" changes the flow. Any other failure (an older
      // Mud Hut without the plan's fields, a network hiccup) installs as before; the install
      // itself reports a real problem in Mud Hut's words.
      choice = untestedOnlyChoice(refusal);
    }
    if (mhPlanning !== app) return;        // the user left the screen meanwhile
    mhPlanning = null;
    if (choice || SPEECH_APPS.includes(app.id)) {
      mhConfirm = { ...app, choice };
      mhSpeech = false;
      mhVersion = choice?.tested ? "tested" : "newest";
    } else startInstall(app);
  }

  async function startInstall(app, { speechPacks = false, allowUnverified = false } = {}) {
    mhConfirm = null;
    const method = mhMethod;
    // download resolves from Adobe; windows/offline install from the scanned source.
    const source = method === "download" ? null : mhSource.trim();
    mhInstall = { phase: "installing", app: app.id, name: app.name, stage: "starting", pct: 0, msg: "", error: null };
    try {
      const r = await installApp({
        app: app.id, method, prefix: mhTarget, source, speechPacks, allowUnverified,
        onEvent: (ev) => {
          if (ev.event === "progress") {
            if (ev.stage) mhInstall.stage = ev.stage;
            if (typeof ev.pct === "number") mhInstall.pct = ev.pct;
            if (ev.msg) mhInstall.msg = ev.msg;
            mhInstall.detail = downloadDetail(ev);
          } else if (ev.event === "note" && ev.msg) {
            mhInstall.msg = ev.msg;
          }
        },
      });
      // A run can end with its result AND a non-zero exit (`exit` is then set on it). `ok:false`
      // is a failure in Mud Hut's own words; anything else installed, possibly with a note.
      if (r?.ok === false) {
        mhInstall = { ...mhInstall, phase: "error",
                      error: r.message ?? r.reason ?? "The install did not finish." };
        return;
      }
      mhInstall = { ...mhInstall, phase: "done",
                    note: r?.exit ? (r.message ?? r.reason ?? null) : null };
      // Point the Apps view at the freshly-installed prefix AND persist it — an install into a
      // non-default location used to be forgotten on restart, leaving the library empty.
      await adoptPrefix(mhTarget);
    } catch (e) {
      mhInstall = { ...mhInstall, phase: "error", error: String(e) };
    }
  }

  // Where the settings live (core/xdg.rs): ~/.config/collider unless XDG_CONFIG_HOME moves it.
  let settingsDir = $state(null);
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
  // phase: idle | running | done | error; kind: setup | update; result: the engine's result
  // ({ ok, steps: [{ step, ok, detail? }] }) once it has finished.
  let nrun = $state({ phase: "idle", kind: null, stage: "", msg: "", error: null, result: null, prereqs: null });
  // After a first setup from a downloaded AppImage: setup put Collider in its own folder and the app menu,
  // so the downloaded copy can go.
  let downloadedCopy = $state(null);
  // `neutron update` succeeds even when it skips a prefix with an app running in it; its
  // `prefixes` step then names the skipped prefixes in `detail`.
  const updatePrefixesNote = $derived(
    nrun.result?.steps?.find((st) => st.step === "prefixes")?.detail ?? null);
  let confirmUninstall = $state(false);
  let uninstallPrefixes = $state(false);
  let uninstalled = $state(null);       // the uninstall result, once it has run

  async function loadVersions() {
    try { vers = await versions(); } catch (_) { vers = { cli: null }; }
  }

  // Set up (first run) or Update: the engine does the work and streams its progress.
  // installPrereqs: the user clicked "Install and continue" under the packages a first run reported missing
  // (the CLI's `prereqs` plan, shown with its command); setup installs them through the password dialog.
  async function runNeutron(kind, installPrereqs = false) {
    nrun = { phase: "running", kind, stage: "starting", msg: "", error: null, result: null, prereqs: null };
    try {
      const result = await neutronSetup(kind === "update", (ev) => {
        if (ev.event === "progress") {
          if (ev.stage) nrun.stage = ev.stage;
          if (ev.msg) nrun.msg = ev.msg;
        } else if (ev.event === "note" && ev.msg) {
          nrun.msg = ev.msg;
        } else if (ev.ok === false && ev.prereqs?.missing?.length) {
          nrun.prereqs = ev.prereqs;
        }
      }, installPrereqs);
      nrun = { ...nrun, phase: "done", result };
      if (kind === "setup") {
        const installed = result?.steps?.find((st) => st.step === "collider")?.path;
        const me = await appimagePath().catch(() => null);
        if (installed && me && me !== installed) downloadedCopy = me;
      }
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
    try { settingsDir = await settingsLocation(); } catch (_) {}
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

    // Adobe app updates: after the library, so the check never delays it. Every registered,
    // valid prefix, passed explicitly, read fresh on each tick (a prefix added later is
    // included). The setting is read on every tick too.
    setUpdateHooks({
      // An update finished (ours, or one left running by an earlier Collider): its apps changed.
      prefixChanged: async (p) => { await loadPrefixes(); if (p === prefix) await refresh(); },
    });
    startUpdateChecks(validPrefixPaths, () => settings.adobe_update_check !== false);
  });
  onDestroy(stopUpdateChecks);

  const validPrefixPaths = () => prefixes.filter((p) => p.valid).map((p) => p.path);

  // --- Adobe app updates: the dialog and its entry points --------------------------------------
  // Every entry point (card, Apps header, Mud Hut's Updates view, the Prefixes line) builds a
  // request and opens the ONE dialog; confirming runs Mud Hut through the shared store.
  let udReq = $state(null);
  let reportOpen = $state(false);   // "Copy details" (ReportDialog): `neutron report` for the selected prefix
  const prefixName = (p) => prefixes.find((x) => x.path === p)?.name ?? p;
  const neutronBusy = $derived(nrun.phase === "running");

  /** Open the dialog for `action` on `row` in prefix `p`:
   *  update | unverified | unverified-newer | yearly | finish | all. */
  function openUpdate(p, action, row = null) {
    const apps = upd.prefixes[p]?.apps ?? [];
    if (action !== "all" && !row) return;   // Mud Hut no longer reports that app here
    if (action === "all") {
      const plan = updateAllPlan(apps);
      if (!plan.include.length) return;
      udReq = { kind: "all", prefix: p, rows: plan.include, excluded: plan.exclude };
    } else if (action === "update") {
      const related = (row.related ?? []).map((id) => apps.find((a) => a.id === id))
        .filter((r) => r && hasVerifiedUpdate(r));
      udReq = { kind: "update", prefix: p, row, related };
    } else if (action === "unverified") {
      udReq = { kind: "unverified", prefix: p, row, target: row.update?.verified === true ? row.newer_unverified : row.update };
    } else if (action === "unverified-newer") {
      udReq = { kind: "unverified", prefix: p, row, target: row.newer_unverified };
    } else if (action === "yearly" || action === "finish") {
      udReq = { kind: action, prefix: p, row };
    }
  }

  function onUpdateConfirm(a) {
    const req = udReq;
    udReq = null;
    if (!req || !a) return;
    if (a.op === "install") {
      runYearlyInstall({ prefix: req.prefix, app: a.app, name: req.row.name, version: a.version, label: a.label,
                         allowUnverified: a.allowUnverified, removePrevious: a.removePrevious,
                         speechPacks: a.speechPacks });
      return;
    }
    const rows = req.kind === "all" ? req.rows
      : [req.row, ...(req.related ?? []).filter((r) => a.apps.includes(r.id))];
    runUpdate({ prefix: req.prefix, apps: a.apps, to: a.to, allowUnverified: a.allowUnverified,
                noSnapshot: settings.adobe_update_snapshot === false,
                labels: targetLabels(rows, req.kind === "unverified" ? req.target : null) });
  }

  // The Apps header's "Update all": the selected prefix's verified point updates, when there are
  // two or more (one is the card's own button) and nothing already holds the prefix.
  const allPlan = $derived(prefix ? updateAllPlan(upd.prefixes[prefix]?.apps) : { include: [], exclude: [] });
  const prefixActive = (p) => activeUpdate(upd.jobs[p], upd.prefixes[p]);
  // The nav count: verified point updates ready across every registered prefix.
  const readyN = $derived(upd.supported ? readyCount(upd.prefixes) : 0);
  // Mud Hut's own per-prefix sentence on the Prefixes tab.
  const summaryFor = (p) => prefixSummary({
    pstate: upd.prefixes[p], job: upd.jobs[p],
    outcomeOf: (r) => outcomeFor(p, r.id, r),
  });
  function onSummaryAction(p, act) {
    if (!act) return;
    if (act.kind === "view") { openUpdates(); return; }
    const row = act.id ? upd.prefixes[p]?.apps?.find((a) => a.id === act.id) : null;
    openUpdate(p, act.kind, row);
  }
  function openUpdates() { view = "mudhut"; mhMethod = "updates"; mhConfirm = null; resetInstall(); resetSource(); }

  // "Last checked Oct 16, 2:02 PM".
  function when(iso) {
    if (!iso) return null;
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
  }
  // A card closed its app: what is open in that prefix changed, so re-read it (locally).
  const onAppExit = () => { if (prefix) check([prefix], { offline: true, partial: true, skipIfBusy: true }); };

  // One app's line in Mud Hut's Updates view (Mud Hut's facts, in words).
  function rowStatus(row) {
    const u = row.update;
    if (u && u.verified === true) {
      const size = upToSize(u.download_bytes);   // an upper bound (see upToSize)
      return `Update to ${targetLabel(u)}${size ? ` · ${size}` : ""}`;
    }
    if (u) return `${targetLabel(u)} is available · not yet tested by Neutron`;
    return "Up to date";
  }
  // What else a row offers besides its main button: a newer build Neutron has not verified, and a
  // new yearly version.
  function rowExtras(row) {
    const out = [];
    if (row.newer_unverified && row.update?.to !== row.newer_unverified.to) {
      out.push({ text: `Also available: ${targetLabel(row.newer_unverified)}, not yet tested by Neutron`,
                 label: `Update to ${targetLabel(row.newer_unverified)}…`, action: "unverified-newer" });
    }
    if (row.new_major && row.update) {
      out.push({ text: `${row.name} ${row.new_major.label ?? row.new_major.year} is available, a new yearly version`,
                 label: `Install ${row.new_major.label ?? row.new_major.year}…`, action: "yearly" });
    } else if (row.new_major) {
      out.push({ text: `${row.name} ${row.new_major.label ?? row.new_major.year} is available, a new yearly version`,
                 label: null, action: null });
    }
    return out;
  }
  const lockFor = (p) => {
    const act = prefixActive(p);
    return act ? waitSentence(act, upd.prefixes[p]) : neutronBusy ? "Wait for the Neutron update to finish." : null;
  };

  async function setUpdateSetting(key, value) {
    settings[key] = value;
    await saveSettings();
    // Turned back on: check now rather than at the next tick (Mud Hut's cache still decides
    // whether Adobe is contacted).
    if (key === "adobe_update_check" && value) check(validPrefixPaths(), { skipIfBusy: true });
  }

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
  // Every Preferences field goes, so a new setting needs no entry here (the old explicit list
  // reset any field it did not name). The backend merges onto the stored settings, and the
  // prefix registry is the Prefixes tab's, so this page's possibly stale copy is left out.
  async function saveSettings() {
    try {
      const { prefixes: _registry, selected_prefix: _selected, ...prefs } = settings;
      await setSettings(prefs);
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
      // Re-read what Mud Hut says about this prefix too (locally, Adobe is not contacted): which
      // apps are open decides whether Update is offered, and an app closed outside Collider
      // would otherwise keep "Close … first" until the next scheduled check.
      check([prefix], { offline: true, partial: true, skipIfBusy: true });
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
      <div class="nav-item" class:active={view === "plugins"} role="button" tabindex="0"
           onclick={() => (view = "plugins")}>Plugins</div>
      <div class="nav-section">Tools</div>
      <div class="nav-item" class:active={view === "mudhut"} role="button" tabindex="0"
           onclick={openMudHut}>Mud Hut
        {#if readyN}<span class="nav-count" title="Adobe app updates ready to install">{readyN}</span>{/if}
      </div>
      <div class="nav-item" class:active={view === "prefixes"} role="button" tabindex="0"
           onclick={openPrefixes}>Prefixes</div>
      <div class="nav-item" class:active={view === "preferences"} role="button" tabindex="0"
           onclick={openPreferences}>Preferences</div>
      <div class="nav-item" class:active={view === "support"} role="button" tabindex="0"
           onclick={() => (view = "support")}>Support</div>
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
      <button class="sys-btn" onclick={() => (reportOpen = true)}
              title="Everything a bug report needs, with your personal details replaced">Copy details</button>
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
            {#if info}· {info.valid ? "valid" : "invalid"}{/if}
          {:else}
            No prefix selected
          {/if}
        </div>
      </div>
      <div class="head-actions">
        {#if prefixValid && allPlan.include.length > 1 && !prefixActive(prefix)}
          <button class="ghost upd-all" disabled={neutronBusy}
                  title={neutronBusy ? "Wait for the Neutron update to finish." : "Update every app that has a tested update"}
                  onclick={() => openUpdate(prefix, "all")}>Update all ({allPlan.include.length})</button>
        {/if}
        <button class="ghost" onclick={refresh} disabled={refreshing}>↻ Refresh</button>
      </div>
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
          Visual C++ runtimes and GDI+ from Microsoft, Microsoft's core fonts and d3dcompiler_47
          from public copies of Microsoft's original files (each checked against a pinned
          checksum), and Adobe's Creative Cloud package from Adobe. You may be asked for your
          password: to install two system packages Neutron needs, if they're missing, and to turn on
          ntsync, which makes the apps much faster.
        </div>
        {#if nrun.phase === "running"}
          <div class="install-stage">{nrun.stage}</div>
          <div class="install-msg">{nrun.msg}</div>
        {:else if nrun.phase === "error" && nrun.prereqs?.can_install}
          <div class="prereq">
            <div class="prereq-h">Neutron needs {nrun.prereqs.missing.join(" and ")} from your system</div>
            <div class="prereq-d">
              Collider can install {nrun.prereqs.missing.length > 1 ? "them" : "it"} now{nrun.prereqs.ntsync
                ? ", and turn on ntsync, which makes the apps much faster" : ""}. Your system will ask for your
              password once, then setup carries on. It runs:
            </div>
            <code class="prereq-cmd">{nrun.prereqs.command}</code>
          </div>
        {:else if nrun.phase === "error" && nrun.prereqs?.how}
          <div class="banner err">
            Neutron needs {nrun.prereqs.missing.join(" and ")} from your system. To finish: {nrun.prereqs.how}.
          </div>
        {:else if nrun.phase === "error"}
          <div class="banner err">{nrun.error}</div>
        {/if}
        <div class="setup-actions">
          {#if nrun.phase === "error" && nrun.prereqs?.can_install}
            <button class="primary" onclick={() => runNeutron("setup", true)}>Install and continue</button>
          {:else}
            <button class="primary" onclick={() => runNeutron("setup")} disabled={nrun.phase === "running"}>
              {nrun.phase === "running" ? "Setting up…" : nrun.phase === "error" ? "Try again" : "Set up"}
            </button>
          {/if}
        </div>
      </section>
    {:else if !prefixValid}
      <!-- First run, or every registered prefix has gone missing. Showing an empty library here
           would be a dead end: it reads as "no apps installed" when the real problem is that
           Collider doesn't know where to look. -->
      <section class="setup">
        <div class="setup-title">No Neutron prefix yet</div>
        {#if downloadedCopy}
          <div class="banner ok-note">
            Neutron is set up, and Collider is now in your app menu. You can delete the file you downloaded:
            <code>{downloadedCopy}</code>
          </div>
        {/if}
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
        <AppCard {app} {prefix} {info} {neutronBusy}
                 onupdate={(action, row) => openUpdate(prefix, action, row)} onexit={onAppExit} />
      {/each}
      {#if apps.length === 0}
        <div class="empty-apps">No apps detected in this prefix — hit ↻ Refresh.</div>
      {/if}
    </section>
    {/if}
    </div>
  {:else if view === "plugins"}
    <header>
      <div>
        <div class="title">Plugins</div>
        <div class="subtitle">
          {#if prefixValid}
            {prefixes.find((p) => p.selected)?.name ?? "Neutron prefix"} · {prefix}
          {:else}
            No prefix selected
          {/if}
        </div>
      </div>
    </header>
    <div class="scroll">
      {#if prefixValid}
        {#key prefix}<PluginsPanel {prefix} />{/key}
      {:else}
        <div class="banner">Select a prefix on the Prefixes tab first.</div>
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
        {@const sm = summaryFor(p.path)}
        {@const plock = prefixActive(p.path) ? waitSentence(prefixActive(p.path), upd.prefixes[p.path]) : null}
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
              {#if upd.supported}
                <div class="pfonts">
                  <span class="dot {sm.dot}"></span>
                  <span class="pfonts-label">Updates</span>
                  <span class="pfonts-msg">{sm.text}</span>
                </div>
              {/if}
            {/if}
          </div>
          <div class="pacts">
            {#if !p.selected && p.valid}
              <button class="primary sm" onclick={() => useprefix(p.path)}>Use</button>
            {/if}
            <button class="ghost sm" onclick={() => startRename(p)}>Rename</button>
            {#if p.valid && upd.supported && sm.action}
              <button class="ghost sm" disabled={neutronBusy && sm.action.kind !== "view"}
                      title={neutronBusy ? "Wait for the Neutron update to finish." : ""}
                      onclick={() => onSummaryAction(p.path, sm.action)}>
                {sm.action.kind === "all" ? "Update all" : sm.action.kind === "update" ? "Update"
                  : sm.action.kind === "finish" ? "Finish update" : "View updates"}
              </button>
            {/if}
            <button class="ghost sm" disabled={provisioning !== null || !p.valid || !!plock}
                    title={plock ?? "Run the Neutron provisioning recipe (wineboot, registry, fonts, DXVK). Takes a few minutes."}
                    onclick={() => runProvision(p.path)}>
              {provisioning === p.path ? "Provisioning…" : "Provision"}
            </button>
            {#if p.valid && fonts[p.path] && fonts[p.path].status !== "good"}
              <button class="ghost sm" disabled={repairingFonts !== null || provisioning !== null || !!plock
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
          {:else if mhMethod === "updates"}Update installed apps
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
          <button class="method-card" onclick={openUpdates}>
            <div class="method-h">Update installed apps{readyN ? ` (${readyN})` : ""}</div>
            <div class="method-d">See which of your Adobe apps have newer versions, and update them. <b>Nothing updates until you choose to.</b></div>
          </button>
        </section>
      {:else if mhMethod === "updates"}
        <!-- Mud Hut's check, prefix by prefix. Every fact and sentence here is Mud Hut's
             (`mudhut check-updates`); the buttons open the one update dialog. -->
        <section class="mudhut">
          <button class="link-back" onclick={backToMethods}>← Back to install options</button>
          <div class="updv">
            <div class="updv-head">
              {#if !upd.supported}
                <div class="updv-quiet">{upd.unsupported === "missing"
                  ? "Set up Neutron to get Adobe update checks."
                  : "Update Neutron to get Adobe update checks."}</div>
              {:else}
                <div class="updv-quiet">
                  {#if upd.refreshing}Checking with Adobe…
                  {:else if upd.lastReachedAt}Last checked {when(upd.lastReachedAt)}
                  {:else}Not checked with Adobe yet{/if}
                </div>
                <button class="ghost sm" disabled={upd.refreshing || !validPrefixPaths().length}
                        onclick={() => checkNow(validPrefixPaths())}>Check now</button>
              {/if}
            </div>
            {#if upd.supported && upd.lastNetwork && !upd.lastNetwork.online}
              <div class="updv-quiet">{upd.lastNetwork.feed_error ?? "Couldn't reach Adobe."} This shows what Mud Hut last knew.</div>
            {/if}
            {#if upd.supported && upd.failed}
              <div class="updv-quiet">Update check failed: {upd.failed}</div>
            {/if}

            {#if upd.supported}
              {#each prefixes.filter((x) => x.valid) as p (p.path)}
                {@const ps = upd.prefixes[p.path]}
                {@const act = prefixActive(p.path)}
                {@const plan = updateAllPlan(ps?.apps)}
                {@const lock = lockFor(p.path)}
                <div class="updv-prefix">
                  <div class="updv-pname">{p.name} <span class="updv-ppath">{p.path}</span></div>
                  {#if !ps}
                    <div class="updv-quiet">Not checked yet</div>
                  {:else if ps.ok === false}
                    <div class="updv-quiet">{prefixError(ps)}</div>
                  {:else}
                    {#if act && !act.ours}
                      <div class="updv-quiet">
                        An update is running{act.startedAt ? ` (started ${when(act.startedAt)})` : ""}.
                        Collider follows it until it finishes.
                        {#if act.log}Log: <span class="sel">{act.log}</span>{/if}
                      </div>
                    {/if}
                    {#each ps.apps ?? [] as row (rowKey(row))}
                      {@const cv = cardView({ row, id: row.id, name: row.name, pstate: ps, job: upd.jobs[p.path],
                                               outcome: outcomeFor(p.path, row.id, row), neutronBusy })}
                      <div class="updv-row">
                        <div class="updv-main">
                          <div class="updv-name">{row.name} <span class="updv-ver">{installedLabel(row)}</span>
                            {#if cv.outcomePill}<span class="updv-pill {cv.outcomePill.cls}">{cv.outcomePill.text}</span>{/if}
                          </div>
                          {#if cv.progress}
                            <div class="prov">
                              <div class="prov-bar"><div class="prov-fill" style="width: {cv.progress.pct ?? 100}%"></div></div>
                              <div class="prov-msg">{cv.progress.pct !== null ? `${cv.progress.pct}% · ` : ""}{cv.progress.text}</div>
                            </div>
                          {:else}
                            <div class="updv-status">{rowStatus(row)}</div>
                            {#each rowExtras(row) as x}
                              <div class="updv-status muted">{x.text}</div>
                            {/each}
                          {/if}
                          {#if cv.button?.reason && row.update?.can_update === false}
                            <div class="updv-note">{cv.button.reason}</div>
                          {/if}
                          {#if cv.note}<div class="updv-note {cv.noteCls}">{cv.note}</div>{/if}
                          {#if cv.log}<div class="updv-note">Log: <span class="sel">{cv.log}</span></div>{/if}
                        </div>
                        <div class="updv-acts">
                          {#if cv.button}
                            <button class="ghost sm" disabled={cv.button.disabled} title={cv.button.reason ?? ""}
                                    onclick={() => openUpdate(p.path, cv.button.action, row)}>{cv.button.label}</button>
                          {/if}
                          {#if !cv.progress}
                            {#each rowExtras(row) as x}
                              {#if x.label}
                                <button class="ghost sm" disabled={!!lock} title={lock ?? ""}
                                        onclick={() => openUpdate(p.path, x.action, row)}>{x.label}</button>
                              {/if}
                            {/each}
                          {/if}
                        </div>
                      </div>
                    {/each}
                    {#if !(ps.apps ?? []).length}
                      <div class="updv-quiet">No Adobe apps that Mud Hut can update were found here.</div>
                    {/if}
                    {#if plan.include.length > 1 && !act}
                      <div class="updv-all">
                        <button class="primary sm" disabled={!!lock} title={lock ?? ""}
                                onclick={() => openUpdate(p.path, "all")}>Update all in {p.name} ({plan.include.length})</button>
                      </div>
                    {/if}
                  {/if}
                </div>
              {/each}
              {#if !prefixes.some((x) => x.valid)}
                <div class="updv-quiet">No prefixes yet. Install an app first.</div>
              {/if}
            {/if}
          </div>
        </section>
      {:else}
        <!-- download / windows / offline — the install phases (progress / done /
             error) are shared; only the pick step differs per method.
             No sign-in here: downloads come from Adobe's public servers, and
             windows/offline install from local files. You sign in inside the app,
             the same as on Windows. -->
        <section class="mudhut">
          <button class="link-back" onclick={backToMethods}>← Back to install options</button>
          {#if mhInstall.phase === "installing"}
            <div class="signin-card">
              <div class="signin-h">Installing {mhInstall.name}…</div>
              <div class="install-stage">{mhInstall.stage || "starting"}</div>
              <div class="progress"><div class="bar" style="width: {mhInstall.pct}%"></div></div>
              <!-- Waiting for apps to close: Mud Hut's sentence names them, so it is shown whole. -->
              <div class="install-msg" class:install-wait={mhInstall.stage === "waiting"}>{mhInstall.msg}</div>
              {#if mhInstall.detail}<div class="install-msg">{mhInstall.detail}</div>{/if}
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
              {#if mhInstall.note}<div class="signin-d warn">{mhInstall.note}</div>{/if}
              <button class="primary" onclick={() => { view = "apps"; }}>Go to Apps</button>
              <button class="ghost" onclick={resetInstall}>Install another</button>
            </div>
          {:else if mhInstall.phase === "error"}
            <div class="signin-card">
              <div class="banner err">{mhInstall.error}</div>
              <button class="primary" onclick={resetInstall}>Back to apps</button>
            </div>
          {:else if mhPlanning}
            <div class="signin-card">
              <div class="signin-h">Install {mhPlanning.name}</div>
              <div class="signin-d">Checking which versions Adobe offers…</div>
              <button class="ghost" onclick={() => (mhPlanning = null)}>Back</button>
            </div>
          {:else if mhConfirm}
            <div class="signin-card">
              <div class="signin-h">Install {mhConfirm.name}</div>
              {#if mhConfirm.choice}
                {#if mhConfirm.choice.tested}
                  <label class="speech-opt">
                    <input type="radio" bind:group={mhVersion} value="tested" />
                    <span>Version {mhConfirm.choice.tested.label}
                      <span class="speech-help">Tested by Neutron. Recommended.</span>
                    </span>
                  </label>
                {/if}
                <label class="speech-opt">
                  <input type="radio" bind:group={mhVersion} value="newest" />
                  <span>{mhConfirm.choice.newest.label ? `Version ${mhConfirm.choice.newest.label}` : "Adobe's newest version"}
                    <span class="speech-help">{UNTESTED_HELP}</span>
                  </span>
                </label>
              {/if}
              {#if SPEECH_APPS.includes(mhConfirm.id)}
                <label class="speech-opt">
                  <input type="checkbox" bind:checked={mhSpeech} />
                  <span>{SPEECH_LABEL}
                    <span class="speech-help">{SPEECH_HELP}</span>
                  </span>
                </label>
              {/if}
              <button class="primary" onclick={() => startInstall(mhConfirm, { speechPacks: mhSpeech, allowUnverified: mhVersion === "newest" })}>Install</button>
              <button class="ghost" onclick={() => (mhConfirm = null)}>Back</button>
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
                  <button class="mh-app" onclick={() => pickInstall(a)}>
                    <span class="mh-app-badge">{a.sap}</span>
                    <span class="mh-app-name">{a.name}</span>
                    <span class="mh-app-go">Install →</span>
                  </button>
                {/each}
                {#if mhCatalogLoad.phase === "error"}
                  <div class="banner err mh-banner">Could not load the app list: {mhCatalogLoad.error}</div>
                {:else if mhCatalogLoad.phase === "loading"}
                  <div class="signin-d">Loading catalog…</div>
                {/if}
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
  {:else if view === "support"}
    <header>
      <div>
        <div class="title">Support</div>
        <div class="subtitle">Help keep Neutron going</div>
      </div>
    </header>
    <div class="scroll"><SupportPanel /></div>
  {:else if view === "preferences"}
    <header>
      <div>
        <div class="title">Preferences</div>
        <div class="subtitle">Collider settings{settingsDir ? ` · stored in ${settingsDir}` : ""}</div>
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
          <div class="pref-label">Adobe app updates</div>
          <div class="pref-desc">
            While it is open, Collider checks with Adobe about once a day which versions of your
            Adobe apps are available. It sends no account or file information. Nothing installs
            until you choose to update.
          </div>
          {#if !upd.supported}
            <div class="pref-desc warn">{upd.unsupported === "missing"
              ? "Set up Neutron to get Adobe update checks."
              : "Update Neutron to get Adobe update checks."}</div>
          {:else}
            <label class="radio">
              <input type="checkbox" checked={settings.adobe_update_check !== false}
                     onchange={(e) => setUpdateSetting("adobe_update_check", e.currentTarget.checked)} />
              Check for Adobe app updates
            </label>
            <div class="pref-row" style="margin-top:10px;">
              <span class="muted pref-small">
                {#if upd.refreshing}Checking with Adobe…
                {:else if upd.lastReachedAt}Last checked {when(upd.lastReachedAt)}
                {:else}Not checked with Adobe yet{/if}
              </span>
              <button class="ghost-btn" disabled={upd.refreshing || !validPrefixPaths().length}
                      onclick={() => checkNow(validPrefixPaths())}>Check now</button>
            </div>
            {#if upd.lastNetwork && !upd.lastNetwork.online}
              <div class="pref-desc muted" style="margin:8px 0 0;">{upd.lastNetwork.feed_error ?? "Couldn't reach Adobe."}
                Collider shows what Mud Hut last knew.</div>
            {/if}
            {#if upd.failed}
              <div class="pref-desc muted" style="margin:8px 0 0;">Update check failed: {upd.failed}</div>
            {/if}
          {/if}
          <label class="radio" style="margin-top:14px;">
            <input type="checkbox" checked={settings.adobe_update_snapshot !== false}
                   onchange={(e) => setUpdateSetting("adobe_update_snapshot", e.currentTarget.checked)} />
            Keep a backup copy before updating (uses disk space)
          </label>
          <div class="pref-desc muted" style="margin:6px 0 0;">
            If an update fails, the backup copy is put back automatically. Neutron recommends the
            versions it has tested; newer ones show as “not yet tested”, and you can still
            update to them.
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
            {#if updatePrefixesNote}
              <div class="pref-desc warn">Update finished. Prefixes: {updatePrefixesNote}</div>
            {:else}
              <div class="pref-desc">Up to date. Prefixes were moved to the newest runtime.</div>
            {/if}
          {:else if nrun.phase === "error" && nrun.kind === "update"}
            <div class="pref-desc warn">⚠ {nrun.error}</div>
          {/if}
          {#if uninstalled}
            <div class="pref-desc">Neutron was uninstalled ({uninstalled.removed} items removed).
              {#if uninstalled.prefixes_not_removed?.length}Your prefixes were kept.{/if}</div>
          {/if}
          <div class="pref-row">
            <!-- "Update Neutron", not "Update": Adobe app updates have their own buttons. The two
                 never run together here (Neutron's update re-provisions prefixes); Mud Hut's prefix
                 lock is what keeps them apart for real. -->
            <button class="ghost-btn" onclick={() => runNeutron("update")}
                    disabled={!vers?.cli || nrun.phase === "running" || anyJob()}
                    title={anyJob() ? "Wait for the Adobe app update to finish." : ""}>
              {nrun.phase === "running" && nrun.kind === "update" ? "Updating…" : "Update Neutron"}
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
      {#if vers?.cli}
        <span class="dot ok"></span>
        Neutron CLI {vers.cli}
      {:else if vers}
        <span class="dot bad"></span>
        Neutron CLI not installed
      {/if}
    </footer>
  </main>
  </div>

  {#if udReq}
    <UpdateDialog req={udReq} prefixName={prefixName(udReq.prefix)}
                  snapshot={settings.adobe_update_snapshot !== false}
                  onconfirm={onUpdateConfirm} oncancel={() => (udReq = null)} />
  {/if}

  {#if reportOpen}
    <ReportDialog prefix={prefixValid ? prefix : null} onclose={() => (reportOpen = false)} />
  {/if}

  {#if closeGuard}
    <div class="modal-back">
      <div class="modal" role="alertdialog" aria-labelledby="close-guard-h">
        <div class="modal-h" id="close-guard-h">Collider is still working</div>
        {#each closeGuard as line}
          <div class="modal-p">{line}</div>
        {/each}
        <div class="modal-actions">
          <button class="primary" onclick={() => (closeGuard = null)}>Keep Collider open</button>
          <button class="ghost-btn danger" onclick={closeAnyway}>Close anyway</button>
        </div>
      </div>
    </div>
  {/if}
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
  .banner.ok-note { margin: 0; background: rgba(46,182,115,0.12); color: #9fe3bd; border: 1px solid rgba(46,182,115,0.3);
                    line-height: 1.5; }
  .banner.ok-note code { overflow-wrap: anywhere; }   /* the path may break anywhere; the sentence only between words */
  .prereq { width: 100%; display: flex; flex-direction: column; gap: 8px; text-align: left; padding: 12px 14px;
            border-radius: 10px; background: rgba(239,159,39,0.08); border: 1px solid rgba(239,159,39,0.28); }
  .prereq-h { font-size: 13px; font-weight: 600; color: #f2c374; }
  .prereq-d { font-size: 12px; line-height: 1.5; color: rgba(255,255,255,0.65); }
  .prereq-cmd { display: block; padding: 7px 10px; border-radius: 7px; background: rgba(0,0,0,0.35);
                font-size: 11.5px; color: #e8e8ec; word-break: break-all; user-select: text; }

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
  .sys-btn { margin: 8px 10px 0; padding: 6px 10px; border-radius: 7px; border: 1px solid rgba(255,255,255,0.14);
             background: rgba(255,255,255,0.04); color: rgba(255,255,255,0.75); font-size: 11.5px; text-align: left;
             cursor: pointer; flex-shrink: 0; }
  .sys-btn:hover { background: rgba(255,255,255,0.08); color: #f0f0f2; }
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
  .install-msg.install-wait { white-space: normal; color: #f2c374; line-height: 1.45; }
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

  /* --- Adobe app updates ------------------------------------------------------------- */
  .head-actions { display: flex; gap: 8px; align-items: center; }
  .ghost.upd-all { border-color: rgba(239,159,39,0.35); color: #f2c374; background: rgba(239,159,39,0.08); }
  .ghost:disabled { opacity: 0.45; cursor: default; }
  .nav-count { display: inline-block; min-width: 16px; margin-left: 6px; padding: 0 5px; border-radius: 9px;
               font-size: 10px; line-height: 16px; text-align: center; font-weight: 600;
               background: rgba(239,159,39,0.2); color: #f2c374; }
  .updv { max-width: 640px; margin: 0 auto; display: flex; flex-direction: column; gap: 12px; }
  .updv-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .updv-quiet { font-size: 11.5px; color: rgba(255,255,255,0.45); line-height: 1.5; }
  .updv-prefix { padding: 13px 15px; border-radius: 11px; border: 1px solid rgba(255,255,255,0.08);
                 background: rgba(255,255,255,0.03); display: flex; flex-direction: column; gap: 8px; }
  .updv-pname { font-size: 13.5px; font-weight: 600; }
  .updv-ppath { font-size: 11px; font-weight: 400; color: rgba(255,255,255,0.4); margin-left: 6px; }
  .updv-row { display: flex; align-items: center; gap: 12px; padding: 8px 0;
              border-top: 1px solid rgba(255,255,255,0.05); }
  .updv-main { flex: 1; min-width: 0; }
  .updv-name { font-size: 12.5px; font-weight: 600; }
  .updv-ver { font-weight: 400; color: rgba(255,255,255,0.4); margin-left: 4px; font-size: 11.5px; }
  .updv-status { font-size: 11.5px; color: rgba(255,255,255,0.65); margin-top: 2px; }
  .updv-pill { font-size: 10px; font-weight: 400; padding: 2px 7px; margin-left: 6px; border-radius: 20px;
               background: rgba(255,255,255,0.06); color: rgba(255,255,255,0.5); border: 1px solid rgba(255,255,255,0.08); }
  .updv-pill.ok { background: rgba(60,200,140,0.12); color: #6fe3b0; border-color: rgba(60,200,140,0.25); }
  .updv-pill.warn { background: rgba(239,159,39,0.12); color: #f2c374; border-color: rgba(239,159,39,0.25); }
  .updv-pill.bad { background: rgba(226,75,74,0.12); color: #ff9b9b; border-color: rgba(226,75,74,0.25); }
  .updv-status.muted { color: rgba(255,255,255,0.4); }
  .updv-note { font-size: 11px; color: rgba(255,255,255,0.5); margin-top: 3px; line-height: 1.45; }
  .updv-note.warn { color: #f2c374; }
  .updv-note.bad { color: #ff9b9b; }
  .updv-acts { display: flex; flex-direction: column; gap: 6px; align-items: flex-end; flex-shrink: 0; }
  .updv-acts button { white-space: nowrap; }
  .updv-all { display: flex; justify-content: flex-end; padding-top: 4px; }
  .sel { user-select: text; -webkit-user-select: text; color: rgba(255,255,255,0.65); word-break: break-all; }
  .signin-d.warn { color: #f0b84a; }
  .pref-small { font-size: 12px; }
  .speech-opt { display: flex; align-items: flex-start; gap: 8px; text-align: left; font-size: 12.5px; align-self: stretch;
                color: rgba(255,255,255,0.85); cursor: pointer; line-height: 1.45; }
  .speech-opt input { accent-color: #9a5cf5; margin-top: 3px; }
  .speech-help { display: block; font-size: 11.5px; color: rgba(255,255,255,0.45); }

  /* --- modal dialogs (close guard) --------------------------------------------------- */
  .modal-back { position: fixed; inset: 0; z-index: 200; background: rgba(0,0,0,0.55);
                display: flex; align-items: center; justify-content: center; padding: 24px; }
  .modal { width: 100%; max-width: 440px; padding: 20px 22px; border-radius: 12px;
           background: #17171c; border: 1px solid rgba(255,255,255,0.12);
           box-shadow: 0 18px 48px -12px rgba(0,0,0,0.8);
           display: flex; flex-direction: column; gap: 10px; }
  .modal-h { font-size: 15px; font-weight: 600; color: #f0f0f2; }
  .modal-p { font-size: 12.5px; color: rgba(255,255,255,0.7); line-height: 1.5; }
  .modal-actions { display: flex; gap: 9px; margin-top: 8px; }

  .prov { margin-top: 7px; }
  .prov-bar { height: 4px; border-radius: 3px; background: rgba(255,255,255,0.1); overflow: hidden; }
  .prov-fill { height: 100%; background: rgba(255,255,255,0.6); transition: width 0.35s ease; }
  .prov-msg { font-size: 10.5px; color: rgba(255,255,255,0.5); margin-top: 4px;
              overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
