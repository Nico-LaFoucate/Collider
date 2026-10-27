<script>
  // Support: the ways to fund Neutron, in the website's own words (neutronproject.org/#support), and where a
  // problem goes instead. Links open in the desktop's browser through plugin-shell, as ReportDialog's do.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { getVersion } from "@tauri-apps/api/app";

  const WAYS = [
    { name: "Patreon", body: "Monthly support through Patreon.", label: "Become a patron",
      url: "https://patreon.com/neutronproject" },
    { name: "GitHub Sponsors", body: "Sponsor directly through GitHub, one-time or recurring.", label: "Sponsor on GitHub",
      url: "https://github.com/sponsors/Nico-LaFoucate" },
    { name: "Ko-fi", body: "Donate through Ko-fi, one-time or monthly.", label: "Support on Ko-fi",
      url: "https://ko-fi.com/neutroncollider" },
  ];
  const STUDIOS = "mailto:contact@neutronproject.org?subject=NEUTRON%20-%20";
  const SITE = "https://neutronproject.org";
  const BUG_FORM = "https://github.com/Nico-LaFoucate/Neutron/issues/new/choose";

  let version = $state("");
  onMount(async () => {
    try { version = await getVersion(); } catch { version = ""; }
  });
</script>

<div class="sp">
  <div class="sp-lead">Neutron was made for free, and every donation helps keep the project alive and in development.</div>

  <div class="sp-ways">
    {#each WAYS as w}
      <div class="sp-card">
        <div class="sp-name">{w.name}</div>
        <div class="sp-body">{w.body}</div>
        <button class="sp-primary" onclick={() => open(w.url)}>{w.label}</button>
      </div>
    {/each}
  </div>

  <div class="sp-card">
    <div class="sp-name">Studios and companies</div>
    <div class="sp-body">
      For businesses that want to sponsor the project, or get in touch about funding a specific part of its
      development. Put <b>NEUTRON</b> or <b>COLLIDER</b> in the subject line so it reaches the right place. Bug
      reports go to GitHub Issues, not this address.
    </div>
    <button class="sp-ghost" onclick={() => open(STUDIOS)}>contact@neutronproject.org</button>
  </div>

  <div class="sp-foot">
    <span>Collider{version ? ` ${version}` : ""}</span>
    <span class="sep">·</span>
    <button class="sp-link" onclick={() => open(SITE)}>neutronproject.org</button>
    <span class="sep">·</span>
    <button class="sp-link" onclick={() => open(BUG_FORM)}>Report a problem</button>
  </div>
</div>

<style>
  .sp { padding: 18px 22px 24px; max-width: 760px; display: flex; flex-direction: column; gap: 14px; }
  .sp-lead { font-size: 13.5px; line-height: 1.55; color: rgba(255,255,255,0.75); }
  .sp-ways { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 12px; }
  .sp-card { padding: 14px 16px; border-radius: 11px; background: rgba(255,255,255,0.035);
             border: 1px solid rgba(255,255,255,0.08); display: flex; flex-direction: column; gap: 8px; }
  .sp-name { font-size: 13.5px; font-weight: 600; color: #f0f0f2; }
  .sp-body { font-size: 12.5px; line-height: 1.5; color: rgba(255,255,255,0.6); flex: 1; }
  .sp-body b { color: rgba(255,255,255,0.8); font-weight: 600; }
  .sp-primary { align-self: flex-start; padding: 8px 16px; border-radius: 9px; border: 1px solid rgba(49,168,255,0.4);
                background: rgba(49,168,255,0.16); color: #8fd0ff; font-size: 13px; font-weight: 600; cursor: pointer; }
  .sp-primary:hover { background: rgba(49,168,255,0.24); }
  .sp-ghost { align-self: flex-start; padding: 8px 14px; border-radius: 9px; border: 1px solid rgba(255,255,255,0.16);
              background: rgba(255,255,255,0.05); color: #e8e8ec; font-size: 12.5px; cursor: pointer; }
  .sp-ghost:hover { background: rgba(255,255,255,0.09); }
  .sp-foot { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 11.5px;
             color: rgba(255,255,255,0.4); margin-top: 4px; }
  .sp-link { background: none; border: none; padding: 0; color: rgba(255,255,255,0.55); font-size: 11.5px;
             cursor: pointer; text-decoration: underline; }
  .sp-link:hover { color: #8fd0ff; }
</style>
