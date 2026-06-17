// core/decoration.rs
//
// Neutron window decoration (client-side). The Neutron wine build defaults to
// client-side decorations, so Wine draws Premiere's own non-client frame. Two
// pieces make it look right, both installed here (idempotently, on first run /
// before launch):
//
//   1. Prefix colors — a small .reg that sets HKCU\Control Panel\Colors so Wine's
//      title bar + menu bar render dark, matching Premiere's chrome. Applied with
//      `wine regedit` against the target prefix.
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
    // Generate the color .reg from the active theme (preset or custom) and import it.
    let settings = crate::core::settings::load();
    let colors = crate::core::theme::active_colors(&settings);
    let reg = crate::core::theme::reg_text(&colors);

    let tmp = std::env::temp_dir().join("neutron-premiere-dark.reg");
    std::fs::write(&tmp, reg).map_err(|e| format!("write reg: {e}"))?;

    let out = display::clean_command(wine_bin())
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .args(["regedit", tmp.to_string_lossy().as_ref()])
        .output()
        .map_err(|e| format!("spawn wine regedit: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Use the Neutron wine if Collider put it on PATH, else system `wine`.
fn wine_bin() -> &'static str {
    if which("neutron-wine") { "neutron-wine" } else { "wine" }
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

    #[test]
    fn light_reg_text_is_well_formed() {
        let txt = theme::reg_text(&theme::light());
        assert!(txt.starts_with("REGEDIT4"));
        assert!(txt.contains("[HKEY_CURRENT_USER\\Control Panel\\Colors]"));
        assert!(txt.contains("\"ActiveTitle\"=\"245 245 245\""));
        assert!(txt.contains("\"WindowText\"=\"20 20 20\""));
        // dark and light expose the same keys, so switching is total
        assert_eq!(theme::dark().keys().collect::<Vec<_>>(),
                   theme::light().keys().collect::<Vec<_>>());
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
}
