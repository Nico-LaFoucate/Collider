<script>
  // "Copy details" from the sidebar: the text of `neutron report` for the selected prefix, shown so it
  // can be read before it is posted, and put on the clipboard. The CLI collects it and replaces the
  // personal details; this dialog only shows and copies it.
  import { onMount, tick } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { report } from "$lib/api.js";

  const BUG_FORM = "https://github.com/Nico-LaFoucate/Neutron/issues/new/choose";

  let { prefix = null, onclose } = $props();

  let text = $state("");
  let error = $state(null);
  let loading = $state(true);
  let copied = $state(false);
  let copyFailed = $state(false);
  let area = $state();

  // The async clipboard first; WebKitGTK builds without it, or a write it refuses outside a click,
  // fall back to selecting the text and copying it the old way.
  async function copy(quiet = false) {
    copied = copyFailed = false;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
    } catch {
      area?.focus();
      area?.select();
      try { copied = document.execCommand("copy"); } catch { copied = false; }
      copyFailed = !copied && !quiet;
    }
  }

  onMount(async () => {
    try {
      text = await report(prefix);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
    if (text) {
      await tick();
      copy(true);
    }
  });

  function onkeydown(e) { if (e.key === "Escape") onclose?.(); }
</script>

<svelte:window {onkeydown} />

<div class="rd-back">
  <div class="rd" role="dialog" aria-modal="true" aria-labelledby="rd-title">
    <div class="rd-title" id="rd-title">Details for a bug report</div>
    <div class="rd-line">Personal details are already replaced with placeholders. Read it before you post it.</div>

    {#if loading}
      <div class="rd-line muted">Collecting the details…</div>
    {:else if error}
      <div class="rd-warn">⚠ {error}</div>
    {:else}
      <textarea class="rd-text" readonly bind:this={area} value={text} spellcheck="false"></textarea>
      {#if copied}
        <div class="rd-line ok">Copied. Paste it into the bug report form.</div>
      {:else if copyFailed}
        <div class="rd-warn">Could not copy it. Select the text above and copy it with Ctrl+C.</div>
      {/if}
    {/if}

    <div class="rd-actions">
      <button class="rd-primary" disabled={loading || !text} onclick={() => copy()}>Copy</button>
      <button class="rd-ghost" onclick={() => open(BUG_FORM)}>Open the bug report form</button>
      <button class="rd-ghost" onclick={() => onclose?.()}>Close</button>
    </div>
  </div>
</div>

<style>
  .rd-back { position: fixed; inset: 0; z-index: 200; background: rgba(0,0,0,0.55);
             display: flex; align-items: center; justify-content: center; padding: 24px; }
  .rd { width: 100%; max-width: 720px; max-height: calc(100vh - 48px);
        padding: 20px 22px; border-radius: 12px; background: #17171c;
        border: 1px solid rgba(255,255,255,0.12); box-shadow: 0 18px 48px -12px rgba(0,0,0,0.8);
        display: flex; flex-direction: column; gap: 9px; color: #e8e8ec; min-height: 0; }
  .rd-title { font-size: 15px; font-weight: 600; color: #f0f0f2; margin-bottom: 2px; }
  .rd-line { font-size: 12.5px; color: rgba(255,255,255,0.7); line-height: 1.5; }
  .rd-line.muted { color: rgba(255,255,255,0.45); }
  .rd-line.ok { color: #7ee2a8; }
  .rd-warn { font-size: 12.5px; line-height: 1.5; padding: 9px 11px; border-radius: 8px;
             background: rgba(239,159,39,0.10); border: 1px solid rgba(239,159,39,0.28); color: #f2c374; }
  .rd-text { flex: 1; min-height: 240px; height: 46vh; resize: none; padding: 10px 12px; border-radius: 8px;
             background: rgba(0,0,0,0.35); border: 1px solid rgba(255,255,255,0.10); color: #d8d8de;
             font: 11.5px/1.45 ui-monospace, "JetBrains Mono", "DejaVu Sans Mono", monospace; white-space: pre;
             overflow: auto; }
  .rd-actions { display: flex; gap: 9px; margin-top: 8px; }
  .rd-primary { padding: 8px 16px; border-radius: 9px; border: 1px solid rgba(49,168,255,0.4);
                background: rgba(49,168,255,0.16); color: #8fd0ff; font-size: 13px; font-weight: 600; cursor: pointer; }
  .rd-primary:hover:not(:disabled) { background: rgba(49,168,255,0.24); }
  .rd-ghost { padding: 8px 14px; border-radius: 9px; border: 1px solid rgba(255,255,255,0.16);
              background: rgba(255,255,255,0.05); color: #e8e8ec; font-size: 12.5px; cursor: pointer; }
  .rd-primary:disabled { opacity: 0.45; cursor: default; }
</style>
