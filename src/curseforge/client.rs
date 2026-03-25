use anyhow::{Context, Result, bail};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;

use crate::addon::WowFlavor;

const CF_API: &str = "https://api.curseforge.com";
const APP_USER_AGENT: &str = concat!("addon-manager/", env!("CARGO_PKG_VERSION"));

/// WoW's game ID on CurseForge. This is a well-known constant.
const WOW_GAME_ID: u32 = 1;

/// CurseForge game version type IDs for WoW flavors.
/// These map to the `gameVersionTypeId` filter on the files endpoint.
///
/// Discovered via `GET /v1/games/1/version-types` and cached here as constants
/// since they are stable values assigned by CurseForge.
const VERSION_TYPE_RETAIL: u32 = 517;
const VERSION_TYPE_CLASSIC: u32 = 67408;
const VERSION_TYPE_CLASSIC_ERA: u32 = 73246;

/// A CurseForge mod (addon project).
#[derive(Debug, Clone)]
pub struct CfMod {
    pub id: u32,
    pub name: String,
}

/// A downloadable file for a CurseForge mod.
#[derive(Debug, Clone)]
pub struct CfFile {
    pub id: u32,
    pub display_name: String,
    pub file_name: String,
    pub download_url: Option<String>,
}

impl CfFile {
    /// Return the download URL, falling back to the edge CDN URL when the API
    /// returns `null` (addon author disabled third-party downloads).
    pub fn resolve_download_url(&self) -> String {
        if let Some(url) = &self.download_url {
            return url.clone();
        }
        // Construct from edge CDN: files/{id/1000}/{id%1000}/{filename}
        let part1 = self.id / 1000;
        let part2 = self.id % 1000;
        let encoded_name = self.file_name.replace(' ', "+");
        format!("https://edge.forgecdn.net/files/{part1}/{part2}/{encoded_name}")
    }
}

// ── API response types ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Deserialize)]
struct ApiMod {
    id: u32,
    name: String,
    slug: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiFile {
    id: u32,
    display_name: String,
    file_name: String,
    download_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiGameVersionType {
    id: u32,
    name: String,
    slug: String,
}

// ── Client ────────────────────────────────────────────────────────────────────

/// Client for the CurseForge REST API.
pub struct CurseForgeClient {
    client: reqwest::Client,
}

impl CurseForgeClient {
    pub fn new(api_key: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(api_key).context("Invalid CurseForge API key format")?,
        );

        let client = reqwest::Client::builder()
            .user_agent(APP_USER_AGENT)
            .default_headers(headers)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { client })
    }

    /// Search for a WoW addon by slug. Returns the first exact match.
    pub async fn find_mod_by_slug(&self, slug: &str) -> Result<CfMod> {
        let url = format!("{CF_API}/v1/mods/search?gameId={WOW_GAME_ID}&slug={slug}&pageSize=5");
        let resp = self.get(&url).await?;
        let body: ApiResponse<Vec<ApiMod>> = resp
            .json()
            .await
            .context("Failed to parse CurseForge search response")?;

        body.data
            .into_iter()
            .find(|m| m.slug == slug)
            .map(|m| CfMod {
                id: m.id,
                name: m.name,
            })
            .ok_or_else(|| anyhow::anyhow!("No CurseForge addon found with slug '{slug}'"))
    }

    /// Get a mod by its numeric ID.
    #[allow(dead_code)]
    pub async fn get_mod(&self, mod_id: u32) -> Result<CfMod> {
        let url = format!("{CF_API}/v1/mods/{mod_id}");
        let resp = self.get(&url).await?;
        let body: ApiResponse<ApiMod> = resp
            .json()
            .await
            .context("Failed to parse CurseForge mod response")?;

        let m = body.data;
        Ok(CfMod {
            id: m.id,
            name: m.name,
        })
    }

    /// List files for a mod, optionally filtered by WoW flavor.
    pub async fn list_files(&self, mod_id: u32, flavor: Option<&WowFlavor>) -> Result<Vec<CfFile>> {
        let mut url = format!("{CF_API}/v1/mods/{mod_id}/files?pageSize=50");
        if let Some(f) = flavor {
            let vtype = version_type_id(f);
            url.push_str(&format!("&gameVersionTypeId={vtype}"));
        }

        let resp = self.get(&url).await?;
        let body: ApiResponse<Vec<ApiFile>> = resp
            .json()
            .await
            .context("Failed to parse CurseForge files response")?;

        Ok(body
            .data
            .into_iter()
            .map(|f| CfFile {
                id: f.id,
                display_name: f.display_name,
                file_name: f.file_name,
                download_url: f.download_url,
            })
            .collect())
    }

    /// Fetch the game version types for WoW. Useful for discovery/debugging
    /// but not needed at runtime since we use hardcoded constants.
    #[allow(dead_code)]
    pub async fn list_version_types(&self) -> Result<Vec<(u32, String, String)>> {
        let url = format!("{CF_API}/v1/games/{WOW_GAME_ID}/version-types");
        let resp = self.get(&url).await?;
        let body: ApiResponse<Vec<ApiGameVersionType>> = resp
            .json()
            .await
            .context("Failed to parse version types response")?;

        Ok(body
            .data
            .into_iter()
            .map(|vt| (vt.id, vt.name, vt.slug))
            .collect())
    }

    async fn get(&self, url: &str) -> Result<reqwest::Response> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Request to {url} failed"))?;

