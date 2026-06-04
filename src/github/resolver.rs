use super::client::ReleaseAsset;
use crate::addon::WowFlavor;

/// Result of attempting to select a release asset for a specific WoW flavor.
#[derive(Debug)]
pub enum AssetSelection {
    /// Exactly one asset matched (or only one zip existed).
    Found(ReleaseAsset),
    /// Multiple assets matched equally; caller should ask the user to pick.
    /// The list is kept for future UI use even though it is not read yet.
    #[allow(dead_code)]
    Ambiguous(Vec<ReleaseAsset>),
    /// No zip asset could be matched to this flavor.
    NotFound,
}

/// Select the best zip asset from a release for the given WoW flavor,
/// optionally filtering by an addon folder name hint first.
///
/// When `name_hint` is provided (e.g. `"HandyNotes_Midnight"`), assets whose
/// lowercase name contains the lowercased hint are tried first. If that
/// subset yields a `Found` result, it is returned. Otherwise falls back to
/// the full asset list with normal flavor scoring.
pub fn select_asset_with_hint(
    assets: &[ReleaseAsset],
    flavor: &WowFlavor,
    name_hint: Option<&str>,
) -> AssetSelection {
    if let Some(hint) = name_hint {
        let hint_lower = hint.to_lowercase();
        let hinted: Vec<ReleaseAsset> = assets
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&hint_lower))
            .cloned()
            .collect();

        if !hinted.is_empty() {
            let result = select_asset(&hinted, flavor);
            if matches!(result, AssetSelection::Found(_)) {
                return result;
            }
        }
    }

    select_asset(assets, flavor)
}

/// Select the best zip asset from a release for the given WoW flavor.
///
/// Strategy:
/// 1. Only consider `.zip` assets.
/// 2. If exactly one zip exists, return it (universal release).
/// 3. Otherwise score each zip by keyword match against the target flavor,
///    penalising keywords that belong to other flavors.
/// 4. Return the highest-scoring asset, or `Ambiguous` when scores tie.
pub fn select_asset(assets: &[ReleaseAsset], flavor: &WowFlavor) -> AssetSelection {
    let zips: Vec<&ReleaseAsset> = assets
        .iter()
        .filter(|a| a.name.to_lowercase().ends_with(".zip"))
        .collect();

    match zips.len() {
        0 => AssetSelection::NotFound,
        1 => AssetSelection::Found(zips[0].clone()),
        _ => select_by_flavor(zips, flavor),
    }
}

fn select_by_flavor(zips: Vec<&ReleaseAsset>, flavor: &WowFlavor) -> AssetSelection {
    let mut scored: Vec<(i32, &ReleaseAsset)> =
        zips.iter().map(|a| (score_asset(a, flavor), *a)).collect();

    scored.sort_by_key(|s| std::cmp::Reverse(s.0));

    let max_score = scored[0].0;

    // All assets clearly belong to other flavors.
    if max_score < 0 {
        return AssetSelection::NotFound;
    }

    // No flavor signals at all — return everything so the user can pick.
    if max_score == 0 {
        return AssetSelection::Ambiguous(zips.iter().map(|a| (*a).clone()).collect());
    }

    let best: Vec<ReleaseAsset> = scored
        .iter()
        .filter(|(s, _)| *s == max_score)
        .map(|(_, a)| (*a).clone())
        .collect();

    if best.len() == 1 {
        AssetSelection::Found(best.into_iter().next().unwrap())
    } else {
        AssetSelection::Ambiguous(best)
    }
}

/// Score an asset filename against a target flavor.
///
/// Two keyword tiers, weighted differently:
///   - **Unique markers** (e.g. "vanilla", "bcc", "mainline") unambiguously
///     identify one flavor. A match scores `+2` for that flavor and `-2`
///     against the others.
///   - **Shared markers** (currently just "classic") can legitimately mean
///     either ClassicEra or progression Classic depending on how the addon
///     author names things. A match scores `+1` for every flavor that lists
///     it as shared and `0` for the rest — never negative.
///
/// This makes `Addon-ClassicEra.zip` (unique) beat `Addon-Classic.zip`
/// (shared) for a ClassicEra target, while `MoveAny-v1.8.299-classic.zip`
/// in a release that also has `-bcc`/`-cata`/`-wrath`/`-mists` still wins
/// for ClassicEra (those expansion suffixes deduct from ClassicEra and the
/// bare `-classic` is the only one that scores positive).
fn score_asset(asset: &ReleaseAsset, flavor: &WowFlavor) -> i32 {
    let tokens = filename_tokens(&asset.name);
    let matches = |kw: &str| tokens.iter().any(|t| t == kw);

    let unique_self: i32 = unique_flavor_keywords(flavor)
        .iter()
        .filter(|kw| matches(kw))
        .count() as i32;

    let unique_other: i32 = WowFlavor::all()
        .iter()
        .filter(|f| *f != flavor)
        .flat_map(|f| unique_flavor_keywords(f).iter())
        .filter(|kw| matches(kw))
        .count() as i32;

    let shared_self: i32 = shared_flavor_keywords(flavor)
        .iter()
        .filter(|kw| matches(kw))
        .count() as i32;

    (unique_self * 2) + shared_self - (unique_other * 2)
}

/// Split a filename into lowercase tokens on `-`, `_`, `.`, and spaces.
fn filename_tokens(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(['-', '_', '.', ' '])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Keywords that unambiguously identify a single flavor in asset filenames.
fn unique_flavor_keywords(flavor: &WowFlavor) -> &'static [&'static str] {
    match flavor {
        WowFlavor::Retail => &["retail", "mainline"],
        // "classicera" handles unhyphenated names like `Addon-ClassicEra.zip`.
        WowFlavor::ClassicEra => &["vanilla", "era", "classicera"],
        // Expansion-specific names for the progression-Classic line. "classic"
        // is NOT here — it's ambiguous and lives in `shared_flavor_keywords`.
        WowFlavor::Classic => &[
            "cata",
            "cataclysm",
            "wrath",
            "wotlk",
            "bcc",
            "tbc",
            "mop",
            "mists",
        ],
    }
}

