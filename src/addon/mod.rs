pub mod installer;
pub mod registry;
pub mod toc;

use serde::{Deserialize, Serialize};

/// Which WoW game flavor an addon targets.
/// Maps directly to the on-disk subdirectory under the WoW root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WowFlavor {
    /// Retail (The War Within and future expansions) → `_retail_/`
    Retail,
    /// Classic Era (vanilla) → `_classic_era_/`
    ClassicEra,
    /// Progression classic (Cataclysm, Mists, …) → `_classic_/`
    Classic,
}

impl WowFlavor {
    /// The subdirectory name under the WoW root for this flavor.
    pub fn dir_name(&self) -> &'static str {
        match self {
            WowFlavor::Retail => "_retail_",
            WowFlavor::ClassicEra => "_classic_era_",
            WowFlavor::Classic => "_classic_",
        }
    }

    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            WowFlavor::Retail => "Retail",
            WowFlavor::ClassicEra => "Classic Era",
            WowFlavor::Classic => "Classic",
        }
    }

    /// All flavors, in display order.
    pub fn all() -> &'static [WowFlavor] {
        &[WowFlavor::Retail, WowFlavor::ClassicEra, WowFlavor::Classic]
    }

    /// `.toc` file suffixes WoW uses for this flavor, in preference order.
    /// e.g. `ElvUI_Mainline.toc` for Retail.
    pub fn toc_suffixes(&self) -> &'static [&'static str] {
        match self {
            WowFlavor::Retail => &["Mainline"],
            WowFlavor::ClassicEra => &["Vanilla"],
            WowFlavor::Classic => &["Cata", "Mists", "Wrath", "TBC"],
        }
    }
}

impl std::fmt::Display for WowFlavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Installation state of an addon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonState {
    /// Installed and up to date.
    Installed,
    /// A newer release is available on GitHub.
    UpdateAvailable,
    /// Download/install in progress.
    Installing,
    /// Update check in progress.
    CheckingForUpdates,
}

/// Where an addon is sourced from for downloads and update checks.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AddonSource {
    /// A GitHub repository release.
    GitHub { url: String },
    /// A CurseForge project (via API).
    CurseForge {
        mod_id: u32,
        file_id: u32,
        url: String,
    },
    /// No remote source — locally tracked only.
    #[default]
    None,
}

impl AddonSource {
    /// The browsable URL for this source, if any.
    pub fn url(&self) -> Option<&str> {
        match self {
            AddonSource::GitHub { url } => Some(url),
            AddonSource::CurseForge { url, .. } => Some(url),
            AddonSource::None => None,
        }
    }

    /// Whether this source can be checked for updates.
    pub fn has_remote(&self) -> bool {
        !matches!(self, AddonSource::None)
    }
}

/// A tracked addon entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "AddonRaw")]
pub struct Addon {
    /// Primary folder name (used for TOC lookup and asset hint).
    pub name: String,
    /// All folders extracted from the release zip (includes companion modules).
    /// Defaults to `[name]` for entries created before multi-folder support.
    #[serde(default)]
    pub folders: Vec<String>,
    /// Where this addon is sourced from.
    pub source: AddonSource,
    /// WoW flavor this addon is installed for.
    pub flavor: WowFlavor,
    /// The release tag that is currently installed, e.g. `v1.2.3`.
    pub installed_version: String,
    /// The latest release tag seen on GitHub, if a check has been run.
    pub latest_version: Option<String>,
    /// ISO 8601 publication date of the installed release.
    #[serde(default)]
    pub release_date: Option<String>,
    /// Current installation state.
    pub state: AddonState,
    /// When true this addon is managed by another tool; skip update checks.
    #[serde(default)]
    pub externally_tracked: bool,
}

