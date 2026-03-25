use std::path::Path;

use crate::addon::WowFlavor;

/// Parsed metadata from a WoW addon `.toc` file.
pub struct TocInfo {
    pub title: Option<String>,
    pub version: Option<String>,
    /// Addon folder names listed in `## RequiredDeps:` / `## Dependencies:`.
    pub dependencies: Vec<String>,
}

/// Read and parse the `.toc` file for the given addon folder.
///
/// WoW supports flavor-suffixed `.toc` files (e.g. `ElvUI_Mainline.toc`).
/// We try, in order:
///   1. `{Name}_{FlavorSuffix}.toc`  (exact match for the active flavor)
///   2. Any `{Name}_*.toc`           (pick the first one found)
///   3. `{Name}.toc`                 (classic fallback)
pub fn read_toc(addons_dir: &Path, folder_name: &str, flavor: &WowFlavor) -> Option<TocInfo> {
    let addon_dir = addons_dir.join(folder_name);

    // 1. Try the flavor-specific suffix first
    for suffix in flavor.toc_suffixes() {
        let path = addon_dir.join(format!("{folder_name}_{suffix}.toc"));
        if let Some(info) = parse_toc_file(&path) {
            return Some(info);
        }
    }

    // 2. Try plain {Name}.toc
    let plain = addon_dir.join(format!("{folder_name}.toc"));
    if let Some(info) = parse_toc_file(&plain) {
        return Some(info);
    }

    // 3. Glob for any {Name}_*.toc as last resort
    if let Ok(entries) = std::fs::read_dir(&addon_dir) {
        let prefix = format!("{folder_name}_");
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix)
                && name.ends_with(".toc")
                && let Some(info) = parse_toc_file(&entry.path())
            {
                return Some(info);
            }
        }
    }

    None
}

/// Parse a single `.toc` file into `TocInfo`.
fn parse_toc_file(path: &Path) -> Option<TocInfo> {
    let contents = std::fs::read_to_string(path).ok()?;

    let mut title = None;
    let mut version = None;
    let mut dependencies: Vec<String> = Vec::new();

    for line in contents.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("## Title:") {
            title = Some(strip_ui_codes(rest.trim()));
        } else if let Some(rest) = line.strip_prefix("## Version:") {
            version = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("## RequiredDeps:") {
            collect_deps(rest, &mut dependencies);
        } else if dependencies.is_empty()
            && let Some(rest) = line.strip_prefix("## Dependencies:")
        {
            collect_deps(rest, &mut dependencies);
        }
    }

    Some(TocInfo {
        title,
        version,
        dependencies,
    })
}

fn collect_deps(s: &str, out: &mut Vec<String>) {
    for dep in s.split(',') {
        let d = dep.trim().to_string();
        if !d.is_empty() {
            out.push(d);
        }
    }
}

/// Strip WoW UI pipe-codes from a string.
///
/// Handles:
/// - `|cAARRGGBB…|r` — color codes
/// - `|T…|t`         — texture references
/// - `|A…|a`         — atlas references
/// - `|n`            — in-string newlines (dropped)
pub fn strip_ui_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'|' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                // |cAARRGGBB — 10 bytes total
                b'c' | b'C' if i + 10 <= bytes.len() => {
                    let hex = &s[i + 2..i + 10];
                    if hex.chars().all(|c| c.is_ascii_hexdigit()) {
                        i += 10;
                        continue;
                    }
                }
                // |r — end color
                b'r' | b'R' => {
                    i += 2;
                    continue;
                }
                // |n — in-string newline, drop it
                b'n' | b'N' => {
                    i += 2;
                    continue;
                }
                // |T…|t — texture reference, skip entire sequence
                b't' | b'T' => {
                    if let Some(off) = find_pipe_close(&s[i + 2..], b't') {
                        i += 2 + off + 2;
                    } else {
                        i += 2;
                    }
                    continue;
                }
                // |A…|a — atlas reference, skip entire sequence
                b'a' | b'A' => {
                    if let Some(off) = find_pipe_close(&s[i + 2..], b'a') {
                        i += 2 + off + 2;
                    } else {
                        i += 2;
                    }
                    continue;
                }
                _ => {}
            }
        }
        if let Some(ch) = s[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            i += 1;
        }
    }

    out.trim().to_string()
}

