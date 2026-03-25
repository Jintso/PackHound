use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use tempfile::NamedTempFile;

/// Download a URL to a temporary file, streaming the response body.
/// `on_progress(downloaded_bytes, total_bytes_opt)` is called after each chunk.
pub async fn download_to_temp(
    client: &reqwest::Client,
    url: &str,
    on_progress: impl Fn(u64, Option<u64>),
) -> Result<NamedTempFile> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url} failed"))?;

    if !response.status().is_success() {
        bail!("Download failed: HTTP {}", response.status());
    }

    let total = response.content_length();
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    let mut tmp = NamedTempFile::new().context("Failed to create temp file")?;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error reading download stream")?;
        std::io::Write::write_all(&mut tmp, &chunk)
            .context("Failed to write chunk to temp file")?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }

    Ok(tmp)
}

/// Extract a zip archive into `dest_dir`.
/// Returns the list of top-level directory names found in the zip (addon folder names).
/// Rejects any entry whose path would escape `dest_dir`.
pub fn extract_addon(zip_path: &Path, dest_dir: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(zip_path)
        .with_context(|| format!("Failed to open {}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("Not a valid zip archive")?;
    let mut top_level: HashSet<String> = HashSet::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("Failed to read zip entry {i}"))?;

        let entry_name = entry.name().to_string();
        let out_path = safe_join(dest_dir, &entry_name)
            .with_context(|| format!("Unsafe path in zip: {entry_name}"))?;

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)
                .with_context(|| format!("Failed to create {}", out_path.display()))?;
            std::io::copy(&mut entry, &mut out_file).context("Failed to extract entry")?;
        }

        // Collect the top-level component (the addon folder name)
        if let Some(top) = entry_name.split('/').next().filter(|s| !s.is_empty()) {
            top_level.insert(top.to_string());
        }
    }

    Ok(top_level.into_iter().collect())
}

/// Join `base` with a zip entry path, rejecting any traversal attempts.
fn safe_join(base: &Path, zip_path: &str) -> Result<PathBuf> {
    let mut result = base.to_path_buf();
    for component in Path::new(zip_path).components() {
        match component {
            Component::Normal(name) => result.push(name),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("Unsafe component in zip path: {zip_path}");
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    fn make_zip(files: &[(&str, &[u8])]) -> NamedTempFile {
        let mut tmp = NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        for (name, content) in files {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(content).unwrap();
        }
        let buf = zip.finish().unwrap().into_inner();
        tmp.write_all(&buf).unwrap();
        tmp
    }

    #[test]
    fn extract_creates_files() {
        let dest = TempDir::new().unwrap();
        let zip = make_zip(&[
            ("MyAddon/MyAddon.lua", b"-- lua"),
            ("MyAddon/MyAddon.toc", b"## Title: MyAddon"),
        ]);
        let folders = extract_addon(zip.path(), dest.path()).unwrap();
        assert!(folders.contains(&"MyAddon".to_string()));
        assert!(dest.path().join("MyAddon/MyAddon.lua").exists());
        assert!(dest.path().join("MyAddon/MyAddon.toc").exists());
    }

    #[test]
    fn extract_returns_multiple_top_level_dirs() {
        let dest = TempDir::new().unwrap();
        let zip = make_zip(&[("Addon1/file.lua", b""), ("Addon2/file.lua", b"")]);
        let mut folders = extract_addon(zip.path(), dest.path()).unwrap();
        folders.sort();
        assert_eq!(folders, vec!["Addon1", "Addon2"]);
    }

    #[test]
    fn safe_join_allows_normal_paths() {
        let base = Path::new("/dest");
        let result = safe_join(base, "Addon/file.lua").unwrap();
        assert_eq!(result, Path::new("/dest/Addon/file.lua"));
    }

    #[test]
    fn safe_join_rejects_parent_traversal() {
        let base = Path::new("/dest");
        assert!(safe_join(base, "../etc/passwd").is_err());
    }

    #[test]
    fn safe_join_rejects_absolute_paths() {
        let base = Path::new("/dest");
        assert!(safe_join(base, "/etc/passwd").is_err());
    }

    #[test]
    fn safe_join_allows_dot_components() {
        let base = Path::new("/dest");
        let result = safe_join(base, "./Addon/./file.lua").unwrap();
        assert_eq!(result, Path::new("/dest/Addon/file.lua"));
    }
}