/// Intermediate struct for backward-compatible deserialization.
/// Handles both old format (`repo_url` field) and new format (`source` field).
#[derive(Deserialize)]
struct AddonRaw {
    name: String,
    #[serde(default)]
    folders: Vec<String>,
    /// New field — present in v1.0.0+ registries.
    #[serde(default)]
    source: Option<AddonSource>,
    /// Legacy field — present in pre-v1.0.0 registries.
    #[serde(default)]
    repo_url: Option<String>,
    flavor: WowFlavor,
    installed_version: String,
    latest_version: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    state: AddonState,
    #[serde(default)]
    externally_tracked: bool,
}

impl From<AddonRaw> for Addon {
    fn from(raw: AddonRaw) -> Self {
        let source = raw.source.unwrap_or(
            // Migrate legacy repo_url field
            match raw.repo_url {
                Some(url) if !url.is_empty() => AddonSource::GitHub { url },
                _ => AddonSource::None,
            },
        );
        Addon {
            name: raw.name,
            folders: raw.folders,
            source,
            flavor: raw.flavor,
            installed_version: raw.installed_version,
            latest_version: raw.latest_version,
            release_date: raw.release_date,
            state: raw.state,
            externally_tracked: raw.externally_tracked,
        }
    }
}

/// Given a list of extracted addon folder names, return the primary folder.
///
/// Looks for a folder whose name followed by `_` is a prefix of at least one
/// other folder (e.g. `BigWigs` → `BigWigs_Core`). Falls back to the shortest
/// name when no such relationship exists.
pub fn find_primary_folder(folders: &[String]) -> String {
    if folders.len() == 1 {
        return folders[0].clone();
    }
    for candidate in folders {
        let prefix = format!("{}_", candidate);
        if folders.iter().any(|f| f.starts_with(&prefix)) {
            return candidate.clone();
        }
    }
    folders
        .iter()
        .min_by_key(|s| s.len())
        .cloned()
        .unwrap_or_default()
}

