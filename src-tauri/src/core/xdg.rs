// core/xdg.rs
//
// XDG base directories, by the same rule as the Neutron CLI's `_xdg_dir` and Mud Hut's `xdg_dir`:
// the variable when it holds an absolute path, else HOME/<default>. Unset, EMPTY and relative
// values all mean the default (XDG Base Directory spec). Until 2026-10-19 Collider hardcoded
// ~/.config, ~/.cache and ~/.local/share, so with a non-default XDG_DATA_HOME it looked for the
// CLI, wrote its KWin script and searched for prefixes where the CLI never put them.

use std::ffi::OsString;
use std::path::PathBuf;

fn from(value: Option<OsString>, home: Option<OsString>, default: &str) -> Option<PathBuf> {
    value
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| PathBuf::from(h).join(default)))
}

fn base(var: &str, default: &str) -> Option<PathBuf> {
    from(std::env::var_os(var), std::env::var_os("HOME"), default)
}

/// $XDG_DATA_HOME, else ~/.local/share.
pub fn data_home() -> Option<PathBuf> {
    base("XDG_DATA_HOME", ".local/share")
}

/// $XDG_CACHE_HOME, else ~/.cache.
pub fn cache_home() -> Option<PathBuf> {
    base("XDG_CACHE_HOME", ".cache")
}

/// Collider's settings folder: $XDG_CONFIG_HOME/collider. ⚠️ Collider and the CLI used to hardcode
/// ~/.config/collider, so when only that one exists it is still the folder in use (no prefixes or
/// theme lost when XDG_CONFIG_HOME is set). The CLI's `collider_config_dir()` applies the SAME
/// rule; change both or neither.
pub fn collider_config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    choose_config_dir(base("XDG_CONFIG_HOME", ".config"), home)
}

fn choose_config_dir(config_home: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    let current = config_home?.join("collider");
    if let Some(h) = home {
        let legacy = h.join(".config/collider");
        if legacy != current && !current.exists() && legacy.is_dir() {
            return Some(legacy);
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_relative_means_the_default() {
        let home = Some(OsString::from("/home/u"));
        let want = Some(PathBuf::from("/home/u/.local/share"));
        assert_eq!(from(Some("".into()), home.clone(), ".local/share"), want);
        assert_eq!(from(Some("data".into()), home.clone(), ".local/share"), want);
        assert_eq!(from(None, home.clone(), ".local/share"), want);
        assert_eq!(from(Some("/srv/data".into()), home, ".local/share"), Some(PathBuf::from("/srv/data")));
    }

    #[test]
    fn collider_config_keeps_the_legacy_folder() {
        let tmp = std::env::temp_dir().join(format!("collider-xdg-{}", std::process::id()));
        let home = tmp.join("home");
        let config = tmp.join("cfg");
        let _ = std::fs::remove_dir_all(&tmp);
        // Neither exists: the XDG one.
        assert_eq!(choose_config_dir(Some(config.clone()), Some(home.clone())), Some(config.join("collider")));
        // Only the old one: keep using it.
        std::fs::create_dir_all(home.join(".config/collider")).unwrap();
        assert_eq!(choose_config_dir(Some(config.clone()), Some(home.clone())), Some(home.join(".config/collider")));
        // Both: the XDG one wins.
        std::fs::create_dir_all(config.join("collider")).unwrap();
        assert_eq!(choose_config_dir(Some(config.clone()), Some(home.clone())), Some(config.join("collider")));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
