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
    // Generate the color .reg from the active theme (preset or custom) and import it.
    let settings = crate::core::settings::load();
    let colors = crate::core::theme::active_colors(&settings);
    let reg = crate::core::theme::reg_text(&colors);

    let Some(wine) = wine_bin(prefix) else {
        return Err("no Neutron wine matches this prefix's stamp; skipping prefix colors \
                    rather than running distro wine (which would wineboot-clobber it)"
            .to_string());
    };

    let tmp = std::env::temp_dir().join("neutron-premiere-dark.reg");
    std::fs::write(&tmp, reg).map_err(|e| format!("write reg: {e}"))?;

    let out = display::clean_command(&wine)
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

// ---- caption-button icons -------------------------------------------------

/// Install the configured caption-icon set into the prefix and point the wine side at
/// it via HKCU\Software\Neutron\Caption (or disable it for "none"). Best-effort.
fn apply_button_icons(prefix: &str) -> Result<(), String> {
    let settings = crate::core::settings::load();
    let win_dir = crate::core::caption_icons::install(&settings.button_icon_set, prefix)?;

    let reg = match &win_dir {
        Some(dir) => format!(
            "REGEDIT4\r\n\r\n[HKEY_CURRENT_USER\\Software\\Neutron\\Caption]\r\n\
             \"Enabled\"=dword:00000001\r\n\"IconDir\"=\"{}\"\r\n",
            dir.replace('\\', "\\\\")
        ),
        None => String::from(
            "REGEDIT4\r\n\r\n[HKEY_CURRENT_USER\\Software\\Neutron\\Caption]\r\n\
             \"Enabled\"=dword:00000000\r\n",
        ),
    };

    let Some(wine) = wine_bin(prefix) else {
        return Err("no Neutron wine matches this prefix's stamp; skipping caption icons \
                    rather than running distro wine (which would wineboot-clobber it)"
            .to_string());
    };

    let tmp = std::env::temp_dir().join("neutron-caption.reg");
    std::fs::write(&tmp, reg).map_err(|e| format!("write reg: {e}"))?;
    let out = display::clean_command(&wine)
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

/// The wine binary to run against `prefix` — ALWAYS the runtime that prefix is
/// stamped for, NEVER a bare PATH lookup.
///
/// ⛔ This used to be `if which("neutron-wine") { .. } else { "wine" }`. No
/// `neutron-wine` existed on PATH, so every decoration pass ran **distro wine**
/// (`/usr/bin/wine`). Its `share/wine/wine.inf` has a different mtime than the
/// pinned runtime's, and wine re-runs the full `wine.inf` install whenever that
/// timestamp differs from `<prefix>/.update-timestamp` — so decoration fired a
/// `wineboot -u` that reinstalled system32 from `/usr/lib/wine`, reverting our
/// patched natives (dwrite 520470 -> 508105, the build APPS.md says wedges).
/// The subsequent real launch, on the pinned runtime, saw the now-distro stamp
/// and fired a SECOND wineboot. Two full prefix reinstalls per launch, silently,
/// on the user's 119 GB daily driver. Found 2026-08-08 by watching /proc for
/// `rundll32 ... InstallHinfSection ... Z:\usr\share\wine\wine.inf`.
///
/// Resolution order — each candidate must exist and be executable:
///   1. `$NEUTRON_WINE` (the engine honours this too)
///   2. the runtime whose `share/wine/wine.inf` mtime == the prefix's stamp
///      (same rule as `preserved-fixes/harnesses/runtime_for_prefix.sh`)
///   3. `neutron-wine` on PATH (the symlink backstop)
///
/// Returns None rather than falling back to `wine`. Decoration is cosmetic;
/// skipping it costs a dark title bar, while guessing costs the prefix.
fn wine_bin(prefix: &str) -> Option<String> {
    if let Ok(w) = std::env::var("NEUTRON_WINE") {
        if is_exec(std::path::Path::new(&w)) {
            return Some(w);
        }
    }
    if let Some(w) = runtime_wine_for_prefix(prefix) {
        return Some(w);
    }
    if which("neutron-wine") {
        return Some("neutron-wine".to_string());
    }
    None
}

fn is_exec(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// The installed runtime this prefix is stamped for, matched on `wine.inf` mtime.
/// Running any other build against it triggers the wineboot clobber described above.
fn runtime_wine_for_prefix(prefix: &str) -> Option<String> {
    let stamp_raw = std::fs::read_to_string(std::path::Path::new(prefix).join(".update-timestamp"))
        .ok()?;
    // The file is CRLF-terminated; keep digits only.
    let stamp: u64 = stamp_raw
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;

    let home = std::env::var("HOME").ok()?;
    let root = std::path::Path::new(&home).join(".local/share/neutron/runtimes");
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let inf = entry.path().join("share/wine/wine.inf");
        let Ok(meta) = std::fs::metadata(&inf) else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        let Ok(secs) = mtime.duration_since(std::time::UNIX_EPOCH) else { continue };
        if secs.as_secs() == stamp {
            let wine = entry.path().join("bin/wine");
            if is_exec(&wine) {
                return Some(wine.to_string_lossy().into_owned());
            }
        }
    }
    None
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

    // Live end-to-end for caption icons: installs the configured button_icon_set into a
    // real prefix + writes HKCU\Software\Neutron\Caption via the production path.
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
    fn bundled_sets_install_idempotently() {
        // every bundled id resolves and yields a Windows icon dir (smoke test, no prefix writes)
        for id in crate::core::caption_icons::SET_IDS {
            if *id == "none" { continue; }
            // install into a temp dir
            let tmp = std::env::temp_dir().join(format!("collider-icontest-{id}"));
            let p = tmp.to_string_lossy().to_string();
            let r = crate::core::caption_icons::install(id, &p).expect("install");
            assert!(r.is_some(), "{id} should enable icons");
            assert!(std::path::Path::new(&p)
                .join("drive_c/windows/neutron/caption")
                .join(id).join("close.ico").exists());
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }
}
