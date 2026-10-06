// core/decoration.rs
//
// Neutron window decoration (client-side). The Neutron wine build defaults to
// client-side decorations, so Wine draws Premiere's own non-client frame. Two
// pieces make it look right, both installed here (idempotently, on first run /
// before launch):
//
//   1. Prefix colors + window-button icons — the theme (HKCU\Control Panel\Colors) and the
//      caption icon set, both written into the prefix by the engine (`neutron theme apply` /
//      `neutron theme icons`); Collider only says which.
//
//   2. Overlay-positioning KWin script — Wayland forbids a client from positioning
//      its own toplevel, so Premiere's Home/Welcome overlay lands at the top,
//      covering the menu bar. The script (`neutron-home-overlay`) nudges it just
//      below the menu bar, reproducing the native Windows layout. KDE/Wayland only;
//      a clean no-op elsewhere (X11 doesn't need it).
//
// Nothing here is global: the colors live in the Wine prefix, and the KWin script
// only ever touches Premiere-class windows. Uninstall = remove the script + revert
// the prefix colors.

use crate::core::display;

const SCRIPT_ID: &str = "neutron-home-overlay";

// Bundled resources (shipped inside the Collider binary). The color .reg is no
// longer bundled — it is generated from the active theme (core/theme.rs).
const KWIN_METADATA: &str = include_str!("../../resources/kwin/metadata.json");
const KWIN_MAIN_JS: &str = include_str!("../../resources/kwin/main.js");

/// Apply the full Neutron decoration setup for `prefix` (a WINEPREFIX path).
/// Best-effort and idempotent; safe to call before every launch. Returns Ok(())
/// even when individual steps are skipped (non-KDE, X11, etc.).
pub fn apply(prefix: &str) -> Result<(), String> {
    // Prefix colors are platform-independent (they only theme Wine's own NC).
    apply_prefix_colors(prefix)?;
    // Custom caption-button icons (or disable them) — also platform-independent.
    apply_button_icons(prefix)?;
    // The overlay-positioning script is KDE/Wayland-only.
    if supported() {
        install_kwin_script()?;
    }
    Ok(())
}

/// Can this session host the overlay-positioning KWin script?
pub fn supported() -> bool {
    display::is_wayland() && display::desktop_kind() == "kde"
}

// ---- 1. prefix colors -----------------------------------------------------

