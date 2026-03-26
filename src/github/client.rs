use anyhow::{Context, Result, bail};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use serde::Deserialize;

const GITHUB_API: &str = "https://api.github.com";
const APP_USER_AGENT: &str = concat!("packhound/", env!("CARGO_PKG_VERSION"));

/// A GitHub release with its downloadable assets.
#[derive(Debug, Clone)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<ReleaseAsset>,
    /// ISO 8601 publication timestamp from the GitHub API, e.g. `2025-01-15T10:30:00Z`.
    pub published_at: Option<String>,
}

/// A single downloadable asset attached to a GitHub release.
#[derive(Debug, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

/// Raw GitHub API response shapes — not exposed outside this module.
#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    assets: Vec<ApiAsset>,
    published_at: Option<String>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Client for the GitHub REST API.
pub struct GitHubClient {
    client: reqwest::Client,
}

impl GitHubClient {
    pub fn new(token: Option<&str>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        if let Some(t) = token {
            let value = HeaderValue::from_str(&format!("Bearer {t}"))
                .context("Invalid GitHub token format")?;
            headers.insert(AUTHORIZATION, value);
        }

        let client = reqwest::Client::builder()
            .user_agent(APP_USER_AGENT)
            .default_headers(headers)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { client })
    }

    /// Fetch the latest release for a GitHub repository.
    pub async fn fetch_latest_release(&self, owner: &str, repo: &str) -> Result<Release> {
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/releases/latest");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Request to {url} failed"))?;

        match response.status().as_u16() {
            200 => {}
            404 => bail!("Repository {owner}/{repo} not found or has no releases"),
            403 | 429 => {
                bail!("GitHub API rate limit exceeded. Add a token in Settings for higher limits.")
            }
            code => bail!("GitHub API returned unexpected status {code}"),
        }

        let api_release: ApiRelease = response
            .json()
            .await
            .context("Failed to parse GitHub release response")?;

        Ok(Release {
            tag_name: api_release.tag_name,
            published_at: api_release.published_at,
            assets: api_release
                .assets
                .into_iter()
                .map(|a| ReleaseAsset {
                    name: a.name,
                    download_url: a.browser_download_url,
                    size: a.size,
                })
                .collect(),
        })
    }
}

/// Parse a GitHub repository URL into `(owner, repo)`.
///
/// Accepts:
/// - `https://github.com/owner/repo`
/// - `https://github.com/owner/repo.git`
/// - `https://github.com/owner/repo/` (trailing slash)
pub fn parse_repo_url(url: &str) -> Result<(String, String)> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);

    let path = url
        .strip_prefix("https://github.com/")
        .with_context(|| format!("Not a GitHub URL: {url}"))?;

    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("URL must be in the form https://github.com/owner/repo, got: {url}");
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_url() {
        let (owner, repo) = parse_repo_url("https://github.com/WeakAuras/WeakAuras2").unwrap();
        assert_eq!(owner, "WeakAuras");
        assert_eq!(repo, "WeakAuras2");
    }

    #[test]
    fn parse_url_with_git_suffix() {
        let (owner, repo) = parse_repo_url("https://github.com/WeakAuras/WeakAuras2.git").unwrap();
        assert_eq!(owner, "WeakAuras");
        assert_eq!(repo, "WeakAuras2");
    }

    #[test]
    fn parse_url_with_trailing_slash() {
        let (owner, repo) = parse_repo_url("https://github.com/WeakAuras/WeakAuras2/").unwrap();
        assert_eq!(owner, "WeakAuras");
        assert_eq!(repo, "WeakAuras2");
    }

    #[test]
    fn parse_url_ignores_subpaths() {
        // e.g. user pastes a releases page URL
        let (owner, repo) =
            parse_repo_url("https://github.com/WeakAuras/WeakAuras2/releases").unwrap();
        assert_eq!(owner, "WeakAuras");
        assert_eq!(repo, "WeakAuras2");
    }

    #[test]
    fn parse_url_rejects_non_github() {
        assert!(parse_repo_url("https://gitlab.com/owner/repo").is_err());
    }

    #[test]
    fn parse_url_rejects_missing_repo() {
        assert!(parse_repo_url("https://github.com/owner").is_err());
    }

    #[test]
    fn parse_url_rejects_empty() {
        assert!(parse_repo_url("").is_err());
    }
}
