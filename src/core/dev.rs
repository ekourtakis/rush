use crate::core::{RushEngine, util};
use crate::models::{
    GitHubRelease, ImportCandidate, InstallEvent, PackageManifest, ScoredAsset, TargetDefinition,
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Developer Tool: Create/Update a local package manifest
pub fn add_package_manual<F>(
    engine: &RushEngine,
    name: String,
    version: String,
    target_arch: String,
    url: String,
    bin_name: Option<String>,
    mut on_event: F,
) -> Result<()>
where
    F: FnMut(InstallEvent),
{
    // 1. Download to get checksum
    let content = util::download_url(&engine.client, &url, &mut on_event)?;

    on_event(InstallEvent::VerifyingChecksum);

    let mut hasher = Sha256::new();
    hasher.update(&content);
    let sha256 = hex::encode(hasher.finalize());

    // 2. Write to file
    write_package_manifest(
        &engine.registry_source,
        &name,
        &version,
        &target_arch,
        &url,
        bin_name,
        &sha256,
    )
}

/// Internal helper: Updates the registry file.
/// We pass `registry_source` string directly since we don't need the whole engine.
fn write_package_manifest(
    registry_source: &str,
    name: &str,
    version: &str,
    target_arch: &str,
    url: &str,
    bin_name: Option<String>,
    sha256: &str,
) -> Result<()> {
    let source_path = ensure_local_registry(registry_source)?;

    // Determine file path: e.g., packages/f/fzf.toml
    let prefix = name.chars().next().context("Package name empty")?;
    let package_dir = source_path.join("packages").join(prefix.to_string());
    let package_path = package_dir.join(format!("{}.toml", name));

    // Load existing or create new manifest
    let mut manifest = if package_path.exists() {
        let content = std::fs::read_to_string(&package_path)?;
        toml::from_str::<PackageManifest>(&content).unwrap_or_else(|_| PackageManifest {
            version: version.to_string(),
            description: None,
            targets: BTreeMap::new(),
        })
    } else {
        if !package_dir.exists() {
            std::fs::create_dir_all(&package_dir)?;
        }
        PackageManifest {
            version: version.to_string(),
            description: None,
            targets: BTreeMap::new(),
        }
    };

    // Update Struct
    manifest.version = version.to_string();
    manifest.targets.insert(
        target_arch.to_string(),
        TargetDefinition {
            url: url.to_string(),
            bin: bin_name.unwrap_or(name.to_string()),
            sha256: sha256.to_string(),
        },
    );

    // Write back
    let toml_string = toml::to_string_pretty(&manifest)?;
    std::fs::write(&package_path, toml_string)?;

    Ok(())
}

/// Ensures registry env variable is set, fails otherwise
pub fn ensure_local_registry(registry_source: &str) -> Result<PathBuf> {
    let source_path = PathBuf::from(registry_source);

    if registry_source.is_empty() || !source_path.exists() || !source_path.is_dir() {
        anyhow::bail!(
            "RUSH_REGISTRY_URL must be set to your local git repository path to alter the local registry. \
             \nTry: \n\texport RUSH_REGISTRY_URL=\"$(pwd)\""
        );
    }

    Ok(source_path)
}

/// Developer Tool: Interactive Import wizard from GitHub
pub fn fetch_github_import_candidates(
    engine: &RushEngine,
    repo: &str,
) -> Result<(String, String, Vec<ImportCandidate>)> {
    ensure_local_registry(&engine.registry_source)?;

    let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);

    let release: GitHubRelease = engine
        .client
        .get(&api_url)
        .send()?
        .error_for_status()?
        .json()?;

    let package_name = repo.split('/').nth(1).unwrap_or("unknown").to_string();

    let (version, candidates) = build_candidates_from_release(&release);

    Ok((package_name, version, candidates))
}

