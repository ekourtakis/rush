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
pub fn write_package_manifest(
    registry_source: &str,
    name: &str,
    version: &str,
    target_arch: &str,
    url: &str,
    bin_name: Option<String>,
    sha256: &str,
) -> Result<()> {
    let source_path = PathBuf::from(registry_source);

    if registry_source.is_empty() || !source_path.exists() || !source_path.is_dir() {
        anyhow::bail!(
            "RUSH_REGISTRY_URL must be set to your local git repository path to use 'dev add'. Try 'export RUSH_REGISTRY_URL=\"$(pwd)\"'"
        );
    }

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

/// Developer Tool: Interactive Import wizard from GitHub
pub fn fetch_github_import_candidates(
    engine: &RushEngine,
    repo: &str,
) -> Result<(String, String, Vec<ImportCandidate>)> {
    let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let release: GitHubRelease = engine
        .client
        .get(&api_url)
        .send()?
        .error_for_status()?
        .json()?;

    let version = release.tag_name.trim_start_matches('v').to_string();
    let package_name = repo.split('/').nth(1).unwrap_or("unknown").to_string();

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

    Ok((package_name, version, candidates))
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
    use tempfile::tempdir;

    #[test]
    fn test_write_package_manifest() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // We don't actually need the engine to test writing a file,
        // we just need the path string!
        let registry_path = root.to_str().unwrap();

        // Call the module function directly
        // Note: You might need to make sure `dev` is pub in core.rs or import it via crate::core::dev
        crate::core::dev::write_package_manifest(
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
}
