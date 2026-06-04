# Collider

### by Nico LaFoucate

**Collider** is a graphical launcher and environment manager that brings the Adobe Creative Suite to Linux, powered by [Neutron](https://github.com/Nico-LaFoucade/neutron) — a Wine-based compatibility layer built specifically for Adobe applications.

> ⚠️ **Early Development** — Collider is currently in active development. Features listed below represent the planned scope of the project. Contributions are welcome.

-----

## What is Collider?

Collider gives Linux users a friendly, GUI-based way to install, configure, and launch Adobe Creative Suite applications without touching the command line. Think of it as a creative-focused alternative to Lutris, built from the ground up for Adobe’s ecosystem.

Under the hood, Collider manages a shared Wine prefix through Neutron, preserving full Adobe Dynamic Link interoperability between apps like Premiere Pro, After Effects, Audition, and Photoshop.

-----

## Powered by Neutron

Collider is the user-facing frontend. **Neutron** is the compatibility engine underneath it — a patched Wine environment with custom DXVK and DirectComposition implementations that make Adobe apps actually run.

- Neutron repo: [github.com/Nico-LaFoucade/neutron](https://github.com/Nico-LaFoucade/neutron)

-----

## Planned Features

### Installation

- Adobe Creative Cloud installer (legitimate, via Adobe servers)
- ISO / disc image mount
- Pre-installed or copied Windows directory
- Offline installer package
- Network share / NAS path
- Portable / USB installation
- Import from existing Proton prefix

### App Library

- Per-app launch profiles with isolated settings
- Shared Adobe Wine prefix for full Dynamic Link support
- Snapshot and rollback before any update
- Run multiple Adobe versions side by side
- Update gating — hold Adobe updates until Neutron confirms compatibility

### Performance & Display

- GPU acceleration mode (DXVK / software fallback)
- VRAM allocation and CPU thread controls
- XWayland / Wayland toggle per app
- Color profile passthrough
- HDR output toggle
- Real-time GPU/CPU/RAM performance overlay

### Adobe-Specific

- Media cache location and size management
- Scratch disk configuration
- Auto-clear cache on launch
- Persistent Creative Cloud login across apps
- Project-based launch profiles
- Integrated project file backup

### Compatibility

- DLL override manager with per-DLL status indicators
- Wine version switcher
- Plain-English compatibility check on first launch
- Community hardware compatibility reports (like ProtonDB for Adobe)
- Per-app debug logging

### Power User

- Plugin manager for Neutron-verified plugins
- Render queue as background Linux system jobs
- One-click migration from Windows installations
- Hardware profile switching (AC/battery/eGPU)

-----

## Supported Applications

|Application             |Status       |
|------------------------|-------------|
|Adobe Premiere Pro 2025 |🔧 In Progress|
|Adobe Photoshop 2025    |🔧 In Progress|
|Adobe After Effects 2025|📋 Planned    |
|Adobe Audition 2025     |📋 Planned    |
|Adobe Illustrator 2025  |📋 Planned    |

-----

## Requirements

> Full requirements will be documented once Neutron reaches a stable release. General prerequisites:

- A modern Linux distribution (CachyOS, Arch, Ubuntu 22.04+)
- Vulkan-capable GPU (NVIDIA or AMD)
- Up-to-date GPU drivers
- A valid Adobe subscription or existing installation

-----

## Installation

> Installer coming soon. For now, see [Neutron](https://github.com/Nico-LaFoucade/neutron) for manual setup instructions.

-----

## Legal

Collider does not include, distribute, or facilitate the acquisition of any proprietary software. Users are responsible for ensuring they have appropriate licenses for any software they run through Collider and Neutron.

Adobe, Premiere Pro, After Effects, Photoshop, Audition, and Illustrator are trademarks of Adobe Inc. Smack Studio and Collider are not affiliated with or endorsed by Adobe Inc.

-----

## License

Collider is licensed under the [Apache License 2.0](LICENSE).

Neutron is licensed under the GNU Lesser General Public License v2.1, inherited from [Wine](https://www.winehq.org/).

-----

## Contributing

Contributions are very welcome. Please open an issue before submitting large pull requests so we can discuss the approach first.

-----

*Collider by [Smack Studio](https://github.com/Nico-LaFoucades) — Smash the barrier.*