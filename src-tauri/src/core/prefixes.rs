// core/prefixes.rs
//
// The prefix registry: which wine prefixes Collider knows about, and which one is selected.
//
// Why a registry and not a single path. Collider used to hardcode "~/.premiere2025",
// which meant that on anyone else's machine it opened against a nonexistent prefix and rendered
// an empty library with no way to fix it. But one settable path is still the wrong shape — the
// documented scope is multi-prefix (kickoff brief: "app library, profiles, prefix registry";
// README: "run multiple Adobe versions side by side", "import from existing Proton prefix"), and
// a real setup already has several. `.premiere2025` in particular is a holdover from before the
// project covered the whole suite, so prefixes carry USER-CHOSEN NAMES; the path is not the label.
//
// Collider passes the selected path EXPLICITLY on every engine call, so Neutron's own
// DEFAULT_PREFIX never applies to the GUI and we don't have to change the CLI's behaviour.

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::core::settings::{self, PrefixEntry};

/// Where a brand-new prefix should go. Namespaced so sibling prefixes (side-by-side versions,
/// imports) have somewhere obvious to live, and so it doesn't collide with an existing ~/Adobe.
///
/// ⚠️ NOT `~/Neutron` -- that was the default until 2026-09-14 and it is a trap. The name differs
/// from the engine's own directory only by case, so `~/Neutron` (prefixes) sits next to `~/neutron`
/// (the source tree on a dev box) and to `~/.local/share/neutron` (the shipped runtime): confusing
/// to read, and an outright collision on a case-insensitive filesystem. Operator, 2026-09-14:
/// "we would have two folders with the same name except one is the actual shipped runtime and the
/// other is the prefix directory". `-Prefixes` says what it holds and cannot collide.
pub fn default_new_prefix_path() -> String {
    home().map(|h| h.join("Neutron-Prefixes/Adobe").to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// A wine prefix is identified by drive_c. Cheap, local, and the same test the engine uses
/// (`resolve_prefix` returns None without it) — so this never disagrees with the CLI.
pub fn is_prefix(path: &Path) -> bool {
    path.join("drive_c").is_dir()
}

/// Adobe applications installed in a prefix. Counted from the filesystem rather than via
/// `neutron apps` because this renders a LIST — several prefixes at once, on every open — and
/// a CLI round-trip each would make the tab sluggish. The Apps view still uses the engine as
/// the authority for what is actually launchable.
fn app_names(path: &Path) -> Vec<String> {
    let dir = path.join("drive_c/Program Files/Adobe");
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out: Vec<String> = rd
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        // "Common" is Adobe's shared-components directory, not an application.
        .filter(|n| n != "Common" && n != "Common Files")
        .collect();
    out.sort();
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct PrefixView {
    pub name: String,
    pub path: String,
    pub valid: bool,
    pub selected: bool,
    pub apps: Vec<String>,
}

/// The registry as the UI should render it. Validity is re-checked on every call so a prefix
/// that was deleted or unmounted shows as broken instead of silently failing at launch.
pub fn list() -> Vec<PrefixView> {
    let s = settings::load();
    s.prefixes
        .iter()
        .map(|e| {
            let p = PathBuf::from(&e.path);
            PrefixView {
                name: e.name.clone(),
                path: e.path.clone(),
                valid: is_prefix(&p),
                selected: s.selected_prefix.as_deref() == Some(e.path.as_str()),
                apps: app_names(&p),
            }
        })
        .collect()
}

/// Prefixes present on disk that aren't registered yet, and that actually contain Adobe apps.
///
/// Scans only places a prefix plausibly lives — $HOME's immediate children, ~/Neutron-Prefixes,
/// ~/Neutron (the pre-2026-09-14 default, kept so existing installs are not orphaned), and the
/// XDG data dir beside the runtimes. Deliberately NOT a deep filesystem walk: $HOME can be huge,
/// and a slow scan on every tab open would be worse than missing an exotic location the user can
/// still add by hand.
///
/// ⚠️ Requires at least one Adobe app. A bare drive_c test is far too loose: this machine has 12
/// wine prefixes under $HOME and only 5 hold Adobe installs — the rest are `.wine`, a games
/// prefix, and throwaway test prefixes. Suggesting those as somewhere to launch Photoshop from
/// is worse than suggesting nothing. Anything unusual can still be added by hand.
pub fn discover() -> Vec<String> {
    let Some(h) = home() else { return Vec::new() };
    let known: Vec<String> = settings::load().prefixes.into_iter().map(|e| e.path).collect();

    // ⚠️ `Neutron-Prefixes` is the current default; `Neutron` is kept because it WAS the default
    // until 2026-09-14 and existing installs have prefixes there. Dropping it would orphan them --
    // discovery is the only thing standing between a user and "no wine prefix found".
    let mut roots = vec![
        h.clone(),
        h.join("Neutron-Prefixes"),
        h.join("Neutron"),
        h.join(".local/share/neutron/prefixes"),
    ];
    roots.retain(|r| r.is_dir());

    let mut found = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            if !p.is_dir() || !is_prefix(&p) || app_names(&p).is_empty() {
                continue;
            }
            let s = p.to_string_lossy().into_owned();
            if !known.contains(&s) && !found.contains(&s) {
                found.push(s);
            }
        }
    }
    found.sort();
    found
}

/// A reasonable label for a newly discovered/added prefix: the directory name, minus a leading
/// dot so `.premiere2025` reads as "premiere2025" rather than looking like a hidden file.
pub fn suggest_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().trim_start_matches('.').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string())
}

