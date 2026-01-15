use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;

// --- REGISTRY DATA ---
/// Represents one file (e.g. `packages/f/fzf.toml`)
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PackageManifest {
    pub version: String,
    pub description: Option<String>,
    pub targets: BTreeMap<String, TargetDefinition>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PackageDefinition {
    pub version: String,
    pub targets: HashMap<String, TargetDefinition>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TargetDefinition {
    pub url: String,
    pub bin: String,
    pub sha256: String,
}

// --- GITHUB API DATA ---
#[derive(Deserialize, Debug)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Deserialize, Debug, Clone)] // Added Clone
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

// --- DATA TRANSFER OBJECTS (Core -> UI) ---

/// Represents a candidate for import found in a GitHub release
#[derive(Debug)]
pub struct ImportCandidate {
    /// The specific system target (e.g. "x86_64-linux")
    pub target_slug: String,
    /// A human-readable description (e.g. "Linux (x86_64)")
    pub target_desc: String,
    /// List of assets sorted by relevance score
    pub assets: Vec<ScoredAsset>,
}

#[derive(Debug)]
pub struct ScoredAsset {
    pub score: i32,
    pub asset: GitHubAsset,
}

// --- STATE DATA ---
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct State {
    pub packages: HashMap<String, InstalledPackage>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct InstalledPackage {
    pub version: String,
    pub binaries: Vec<String>,
}

// -- FUNCTION RESULTS ---

/// Result of `RushEngine::clean_trash()`
#[derive(Debug)]
pub struct CleanResult {
    pub files_cleaned: Vec<String>,
}

/// Result of `RushEngine::uninstall_package()`
#[derive(Debug)]
pub struct UninstallResult {
    /// The name of the package that was uninstalled.
    pub package_name: String,
    /// The list of binary files that were deleted.
    pub binaries_removed: Vec<String>,
}

/// Result of RushEngine::update_registry()
#[derive(Debug)]
pub struct UpdateResult {
    /// The source URL or path the registry was updated from.
    pub source: String,
}

/// Result of RushEngine::install_package()
#[derive(Debug)]
pub struct InstallResult {
    /// The name of the package installed
    pub package_name: String,
    /// The version installed
    pub version: String,
    /// The final path to the binary on disk
    pub path: PathBuf,
}

// REAL-TIME EVENTS

/// Event from `RushEngine::install_package()` and `add_package_manual`
pub enum InstallEvent {
    /// The download has started
    Downloading { total_bytes: u64 },
    /// A chunk of the download has been received
    Progress { bytes: u64, total: u64 },
    /// Calculating SHA256
    VerifyingChecksum,
    /// Extracting the archive
    Extracting,
    /// Installation complete (before returning result)
    Success,
}

/// Event from `RushEngine::update_registry()`
pub enum UpdateEvent {
    /// The download of the registry has started.
    Fetching { source: String },
    /// A chunk of the download has been received.
    Progress { bytes: u64, total: u64 },
    /// The download is complete and is being unpacked.
    Unpacking,
}

// --- VERIFICATION RESULTS ---

#[derive(Debug)]
pub struct VerifyResult {
    pub packages_checked: usize,
    pub targets_checked: usize,
    pub failures: Vec<VerificationFailure>,
}

#[derive(Debug)]
pub struct VerificationFailure {
    pub package_name: String,
    pub version: String,
    pub target: String,
    pub error: String,
}

// REAL TIME EVENTS

/// Event from `RushEngine::verify_registry()`
pub enum VerifyEvent {
    /// We are starting to check a specific target
    Checking { name: String, target: String },
    /// Progress updates from the underlying download/install logic
    Progress(InstallEvent),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Verify we can parse an existing package manifest format
    /// If this test fails, it means we broke compatibility with our own registry.
    fn test_package_manifest_contract() {
        // This matches the registry structure (one file per package)
        let toml_input = r#"
        version = "1.0.0"
        description = "A test tool"
        
        [targets.x86_64-linux]
        url = "https://example.com/tool.tar.gz"
        bin = "tool"
        sha256 = "abc123456"
        "#;

        let manifest: PackageManifest =
            toml::from_str(toml_input).expect("Failed to parse package manifest");

        assert_eq!(manifest.version, "1.0.0");

        let target = &manifest.targets["x86_64-linux"];
        assert_eq!(target.bin, "tool");
    }

    #[test]
    /// Verify we can parse an existing installed.json format
    /// If this test fails, it means we broke compatibility with our existing state files.
    fn test_state_json_contract() {
        let json_input = r#"
        {
        "packages": {
            "grep": {
                    "version": "2.0",
                    "binaries": ["grep"]
                }
            }
        }
        "#;

        let state: State = serde_json::from_str(json_input).expect("Failed to parse state JSON");
        assert_eq!(state.packages["grep"].version, "2.0");
    }

    #[test]
    /// Verify that Saving -> Loading gives the exact same data
    fn test_state_round_trip() {
        let mut original = State::default();
        original.packages.insert(
            "foo".to_string(),
            InstalledPackage {
                version: "1.0".to_string(),
                binaries: vec!["bar".to_string()],
            },
        );

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: State = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original.packages["foo"], deserialized.packages["foo"]);
    }

    #[test]
    /// Verify we can parse GitHub Releases API JSON
    fn test_github_release_deserialization() {
        // Sample JSON from GitHub Releases API (abbreviated)
        let json = r#"{
            "url": "https://api.github.com/repos/octocat/Hello-World/releases/1",
            "tag_name": "v1.0.0",
            "assets": [
                {
                "name": "example.zip",
                "browser_download_url": "https://github.com/octocat/Hello-World/releases/download/v1.0.0/example.zip"
                }
                ]
            }"#;

        let release: GitHubRelease =
            serde_json::from_str(json).expect("Failed to parse GitHub JSON");

        assert_eq!(release.tag_name, "v1.0.0");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "example.zip");
    }
}
