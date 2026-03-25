use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::addon::WowFlavor;

const CONFIG_DIR: &str = "addon-manager";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Path to the WoW installation root (contains `_retail_/`, `_classic_/`, etc.).
    pub wow_root: Option<PathBuf>,
    /// Optional GitHub personal access token for higher API rate limits.
    pub github_token: Option<String>,
    /// CurseForge API key from console.curseforge.com.
    pub curseforge_api_key: Option<String>,
}

impl Config {
    /// Load config from disk, returning a default config if the file doesn't exist.
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        toml::from_str(&contents).with_context(|| "Failed to parse config.toml")
    }

    /// Persist config to disk, creating the config directory if needed.
    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config dir {}", parent.display()))?;
        }
        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write config to {}", path.display()))
    }

    /// Return the AddOns directory for the given flavor, or None if wow_root isn't set
    /// or the path doesn't exist on disk.
    pub fn addons_dir(&self, flavor: &WowFlavor) -> Option<PathBuf> {
        let root = self.wow_root.as_ref()?;
        let path = root
            .join(flavor.dir_name())
            .join("Interface")
            .join("AddOns");
        if path.exists() { Some(path) } else { None }
    }

    /// Try to auto-detect the WoW installation root from common Linux locations.
    /// Returns the first valid path found, or None.
    pub fn detect_wow_root() -> Option<PathBuf> {
        candidate_paths().into_iter().find(|p| is_wow_root(p))
    }
}

/// Returns `~/.config/addon-manager/config.toml`.
fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("Could not determine config directory")?;
    Ok(dir.join(CONFIG_DIR).join(CONFIG_FILE))
}

/// Returns true if the given path looks like a WoW installation root
/// (i.e. contains at least one of the expected flavor subdirectories).
pub fn is_wow_root(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    WowFlavor::all()
        .iter()
        .any(|f| path.join(f.dir_name()).is_dir())
}

/// Candidate WoW root paths to probe during auto-detection, ordered by likelihood.
fn candidate_paths() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return vec![];
    };

    vec![
        // Lutris default slug
        home.join("Games/world-of-warcraft/drive_c/Program Files (x86)/World of Warcraft"),
        // Lutris alternate capitalisation
        home.join("Games/World of Warcraft/drive_c/Program Files (x86)/World of Warcraft"),
        // Plain Wine prefix
        home.join(".wine/drive_c/Program Files (x86)/World of Warcraft"),
        // Bottles (native)
        home.join(".local/share/bottles/bottles/World of Warcraft/drive_c/Program Files (x86)/World of Warcraft"),
        // Bottles (Flatpak)
        home.join(".var/app/com.usebottles.bottles/data/bottles/bottles/World of Warcraft/drive_c/Program Files (x86)/World of Warcraft"),
        // Heroic Games Launcher (default Battle.net prefix)
        home.join("Games/Heroic/Prefixes/default/Battle.net/drive_c/Program Files (x86)/World of Warcraft"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_wow_root(base: &Path, flavors: &[&str]) -> PathBuf {
        for flavor in flavors {
            fs::create_dir_all(base.join(flavor)).unwrap();
        }
        base.to_path_buf()
    }

    #[test]
    fn is_wow_root_true_when_flavor_dir_exists() {
        let tmp = TempDir::new().unwrap();
        make_wow_root(tmp.path(), &["_retail_"]);
        assert!(is_wow_root(tmp.path()));
    }

    #[test]
    fn is_wow_root_false_for_empty_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(!is_wow_root(tmp.path()));
    }

    #[test]
    fn is_wow_root_false_for_nonexistent_path() {
        assert!(!is_wow_root(Path::new("/nonexistent/path/wow")));
    }

    #[test]
    fn addons_dir_returns_none_without_wow_root() {
        let config = Config::default();
        assert!(config.addons_dir(&WowFlavor::Retail).is_none());
    }

    #[test]
    fn addons_dir_returns_path_when_exists() {
        let tmp = TempDir::new().unwrap();
        let addons = tmp.path().join("_retail_/Interface/AddOns");
        fs::create_dir_all(&addons).unwrap();

        let config = Config {
            wow_root: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        assert_eq!(config.addons_dir(&WowFlavor::Retail), Some(addons));
    }

    #[test]
    fn addons_dir_returns_none_when_path_missing() {
        let tmp = TempDir::new().unwrap();
        let config = Config {
            wow_root: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        assert!(config.addons_dir(&WowFlavor::Retail).is_none());
    }

    #[test]
    fn config_roundtrip_toml() {
        let tmp = TempDir::new().unwrap();
        let wow_root = tmp.path().join("wow");

        let original = Config {
            wow_root: Some(wow_root.clone()),
            github_token: Some("ghp_testtoken".to_string()),
            curseforge_api_key: Some("cf_testkey".to_string()),
        };

        let serialized = toml::to_string_pretty(&original).unwrap();
        let decoded: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(decoded.wow_root, Some(wow_root));
        assert_eq!(decoded.github_token, Some("ghp_testtoken".to_string()));
    }
}
