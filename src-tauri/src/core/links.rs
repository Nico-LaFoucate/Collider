// core/links.rs
//
// Opening a link (Support's donation pages, the bug form, the studios address) in the desktop's
// browser or mail app.
//
// tauri-plugin-shell's open() runs xdg-open with no activation token. On Wayland the compositor
// only lets a window take the focus when the request carries a token minted from the user's
// input, so the browser opened the page and stayed BEHIND Collider: KWin flags it for attention
// and nothing visible happens (Collider#3, "None of the buttons or links on the Support button do
// anything when clicked"). Measured on KDE 6.8 with a running browser: plain xdg-open left the
// browser behind 3/3, xdg-open with a token from the clicked window brought it forward 3/3.
//
// So, as a desktop launcher does: ask GTK for a launch context on Collider's own window (the
// click that called us is that window's latest input), take its startup-notify id (an
// xdg-activation token on Wayland, an X11 startup id on X11), and hand it to xdg-open in
// XDG_ACTIVATION_TOKEN and DESKTOP_STARTUP_ID. xdg-open passes it on to the browser or mail app.

use std::process::Stdio;

use gtk::prelude::*;

/// Schemes the UI may open. The donation, site and bug-form links are https; the studios
/// address is mailto. Anything else is refused (the plugin's default scope allowed the same).
fn allowed(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("mailto:")
}

/// Variables the AppImage's GTK hook and Collider's own main.rs set for COLLIDER's GTK and
/// WebKit. A browser or mail app started from here must get the desktop's values, not these:
/// GTK_THEME=Adwaita:dark would restyle a freshly started browser, and the module paths point
/// into the AppImage mount.
const APPIMAGE_GTK_VARS: [&str; 12] = [
    "GTK_THEME", "GTK_DATA_PREFIX", "GTK_EXE_PREFIX", "GTK_PATH", "GTK_IM_MODULE_FILE",
    "GDK_BACKEND", "GIO_EXTRA_MODULES", "GSETTINGS_SCHEMA_DIR", "GI_TYPELIB_PATH",
    "WEBKIT_DISABLE_DMABUF_RENDERER", "DESKTOP_STARTUP_ID", "XDG_ACTIVATION_TOKEN",
];

/// An activation token for `window`, from GTK's launch context. None when GTK has none to give
/// (no seat focus, or a compositor without xdg-activation); the link still opens, unfocused.
/// Must run on the main thread (GTK).
fn activation_token(window: &tauri::WebviewWindow) -> Option<String> {
    let gtk_window = window.gtk_window().ok()?;
    let ctx = WidgetExt::display(&gtk_window).app_launch_context()?;
    ctx.set_timestamp(gtk::current_event_time());
    // GTK's Wayland and X11 launch contexts ignore the app info; gio's binding requires one.
    let info = gio::AppInfo::create_from_commandline("xdg-open", None, gio::AppInfoCreateFlags::NONE).ok()?;
    ctx.startup_notify_id(&info, &[]).map(|id| id.to_string()).filter(|id| !id.is_empty())
}

/// The desktop's own XDG_DATA_DIRS, as it was before the AppImage's GTK hook prepended
/// "$APPDIR/usr/share:/usr/share:". That list is where xdg-open finds the default browser's
/// .desktop file, Flatpak browsers included (their files live in the flatpak exports dirs the
/// session adds), so it must come back exactly: Some(value) to set, None to leave the variable
/// unset (the desktop never set it, so the spec's default applies).
fn desktop_data_dirs() -> Option<String> {
    restore_data_dirs(
        &std::env::var("XDG_DATA_DIRS").unwrap_or_default(),
        &std::env::var("APPDIR").unwrap_or_default(),
    )
}

fn restore_data_dirs(dirs: &str, appdir: &str) -> Option<String> {
    let dirs = dirs.to_string();
    if appdir.is_empty() {
        return (!dirs.is_empty()).then_some(dirs);
    }
    let hook = format!("{appdir}/usr/share:/usr/share:");
    let original = match dirs.strip_prefix(&hook) {
        Some(rest) => rest.to_string(),
        // Not the hook's shape: keep everything that is not inside the AppImage.
        None => dirs.split(':').filter(|d| !d.is_empty() && !d.starts_with(&appdir)).collect::<Vec<_>>().join(":"),
    };
    (!original.is_empty()).then_some(original)
}

/// Open `url` with xdg-open, handing it an activation token from `window`.
pub fn open_link(window: &tauri::WebviewWindow, url: &str) -> Result<(), String> {
    if !allowed(url) {
        return Err(format!("refusing to open {url}: only https:// and mailto: links"));
    }
    let token = activation_token(window);
    let mut cmd = crate::core::display::clean_command("xdg-open");
    for var in APPIMAGE_GTK_VARS {
        cmd.env_remove(var);
    }
    match desktop_data_dirs() {
        Some(dirs) => cmd.env("XDG_DATA_DIRS", dirs),
        None => cmd.env_remove("XDG_DATA_DIRS"),
    };
    if let Some(t) = &token {
        cmd.env("XDG_ACTIVATION_TOKEN", t).env("DESKTOP_STARTUP_ID", t);
    }
    cmd.arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("could not run xdg-open: {e}"))?;
    // Reap it: xdg-open returns once the browser has the URL.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::restore_data_dirs;

    const APPDIR: &str = "/tmp/.mount_ColliXYZ";

    #[test]
    fn data_dirs_come_back_exactly_with_flatpak_exports() {
        let session = "/home/u/.local/share/flatpak/exports/share:/var/lib/flatpak/exports/share:/usr/local/share:/usr/share";
        let hooked = format!("{APPDIR}/usr/share:/usr/share:{session}");
        assert_eq!(restore_data_dirs(&hooked, APPDIR).as_deref(), Some(session));
    }

    #[test]
    fn data_dirs_unset_by_the_desktop_stay_unset() {
        // The hook turned an unset variable into "$APPDIR/usr/share:/usr/share:".
        assert_eq!(restore_data_dirs(&format!("{APPDIR}/usr/share:/usr/share:"), APPDIR), None);
    }

    #[test]
    fn data_dirs_in_another_shape_lose_only_the_appimage_entries() {
        let dirs = format!("/usr/share:{APPDIR}/usr/share:/var/lib/flatpak/exports/share");
        assert_eq!(restore_data_dirs(&dirs, APPDIR).as_deref(), Some("/usr/share:/var/lib/flatpak/exports/share"));
    }

    #[test]
    fn data_dirs_outside_an_appimage_are_untouched() {
        assert_eq!(restore_data_dirs("/a:/b", "").as_deref(), Some("/a:/b"));
        assert_eq!(restore_data_dirs("", ""), None);
    }
}