/// Helper: Transforms a GitHub Release into sorted ImportCandidates
fn build_candidates_from_release(release: &GitHubRelease) -> (String, Vec<ImportCandidate>) {
    let version = release.tag_name.trim_start_matches('v').to_string();

    let target_defs = vec![
        ("Linux (x86_64)", "x86_64-linux"),
        ("macOS (Apple Silicon)", "aarch64-macos"),
    ];

    let mut candidates = Vec::new();

    for (desc, target_key) in target_defs {
        let mut scored_assets: Vec<ScoredAsset> = release
            .assets
            .iter()
            .map(|asset| ScoredAsset {
                score: calculate_asset_score(&asset.name, target_key),
                asset: asset.clone(),
            })
            .collect();

        // Sort by score descending
        scored_assets.sort_by(|a, b| b.score.cmp(&a.score));

        candidates.push(ImportCandidate {
            target_desc: desc.to_string(),
            target_slug: target_key.to_string(),
            assets: scored_assets,
        });
    }

    (version, candidates)
}

/// Helper to rank assets
fn calculate_asset_score(name: &str, target_arch: &str) -> i32 {
    let name = name.to_lowercase();
    let mut score = 0;

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        score += 20;
    }
    if name.ends_with(".zip") {
        score -= 10;
    }
    if name.ends_with(".deb") || name.ends_with(".rpm") || name.ends_with(".msi") {
        score -= 100;
    }
    if name.contains("sha256") || name.contains("sum") || name.contains("sig") {
        score -= 100;
    }

    match target_arch {
        "x86_64-linux" => {
            if name.contains("linux") {
                score += 10;
            }
            if name.contains("x86_64") || name.contains("amd64") {
                score += 10;
            }
            if name.contains("musl") {
                score += 5;
            }
            if name.contains("gnu") {
                score += 3;
            }
            if name.contains("aarch64") || name.contains("arm") {
                score -= 50;
            }
            if name.contains("darwin") || name.contains("apple") || name.contains("macos") {
                score -= 50;
            }
            if name.contains("windows") || name.contains(".exe") {
                score -= 50;
            }
        }
        "aarch64-macos" => {
            if name.contains("apple") || name.contains("darwin") || name.contains("macos") {
                score += 10;
            }
            if name.contains("aarch64") || name.contains("arm64") {
                score += 10;
            }
            if name.contains("linux") {
                score -= 50;
            }
            if name.contains("x86_64") || name.contains("amd64") {
                score -= 50;
            }
            if name.contains("windows") || name.contains(".exe") {
                score -= 50;
            }
        }
        _ => {}
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GitHubAsset;
    use tempfile::tempdir;

    #[test]
    fn test_write_package_manifest() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let registry_path = root.to_str().unwrap();

        write_package_manifest(
            registry_path,
            "test-tool",
            "1.0.0",
            "x86_64-linux",
            "http://example.com",
            Some("binary-name".to_string()),
            "fake-hash-123",
        )
        .unwrap();

        let expected_path = root.join("packages").join("t").join("test-tool.toml");
        assert!(expected_path.exists());
    }

    #[test]
    fn test_write_package_manifest_invalid_path() {
        // Pass a non-existent path
        let result = write_package_manifest(
            "/path/to/nowhere",
            "pkg",
            "1.0",
            "target",
            "url",
            None,
            "hash",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be set"));
    }

    #[test]
    fn test_write_package_manifest_empty_name() {
        let temp_dir = tempdir().unwrap();
        let registry_path = temp_dir.path().to_str().unwrap();

        // Pass empty name
        let result = write_package_manifest(
            registry_path,
            "", // Empty name
            "1.0",
            "target",
            "url",
            None,
            "hash",
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Package name empty")
        );
    }

    #[test]
    fn test_write_package_manifest_update_existing() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let registry_path = root.to_str().unwrap();

        // 1. First Add: Linux
        write_package_manifest(
            registry_path,
            "multi-tool",
            "1.0.0",
            "x86_64-linux",
            "http://linux.tar.gz",
            None,
            "hash1",
        )
        .unwrap();

        // 2. Second Add: macOS (Same version, different target)
        write_package_manifest(
            registry_path,
            "multi-tool",
            "1.0.0",
            "aarch64-macos",
            "http://mac.tar.gz",
            None,
            "hash2",
        )
        .unwrap();

        // 3. Read back the file
        let toml_path = root.join("packages").join("m").join("multi-tool.toml");
        let content = std::fs::read_to_string(toml_path).unwrap();
        let manifest: PackageManifest = toml::from_str(&content).unwrap();

        // 4. Verify BOTH targets exist
        assert!(
            manifest.targets.contains_key("x86_64-linux"),
            "Lost Linux target"
        );
        assert!(
            manifest.targets.contains_key("aarch64-macos"),
            "Failed to add Mac target"
        );

        assert_eq!(manifest.targets["x86_64-linux"].sha256, "hash1");
        assert_eq!(manifest.targets["aarch64-macos"].sha256, "hash2");
    }

    #[test]
    fn test_calculate_asset_score() {
        // CASE 1: Linux x86_64
        let target = "x86_64-linux";

        // Perfect match: tar.gz (+20), linux (+10), x86_64 (+10), musl (+5) = 45
        assert_eq!(
            calculate_asset_score("app-x86_64-unknown-linux-musl.tar.gz", target),
            45
        );

        // Good match: tar.gz (+20), linux (+10), amd64 (+10) = 40
        assert_eq!(calculate_asset_score("app-linux-amd64.tar.gz", target), 40);

        // Okay match: zip (-10), linux (+10), amd64 (+10) = 10
        assert_eq!(calculate_asset_score("app-linux-amd64.zip", target), 10);

        // Wrong Arch: tar.gz (+20), linux (+10), arm64 (-50) = -20
        assert_eq!(calculate_asset_score("app-linux-arm64.tar.gz", target), -20);

        // Wrong OS: tar.gz (+20), x86_64 (+10), macos (-50) = -20
        assert_eq!(
            calculate_asset_score("app-x86_64-apple-darwin.tar.gz", target),
            -20
        );

        // Garbage: deb (-100), amd64 (+10) = -90
        assert_eq!(calculate_asset_score("app_amd64.deb", target), -90);

        // CASE 2: macOS ARM
        let target = "aarch64-macos";

        // Perfect match: tar.gz (+20), apple (+10), aarch64 (+10) = 40
        assert_eq!(
            calculate_asset_score("app-aarch64-apple-darwin.tar.gz", target),
            40
        );

        // Wrong Arch: tar.gz (+20), apple (+10), x86_64 (-50) = -20
        assert_eq!(
            calculate_asset_score("app-x86_64-apple-darwin.tar.gz", target),
            -20
        );

        // CASE 3: Global filters
        // Checksums should be heavily penalized regardless of platform
        assert!(calculate_asset_score("app-linux-amd64.tar.gz.sha256", "x86_64-linux") < -50);
    }

    #[test]
    fn test_candidate_ranking_logic() {
        // 1. Create a Fake Release with a mix of good and bad assets
        let release = GitHubRelease {
            tag_name: "v1.2.3".to_string(),
            assets: vec![
                GitHubAsset {
                    name: "app.deb".to_string(),
                    browser_download_url: "url".to_string(),
                }, // Bad (-80)
                GitHubAsset {
                    name: "app.tar.gz".to_string(),
                    browser_download_url: "url".to_string(),
                }, // Ambiguous (+20)
                GitHubAsset {
                    name: "app-x86_64-linux.tar.gz".to_string(),
                    browser_download_url: "url".to_string(),
                }, // Best (+45)
                GitHubAsset {
                    name: "app-linux.zip".to_string(),
                    browser_download_url: "url".to_string(),
                }, // Mediocre (+10)
            ],
        };

        // 2. Run logic
        let (version, candidates) = build_candidates_from_release(&release);

        assert_eq!(version, "1.2.3");

        // 3. Find the Linux candidate list
        let linux_candidate = candidates
            .iter()
            .find(|c| c.target_slug == "x86_64-linux")
            .expect("Should have generated linux targets");

        // 4. Verify Sorting Order (Best score first)
        let filenames: Vec<&str> = linux_candidate
            .assets
            .iter()
            .map(|sa| sa.asset.name.as_str())
            .collect();

        assert_eq!(
            filenames[0], "app-x86_64-linux.tar.gz",
            "Best match should be first"
        );
        assert_eq!(
            filenames[1], "app.tar.gz",
            "Generic tarball should be second"
        );
        assert_eq!(filenames[2], "app-linux.zip", "Zip should be third");
        assert_eq!(filenames[3], "app.deb", "Deb should be last");
    }
}