        match resp.status().as_u16() {
            200 => Ok(resp),
            401 | 403 => bail!("CurseForge API key is invalid or unauthorized"),
            404 => bail!("Not found on CurseForge"),
            429 => bail!("CurseForge API rate limit exceeded"),
            code => bail!("CurseForge API returned unexpected status {code}"),
        }
    }
}

/// Map a WoW flavor to the CurseForge game version type ID.
pub fn version_type_id(flavor: &WowFlavor) -> u32 {
    match flavor {
        WowFlavor::Retail => VERSION_TYPE_RETAIL,
        WowFlavor::Classic => VERSION_TYPE_CLASSIC,
        WowFlavor::ClassicEra => VERSION_TYPE_CLASSIC_ERA,
    }
}

/// Parse a CurseForge addon URL into the project slug.
///
/// Accepts:
/// - `https://www.curseforge.com/wow/addons/bigwigs`
/// - `https://curseforge.com/wow/addons/bigwigs`
/// - `https://www.curseforge.com/wow/addons/bigwigs/files` (trailing path)
pub fn parse_curseforge_url(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');

    // Strip scheme and www prefix
    let path = url
        .strip_prefix("https://www.curseforge.com/")
        .or_else(|| url.strip_prefix("https://curseforge.com/"))
        .with_context(|| format!("Not a CurseForge URL: {url}"))?;

    // Expected: wow/addons/{slug}[/...]
    let parts: Vec<&str> = path.splitn(4, '/').collect();
    if parts.len() < 3 || parts[0] != "wow" || parts[1] != "addons" || parts[2].is_empty() {
        bail!("URL must be in the form https://www.curseforge.com/wow/addons/{{slug}}, got: {url}");
    }

    Ok(parts[2].to_string())
}

/// Detect whether a URL is a CurseForge addon URL.
pub fn is_curseforge_url(url: &str) -> bool {
    let url = url.trim();
    (url.starts_with("https://www.curseforge.com/wow/addons/")
        || url.starts_with("https://curseforge.com/wow/addons/"))
        && parse_curseforge_url(url).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_curseforge_url() {
        let slug = parse_curseforge_url("https://www.curseforge.com/wow/addons/bigwigs").unwrap();
        assert_eq!(slug, "bigwigs");
    }

    #[test]
    fn parse_curseforge_url_without_www() {
        let slug = parse_curseforge_url("https://curseforge.com/wow/addons/details").unwrap();
        assert_eq!(slug, "details");
    }

    #[test]
    fn parse_curseforge_url_with_subpath() {
        let slug =
            parse_curseforge_url("https://www.curseforge.com/wow/addons/bigwigs/files").unwrap();
        assert_eq!(slug, "bigwigs");
    }

    #[test]
    fn parse_curseforge_url_with_trailing_slash() {
        let slug = parse_curseforge_url("https://www.curseforge.com/wow/addons/bigwigs/").unwrap();
        assert_eq!(slug, "bigwigs");
    }

    #[test]
    fn parse_curseforge_url_rejects_non_curseforge() {
        assert!(parse_curseforge_url("https://github.com/owner/repo").is_err());
    }

    #[test]
    fn parse_curseforge_url_rejects_non_wow() {
        assert!(
            parse_curseforge_url("https://www.curseforge.com/minecraft/mods/something").is_err()
        );
    }

    #[test]
    fn parse_curseforge_url_rejects_missing_slug() {
        assert!(parse_curseforge_url("https://www.curseforge.com/wow/addons/").is_err());
    }

    #[test]
    fn is_curseforge_url_detects_correctly() {
        assert!(is_curseforge_url(
            "https://www.curseforge.com/wow/addons/bigwigs"
        ));
        assert!(is_curseforge_url(
            "https://curseforge.com/wow/addons/bigwigs"
        ));
        assert!(!is_curseforge_url("https://github.com/owner/repo"));
        assert!(!is_curseforge_url("https://www.curseforge.com/wow/addons/"));
    }

    #[test]
    fn resolve_download_url_uses_api_url_when_present() {
        let file = CfFile {
            id: 6203042,
            display_name: "v1.0".into(),
            file_name: "MyAddon-1.0.zip".into(),
            download_url: Some("https://example.com/direct.zip".into()),
        };
        assert_eq!(
            file.resolve_download_url(),
            "https://example.com/direct.zip"
        );
    }

    #[test]
    fn resolve_download_url_falls_back_to_cdn() {
        let file = CfFile {
            id: 6203042,
            display_name: "v1.0".into(),
            file_name: "MyAddon-1.0.zip".into(),
            download_url: None,
        };
        assert_eq!(
            file.resolve_download_url(),
            "https://edge.forgecdn.net/files/6203/42/MyAddon-1.0.zip"
        );
    }

    #[test]
    fn resolve_download_url_encodes_spaces() {
        let file = CfFile {
            id: 5100003,
            display_name: "v2.0".into(),
            file_name: "My Addon File.zip".into(),
            download_url: None,
        };
        assert_eq!(
            file.resolve_download_url(),
            "https://edge.forgecdn.net/files/5100/3/My+Addon+File.zip"
        );
    }

    #[test]
    fn version_type_mapping() {
        assert_eq!(version_type_id(&WowFlavor::Retail), VERSION_TYPE_RETAIL);
        assert_eq!(version_type_id(&WowFlavor::Classic), VERSION_TYPE_CLASSIC);
        assert_eq!(
            version_type_id(&WowFlavor::ClassicEra),
            VERSION_TYPE_CLASSIC_ERA
        );
    }
}