impl Addon {
    #[allow(dead_code)] // used in tests; not yet called in production code
    pub fn new(
        name: impl Into<String>,
        source: AddonSource,
        flavor: WowFlavor,
        installed_version: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self {
            folders: vec![name.clone()],
            name,
            source,
            flavor,
            installed_version: installed_version.into(),
            latest_version: None,
            release_date: None,
            state: AddonState::Installed,
            externally_tracked: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github_source(url: &str) -> AddonSource {
        AddonSource::GitHub {
            url: url.to_string(),
        }
    }

    #[test]
    fn flavor_dir_names() {
        assert_eq!(WowFlavor::Retail.dir_name(), "_retail_");
        assert_eq!(WowFlavor::ClassicEra.dir_name(), "_classic_era_");
        assert_eq!(WowFlavor::Classic.dir_name(), "_classic_");
    }

    #[test]
    fn flavor_roundtrip_json() {
        for flavor in WowFlavor::all() {
            let json = serde_json::to_string(flavor).unwrap();
            let decoded: WowFlavor = serde_json::from_str(&json).unwrap();
            assert_eq!(*flavor, decoded);
        }
    }

    #[test]
    fn flavor_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&WowFlavor::Retail).unwrap(),
            r#""retail""#
        );
        assert_eq!(
            serde_json::to_string(&WowFlavor::ClassicEra).unwrap(),
            r#""classic_era""#
        );
        assert_eq!(
            serde_json::to_string(&WowFlavor::Classic).unwrap(),
            r#""classic""#
        );
    }

    #[test]
    fn addon_state_roundtrip_json() {
        let states = [
            AddonState::Installed,
            AddonState::UpdateAvailable,
            AddonState::Installing,
            AddonState::CheckingForUpdates,
        ];
        for state in &states {
            let json = serde_json::to_string(state).unwrap();
            let decoded: AddonState = serde_json::from_str(&json).unwrap();
            assert_eq!(*state, decoded);
        }
    }

    #[test]
    fn addon_roundtrip_json() {
        let addon = Addon::new(
            "WeakAuras",
            github_source("https://github.com/WeakAuras/WeakAuras2"),
            WowFlavor::Retail,
            "v6.0.0",
        );

        let json = serde_json::to_string(&addon).unwrap();
        let decoded: Addon = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.name, "WeakAuras");
        assert_eq!(
            decoded.source,
            github_source("https://github.com/WeakAuras/WeakAuras2")
        );
        assert_eq!(decoded.flavor, WowFlavor::Retail);
        assert_eq!(decoded.installed_version, "v6.0.0");
        assert_eq!(decoded.latest_version, None);
        assert_eq!(decoded.state, AddonState::Installed);
    }

    #[test]
    fn addon_with_update_available() {
        let mut addon = Addon::new(
            "WeakAuras",
            github_source("https://github.com/WeakAuras/WeakAuras2"),
            WowFlavor::Retail,
            "v5.0.0",
        );
        addon.latest_version = Some("v6.0.0".to_string());
        addon.state = AddonState::UpdateAvailable;

        let json = serde_json::to_string(&addon).unwrap();
        let decoded: Addon = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.latest_version, Some("v6.0.0".to_string()));
        assert_eq!(decoded.state, AddonState::UpdateAvailable);
    }

    #[test]
    fn addon_source_none_serializes() {
        let addon = Addon::new("LocalOnly", AddonSource::None, WowFlavor::Retail, "unknown");
        let json = serde_json::to_string(&addon).unwrap();
        let decoded: Addon = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.source, AddonSource::None);
    }

    #[test]
    fn addon_legacy_repo_url_migrates_to_source() {
        // Simulates an old addons.json entry with repo_url instead of source
        let legacy_json = r#"{
            "name": "WeakAuras",
            "folders": ["WeakAuras"],
            "repo_url": "https://github.com/WeakAuras/WeakAuras2",
            "flavor": "retail",
            "installed_version": "v6.0.0",
            "latest_version": null,
            "state": "installed",
            "externally_tracked": false
        }"#;
        let addon: Addon = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(
            addon.source,
            github_source("https://github.com/WeakAuras/WeakAuras2")
        );
    }

    #[test]
    fn addon_legacy_empty_repo_url_migrates_to_none() {
        let legacy_json = r#"{
            "name": "SomeAddon",
            "folders": ["SomeAddon"],
            "repo_url": "",
            "flavor": "retail",
            "installed_version": "unknown",
            "state": "installed"
        }"#;
        let addon: Addon = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(addon.source, AddonSource::None);
    }

    #[test]
    fn addon_source_url_helper() {
        let gh = AddonSource::GitHub {
            url: "https://github.com/a/b".to_string(),
        };
        assert_eq!(gh.url(), Some("https://github.com/a/b"));
        assert!(gh.has_remote());

        let cf = AddonSource::CurseForge {
            mod_id: 123,
            file_id: 456,
            url: "https://curseforge.com/wow/addons/test".to_string(),
        };
        assert_eq!(cf.url(), Some("https://curseforge.com/wow/addons/test"));
        assert!(cf.has_remote());

        assert_eq!(AddonSource::None.url(), None);
        assert!(!AddonSource::None.has_remote());
    }

    #[test]
    fn find_primary_folder_prefix_wins() {
        let folders = vec![
            "BigWigs_Core".to_string(),
            "BigWigs_Options".to_string(),
            "BigWigs_Plugins".to_string(),
            "BigWigs".to_string(),
        ];
        assert_eq!(find_primary_folder(&folders), "BigWigs");
    }

    #[test]
    fn find_primary_folder_single() {
        let folders = vec!["WeakAuras".to_string()];
        assert_eq!(find_primary_folder(&folders), "WeakAuras");
    }

    #[test]
    fn find_primary_folder_falls_back_to_shortest() {
        let folders = vec![
            "AddonB".to_string(),
            "AddonA".to_string(),
            "Addon".to_string(),
        ];
        assert_eq!(find_primary_folder(&folders), "Addon");
    }
}
