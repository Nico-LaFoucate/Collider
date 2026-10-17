<script>
  // The one confirmation dialog for Adobe app updates. Every entry point (card, Apps header,
  // Mud Hut's "Update installed apps", the Prefixes tab) opens this with a request; its text comes
  // from updates-logic.js dialogModel, so it reads the same wherever it was opened from.
  //
  // It re-reads the prefix when it opens (a local read, Adobe is not contacted), so "close these
  // apps first" reflects what is open NOW rather than the last scheduled check. Mud Hut still
  // refuses at the start if something opened in between; that refusal changes nothing.
  import { onMount } from "svelte";
  import { upd, check } from "$lib/updates.svelte.js";
  import { dialogModel, dialogAction } from "$lib/updates-logic.js";

  // req = { kind, prefix, row, related, rows, excluded, target } (see dialogModel)
  let { req, prefixName, snapshot = true, onconfirm, oncancel } = $props();

  let checking = $state(true);
  let choices = $state({});
  const appsRunning = $derived(checking ? [] : (upd.prefixes[req.prefix]?.apps_running ?? []));
  const m = $derived(dialogModel(req, { prefixName, snapshot, appsRunning }));
  // For a build Neutron has not verified, waiting is the recommended answer, so it is the button
  // that looks like the default.
  const waitFirst = $derived(req.kind === "unverified");

  async function recheck() {
    checking = true;
    try { await check([req.prefix], { offline: true, partial: true }); }
    finally { checking = false; }
  }

  onMount(() => {
    for (const o of dialogModel(req, { prefixName }).options) choices[o.key] = o.value;
    recheck();
  });

  function confirm() {
    const a = dialogAction(req, choices);
    if (a) onconfirm?.(a);
  }
  function onkeydown(e) { if (e.key === "Escape") oncancel?.(); }
</script>

<svelte:window {onkeydown} />

<div class="ud-back">
  <div class="ud" role="dialog" aria-modal="true" aria-labelledby="ud-title">
    <div class="ud-title" id="ud-title">{m.title}</div>
    {#if m.items.length}
      <ul class="ud-items">
        {#each m.items as it}<li>{it}</li>{/each}
      </ul>
    {/if}
    {#each m.lines as line}
      <div class="ud-line">{line}</div>
    {/each}
    {#each m.options as o (o.key)}
      <label class="ud-opt">
        <input type="checkbox" bind:checked={choices[o.key]} />
        <span>
          {o.label}
          {#if o.help}<span class="ud-help">{o.help}</span>{/if}
        </span>
      </label>
    {/each}

    {#if checking}
      <div class="ud-line muted">Checking which Adobe apps are open…</div>
    {:else if m.blockedBy}
      <div class="ud-block">
        {m.blockedBy}
        <button class="ud-link" onclick={recheck}>Check again</button>
      </div>
    {/if}

    <div class="ud-actions">
      {#if waitFirst}
        <button class="ud-primary" onclick={() => oncancel?.()}>{m.cancel}</button>
        <button class="ud-ghost" disabled={checking || !!m.blockedBy} onclick={confirm}>{m.confirm}</button>
      {:else}
        <button class="ud-primary" disabled={checking || !!m.blockedBy} onclick={confirm}>{m.confirm}</button>
        <button class="ud-ghost" onclick={() => oncancel?.()}>{m.cancel}</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .ud-back { position: fixed; inset: 0; z-index: 200; background: rgba(0,0,0,0.55);
             display: flex; align-items: center; justify-content: center; padding: 24px; }
  .ud { width: 100%; max-width: 460px; max-height: calc(100vh - 48px); overflow-y: auto;
        padding: 20px 22px; border-radius: 12px; background: #17171c;
        border: 1px solid rgba(255,255,255,0.12); box-shadow: 0 18px 48px -12px rgba(0,0,0,0.8);
        display: flex; flex-direction: column; gap: 9px; color: #e8e8ec; }
  .ud-title { font-size: 15px; font-weight: 600; color: #f0f0f2; margin-bottom: 2px; }
  .ud-line { font-size: 12.5px; color: rgba(255,255,255,0.7); line-height: 1.5; }
  .ud-line.muted { color: rgba(255,255,255,0.45); }
  .ud-items { margin: 0; padding-left: 18px; font-size: 12.5px; color: #e8e8ec; line-height: 1.6; }
  .ud-opt { display: flex; align-items: flex-start; gap: 8px; font-size: 12.5px; color: rgba(255,255,255,0.85);
            cursor: pointer; line-height: 1.45; margin-top: 2px; }
  .ud-opt input { accent-color: #9a5cf5; margin-top: 3px; }
  .ud-help { display: block; font-size: 11.5px; color: rgba(255,255,255,0.45); }
  .ud-block { font-size: 12.5px; line-height: 1.5; padding: 9px 11px; border-radius: 8px;
              background: rgba(239,159,39,0.10); border: 1px solid rgba(239,159,39,0.28); color: #f2c374; }
  .ud-link { background: none; border: none; padding: 0; margin-left: 4px; color: #8fd0ff;
             font-size: 12.5px; cursor: pointer; text-decoration: underline; }
  .ud-actions { display: flex; gap: 9px; margin-top: 8px; }
  .ud-primary { padding: 8px 16px; border-radius: 9px; border: 1px solid rgba(49,168,255,0.4);
                background: rgba(49,168,255,0.16); color: #8fd0ff; font-size: 13px; font-weight: 600; cursor: pointer; }
  .ud-primary:hover:not(:disabled) { background: rgba(49,168,255,0.24); }
  .ud-ghost { padding: 8px 14px; border-radius: 9px; border: 1px solid rgba(255,255,255,0.16);
              background: rgba(255,255,255,0.05); color: #e8e8ec; font-size: 12.5px; cursor: pointer; }
  .ud-primary:disabled, .ud-ghost:disabled { opacity: 0.45; cursor: default; }
</style>
