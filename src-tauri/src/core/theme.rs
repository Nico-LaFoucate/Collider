// core/theme.rs
//
// Window-decoration color themes for the Neutron CSD frame. Premiere's self-drawn
// non-client frame (title bar + menu bar + caption buttons) is themed entirely by
// the Wine prefix's HKCU\Control Panel\Colors, so a "theme" here is just a set of
// those color values. This module is the single source of truth for the built-in
// presets and turns a color set into the .reg text that core/decoration.rs imports
// into the prefix on launch.
//
// Caption-button states map onto these colors (see neutron-wine
// neutron-caption-buttons): ActiveTitle = normal button bg, ButtonShadow = pressed,
// ButtonHilight = min/max hover, ButtonText = glyph. The close-hover red is hardcoded
// in the wine patch (intentional, like Windows) and is NOT themeable here.

use std::collections::BTreeMap;

/// A color set: Control Panel color key -> "R G B" (the format Wine expects).
/// BTreeMap keeps the generated .reg deterministic (stable key order).
pub type ColorMap = BTreeMap<String, String>;

/// Built-in preset ids, in display order. ("match-system" lands in M4.)
pub const PRESET_IDS: &[&str] = &["dark", "light"];

/// Human label for a preset id (for the Preferences dropdown).
pub fn preset_label(id: &str) -> &'static str {
    match id {
        "dark" => "Dark",
        "light" => "Light",
        "custom" => "Custom",
        _ => "Dark",
    }
}

/// The color map for a built-in preset id, or None if unknown ("custom" is not a
/// preset — its colors live in Settings).
pub fn preset(id: &str) -> Option<ColorMap> {
    match id {
        "dark" => Some(dark()),
        "light" => Some(light()),
        _ => None,
    }
}

// `active_colors()` also removed 2026-09-11: the preset-vs-custom decision now sits in
// `decoration::apply_prefix_colors`, next to the call that acts on it, because the two answers
// differ — a customized theme is sent to the engine, the default is left to the engine.


// ⛔ `reg_text()` lived here until 2026-09-11 and is GONE ON PURPOSE.
//
// Writing a theme into a prefix is the ENGINE's job: `neutron theme apply`. Collider is the GUI
// front end — it decides WHICH colours (presets above, or the user's custom set) and hands them to
// the CLI on stdin. It does not decide HOW they are written.
//
// This is not pedantry. While the .reg lived here, two things followed: a prefix provisioned and
// launched from the CLI or a .desktop entry never got the theme at all, and the visual-style fix
// (ThemeActive=0, without which comctl32 v6 controls ignore the palette entirely) would have
// shipped to GUI users only. Both were invisible until someone looked at a file dialog.
//
// The default theme and the .reg format now live in `bin/neutron` (DEFAULT_THEME_COLORS /
// theme_reg_text), with the round-trip test beside them.


fn map(pairs: &[(&str, &str)]) -> ColorMap {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// Dark preset — the Neutron default. Matches the original bundled
/// neutron-premiere-dark.reg (dark chrome + flat-button palette).
pub fn dark() -> ColorMap {
    map(&[
        // title bar
        ("ActiveTitle", "43 43 43"),
        ("GradientActiveTitle", "43 43 43"),
        ("TitleText", "224 224 224"),
        ("InactiveTitle", "32 32 32"),
        ("GradientInactiveTitle", "32 32 32"),
        ("InactiveTitleText", "140 140 140"),
        ("ActiveBorder", "43 43 43"),
        ("InactiveBorder", "32 32 32"),
        ("WindowFrame", "20 20 20"),
        // menu bar
        ("Menu", "43 43 43"),
        ("MenuText", "224 224 224"),
        ("MenuBar", "43 43 43"),
        ("MenuHilight", "64 64 64"),
        // window / general
        ("Window", "43 43 43"),
        ("WindowText", "224 224 224"),
        ("HilightText", "255 255 255"),
        // the classic scrollbar TRACK (the thumb and arrows follow the button colors below).
        // Same value as the engine's default theme (`DEFAULT_THEME_COLORS` in bin/neutron);
        // without it here, a theme Collider sends could not set the track, and a prefix
        // provisioned with the dark default kept dark tracks under the Light preset.
        ("Scrollbar", "30 30 30"),
        // dialogs & controls — the surfaces Wine draws for MESSAGE BOXES, common dialogs and
        // any control an app does not draw itself. Themed, deliberately: the whole point is that
        // a popup matches the rest of the dark UI.
        //
        // ⚠️ These no longer touch the chrome. The caption buttons, the window frame and the rule
        // under the menu bar used to read COLOR_BTNTEXT/BTNHIGHLIGHT/BTNSHADOW/3DFACE, which
        // coupled the title bar to this group; neutron-wine patches zzzzzzzzzp/q/r moved them
        // onto the caption, border and menu colours. So this group is now free to be exactly what
        // its name says, and darkening it cannot put a white border anywhere.
        ("ButtonFace", "43 43 43"),
        ("ButtonHilight", "60 60 60"),
        ("ButtonLight", "50 50 50"),
        ("ButtonShadow", "30 30 30"),
        ("ButtonDkShadow", "16 16 16"),
        ("ButtonText", "224 224 224"),
        ("3DLight", "60 60 60"),
        ("3DDarkShadow", "20 20 20"),
    ])
}

/// Light preset — a clean light chrome (same key set as Dark so switching is total).
pub fn light() -> ColorMap {
    map(&[
        // title bar
        ("ActiveTitle", "245 245 245"),
        ("GradientActiveTitle", "245 245 245"),
        ("TitleText", "20 20 20"),
        ("InactiveTitle", "235 235 235"),
        ("GradientInactiveTitle", "235 235 235"),
        ("InactiveTitleText", "120 120 120"),
        ("ActiveBorder", "245 245 245"),
        ("InactiveBorder", "235 235 235"),
        ("WindowFrame", "180 180 180"),
        // menu bar
        ("Menu", "245 245 245"),
        ("MenuText", "20 20 20"),
        ("MenuBar", "245 245 245"),
        ("MenuHilight", "210 210 210"),
        // window / general
        ("Window", "255 255 255"),
        ("WindowText", "20 20 20"),
        ("HilightText", "255 255 255"),
        // scrollbar track: Windows' default light gray.
        ("Scrollbar", "200 200 200"),
        // dialogs & controls — see the note in dark(); these are app-facing surfaces, not chrome.
        ("ButtonFace", "245 245 245"),
        ("ButtonHilight", "255 255 255"),
        ("ButtonLight", "235 235 235"),
        ("ButtonShadow", "160 160 160"),
        ("ButtonDkShadow", "120 120 120"),
        ("ButtonText", "20 20 20"),
        ("3DLight", "235 235 235"),
        ("3DDarkShadow", "120 120 120"),
    ])
}
