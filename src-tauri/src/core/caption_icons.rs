// core/caption_icons.rs
//
// Custom window-button (caption) icon sets for the Neutron CSD frame. The Neutron wine build's
// user32 draws per-button .ico files over the themed button background when
// HKCU\Software\Neutron\Caption\Enabled is set (see neutron-wine neutron-caption-buttons).
// This module owns the SETS: the bundled ones (embedded in the Collider binary), the user's
// imported one, and their previews. Putting a set into a prefix is the engine's job:
// `neutron theme icons --prefix P --from <folder>` (DECISIONS C20). Collider hands it a folder.

use std::path::Path;

/// Built-in set ids in display order. "none" = Wine's default Marlett glyphs.
pub const SET_IDS: &[&str] = &["none", "macos", "win11", "minimal", "adobe-flat"];

/// Human label for a set id (for the Appearance picker).
pub fn label(id: &str) -> &'static str {
    match id {
        "none" => "Default glyphs",
        "macos" => "macOS",
        "win11" => "Windows 11",
        "minimal" => "Minimal",
        "adobe-flat" => "Adobe flat",
        "custom" => "Custom (imported)",
        _ => id_is_unknown(),
    }
}
fn id_is_unknown() -> &'static str { "Custom" }

/// The four button .ico files for a bundled set, embedded at compile time.
type IconSet = &'static [(&'static str, &'static [u8])];

macro_rules! set {
    ($dir:literal) => {
        &[
            ("close.ico",   include_bytes!(concat!("../../resources/caption-icons/", $dir, "/close.ico")) as &[u8]),
            ("min.ico",     include_bytes!(concat!("../../resources/caption-icons/", $dir, "/min.ico")) as &[u8]),
            ("max.ico",     include_bytes!(concat!("../../resources/caption-icons/", $dir, "/max.ico")) as &[u8]),
            ("restore.ico", include_bytes!(concat!("../../resources/caption-icons/", $dir, "/restore.ico")) as &[u8]),
        ]
    };
}

fn bundled(id: &str) -> Option<IconSet> {
    match id {
        "macos" => Some(set!("macos")),
        "win11" => Some(set!("win11")),
        "minimal" => Some(set!("minimal")),
        "adobe-flat" => Some(set!("adobe-flat")),
        _ => None,
    }
}

/// A folder holding the set's close/min/max/restore .ico files, for `neutron theme icons
/// --from`: bundled sets are written out to ~/.cache/collider/caption-icons/<id>/, the imported
/// set is its own folder. Ok(None) for "none" (Wine's default glyphs).
pub fn materialize(id: &str) -> Result<Option<std::path::PathBuf>, String> {
    if id == "none" || id.is_empty() {
        return Ok(None);
    }
    if id == "custom" {
        let dir = custom_set_dir().ok_or("no HOME for custom icons")?;
        if !dir.join("close.ico").exists() {
            return Err("no imported icons found".into());
        }
        return Ok(Some(dir));
    }
    let set = bundled(id).ok_or_else(|| format!("unknown icon set: {id}"))?;
    let home = std::env::var_os("HOME").ok_or("no HOME")?;
    let dir = Path::new(&home).join(".cache/collider/caption-icons").join(id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    for (name, bytes) in set {
        std::fs::write(dir.join(name), bytes).map_err(|e| format!("write {name}: {e}"))?;
    }
    Ok(Some(dir))
}

/// Raw .ico bytes for one button of a set (bundled from the embedded table, or read from
/// the imported "custom" dir). Used for preview thumbnails.
fn icon_bytes(id: &str, button: &str) -> Option<Vec<u8>> {
    let fname = format!("{button}.ico");
    if let Some(set) = bundled(id) {
        return set.iter().find(|(n, _)| *n == fname).map(|(_, b)| b.to_vec());
    }
    if id == "custom" {
        return std::fs::read(custom_set_dir()?.join(&fname)).ok();
    }
    None
}

/// A small mini-title-bar PNG preview of the set (min, max, close on a dark strip),
/// returned as a `data:image/png;base64,...` URI for the Appearance picker. None for
/// "none" or a set with no loadable icons.
pub fn preview_data_uri(id: &str) -> Option<String> {
    use base64::Engine;
    use image::{imageops, imageops::FilterType, Rgba, RgbaImage};

    if id == "none" {
        return None;
    }
    let (cell, isz) = (22u32, 15u32);
    let pad = ((cell - isz) / 2) as i64;
    let mut canvas = RgbaImage::from_pixel(cell * 3, cell, Rgba([43, 43, 43, 255]));
    for (i, b) in ["min", "max", "close"].iter().enumerate() {
        let bytes = icon_bytes(id, b)?;
        let img = image::load_from_memory(&bytes)
            .ok()?
            .resize_exact(isz, isz, FilterType::Lanczos3)
            .to_rgba8();
        imageops::overlay(&mut canvas, &img, i as i64 * cell as i64 + pad, pad);
    }
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(canvas)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png)
    ))
}

/// Directory where imported custom .ico files are stored (~/.config/collider/caption-custom).
pub fn custom_set_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join(".config/collider/caption-custom"))
}

/// Import a user icon set from `src_dir`: looks for close/min/max/restore as `.ico`
/// (copied as-is) or a raster image (`.png/.jpg/.jpeg/.bmp`, re-encoded to 64x64 `.ico`),
/// writing the results into custom_set_dir(). Returns how many buttons were imported.
/// Errors if the folder has none of the four.
pub fn import_set(src_dir: &str) -> Result<u32, String> {
    let dest = custom_set_dir().ok_or("no HOME for custom icons")?;
    std::fs::create_dir_all(&dest).map_err(|e| format!("mkdir custom dir: {e}"))?;

    let mut found = 0u32;
    for name in ["close", "min", "max", "restore"] {
        let out = dest.join(format!("{name}.ico"));
        let ico = Path::new(src_dir).join(format!("{name}.ico"));
        if ico.exists() {
            std::fs::copy(&ico, &out).map_err(|e| format!("copy {name}.ico: {e}"))?;
            found += 1;
            continue;
        }
        for ext in ["png", "PNG", "jpg", "jpeg", "bmp"] {
            let p = Path::new(src_dir).join(format!("{name}.{ext}"));
            if p.exists() {
                let img = image::open(&p).map_err(|e| format!("decode {name}.{ext}: {e}"))?;
                img.resize(64, 64, image::imageops::FilterType::Lanczos3)
                    .save_with_format(&out, image::ImageFormat::Ico)
                    .map_err(|e| format!("encode {name}.ico: {e}"))?;
                found += 1;
                break;
            }
        }
    }
    if found == 0 {
        // wipe a possibly-stale partial set so "custom" doesn't linger half-populated
        let _ = std::fs::remove_file(dest.join("close.ico"));
        return Err("no close/min/max/restore image (.png/.jpg/.bmp/.ico) found in that folder".into());
    }
    Ok(found)
}
