# Collider

### by Nico LaFoucate

**Collider** is a graphical launcher and environment manager that brings the Adobe Creative Suite to Linux, powered by [Neutron](https://github.com/Nico-LaFoucate/neutron) — a Wine-based compatibility layer built specifically for Adobe applications.

> ⚠️ **Beta** — Collider is in beta, like the rest of Neutron. Contributions are welcome.

-----

## What is Collider?

Collider gives Linux users a friendly, GUI-based way to install, configure, and launch Adobe Creative Suite applications without touching the command line. Think of it as a creative-focused alternative to Lutris, built from the ground up for Adobe’s ecosystem.

Under the hood, Collider manages a shared Wine prefix through Neutron, preserving full Adobe Dynamic Link interoperability between Premiere Pro and After Effects.

-----

## Powered by Neutron

Collider is the user-facing frontend. **Neutron** is the compatibility engine underneath it — a patched Wine environment with custom DXVK and DirectComposition implementations that make Adobe apps actually run.

- Neutron repo: [github.com/Nico-LaFoucate/neutron](https://github.com/Nico-LaFoucate/neutron)

-----

## Features

### Installation (through Mud Hut)

- Download from Adobe (Adobe's own servers and installer)
- Install from an offline package or an `.iso` disc image
- Copy from an existing Windows install

### App Library

- One tile per installed app: launch, and force quit if an app hangs
- Prefixes: provision, rename, check and repair fonts, forget, add an existing prefix
- Health checks from `neutron doctor`, shown in the sidebar

### Display & Appearance

- Display scale: Auto (match desktop) or a Windows scale step
- Color themes and caption-button icon sets for the apps' windows

### Setup & Updates

- Set up, Update and Uninstall buttons, which run `neutron setup`, `neutron update` and `neutron uninstall`

-----

## Planned

- Lightroom (cloud) support
- AMD and Intel GPU validation
- Snapshot/rollback and holding Adobe updates until verified
- Per-app GPU and color controls
- Distro packages (AUR)

-----

## Supported Applications

Collider launches the apps Neutron supports: Premiere Pro, After Effects, Photoshop, Lightroom Classic, Illustrator, Media Encoder and Animate. Status per app and per release is on the [status board](https://neutronproject.org) and in [Neutron's compatibility table](https://github.com/Nico-LaFoucate/neutron#compatibility).

-----

## Requirements

Collider has the same requirements as Neutron:

- 64-bit Linux with glibc 2.39 or newer (Ubuntu 24.04, Fedora 40, current Arch / CachyOS / Manjaro, openSUSE Tumbleweed) and a **Wayland** session
- An **NVIDIA GPU**. AMD and Intel GPUs are not validated yet.
- Up-to-date GPU drivers
- A valid Adobe subscription or existing installation

Neutron is tested on three machines, all CachyOS with KDE Plasma (Wayland) and NVIDIA GPUs.

-----

## Installation

`neutron setup` installs Collider and adds **Neutron Collider** to your application menu. See [Getting Started](https://github.com/Nico-LaFoucate/neutron#getting-started) in Neutron's README.

To build from source: `npm ci`, then `npm run tauri dev` for a development build, or `./release.sh` for the release AppImage (written to `dist/`). You need Rust, Node.js and Tauri's Linux system dependencies.

-----

## Legal

Collider does not include or distribute any proprietary software. Apps you install through its Mud Hut tab come from Adobe's own servers or from your own copies. Users are responsible for ensuring they have appropriate licenses for any software they run through Collider and Neutron.

Neutron is an independent project by Nico LaFoucate and Ficus Media Group. Adobe and its product names are trademarks of Adobe Inc. Neutron is not affiliated with or endorsed by Adobe.

-----

## License

Collider is licensed under the **Apache License, Version 2.0** (`Apache-2.0`). See [`LICENSE`](LICENSE) for the full text.

Neutron (the CLI) and neutron-wine are licensed under the GNU Lesser General Public License, version 2.1 or later (`LGPL-2.1-or-later`), the same as [Wine](https://www.winehq.org/).

-----

## Contributing

Contributions are very welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md). Please open an issue before submitting large pull requests so we can discuss the approach first.

-----

## Reporting bugs

Report problems with the Collider app on this repository's [Issues](https://github.com/Nico-LaFoucate/Collider/issues/new/choose). If an Adobe app misbehaves while running, report it on [Neutron's Issues](https://github.com/Nico-LaFoucate/Neutron/issues/new/choose) instead: that is where launching and running the apps are handled. Questions go to [Discussions](https://github.com/Nico-LaFoucate/Neutron/discussions). Report security problems privately: see [`SECURITY.md`](SECURITY.md).

-----

*Collider by [Nico LaFoucate](https://github.com/Nico-LaFoucate) - For Creators, Not Coders*