/// Keywords that may legitimately identify more than one flavor. A match adds
/// `+1` for each claiming flavor without deducting from the others.
fn shared_flavor_keywords(flavor: &WowFlavor) -> &'static [&'static str] {
    match flavor {
        // "classic" is the bare suffix authors use ambiguously: sometimes for
        // vanilla Classic Era (Questie, MoveAny) and sometimes as a generic
        // catch-all for progression Classic when no expansion is specified.
        WowFlavor::ClassicEra | WowFlavor::Classic => &["classic"],
        WowFlavor::Retail => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            download_url: format!("https://example.com/{name}"),
            size: 1024,
        }
    }

    #[test]
    fn single_zip_is_universal() {
        let assets = vec![asset("WeakAuras-v6.0.0.zip")];
        let result = select_asset(&assets, &WowFlavor::Retail);
        assert!(matches!(result, AssetSelection::Found(_)));
    }

    #[test]
    fn no_zips_returns_not_found() {
        let assets = vec![asset("README.md"), asset("source.tar.gz")];
        let result = select_asset(&assets, &WowFlavor::Retail);
        assert!(matches!(result, AssetSelection::NotFound));
    }

    #[test]
    fn selects_retail_asset() {
        let assets = vec![
            asset("WeakAuras-Retail-v6.zip"),
            asset("WeakAuras-Classic-v6.zip"),
        ];
        let AssetSelection::Found(a) = select_asset(&assets, &WowFlavor::Retail) else {
            panic!("expected Found");
        };
        assert!(a.name.to_lowercase().contains("retail"));
    }

    #[test]
    fn selects_classic_era_asset() {
        let assets = vec![
            asset("Addon-Retail.zip"),
            asset("Addon-ClassicEra.zip"),
            asset("Addon-Classic.zip"),
        ];
        let AssetSelection::Found(a) = select_asset(&assets, &WowFlavor::ClassicEra) else {
            panic!("expected Found");
        };
        assert!(a.name.to_lowercase().contains("classicera"));
    }

    #[test]
    fn selects_classic_asset() {
        let assets = vec![asset("Addon-Retail.zip"), asset("Addon-Classic.zip")];
        let AssetSelection::Found(a) = select_asset(&assets, &WowFlavor::Classic) else {
            panic!("expected Found");
        };
        assert!(a.name.to_lowercase().contains("classic"));
    }

    #[test]
    fn no_matching_flavor_returns_not_found() {
        let assets = vec![asset("Addon-Retail.zip"), asset("Addon-Mainline.zip")];
        let result = select_asset(&assets, &WowFlavor::Classic);
        assert!(matches!(result, AssetSelection::NotFound));
    }

    #[test]
    fn no_flavor_signals_returns_ambiguous() {
        let assets = vec![asset("Addon-v1.zip"), asset("Addon-v1-extra.zip")];
        let result = select_asset(&assets, &WowFlavor::Retail);
        assert!(matches!(result, AssetSelection::Ambiguous(_)));
    }

    #[test]
    fn classic_era_picks_bare_classic_when_release_has_expansion_siblings() {
        // MoveAny pattern: release contains a zip per progression expansion
        // plus a bare `-classic` zip (which the author uses to mean vanilla).
        let assets = vec![
            asset("MoveAny-v1.8.299-bcc.zip"),
            asset("MoveAny-v1.8.299-cata.zip"),
            asset("MoveAny-v1.8.299-classic.zip"),
            asset("MoveAny-v1.8.299-mists.zip"),
            asset("MoveAny-v1.8.299-wrath.zip"),
            asset("MoveAny-v1.8.299.zip"),
        ];
        let AssetSelection::Found(a) = select_asset(&assets, &WowFlavor::ClassicEra) else {
            panic!("expected Found");
        };
        assert_eq!(a.name, "MoveAny-v1.8.299-classic.zip");
    }

    #[test]
    fn progression_classic_picks_expansion_specific_over_bare_classic() {
        // For a progression-Classic user on the same MoveAny release, the
        // expansion-specific siblings outscore the bare `-classic`. With
        // multiple expansion-specific zips tied, we return Ambiguous so the
        // user picks which expansion they're on.
        let assets = vec![
            asset("MoveAny-v1.8.299-bcc.zip"),
            asset("MoveAny-v1.8.299-cata.zip"),
            asset("MoveAny-v1.8.299-classic.zip"),
            asset("MoveAny-v1.8.299-mists.zip"),
            asset("MoveAny-v1.8.299-wrath.zip"),
            asset("MoveAny-v1.8.299.zip"),
        ];
        let result = select_asset(&assets, &WowFlavor::Classic);
        match result {
            AssetSelection::Ambiguous(picked) => {
                let names: Vec<_> = picked.iter().map(|a| a.name.as_str()).collect();
                assert!(names.contains(&"MoveAny-v1.8.299-bcc.zip"));
                assert!(names.contains(&"MoveAny-v1.8.299-mists.zip"));
                // The bare `-classic` should NOT be among the tied winners.
                assert!(!names.contains(&"MoveAny-v1.8.299-classic.zip"));
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn filename_tokens_splits_correctly() {
        let tokens = super::filename_tokens("WeakAuras-Classic-v6.0.0.zip");
        assert!(tokens.contains(&"classic".to_string()));
        assert!(tokens.contains(&"weakauras".to_string()));
    }
}
