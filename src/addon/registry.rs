use std::path::PathBuf;

use anyhow::{Context, Result};

use super::Addon;

const REGISTRY_FILE: &str = "addons.json";

/// In-memory store of all tracked addons, backed by `addons.json`.
pub struct AddonRegistry {
    pub addons: Vec<Addon>,
    path: PathBuf,
}

impl AddonRegistry {
    /// Load the registry from disk. Returns an empty registry if the file doesn't exist.
    pub fn load() -> Result<Self> {
        let path = registry_path()?;
        if !path.exists() {
            return Ok(Self {
                addons: vec![],
                path,
            });
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let addons: Vec<Addon> =
            serde_json::from_str(&contents).context("Failed to parse addons.json")?;
        Ok(Self { addons, path })
    }

    /// Persist the registry to disk.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        let contents =
            serde_json::to_string_pretty(&self.addons).context("Failed to serialize addons")?;
        std::fs::write(&self.path, contents)
            .with_context(|| format!("Failed to write {}", self.path.display()))
    }

    pub fn addons(&self) -> &[Addon] {
        &self.addons
    }

    /// Scan all configured AddOns directories and return `(flavor, folder_name)` pairs
    /// for addon folders that are not currently tracked in this registry.
    pub fn scan_untracked(
        &self,
        config: &crate::config::Config,
    ) -> Vec<(super::WowFlavor, String)> {
        let tracked: std::collections::HashSet<&str> = self
            .addons
            .iter()
            .flat_map(|a| {
                // Include both the primary name and all companion folders
                let folders: &[String] = if a.folders.is_empty() {
                    std::slice::from_ref(&a.name)
                } else {
                    &a.folders
                };
                folders.iter().map(|s| s.as_str()).collect::<Vec<_>>()
            })
            .collect();

        let mut result = Vec::new();
        for flavor in super::WowFlavor::all() {
            if let Some(addons_dir) = config.addons_dir(flavor)
                && let Ok(entries) = std::fs::read_dir(&addons_dir)
            {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        // Skip hidden dirs and Blizzard's own folders
                        if !name.starts_with('.') && !tracked.contains(name.as_str()) {
                            result.push((flavor.clone(), name));
                        }
                    }
                }
            }
        }
        result.sort_by(|a, b| a.1.cmp(&b.1));
        result
    }
}

impl Default for AddonRegistry {
    fn default() -> Self {
        Self {
            addons: vec![],
            path: registry_path().unwrap_or_default(),
        }
    }
}

fn registry_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("Could not determine config directory")?;
    Ok(dir.join("addon-manager").join(REGISTRY_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addon::{AddonSource, WowFlavor};
    use tempfile::TempDir;

    #[test]
    fn load_returns_empty_when_file_missing() {
        // Use a path that definitely won't exist
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("addons.json");
        let registry = AddonRegistry {
            addons: vec![],
            path,
        };
        assert!(registry.addons().is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("addons.json");

        let addon = Addon::new(
            "WeakAuras",
            AddonSource::GitHub {
                url: "https://github.com/WeakAuras/WeakAuras2".to_string(),
            },
            WowFlavor::Retail,
            "v6.0.0",
        );

        let registry = AddonRegistry {
            addons: vec![addon],
            path: path.clone(),
        };
        registry.save().unwrap();

        // Read back raw JSON to verify structure
        let raw = std::fs::read_to_string(&path).unwrap();
        let loaded: Vec<Addon> = serde_json::from_str(&raw).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "WeakAuras");
        assert_eq!(loaded[0].flavor, WowFlavor::Retail);
        assert_eq!(loaded[0].installed_version, "v6.0.0");
    }

    #[test]
    fn save_creates_parent_directories() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested/dir/addons.json");
        let registry = AddonRegistry {
            addons: vec![],
            path,
        };
        registry.save().unwrap();
        assert!(tmp.path().join("nested/dir/addons.json").exists());
    }
}