fn save(mut f: impl FnMut(&mut settings::Settings)) -> Result<(), String> {
    let mut s = settings::load();
    f(&mut s);
    settings::save(&s).map_err(|e| e.to_string())
}

/// Register a prefix. Idempotent on path; selects it if nothing is selected yet, so adding your
/// first prefix doesn't then require a second click to use it.
pub fn add(path: &str, name: Option<&str>) -> Result<(), String> {
    if !is_prefix(Path::new(path)) {
        return Err(format!("not a wine prefix (no drive_c): {path}"));
    }
    let label = name.map(str::to_string).unwrap_or_else(|| suggest_name(path));
    save(|s| {
        if let Some(e) = s.prefixes.iter_mut().find(|e| e.path == path) {
            e.name = label.clone();
        } else {
            s.prefixes.push(PrefixEntry { name: label.clone(), path: path.to_string() });
        }
        if s.selected_prefix.is_none() {
            s.selected_prefix = Some(path.to_string());
        }
    })
}

/// Forget a prefix. Removes the registry entry ONLY — never touches the prefix on disk, which
/// may hold a 119 GB Adobe install. If it was selected, fall back to the first remaining entry.
pub fn remove(path: &str) -> Result<(), String> {
    save(|s| {
        s.prefixes.retain(|e| e.path != path);
        if s.selected_prefix.as_deref() == Some(path) {
            s.selected_prefix = s.prefixes.first().map(|e| e.path.clone());
        }
    })
}

pub fn rename(path: &str, name: &str) -> Result<(), String> {
    save(|s| {
        if let Some(e) = s.prefixes.iter_mut().find(|e| e.path == path) {
            e.name = name.to_string();
        }
    })
}

pub fn select(path: &str) -> Result<(), String> {
    save(|s| s.selected_prefix = Some(path.to_string()))
}

/// The prefix the UI should work in: the selected entry if it's still valid, else the first
/// valid registered one. None means first-run setup — there is nothing usable to open.
pub fn selected() -> Option<String> {
    let s = settings::load();
    let valid = |p: &String| is_prefix(Path::new(p));
    s.selected_prefix
        .filter(|p| valid(p))
        .or_else(|| s.prefixes.iter().map(|e| e.path.clone()).find(valid))
}
