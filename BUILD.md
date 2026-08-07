# Collider — Build & Run (CachyOS)

This scaffold is the Phase 1 MVP: detect a Neutron prefix, show its health, and a
single Launch button that runs the full launch loop (display fix → launch Premiere →
start hwmux → stop on exit), with live status. It calls the **Neutron CLI** — that must
be installed and on your `PATH` first (`neutron --json doctor` should return JSON).

Everything below is copy-paste. Each step ends with a check so you know it worked.

---

## 0. One-time: install the toolchain

You need Rust, Node, and the Tauri system dependencies.

```fish
# Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# then restart your shell, or:
source $HOME/.cargo/env

# Node (CachyOS / Arch)
sudo pacman -S --needed nodejs npm

# Tauri v2 system deps on Arch/CachyOS
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file \
  openssl appmenu-gtk-module libappindicator-gtk3 librsvg
```

Check:
```fish
cargo --version; and node --version; and npm --version
echo "neutron present?"; and neutron --json doctor
```
The last line should print a JSON object. If `neutron` isn't found, install/symlink the
Neutron CLI onto your `PATH` before continuing — Collider is useless without it.

---

## 1. Put the scaffold in place

Unzip `collider-app.zip` wherever you keep projects, e.g. under `~/Projects`. It
extracts to a folder named `collider-app`. `cd` into that folder:

```fish
# adjust the path to wherever you unzipped it
cd ~/Projects/collider-app
```

Check:
```fish
ls
# expect: package.json  svelte.config.js  vite.config.js  src/  src-tauri/
```
If `ls` shows a single `collider-app/` entry instead of the files above, you're one
level too high — `cd collider-app` and check again.

---

## 2. Install frontend dependencies

```fish
npm install
```

Check: a `node_modules/` folder appears and the command exits 0.

---

## 3. Add app icons (required by Tauri to build)

Tauri needs at least one icon. The quickest path — generate placeholders from any PNG:

```fish
# from any square PNG you have, e.g. a logo:
npx @tauri-apps/cli icon path/to/some-square-image.png
```
This populates `src-tauri/icons/`. (You can replace these with a real Collider icon
later — the atom mark from the mockup would fit.)

Check:
```fish
ls src-tauri/icons
# expect: 32x32.png  128x128.png  icon.png  (and others)
```

---

## 4. Run in dev mode

```fish
npm run tauri dev
```

The first run compiles the Rust core (slow — a few minutes; later runs are fast). A
Collider window should open.

What you should see and do:
- Click **Refresh** in the header. The Premiere card's pills update from the real
  prefix (Prefix healthy / Display fix), and a Health panel lists the `doctor` checks.
- If the prefix path is wrong, fix it: open `src/routes/+page.svelte` and change the
  `prefix` default near the top, then save (the dev server hot-reloads).
- Click **Launch**. Watch the status line walk the loop:
  `Resolving → Applying display fix → Launching Premiere → Starting daemon → Running`.
  Premiere should open through Neutron, and the hwmux daemon should be supervised.
- Click **Stop** to tear down the daemon.

If launch fails, the status line and the red banner show the engine's own reason —
e.g. "display-fix: Premiere is running; cannot edit prefs" (exit 3). Close Premiere and
retry.

---

## 5. Build a distributable (when you're ready)

```fish
npm run tauri build
```
Produces an AppImage and a `.deb` under
`src-tauri/target/release/bundle/`. These are what you'd hand to the Linux community
once the MVP is solid.

---

## Where to edit what

- **UI** (cards, pills, status, layout): `src/routes/+page.svelte`
- **Frontend ↔ backend calls**: `src/lib/api.js` (one wrapper per IPC command)
- **IPC commands**: `src-tauri/src/commands.rs`
- **The launch loop logic**: `src-tauri/src/core/launch.rs`
- **The Neutron CLI boundary**: `src-tauri/src/neutron/mod.rs` (the only place that
  shells out to `neutron`)
- **Prefix resolution / daemon supervision**: `src-tauri/src/core/`

The systems logic lives in Rust and changes rarely; day-to-day tweaks happen in the
Svelte file and `api.js`.

---

## Known v0 scope / gaps (by design)

- **One app, one prefix.** The prefix path is hardcoded as a default in the UI; a real
  prefix registry comes in Phase 2.
- **No software-render toggle.** Neutron's `--software` is an honest gap (exit 4), so
  Collider doesn't offer it.
- **Premiere-exit detection is manual** (the Stop button). Auto-stopping hwmux when the
  Premiere PID exits is a small follow-up — poll the PID and call `stop_session`.
- **`doctor` / `prefix info` schema is provisional** until Neutron's minimal-stack
  cleanup lands. The UI renders the `checks` list generically so a schema change won't
  break it.

## Building the AppImage (release)

```
cd ~/Collider
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 ARCH=x86_64 ./node_modules/.bin/tauri build
```

**All three variables are required on Arch/CachyOS.** Without them `tauri build` fails with a
useless `failed to run linuxdeploy` and swallows the real error. What each one fixes:

| var | why |
|---|---|
| `APPIMAGE_EXTRACT_AND_RUN=1` | linuxdeploy is itself an AppImage; without this it needs FUSE to mount |
| `NO_STRIP=1` | linuxdeploy bundles a 2024-vintage `strip` that does not understand **`.relr.dyn`** (section type `0x13`, SHT_RELR), which every current Arch library uses. Every strip call errors and the run aborts |
| `ARCH=x86_64` | appimagetool refuses to guess: *"More than one architectures were found of the AppDir"* |

⚠️ **The gtk plugin is NOT idempotent** — re-running the bundle step over an existing
`target/release/bundle/appimage/` dies with `Manually setting rpath for GTK modules: File exists`.
`rm -rf src-tauri/target/release/bundle/appimage` before a retry.

⭐ To see what actually failed, run linuxdeploy by hand against the AppDir tauri prepared:
```
cd src-tauri/target/release/bundle/appimage
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 ARCH=x86_64 OUTPUT=Collider_0.1.0_amd64.AppImage \
  ~/.cache/tauri/linuxdeploy-x86_64.AppImage --appdir Collider.AppDir --plugin gtk --output appimage
```
