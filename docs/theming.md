# Neutron Theming Subsystem

Status: **in progress** — M1 (colors) under construction.

Lets the end user theme Premiere's client-side window decoration (CSD): the dark
theme auto-applies as the default, and a **Preferences → Appearance** panel lets
the user pick a preset, edit colors freely, and choose/import caption-button icons.
Per-prefix; applied on launch (no separate apply step).

## Background

Neutron's wine build draws Premiere's own non-client frame (title bar + menu bar +
caption buttons) — see `neutron-wine` `neutron-winewayland-decoration` (CSD) and
`neutron-caption-buttons` (flat buttons). Everything the frame renders is driven by
the Wine prefix's `HKCU\Control Panel\Colors`, so theming the frame = writing those
colors into the prefix. That `.reg` write already happens automatically on every
launch (`core/decoration.rs`). This subsystem turns that invisible bundled blob into
the **serialized output of a real themes feature**.

## Model

A *theme* = `{ colors, buttonIconSet }`, stored in Collider settings and serialized
into the target prefix as:

1. **Colors** → a generated `.reg` written to `HKCU\Control Panel\Colors`
   (`core/theme.rs` is the single source of truth for the built-in presets).
2. **Button icons** → an icon asset dir inside the prefix + a registry pointer the
   wine-side drawer reads (M3).

The existing `decoration::apply(prefix)` on-launch path writes both. The bundled
static `.reg` resource is **replaced** by `theme.rs`-generated output (so the dark
preset has one definition, not two that can drift).

## Colors (M1 — no wine change)

The flat-button renderer and frame already read everything from `Control Panel\Colors`,
so the panel only edits + serializes them. UI groups → keys:

| UI group  | Keys |
|-----------|------|
| Title bar | `ActiveTitle`, `GradientActiveTitle`, `TitleText`, `InactiveTitle`, `GradientInactiveTitle`, `InactiveTitleText`, `ActiveBorder`, `InactiveBorder`, `WindowFrame` |
| Menu bar  | `Menu`, `MenuBar`, `MenuText`, `MenuHilight` |
| Window    | `Window`, `WindowText`, `HilightText` |
| Buttons   | `ButtonFace`(general 3D ctrl face), `ButtonText`(glyph), `ButtonShadow`(press), `ButtonHilight`(min/max hover), `ButtonLight`, `ButtonDkShadow`, `3DLight`, `3DDarkShadow` |

Caption-button states map to: `ActiveTitle`→normal bg, `ButtonShadow`→press,
`ButtonHilight`→min/max hover, `ButtonText`→glyph. **Close-hover red + white glyph
are hardcoded in the wine patch** (intentional, like Windows) and not themeable.

Presets: **Dark (default), Light**, with **Match-system** to follow (read the KDE
color scheme; falls back to Dark). Plus **Reset to Dark** and a frontend contrast
guard (warn on unreadable text/bg pairs). Changes need a **relaunch** to show (NC
colors are read at window creation) → panel shows a "Restart Premiere to apply" hint.

## Custom icons — wine patch, Route A (M3)

Replaces the Marlett-glyph drawing in `neutron-caption-buttons` with image rendering,
via Wine's existing `NtUserDrawNonClientButton` → `pNonClientButtonDraw` user-mode
callback (the same path uxtheme uses):

1. Extend `enum NONCLIENT_BUTTON_TYPE` with `CAPTION_CLOSE/MIN/MAX/RESTORE`.
2. Add a `hot` field to `struct draw_non_client_button_params` (we have `down`/`grayed`).
3. Route `draw_close/max/min_button` through the callback instead of calling
   `draw_frame_caption` directly.
4. User-mode drawer (`USER_NonClientButtonDraw`): if custom icons are configured for
   the prefix → load the icon for `(type, state)` and blit (`AlphaBlend`/`DrawIconEx`);
   else → the current flat fill + Marlett glyph (relocated verbatim, so default
   behavior is byte-identical). Missing/failed load → graceful fallback to the glyph.

Assets in-prefix: `drive_c/windows/neutron/caption/<setid>/{close,min,max,restore}[_hover][_press].png`,
advertised via `HKCU\Software\Neutron\Caption` (`Enabled`, `IconDir`). Format: PNG with
alpha (WIC in user-mode), `.ico` also accepted; scaled to `SM_CXSIZE` per-DPI.

Supply model: **bundled sets + import**. Ship curated sets (Windows-11, macOS
traffic-lights, minimal/square) as Collider resources; **Import** copies/validates
user files into the prefix asset dir.

## Contract (Collider ↔ wine patch)

- **Colors:** `HKCU\Control Panel\Colors` (existing; M1 writes it).
- **Icons:** `HKCU\Software\Neutron\Caption` → `Enabled` (bool), `IconDir` (prefix path).
  The user-mode drawer reads these (M3).

## Milestones

- **M1** Colors: `theme.rs` (presets + `.reg` generation), settings (`theme` +
  `custom_colors`), `decoration` generates the `.reg` from the active theme,
  Preferences → Appearance panel (preset picker, per-group color editors, reset,
  contrast guard, relaunch hint). **No wine change.** ← *current*
- **M2** Wine patch: caption buttons routed through the `NtUserDrawNonClientButton`
  callback; current flat default relocated to the user-mode drawer (behavior-identical).
- **M3** Custom-icon load/blit + asset pipeline + bundled sets + import UI.
- **M4** Polish: Match-system preset, uninstall/reset, retire `window_rule.rs` (fold
  positioning into the KWin overlay script driven by `home_window_y`), live-apply
  investigation.

## Resolved decisions

- Per-state icons: accept up to 3 assets (normal/hover/press) per button; fall back to
  a single base + auto-tint when only one is supplied.
- Apply timing: relaunch (live NC repaint via `WM_SYSCOLORCHANGE` is unreliable for NC).
- First bundled sets: Windows-11, macOS traffic-lights, minimal/square.
