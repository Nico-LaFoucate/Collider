# Neutron Theming Subsystem

Status: **shipped** (color themes and window-button icon sets, Collider 0.1.0).

Lets the end user theme the client-side window decoration (CSD) Neutron draws around
every app's window: the dark theme applies as the default, and the **Preferences →
Window decoration theme** section lets the user pick a preset, edit colors freely, and
choose/import caption-button icons. One theme for all prefixes, written into a prefix
when Collider launches an app from it; `neutron prefix provision` writes the default.

## Background

Neutron's wine build draws each app's own non-client frame (title bar + menu bar +
caption buttons) — see `neutron-wine` `neutron-winewayland` (CSD) and
`neutron-caption-buttons` (flat buttons). Everything the frame renders is driven by
the Wine prefix's `HKCU\Control Panel\Colors`, so theming the frame = writing those
colors into the prefix. The engine writes them: Collider sends a customized or
non-default palette to `neutron theme apply --prefix P --colors -` before a launch,
and runs `neutron theme apply --prefix P` (the default) otherwise.

## Model

A *theme* = `{ colors, buttonIconSet }`, stored in Collider settings. The default
theme is the engine's (`DEFAULT_THEME_COLORS` in `bin/neutron`), written by
`prefix provision` and by `theme apply` without `--colors`. Collider's Dark and Light
presets (`core/theme.rs`) are what the editor shows.

1. **Colors** → `neutron theme apply` writes them to `HKCU\Control Panel\Colors`.
2. **Button icons** → `neutron theme icons` copies the set into the prefix and sets the
   registry pointer the wine-side drawer reads.

`decoration::apply(prefix)` runs both commands on the way into every launch.

## Colors (M1 — no wine change)

The flat-button renderer and frame already read everything from `Control Panel\Colors`,
so the panel only edits + serializes them. The editor's groups → keys
(`COLOR_GROUPS` in `src/routes/+page.svelte`):

| UI group            | Keys |
|---------------------|------|
| Title bar & buttons | `ActiveTitle`, `TitleText`, `InactiveTitle` |
| Menu bar            | `MenuBar`, `MenuText`, `MenuHilight` |
| Window              | `Window`, `WindowText`, `WindowFrame` |
| Dialogs & controls  | `ButtonFace`, `ButtonText`, `ButtonShadow` (message boxes, common dialogs, and any control an app doesn't draw itself) |

The presets set further keys (gradients, borders, the other button shades) that the
editor doesn't show. Caption buttons take their colors from the caption palette
(neutron-wine `zzzzzzzzzp`/`q`/`r`): `ActiveTitle` → button background, with the hover
and press shades derived from it; `TitleText` → glyph. **Close-hover red + white glyph
are hardcoded in the wine patch** (intentional, like Windows) and not themeable.

Presets: **Dark (default), Light**. Plus **Reset to Dark** and a frontend contrast
guard (warn on unreadable text/bg pairs). Changes need a **relaunch** to show (NC
colors are read at window creation) → the section shows a "Restart the app to apply" hint.

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

Assets in-prefix: `drive_c/windows/neutron/caption/<setid>/{close,min,max,restore}.ico`,
advertised via `HKCU\Software\Neutron\Caption` (`Enabled`, `IconDir`). Format: `.ico`
only (loaded with user32's own `LoadImageW`, no WIC in the non-client paint path),
scaled to the button size. The drawer also looks for optional `<button>_hover.ico` /
`<button>_press.ico` and falls back to the base icon.

Supply model: **bundled sets + import**. Collider ships four sets (macOS, Windows 11,
Minimal, Adobe flat) and imports a folder of close/min/max/restore images
(.png/.jpg/.bmp/.ico, re-encoded to .ico) into `~/.config/collider/caption-custom`.
`neutron theme icons --prefix P --from <folder> --name <id>` copies the four .ico files
to `C:\windows\neutron\caption\<id>` and sets `HKCU\Software\Neutron\Caption`
(`Enabled`, `IconDir`); `--off` returns to Wine's glyphs.

## Contract (engine ↔ wine patch)

- **Colors:** `HKCU\Control Panel\Colors` (written by `neutron theme apply`).
- **Icons:** `HKCU\Software\Neutron\Caption` → `Enabled` (bool), `IconDir` (prefix path),
  written by `neutron theme icons`. The user-mode drawer reads these (M3).

## Milestones

- **M1** ✅ Colors: `theme.rs` (presets), settings (`theme` + `custom_colors`), the
  Preferences → Window decoration theme section (preset picker, per-group color
  editors, reset, contrast guard, relaunch hint). **No wine change.** Collider generated
  the `.reg` itself until 2026-09-11; the engine writes it now (see Model).
- **M2** ✅ Wine patch: caption buttons routed through the `NtUserDrawNonClientButton`
  callback; current flat default relocated to the user-mode drawer (behavior-identical).
- **M3** ✅ Custom-icon load/blit (neutron-wine, shipped) + Collider asset pipeline:
  `core/caption_icons.rs` embeds 4 bundled sets (macOS / Windows-11 / minimal / Adobe-flat)
  and hands the chosen set's folder to `neutron theme icons`, which installs it into the
  prefix and writes `HKCU\Software\Neutron\Caption` (Enabled/IconDir); import converts user
  PNG/JPG/BMP/ICO → `.ico` (the `image` crate) into `~/.config/collider/caption-custom`; the
  section has a set picker + Import button. Verified live (bundled + imported render in Premiere).

## Resolved decisions

- Per-state icons: the drawer uses `_hover` / `_press` variants when present and falls
  back to the base icon; Collider's sets and imports supply only the four base icons.
- Apply timing: relaunch (live NC repaint via `WM_SYSCOLORCHANGE` is unreliable for NC).
- Bundled sets: macOS, Windows 11, Minimal, Adobe flat.