fn apply_prefix_colors(prefix: &str) -> Result<(), String> {
    // ⭐ THE ENGINE OWNS THE THEME. Collider is the GUI front end — a graphical way to execute
    // what the Neutron CLI already does — so it must not carry its own copy of "how a theme is
    // written into a prefix". It decides WHICH colours (that is the user-facing half); the CLI
    // decides HOW, and applies the default.
    //
    // Before 2026-09-11 this function generated the .reg itself and ran `wine regedit`. Two
    // consequences, both bad: a prefix provisioned and launched from the CLI or a .desktop entry
    // never got the theme at all, and when the visual-style fix (ThemeActive=0) was found, it had
    // to be written here rather than in the engine — so it too would have reached GUI users only.
    //
    // ⛔ Do not reintroduce local .reg generation. If the engine needs to do something new with a
    // theme, add it to `neutron theme` and call it from here.
    let settings = crate::core::settings::load();

    // Send colours ONLY when the user has actually customized. Otherwise pass nothing and let the
    // engine write its own default, so there is exactly one definition of "the default theme".
    let custom: Option<crate::core::theme::ColorMap> = if settings.theme == "custom" {
        settings.custom_colors.clone().filter(|c| !c.is_empty())
    } else if settings.theme != "dark" {
        // A non-default built-in preset (e.g. Light) is a user choice like any other.
        crate::core::theme::preset(&settings.theme)
    } else {
        None
    };

    let neutron = neutron_bin().ok_or_else(|| "neutron CLI not found on PATH".to_string())?;
    let mut cmd = display::clean_command(&neutron);
    cmd.args(["--json", "theme", "apply", "--prefix", prefix]);
    if custom.is_some() {
        cmd.args(["--colors", "-"]);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn neutron theme apply: {e}"))?;
    if let Some(colors) = &custom {
        use std::io::Write;
        let json = serde_json::to_string(colors).map_err(|e| format!("serialize colors: {e}"))?;
        child.stdin.as_mut().ok_or("no stdin on neutron theme apply")?
            .write_all(json.as_bytes()).map_err(|e| format!("write colors: {e}"))?;
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().map_err(|e| format!("neutron theme apply: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let so = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Err(if err.is_empty() { so } else { err })
    }
}

/// The Neutron CLI: $NEUTRON_BIN, else `neutron` on PATH (main.rs puts ~/.local/bin on it).
fn neutron_bin() -> Option<String> {
    if let Ok(p) = std::env::var("NEUTRON_BIN") {
        if !p.is_empty() { return Some(p); }
    }
    if which("neutron") { Some("neutron".to_string()) } else { None }
}

// ---- caption-button icons -------------------------------------------------

/// Point the prefix at the configured caption-icon set (or Wine's glyphs for "none") through
/// `neutron theme icons`. The engine picks a wine that cannot re-stamp the prefix; Collider no
/// longer runs wine itself (it once ran distro wine here and clobbered the prefix twice per
/// launch -- see the engine's wine_for_prefix).
fn apply_button_icons(prefix: &str) -> Result<(), String> {
    let settings = crate::core::settings::load();
    let set = settings.button_icon_set.clone();
    let dir = crate::core::caption_icons::materialize(&set)?;
    let dir_s = dir.as_ref().map(|d| d.to_string_lossy().into_owned());
    crate::neutron::caption_icons(prefix, dir_s.as_deref().map(|d| (d, set.as_str())))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---- 2. overlay-positioning KWin script -----------------------------------

fn install_kwin_script() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "no HOME".to_string())?;
    let base = format!("{home}/.local/share/kwin/scripts/{SCRIPT_ID}");
    let code = format!("{base}/contents/code");
    std::fs::create_dir_all(&code).map_err(|e| format!("mkdir {code}: {e}"))?;
    std::fs::write(format!("{base}/metadata.json"), KWIN_METADATA)
        .map_err(|e| format!("write metadata: {e}"))?;
    std::fs::write(format!("{code}/main.js"), KWIN_MAIN_JS)
        .map_err(|e| format!("write main.js: {e}"))?;

    // Enable the plugin (loads on next login) and ask KWin to (re)load now.
    run_ok(display::clean_command(kwriteconfig()).args([
        "--file", "kwinrc", "--group", "Plugins",
        "--key", &format!("{SCRIPT_ID}Enabled"), "true",
    ]))?;
    reconfigure()
}

fn reconfigure() -> Result<(), String> {
    run_ok(display::clean_command("gdbus").args([
        "call", "--session",
        "--dest", "org.kde.KWin",
        "--object-path", "/KWin",
        "--method", "org.kde.KWin.reconfigure",
    ]))
}

// ---- shared helpers (mirrors window_rule.rs) ------------------------------

fn run_ok(cmd: &mut std::process::Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("spawn failed: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn kwriteconfig() -> &'static str {
    if which("kwriteconfig6") { "kwriteconfig6" } else { "kwriteconfig" }
}

fn which(bin: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v {bin}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use crate::core::theme;

    // ⛔ The reg_text tests moved to the engine with the code they covered — the .reg format and
    // the ThemeActive=0 rule are `bin/neutron`'s, tested there. What is worth asserting HERE is
    // the half Collider still owns: that the presets stay interchangeable.
    #[test]
    fn presets_expose_the_same_keys() {
        assert_eq!(theme::dark().keys().collect::<Vec<_>>(),
                   theme::light().keys().collect::<Vec<_>>(),
                   "dark and light must expose the same keys, or switching leaves a colour unset");
        assert_eq!(theme::preset("dark"), Some(theme::dark()));
        assert!(theme::preset("nonsense").is_none());
    }

    // Live end-to-end: apply the active theme (per ~/.config/collider/config.json) to a
    // real prefix via the exact production path. Ignored by default; run with:
    //   COLLIDER_TEST_PREFIX=~/.premiere2025 cargo test live_apply_colors -- --ignored
    #[test]
    #[ignore]
    fn live_apply_colors() {
        let prefix = std::env::var("COLLIDER_TEST_PREFIX")
            .expect("set COLLIDER_TEST_PREFIX to a WINEPREFIX");
        super::apply_prefix_colors(&prefix).expect("apply_prefix_colors failed");
    }

    // Live end-to-end for caption icons: installs the configured button_icon_set into a
    // real prefix through `neutron theme icons` (the production path).
    //   COLLIDER_TEST_PREFIX=~/.premiere2025 cargo test live_apply_icons -- --ignored
    #[test]
    #[ignore]
    fn live_apply_icons() {
        let prefix = std::env::var("COLLIDER_TEST_PREFIX")
            .expect("set COLLIDER_TEST_PREFIX to a WINEPREFIX");
        super::apply_button_icons(&prefix).expect("apply_button_icons failed");
    }

    // Live import smoke test: COLLIDER_TEST_IMPORT_DIR=<folder of close/min/max/restore.png>
    //   cargo test live_import -- --ignored
    #[test]
    #[ignore]
    fn live_import() {
        let dir = std::env::var("COLLIDER_TEST_IMPORT_DIR").expect("set COLLIDER_TEST_IMPORT_DIR");
        let n = crate::core::caption_icons::import_set(&dir).expect("import_set failed");
        assert!(n > 0);
        eprintln!("imported {n} buttons");
    }

    #[test]
    fn preview_is_valid_png_data_uri() {
        use base64::Engine;
        let uri = crate::core::caption_icons::preview_data_uri("win11").expect("preview");
        assert!(uri.starts_with("data:image/png;base64,"));
        let b = base64::engine::general_purpose::STANDARD
            .decode(uri.strip_prefix("data:image/png;base64,").unwrap())
            .expect("base64 decode");
        assert_eq!(&b[1..4], b"PNG"); // PNG signature
        assert!(crate::core::caption_icons::preview_data_uri("none").is_none());
    }

    // Dump set previews to /tmp for visual inspection:
    //   cargo test dump_previews -- --ignored
    #[test]
    #[ignore]
    fn dump_previews() {
        use base64::Engine;
        for id in ["macos", "win11", "minimal", "adobe-flat"] {
            let uri = crate::core::caption_icons::preview_data_uri(id).unwrap();
            let b = base64::engine::general_purpose::STANDARD
                .decode(uri.strip_prefix("data:image/png;base64,").unwrap())
                .unwrap();
            std::fs::write(format!("/tmp/preview_{id}.png"), b).unwrap();
        }
    }

    #[test]
    fn bundled_sets_materialize() {
        // every bundled id writes a complete folder for `neutron theme icons --from`
        for id in crate::core::caption_icons::SET_IDS {
            let dir = crate::core::caption_icons::materialize(id).expect("materialize");
            if *id == "none" { assert!(dir.is_none()); continue; }
            let dir = dir.expect("a folder");
            for b in ["close", "min", "max", "restore"] {
                assert!(dir.join(format!("{b}.ico")).exists(), "{id}: {b}.ico");
            }
        }
    }
}
