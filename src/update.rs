use anyhow::Result;

use crate::{
    addon::{AddonSource, AddonState, registry::AddonRegistry},
    config::Config,
    curseforge::client::CurseForgeClient,
    github::{client::GitHubClient, parse_repo_url},
};

const APP_REPO_OWNER: &str = "Jintso";
const APP_REPO_NAME: &str = "PackHound";

/// Result of an update check cycle.
pub struct UpdateCheckResult {
    /// Number of addons with updates available.
    pub updates_available: usize,
    /// User-facing warnings (e.g. rate limit exceeded).
    pub warnings: Vec<String>,
}

/// Check all tracked addons for newer releases, updating the registry
/// in place.
///
/// Dispatches to the appropriate backend (GitHub, CurseForge) based on
/// each addon's source type. Errors for individual addons are collected
/// as warnings rather than aborting the entire check.
pub async fn check_all_updates(token: Option<&str>) -> Result<UpdateCheckResult> {
    let mut registry = AddonRegistry::load()?;
    let mut updates_available = 0;
    let mut warnings: Vec<String> = Vec::new();

    if registry.addons.is_empty() {
        return Ok(UpdateCheckResult {
            updates_available,
            warnings,
        });
    }

    let gh_client = GitHubClient::new(token)?;
    let config = Config::load().unwrap_or_default();
    let cf_client = config
        .curseforge_api_key
        .as_deref()
        .and_then(|k| CurseForgeClient::new(k).ok());

    for addon in registry.addons.iter_mut() {
        if !addon.source.has_remote() || addon.externally_tracked {
            continue;
        }

        let result: Result<String> = match &addon.source {
            AddonSource::GitHub { url } => match parse_repo_url(url) {
                Ok((owner, repo)) => match gh_client.fetch_latest_release(&owner, &repo).await {
                    Ok(release) => Ok(release.tag_name),
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            },
            AddonSource::CurseForge {
                mod_id, file_id, ..
            } => {
                let Some(ref client) = cf_client else {
                    // Only warn once about missing API key
                    let msg = "CurseForge API key not configured. Add it in Preferences.";
                    if !warnings.iter().any(|w| w.contains("CurseForge API key")) {
                        warnings.push(msg.to_string());
                    }
                    continue;
                };
                match client.list_files(*mod_id, Some(&addon.flavor)).await {
                    Ok(files) => match files.first() {
                        Some(latest_file) if latest_file.id != *file_id => {
                            Ok(latest_file.display_name.clone())
                        }
                        Some(_) => {
                            addon.state = AddonState::Installed;
                            continue;
                        }
                        None => continue,
                    },
                    Err(e) => Err(e),
                }
            }
            AddonSource::None => continue,
        };

        match result {
            Ok(latest) => {
                let has_update = latest != addon.installed_version;
                addon.latest_version = Some(latest);
                addon.state = if has_update {
                    updates_available += 1;
                    AddonState::UpdateAvailable
                } else {
                    AddonState::Installed
                };
            }
            Err(e) => {
                let msg = format!("{e}");
                // Surface rate-limit errors as user-facing warnings
                if msg.contains("rate limit") {
                    if !warnings.iter().any(|w| w.contains("rate limit")) {
                        warnings.push(msg);
                    }
                    // Stop checking further GitHub addons — they'll all fail
                    if matches!(addon.source, AddonSource::GitHub { .. }) {
                        break;
                    }
                } else {
                    eprintln!("Update check failed for {}: {e}", addon.name);
                }
            }
        }
    }

    registry.save()?;
    Ok(UpdateCheckResult {
        updates_available,
        warnings,
    })
}

/// Info about an available app update.
pub struct AppUpdate {
    /// The new version tag (e.g. "v1.2.0").
    pub version: String,
    /// URL to the GitHub release page.
    pub release_url: String,
}

/// Check if a newer version of PackHound is available on GitHub.
///
/// Compares the latest release tag against `CARGO_PKG_VERSION`. Returns
/// `Ok(Some(AppUpdate))` if a newer version exists, `Ok(None)` if up to date.
pub async fn check_app_update(token: Option<&str>) -> Result<Option<AppUpdate>> {
    let client = GitHubClient::new(token)?;
    let release = client
        .fetch_latest_release(APP_REPO_OWNER, APP_REPO_NAME)
        .await?;

    let remote = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let local = env!("CARGO_PKG_VERSION");

    if is_newer_version(remote, local) {
        Ok(Some(AppUpdate {
            version: release.tag_name.clone(),
            release_url: format!(
                "https://github.com/{APP_REPO_OWNER}/{APP_REPO_NAME}/releases/tag/{}",
                release.tag_name
            ),
        }))
    } else {
        Ok(None)
    }
}

/// Returns true if `remote` is a newer semver than `local`.
///
/// Compares major.minor.patch numerically. Returns false if either version
/// can't be parsed (assumes up to date on parse failure).
fn is_newer_version(remote: &str, local: &str) -> bool {
    let parse = |v: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    };

    match (parse(remote), parse(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addon::{Addon, AddonSource, WowFlavor};

    fn github_source(url: &str) -> AddonSource {
        AddonSource::GitHub {
            url: url.to_string(),
        }
    }

    #[test]
    fn update_count_zero_for_empty_registry() {
        let registry = AddonRegistry::default();
        assert!(registry.addons.is_empty());
    }

    #[test]
    fn addon_state_update_available_when_versions_differ() {
        let mut addon = Addon::new(
            "WeakAuras",
            github_source("https://github.com/WeakAuras/WeakAuras2"),
            WowFlavor::Retail,
            "v5.0.0",
        );
        addon.latest_version = Some("v6.0.0".to_string());
        addon.state = if addon.latest_version.as_deref() != Some(&addon.installed_version) {
            AddonState::UpdateAvailable
        } else {
            AddonState::Installed
        };
        assert_eq!(addon.state, AddonState::UpdateAvailable);
    }

    #[test]
    fn newer_version_major() {
        assert!(is_newer_version("2.0.0", "1.1.0"));
    }

    #[test]
    fn newer_version_minor() {
        assert!(is_newer_version("1.2.0", "1.1.0"));
    }

    #[test]
    fn newer_version_patch() {
        assert!(is_newer_version("1.1.1", "1.1.0"));
    }

    #[test]
    fn same_version_not_newer() {
        assert!(!is_newer_version("1.1.0", "1.1.0"));
    }

    #[test]
    fn older_version_not_newer() {
        assert!(!is_newer_version("1.0.0", "1.1.0"));
    }

    #[test]
    fn invalid_version_not_newer() {
        assert!(!is_newer_version("abc", "1.1.0"));
        assert!(!is_newer_version("1.1.0", "abc"));
    }

    #[test]
    fn addon_state_installed_when_versions_equal() {
        let mut addon = Addon::new(
            "WeakAuras",
            github_source("https://github.com/WeakAuras/WeakAuras2"),
            WowFlavor::Retail,
            "v6.0.0",
        );
        addon.latest_version = Some("v6.0.0".to_string());
        addon.state = if addon.latest_version.as_deref() != Some(&addon.installed_version) {
            AddonState::UpdateAvailable
        } else {
            AddonState::Installed
        };
        assert_eq!(addon.state, AddonState::Installed);
    }
}
