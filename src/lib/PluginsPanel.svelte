<script>
  // Plugins tab: what the selected prefix holds, and a way to add more. `neutron plugins` does the work
  // (where each kind of file goes, the extension signature check, running a vendor installer in the
  // prefix); this panel picks or receives files, shows the engine's verdict for each, and opens folders.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { pluginsList, pluginsFolders, pluginsInstall, openPrefixFolder } from "$lib/api.js";

  let { prefix } = $props();

  const APP_NAMES = { aftereffects: "After Effects", photoshop: "Photoshop", illustrator: "Illustrator" };
  // What the file picker offers (the engine also takes a folder of .aex/.prm/.8bf/.aip files).
  const EXTENSIONS = ["aex", "prm", "8bf", "8li", "8ba", "8bi", "8be", "8bx", "8by", "8bs", "aip", "vst3",
                      "jsx", "jsxbin", "zxp", "ccx", "exe", "msi"];
  const FOLDER_LABELS = {
    shared: "After Effects, Premiere Pro and Media Encoder plugins",
    extensions: "Extension panels",
    uxp: "UXP plugins",
    vst3: "VST3 audio plugins",
    "aftereffects-plugins": "After Effects only plugins",
    "aftereffects-scripts": "After Effects scripts (ScriptUI panels)",
    "photoshop-plugins": "Photoshop plugins",
    "photoshop-scripts": "Photoshop scripts",
    "illustrator-plugins": "Illustrator plugins",
    "illustrator-scripts": "Illustrator scripts",
  };

  let inv = $state(null);
  let folders = $state({});
  let error = $state(null);
  let loading = $state(true);
  let busy = $state(false);          // an install is running (a vendor installer may take minutes)
  let busyText = $state("");
  let results = $state(null);        // the engine's per-file verdicts of the last install
  let note = $state(null);
  let pendingScripts = $state(null); // scripts waiting for "which app is this for?"
  let scriptApp = $state(null);
  let dropping = $state(false);

  let scriptApps = $derived(inv ? Object.keys(inv.apps ?? {}).filter((a) => APP_NAMES[a]) : []);
  // The folders most plugins go into first, in FOLDER_LABELS' order; any the engine adds later at the end.
  let folderList = $derived([
    ...Object.keys(FOLDER_LABELS).filter((k) => folders[k]),
    ...Object.keys(folders).filter((k) => !(k in FOLDER_LABELS)),
  ].map((k) => [k, folders[k]]));

  async function load() {
    loading = true;
    error = null;
    try {
      const [l, f] = await Promise.all([pluginsList(prefix), pluginsFolders(prefix)]);
      inv = l;
      folders = f.folders ?? {};
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const isScript = (p) => /\.(jsx|jsxbin)$/i.test(p);
  const isInstaller = (p) => /\.(exe|msi)$/i.test(p);
  const base = (p) => p.replace(/\/+$/, "").split("/").pop();

  // Scripts need to know their app; everything else is placed by its type. Ask once for the scripts,
  // install the rest right away.
  async function install(paths) {
    if (!paths?.length || busy) return;
    const scripts = paths.filter(isScript);
    const rest = paths.filter((p) => !isScript(p));
    if (scripts.length) {
      pendingScripts = scripts;
      scriptApp = scriptApps.length === 1 ? scriptApps[0] : null;
    }
    if (rest.length) await run(rest, null);
  }

  async function installScripts() {
    const files = pendingScripts;
    pendingScripts = null;
    await run(files, scriptApp);
  }

  async function run(files, app) {
    busy = true;
    busyText = files.some(isInstaller)
      ? "Running the installer. Finish it in its own window; this waits until it is done."
      : "Installing…";
    try {
      const r = await pluginsInstall(prefix, files, app);
      results = [...(results ?? []), ...(r.installed ?? [])];
      note = (r.installed ?? []).some((x) => x.ok) ? r.note : note;
    } catch (e) {
      results = [...(results ?? []), ...files.map((f) => ({ file: f, ok: false, reason: String(e) }))];
    } finally {
      busy = false;
      await load();
    }
  }

  async function pickFiles() {
    const picked = await open({ multiple: true, directory: false, title: "Choose plugins to install",
                                filters: [{ name: "Plugins, extensions, scripts and installers", extensions: EXTENSIONS }] });
    if (picked) install(Array.isArray(picked) ? picked : [picked]);
  }

  async function pickFolder() {
    const picked = await open({ directory: true, title: "Choose a plugin folder (a .vst3 bundle, or a folder of .aex, .prm, .8bf or .aip files)" });
    if (picked) install([picked]);
  }

  async function show(path) {
    try { await openPrefixFolder(path); } catch (e) { error = String(e); }
  }

  onMount(() => {
    load();
    let unlisten = null;
    getCurrentWebview()
      .onDragDropEvent((ev) => {
        const t = ev.payload.type;
        if (t === "enter" || t === "over") dropping = true;
        else if (t === "leave") dropping = false;
        else if (t === "drop") { dropping = false; install(ev.payload.paths); }
      })
      .then((u) => (unlisten = u))
      .catch(() => {});
    return () => unlisten?.();
  });
</script>

<div class="pp" class:dropping>
  <section class="add">
    <div class="add-title">Add plugins</div>
    <div class="add-body">
      Drop plugin files, VST3 plugins, UXP plugins (.ccx), extension packages (.zxp), scripts or a vendor's installer here, or choose them.
      Neutron puts each where its Windows installer would. Close the Adobe apps before running an installer,
      and restart an app to load what you added.
    </div>
    <div class="add-actions">
      <button class="pp-primary" onclick={pickFiles} disabled={busy}>Choose files…</button>
      <button class="pp-ghost" onclick={pickFolder} disabled={busy}>Choose a folder…</button>
    </div>
    {#if busy}<div class="pp-line">{busyText}</div>{/if}

    {#if pendingScripts}
      <div class="ask">
        <div class="pp-line">Which app {pendingScripts.length === 1 ? "is this script" : "are these scripts"} for?
          <span class="muted">{pendingScripts.map(base).join(", ")}</span></div>
        {#if scriptApps.length}
          <div class="ask-row">
            {#each scriptApps as a}
              <label class="ask-opt"><input type="radio" bind:group={scriptApp} value={a} /> {APP_NAMES[a]}</label>
            {/each}
          </div>
          <div class="add-actions">
            <button class="pp-primary" disabled={!scriptApp || busy} onclick={installScripts}>Install</button>
            <button class="pp-ghost" onclick={() => (pendingScripts = null)}>Cancel</button>
          </div>
        {:else}
          <div class="pp-warn">Scripts go into After Effects, Photoshop or Illustrator, and none of them is installed in this prefix.</div>
          <div class="add-actions"><button class="pp-ghost" onclick={() => (pendingScripts = null)}>OK</button></div>
        {/if}
      </div>
    {/if}

    {#if results?.length}
      <div class="res">
        {#each results as r}
          <div class="res-row">
            <span class="dot {r.ok ? 'ok' : 'bad'}"></span>
            <span class="res-name">{base(r.file)}</span>
            <span class="res-msg">
              {#if r.ok}
                {r.type ?? "installed"}{r.replaced ? ", replaced the one that was there" : ""}{r.note ? ` · ${r.note}` : ""}
              {:else}
                {r.reason ?? "not installed"}
              {/if}
            </span>
          </div>
        {/each}
        {#if note}<div class="pp-line ok">{note}</div>{/if}
        <button class="pp-link" onclick={() => { results = null; note = null; }}>Clear</button>
      </div>
    {/if}
  </section>

  {#if error}
    <div class="pp-warn">⚠ {error}</div>
  {/if}

  {#if loading && !inv}
    <div class="pp-line muted">Looking at the prefix…</div>
  {:else if inv}
    <section class="group">
      <div class="g-title">Extension panels</div>
      {#each inv.extensions as e (e.id)}
        <div class="item"><span class="i-name">{e.name}</span><span class="i-meta">{e.version ?? ""}</span></div>
      {:else}
        <div class="i-none">None installed.</div>
      {/each}
    </section>
    <section class="group">
      <div class="g-title">UXP plugins</div>
      {#each inv.uxp ?? [] as e (e.path)}
        <div class="item"><span class="i-name">{e.name}</span><span class="i-meta">{e.version ?? ""}</span></div>
      {:else}
        <div class="i-none">None installed.</div>
      {/each}
    </section>
    <section class="group">
      <div class="g-title">After Effects, Premiere Pro and Media Encoder plugins</div>
      {#each inv.shared as n}
        <div class="item"><span class="i-name">{n}</span></div>
      {:else}
        <div class="i-none">None installed.</div>
      {/each}
    </section>
    {#if inv.vst3?.length}
      <section class="group">
        <div class="g-title">VST3 audio plugins</div>
        {#each inv.vst3 as n}<div class="item"><span class="i-name">{n}</span></div>{/each}
      </section>
    {/if}
    <section class="group">
      <div class="g-title">Other programs in this prefix</div>
      {#each inv.programs as p (p.name)}
        <div class="item"><span class="i-name">{p.name}</span><span class="i-meta">{[p.publisher, p.version].filter(Boolean).join(" · ")}</span></div>
      {:else}
        <div class="i-none">None.</div>
      {/each}
    </section>
    <section class="group">
      <div class="g-title">Folders</div>
      <div class="g-sub">To add a plugin by hand, put it in its folder.</div>
      {#each folderList as [key, path] (key)}
        <div class="item">
          <span class="i-name">{FOLDER_LABELS[key] ?? key}</span>
          <button class="pp-ghost small" onclick={() => show(path)} title={path}>Open folder</button>
        </div>
      {/each}
    </section>
  {/if}
</div>

<style>
  .pp { padding: 14px 22px 24px; display: flex; flex-direction: column; gap: 14px; min-height: 100%;
        border: 2px dashed transparent; border-radius: 12px; }
  .pp.dropping { border-color: rgba(49,168,255,0.5); background: rgba(49,168,255,0.05); }
  .add, .group { padding: 14px 16px; border-radius: 11px; background: rgba(255,255,255,0.035);
                 border: 1px solid rgba(255,255,255,0.08); display: flex; flex-direction: column; gap: 8px; }
  .add-title, .g-title { font-size: 13.5px; font-weight: 600; color: #f0f0f2; }
  .g-sub { font-size: 11.5px; color: rgba(255,255,255,0.45); margin-top: -4px; }
  .add-body { font-size: 12.5px; line-height: 1.5; color: rgba(255,255,255,0.65); }
  .add-actions { display: flex; gap: 9px; margin-top: 2px; }
  .pp-line { font-size: 12.5px; color: rgba(255,255,255,0.7); line-height: 1.5; }
  .pp-line.ok { color: #7ee2a8; }
  .muted, .pp-line.muted { color: rgba(255,255,255,0.45); }
  .pp-warn { font-size: 12.5px; line-height: 1.5; padding: 9px 11px; border-radius: 8px;
             background: rgba(239,159,39,0.10); border: 1px solid rgba(239,159,39,0.28); color: #f2c374; }
  .pp-primary { padding: 8px 16px; border-radius: 9px; border: 1px solid rgba(49,168,255,0.4);
                background: rgba(49,168,255,0.16); color: #8fd0ff; font-size: 13px; font-weight: 600; cursor: pointer; }
  .pp-primary:hover:not(:disabled) { background: rgba(49,168,255,0.24); }
  .pp-ghost { padding: 8px 14px; border-radius: 9px; border: 1px solid rgba(255,255,255,0.16);
              background: rgba(255,255,255,0.05); color: #e8e8ec; font-size: 12.5px; cursor: pointer; }
  .pp-ghost.small { padding: 4px 10px; font-size: 11.5px; border-radius: 7px; }
  .pp-primary:disabled, .pp-ghost:disabled { opacity: 0.45; cursor: default; }
  .pp-link { align-self: flex-start; background: none; border: none; padding: 0; color: rgba(255,255,255,0.45);
             font-size: 11.5px; cursor: pointer; text-decoration: underline; }
  .ask { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border-radius: 9px;
         background: rgba(0,0,0,0.25); border: 1px solid rgba(255,255,255,0.08); }
  .ask-row { display: flex; gap: 16px; }
  .ask-opt { font-size: 12.5px; color: #e8e8ec; display: flex; align-items: center; gap: 6px; cursor: pointer; }
  .res { display: flex; flex-direction: column; gap: 6px; padding-top: 4px; }
  .res-row { display: flex; align-items: baseline; gap: 8px; font-size: 12.5px; }
  .res-name { color: #f0f0f2; font-weight: 600; flex-shrink: 0; }
  .res-msg { color: rgba(255,255,255,0.6); line-height: 1.45; }
  .item { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 5px 0;
          border-top: 1px solid rgba(255,255,255,0.05); font-size: 12.5px; }
  .item:first-of-type { border-top: none; }
  .i-name { color: #e8e8ec; }
  .i-meta { color: rgba(255,255,255,0.45); font-size: 11.5px; }
  .i-none { font-size: 12px; color: rgba(255,255,255,0.4); }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; align-self: center; }
  .dot.ok { background: #3ce08c; }
  .dot.bad { background: #e24b4a; }
</style>