/// Find the first `|X` (case-insensitive) in `s`, returning the byte offset
/// of the `|` within `s`.
fn find_pipe_close(s: &str, close: u8) -> Option<usize> {
    let upper = close.to_ascii_uppercase();
    let bytes = s.as_bytes();
    (0..bytes.len().saturating_sub(1))
        .find(|&j| bytes[j] == b'|' && (bytes[j + 1] == close || bytes[j + 1] == upper))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_color_codes() {
        assert_eq!(
            strip_ui_codes("|cff00ff00HandyNotes|r: Midnight"),
            "HandyNotes: Midnight"
        );
    }

    #[test]
    fn strips_interleaved_color_codes() {
        // MoveAny-style title
        assert_eq!(strip_ui_codes("M|cff3FC7EBove|rA|cff3FC7EBny|r"), "MoveAny");
    }

    #[test]
    fn strips_multiple_codes() {
        assert_eq!(
            strip_ui_codes("|cffff0000Red|r and |cff0000ffBlue|r"),
            "Red and Blue"
        );
    }

    #[test]
    fn strips_texture_codes() {
        assert_eq!(
            strip_ui_codes("Addon|TInterface/path/icon.tga:16:16|t Name"),
            "Addon Name"
        );
    }

    #[test]
    fn no_codes_unchanged() {
        assert_eq!(strip_ui_codes("WeakAuras"), "WeakAuras");
    }

    #[test]
    fn reads_toc_title_version_deps() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let addon_dir = tmp.path().join("MyAddon");
        fs::create_dir_all(&addon_dir).unwrap();
        fs::write(
            addon_dir.join("MyAddon.toc"),
            "## Title: |cff00ff00My Addon|r\n\
             ## Version: 2.1.0\n\
             ## RequiredDeps: LibStub, CallbackHandler\n",
        )
        .unwrap();

        let info = read_toc(tmp.path(), "MyAddon", &WowFlavor::Retail).unwrap();
        assert_eq!(info.title.as_deref(), Some("My Addon"));
        assert_eq!(info.version.as_deref(), Some("2.1.0"));
        assert_eq!(info.dependencies, vec!["LibStub", "CallbackHandler"]);
    }

    #[test]
    fn reads_flavor_suffixed_toc() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let addon_dir = tmp.path().join("ElvUI");
        fs::create_dir_all(&addon_dir).unwrap();
        fs::write(
            addon_dir.join("ElvUI_Mainline.toc"),
            "## Title: ElvUI\n## Version: 13.80\n",
        )
        .unwrap();

        let info = read_toc(tmp.path(), "ElvUI", &WowFlavor::Retail).unwrap();
        assert_eq!(info.title.as_deref(), Some("ElvUI"));
        assert_eq!(info.version.as_deref(), Some("13.80"));
    }

    #[test]
    fn flavor_toc_preferred_over_plain() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let addon_dir = tmp.path().join("Test");
        fs::create_dir_all(&addon_dir).unwrap();
        fs::write(
            addon_dir.join("Test.toc"),
            "## Title: Test Plain\n## Version: 1.0\n",
        )
        .unwrap();
        fs::write(
            addon_dir.join("Test_Mainline.toc"),
            "## Title: Test Mainline\n## Version: 2.0\n",
        )
        .unwrap();

        let info = read_toc(tmp.path(), "Test", &WowFlavor::Retail).unwrap();
        assert_eq!(info.title.as_deref(), Some("Test Mainline"));
        assert_eq!(info.version.as_deref(), Some("2.0"));
    }

    #[test]
    fn falls_back_to_glob_toc() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let addon_dir = tmp.path().join("Foo");
        fs::create_dir_all(&addon_dir).unwrap();
        // No Foo.toc, no Foo_Vanilla.toc, but Foo_Wrath.toc exists
        fs::write(
            addon_dir.join("Foo_Wrath.toc"),
            "## Title: Foo Wrath\n## Version: 3.0\n",
        )
        .unwrap();

        let info = read_toc(tmp.path(), "Foo", &WowFlavor::ClassicEra).unwrap();
        assert_eq!(info.title.as_deref(), Some("Foo Wrath"));
    }
}
