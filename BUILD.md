# Building Collider

Most people don't need to build Collider: `neutron setup` installs the release AppImage. This page
is for working on Collider itself.

Collider is a Tauri 2 app: a Svelte front end with a Rust back end. It drives the `neutron` and
`mudhut` commands, so install Neutron first (`neutron setup`), which puts both on your `PATH`.
Without the CLI or neutron-wine, Collider shows **Set up**, which installs everything.

## 1. Install the toolchain

You need Rust, Node.js with npm, and Tauri's Linux system dependencies.

```sh
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Node.js and Tauri's system dependencies on Arch / CachyOS
sudo pacman -S --needed nodejs npm webkit2gtk-4.1 base-devel curl wget file \
  openssl appmenu-gtk-module libappindicator-gtk3 librsvg
```

On other distributions, see Tauri's [prerequisites](https://v2.tauri.app/start/prerequisites/) for
the matching packages.

Check:

```sh
cargo --version && node --version && npm --version
neutron --version
```

## 2. Run a development build

From the repository root:

```sh
npm ci
npm run tauri dev
```

The first run compiles the Rust side and takes a few minutes; later runs are fast. A Collider
window opens, and changes to the Svelte files reload in place.

## 3. Build the release AppImage

```sh
./release.sh
```

`release.sh` needs [Podman](https://podman.io/) (rootless is fine). It builds a fresh clone of
`HEAD` inside an Ubuntu 24.04 container (`release.Containerfile`), so commit your changes first.
The result is `dist/Collider_<version>_amd64.AppImage` and `dist/SHA256SUMS`. At the end it prints
the commands that publish the release; `neutron setup` downloads that AppImage and checks it
against `SHA256SUMS`.

Why a container: an AppImage carries its own copies of WebKit, GLib and friends, and those need
the glibc of the system they were built on. Built on Ubuntu 24.04 they need glibc 2.39 at most,
the oldest Neutron supports. The build fails if any file in the AppImage needs a newer glibc, if
it contains your home directory's path, or if it bundles the Wayland client libraries (they are
left to the system, whose newer Mesa needs its own copy). `COLLIDER_RELEASE_NATIVE=1 ./release.sh`
builds on your own system instead, for a quick local test; the glibc gate still applies.

If the AppImage step fails:

- `failed to run linuxdeploy` on a system without FUSE: run it again with
  `APPIMAGE_EXTRACT_AND_RUN=1` set.
- `Manually setting rpath for GTK modules: File exists`: the bundle step can't run over an earlier
  attempt. Delete `src-tauri/target/release/bundle/appimage` and try again.

## Where to edit what

| What | Where |
| --- | --- |
| Pages, layout and the Preferences tab | `src/routes/+page.svelte` |
| App tiles | `src/lib/AppCard.svelte` |
| Front end ↔ back end calls | `src/lib/api.js` (one wrapper per command) |
| Commands the front end can call | `src-tauri/src/commands.rs` |
| Talking to the Neutron CLI | `src-tauri/src/neutron/mod.rs` |
| Talking to Mud Hut | `src-tauri/src/mudhut/mod.rs` |
| Launching, prefixes, display, settings and theming | `src-tauri/src/core/` |

Behavior belongs in the Neutron CLI; Collider is a front end for it. `neutron --help` lists the
commands, and the CLI is the authority on how they behave.
