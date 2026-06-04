use crate::addon::{
    AddonState, find_primary_folder, registry::AddonRegistry, toc::read_toc, versions_equal,
};
use crate::config::Config;

/// Result of a startup reconciliation pass against on-disk state.
pub struct SyncReport {
    /// Addon names whose `installed_version` was synced from a changed `.toc`.
    pub updated: Vec<String>,
    /// Addon names whose primary folder is gone; dropped from the registry.
    pub removed: Vec<String>,
}

impl SyncReport {
    pub fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.removed.is_empty()
    }
}

/// Reconcile the registry against what's actually on disk.
///
/// Two kinds of drift are detected per addon:
///   - **Version drift:** the addon's `.toc` `## Version:` line differs from
///     `installed_version` → rewrite `installed_version`, clear
///     `latest_version`, and reset `UpdateAvailable` / `CheckError` to
///     `Installed` so the next remote check re-evaluates cleanly.
///   - **Missing folder:** the addon's primary folder is gone from disk →
///     drop the entry from the registry.
///
/// Externally-tracked addons are skipped (user manages them another way).
/// Addons for a flavor whose WoW path isn't configured are also skipped,
/// so we never wipe entries due to a temporarily-missing config path.
///
/// Does not call `registry.save()` — the caller decides.
pub fn sync_from_disk(registry: &mut AddonRegistry, config: &Config) -> SyncReport {
    let mut updated: Vec<String> = Vec::new();
    let mut to_remove: Vec<String> = Vec::new();

    for addon in registry.addons.iter_mut() {
        if addon.externally_tracked {
            continue;
        }

        let Some(addons_dir) = config.addons_dir(&addon.flavor) else {
            continue;
        };

        let folders: Vec<String> = if addon.folders.is_empty() {
            vec![addon.name.clone()]
        } else {
            addon.folders.clone()
        };
        let primary = find_primary_folder(&folders);

        let addon_path = addons_dir.join(&primary);
        if !addon_path.exists() {
            to_remove.push(addon.name.clone());
            continue;
        }

        let Some(toc) = read_toc(&addons_dir, &primary, &addon.flavor) else {
            continue;
        };
        let Some(toc_version) = toc.version else {
            continue;
        };
        if versions_equal(&toc_version, &addon.installed_version) {
            continue;
        }

        addon.installed_version = toc_version;
        addon.latest_version = None;
        if addon.state == AddonState::UpdateAvailable
            || matches!(addon.state, AddonState::CheckError(_))
        {
            addon.state = AddonState::Installed;
        }
        updated.push(addon.name.clone());
    }

    let removed = to_remove.clone();
    registry
        .addons
        .retain(|a| !to_remove.iter().any(|n| n == &a.name));

    SyncReport { updated, removed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addon::{Addon, AddonSource, WowFlavor};
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// Build a fake WoW root with a single addon folder + .toc on disk.
    /// Returns (TempDir guard, addons_dir path) so the caller can write more files.
    fn fake_wow_root(flavor: &WowFlavor) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let addons_dir = tmp
            .path()
            .join(flavor.dir_name())
            .join("Interface")
            .join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        (tmp, addons_dir)
    }

    fn write_toc(addons_dir: &Path, folder: &str, version: &str) {
        let addon_dir = addons_dir.join(folder);
        fs::create_dir_all(&addon_dir).unwrap();
        fs::write(
            addon_dir.join(format!("{folder}.toc")),
            format!("## Title: {folder}\n## Version: {version}\n"),
        )
        .unwrap();
    }

    fn config_for(tmp: &TempDir) -> Config {
        Config {
            wow_root: Some(tmp.path().to_path_buf()),
            ..Config::default()
        }
    }

    fn registry_with(addons: Vec<Addon>) -> AddonRegistry {
        let mut reg = AddonRegistry::default();
        reg.addons = addons;
        reg
    }

    fn make_addon(name: &str, version: &str, flavor: WowFlavor) -> Addon {
        Addon::new(
            name,
            AddonSource::GitHub {
                url: format!("https://github.com/x/{name}"),
            },
            flavor,
            version,
        )
    }

    #[test]
    fn version_drift_updates_registry() {
        let (tmp, addons_dir) = fake_wow_root(&WowFlavor::Retail);
        write_toc(&addons_dir, "WeakAuras", "v6.1.0");

        let mut addon = make_addon("WeakAuras", "v6.0.0", WowFlavor::Retail);
        addon.latest_version = Some("v6.0.0".to_string());
        let mut reg = registry_with(vec![addon]);

        let report = sync_from_disk(&mut reg, &config_for(&tmp));

        assert_eq!(report.updated, vec!["WeakAuras".to_string()]);
        assert!(report.removed.is_empty());
        assert_eq!(reg.addons[0].installed_version, "v6.1.0");
        assert_eq!(reg.addons[0].latest_version, None);
    }

    #[test]
    fn missing_folder_marks_for_removal() {
        let (tmp, _addons_dir) = fake_wow_root(&WowFlavor::Retail);
        // Intentionally do not create a folder for the tracked addon.

        let addon = make_addon("Ghost", "v1.0.0", WowFlavor::Retail);
        let mut reg = registry_with(vec![addon]);

        let report = sync_from_disk(&mut reg, &config_for(&tmp));

        assert_eq!(report.removed, vec!["Ghost".to_string()]);
        assert!(report.updated.is_empty());
        assert!(reg.addons.is_empty());
    }

    #[test]
    fn externally_tracked_is_skipped() {
        let (tmp, addons_dir) = fake_wow_root(&WowFlavor::Retail);
        // On-disk version drifts, but addon is externally tracked.
        write_toc(&addons_dir, "ElvUI", "13.99");

        let mut addon = make_addon("ElvUI", "13.80", WowFlavor::Retail);
        addon.externally_tracked = true;
        let mut reg = registry_with(vec![addon]);

        let report = sync_from_disk(&mut reg, &config_for(&tmp));

        assert!(report.is_empty());
        assert_eq!(reg.addons[0].installed_version, "13.80");
        assert!(reg.addons[0].externally_tracked);
    }

    #[test]
    fn wow_path_unset_does_not_remove() {
        // No wow_root configured → addons_dir() returns None → addons must not be touched.
        let addon = make_addon("Orphan", "v1.0.0", WowFlavor::Retail);
        let mut reg = registry_with(vec![addon]);

        let report = sync_from_disk(&mut reg, &Config::default());

        assert!(report.is_empty());
        assert_eq!(reg.addons.len(), 1);
        assert_eq!(reg.addons[0].name, "Orphan");
    }

    #[test]
    fn up_to_date_addon_unchanged() {
        let (tmp, addons_dir) = fake_wow_root(&WowFlavor::Retail);
        write_toc(&addons_dir, "Recount", "v9.0.0");

        let mut addon = make_addon("Recount", "v9.0.0", WowFlavor::Retail);
        addon.latest_version = Some("v9.0.0".to_string());
        let mut reg = registry_with(vec![addon]);

        let report = sync_from_disk(&mut reg, &config_for(&tmp));

        assert!(report.is_empty());
        assert_eq!(reg.addons[0].installed_version, "v9.0.0");
        assert_eq!(reg.addons[0].latest_version, Some("v9.0.0".to_string()));
    }

    #[test]
    fn check_error_state_resets_on_drift() {
        let (tmp, addons_dir) = fake_wow_root(&WowFlavor::Retail);
        write_toc(&addons_dir, "Bartender4", "4.13.4");

        let mut addon = make_addon("Bartender4", "4.13.0", WowFlavor::Retail);
        addon.state = AddonState::CheckError("repo not found".to_string());
        let mut reg = registry_with(vec![addon]);

        let report = sync_from_disk(&mut reg, &config_for(&tmp));

        assert_eq!(report.updated, vec!["Bartender4".to_string()]);
        assert_eq!(reg.addons[0].state, AddonState::Installed);
        assert_eq!(reg.addons[0].installed_version, "4.13.4");
    }
}
