//! `cargo xtask assets`: every non-code asset must be attributed in `docs/attribution.md`.
//!
//! Scans all git-tracked (and untracked, not ignored) files and fails if an asset is not listed
//! as `` `path` `` in docs/attribution.md's table. An asset is:
//! - any file under `assets/`;
//! - any file in `examples/` with an asset or sample-workbook extension (`.xlsx`, `.csv`, ...);
//! - anywhere else, any image, icon or font file, or a workbook (`.xlsx`/`.xlsm`/`.xls`).
//!
//! See the asset rule in AGENTS.md: no vendor artwork, and every asset has an attribution row.

use std::path::Path;
use std::process::Command;

/// Image, icon and font extensions: an asset wherever it lives.
const ASSET_EXT: &[&str] =
    &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "ico", "icns", "svg", "pdf", "ttf", "otf", "woff", "woff2", "xlsx", "xlsm", "xls"];
/// Extra extensions that count as assets inside `examples/` (sample data).
const EXAMPLE_EXT: &[&str] = &["csv", "tsv", "json"];
/// Every file under these directories is an asset.
const ASSET_DIRS: &[&str] = &["assets/"];

fn ext(path: &str) -> String {
    Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}

pub fn is_asset(path: &str) -> bool {
    let e = ext(path);
    if ASSET_DIRS.iter().any(|d| path.starts_with(d)) || ASSET_EXT.contains(&e.as_str()) {
        return true;
    }
    (path.starts_with("examples/") || path.contains("/examples/")) && EXAMPLE_EXT.contains(&e.as_str())
}

/// Paths that need a docs/attribution.md row but lack one.
pub fn missing(files: &[String], attribution: &str) -> Vec<String> {
    files.iter().filter(|f| is_asset(f) && !attribution.contains(&format!("`{f}`"))).cloned().collect()
}

pub fn run(root: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err(format!("git ls-files failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let files: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().filter(|l| root.join(l).exists()).map(str::to_owned).collect();
    let md = std::fs::read_to_string(root.join("docs/attribution.md")).map_err(|e| format!("docs/attribution.md: {e}"))?;
    let miss = missing(&files, &md);
    if miss.is_empty() {
        println!("assets: all {} asset files attributed in docs/attribution.md", files.iter().filter(|f| is_asset(f)).count());
        Ok(())
    } else {
        Err(format!("{} asset file(s) lack a docs/attribution.md row (author, source, licence):\n  {}", miss.len(), miss.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unattributed_assets() {
        let files = vec!["assets/icon.svg".to_string(), "crates/x/src/lib.rs".into(), "docs/images/b.png".into(), "tests/fixture.xlsx".into()];
        let md = "| `assets/icon.svg` | me | here | MIT |";
        assert_eq!(missing(&files, md), vec!["docs/images/b.png".to_string(), "tests/fixture.xlsx".into()]);
    }

    #[test]
    fn classifies_paths() {
        assert!(!is_asset("crates/x/src/lib.rs"));
        assert!(!is_asset("docs/architecture.md"));
        assert!(is_asset("assets/icon.svg"));
        assert!(is_asset("examples/sales.csv"));
        assert!(is_asset("crates/engine/engine/examples/budget.xlsx"));
        assert!(!is_asset("crates/formats/xlsx/tests/data.csv"));
        assert!(is_asset("crates/engine/core/icon.PNG"));
    }
}
