pub mod clean;
pub mod uninstall;
pub mod util;

use crate::models::{
    GitHubRelease, ImportCandidate, InstallEvent, InstallResult, InstalledPackage, PackageManifest,
    ScoredAsset, State, TargetDefinition, UninstallResult, UpdateEvent, UpdateResult,
};
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::fs::{self};
use std::path::PathBuf;
use tar::Archive;
use walkdir::WalkDir;

/// Default URL to fetch the registry from, overridable by env variable
const DEFAULT_REGISTRY_URL: &str =
    "https://github.com/ekourtakis/rush/archive/refs/heads/main.tar.gz";

/// The core engine that handles state and I/O
pub struct RushEngine {
    pub state: State,
    state_path: PathBuf,               // ~/.local/share/rush/installed.json
    registry_dir: PathBuf,             // ~/.local/share/rush/registry/
    bin_path: PathBuf,                 // ~/.local/bin
    client: reqwest::blocking::Client, // HTTP Client
    registry_source: String,
}

impl RushEngine {
    /// Standard constructor
    /// Reads HOME and Env Vars automatically.
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("No home dir")?;
        let source =
            std::env::var("RUSH_REGISTRY_URL").unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string());
        Self::init(home, source)
    }

    /// Test constructor: Isolated Root + Default Registry
    pub fn with_root(root: PathBuf) -> Result<Self> {
        Self::init(root, DEFAULT_REGISTRY_URL.to_string())
    }

    /// Test constructor: Isolated Root + Custom Registry Source
    pub fn with_root_and_registry(root: PathBuf, registry_source: String) -> Result<Self> {
        Self::init(root, registry_source)
    }

    /// Shared initialization logic
    fn init(root: PathBuf, registry_source: String) -> Result<Self> {
        let state_dir = root.join(".local/share/rush");
        let bin_path = root.join(".local/bin");
        let state_path = state_dir.join("installed.json");
        let registry_dir = state_dir.join("registry");

        fs::create_dir_all(&state_dir)?;
        fs::create_dir_all(&bin_path)?;

        let state = if state_path.exists() {
            let content = fs::read_to_string(&state_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            State::default()
        };

        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("rush/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            state,
            state_path,
            registry_dir,
            bin_path,
            client,
            registry_source,
        })
    }

    /// Save state to disk
    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_path, content)?;
        Ok(())
    }

    /// Download and Install a package.
    pub fn install_package<F>(
        &mut self,
        name: &str,
        version: &str,
        target: &TargetDefinition,
        mut on_event: F,
    ) -> Result<InstallResult>
    where
        F: FnMut(InstallEvent),
    {
        // 1. Download
        let content = util::download_url(&self.client, &target.url, &mut on_event)?;

        // 2. Verify Checksum
        on_event(InstallEvent::VerifyingChecksum);
        util::verify_checksum(&content, &target.sha256)?;

        // 3. Extract
        on_event(InstallEvent::Extracting);
        let tar = GzDecoder::new(&content[..]);
        let mut archive = Archive::new(tar);
        let mut found = false;
        let mut final_path = PathBuf::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            // Modify try_extract_binary to return the path if successful
            if let Some(dest) = self.try_extract_binary(&mut entry, &target.bin)? {
                final_path = dest;
                found = true;
                break;
            }
        }

        if !found {
            anyhow::bail!("Binary '{}' not found in archive", target.bin);
        }

        // 4. Update State
        self.state.packages.insert(
            name.to_string(),
            InstalledPackage {
                version: version.to_string(),
                binaries: vec![target.bin.clone()],
            },
        );
        self.save()?;

        on_event(InstallEvent::Success);

        Ok(InstallResult {
            package_name: name.to_string(),
            version: version.to_string(),
            path: final_path,
        })
    }

    // Helper: Returns Some(path) if successful, None if skipped
    fn try_extract_binary<R: std::io::Read>(
        &self,
        entry: &mut tar::Entry<R>,
        target_bin_name: &str,
    ) -> Result<Option<PathBuf>> {
        let path = entry.path()?;

        // Guard Clause 1: Check if filename exists
        let fname = match path.file_name() {
            Some(f) => f,
            None => return Ok(None),
        };

        // Guard Clause 2: Check if filename matches target
        if fname != std::ffi::OsStr::new(target_bin_name) {
            return Ok(None);
        }

        // --- ATOMIC INSTALL LOGIC ---
        let dest = self.bin_path.join(target_bin_name);

        let mut temp_file = tempfile::Builder::new()
            .prefix(".rush-tmp-")
            .tempfile_in(&self.bin_path)?;

        std::io::copy(entry, &mut temp_file)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = temp_file.as_file().metadata()?.permissions();
            p.set_mode(0o755);
            temp_file.as_file().set_permissions(p)?;
        }

        temp_file.persist(&dest)?;

        Ok(Some(dest))
    }

    pub fn uninstall_package(&mut self, name: &str) -> Result<Option<UninstallResult>> {
        uninstall::uninstall_package(self, name)
    }

    /// Download the registry from the internet OR copy it from a local directory
    pub fn update_registry<F>(&self, mut on_event: F) -> Result<UpdateResult>
    where
        F: FnMut(UpdateEvent),
    {
        // Dependecy injection
        let source = &self.registry_source;
        on_event(UpdateEvent::Fetching {
            source: source.clone(),
        });

        // Wipe old registry for a clean update
        if self.registry_dir.exists() {
            fs::remove_dir_all(&self.registry_dir)?;
        }
        fs::create_dir_all(&self.registry_dir)?;

        // --- Guard Clause for Local Directory (Dev/Test Mode) ---
        if !source.starts_with("http") {
            let source_path = PathBuf::from(source);
            if !source_path.exists() {
                anyhow::bail!("Local registry path not found: {:?}", source_path);
            }

            let pkg_source = source_path.join("packages");
            if !pkg_source.exists() {
                // If 'packages' folder doesn't exist, there's nothing to copy.
                return Ok(UpdateResult {
                    source: source.clone(),
                });
            }

            let pkg_dest = self.registry_dir.join("packages");
            for entry in WalkDir::new(&pkg_source) {
                let entry = entry?;
                if let Ok(rel_path) = entry.path().strip_prefix(&pkg_source) {
                    let dest_path = pkg_dest.join(rel_path);
                    if entry.file_type().is_dir() {
                        fs::create_dir_all(&dest_path)?;
                    } else {
                        fs::copy(entry.path(), &dest_path)?;
                    }
                }
            }
            // Return after the local copy is finished.
            return Ok(UpdateResult {
                source: source.clone(),
            });
        }

        // --- Remote Tarball ---
        // We create a "Mapper Closure" here.
        // It takes the util function's "InstallEvent" and converts it to "UpdateEvent"
        // This allows us to reuse the download logic despite the mismatched types.
        let content = util::download_url(&self.client, source, &mut |event| {
            match event {
                // Map the progress
                crate::models::InstallEvent::Progress { bytes, total } => {
                    on_event(UpdateEvent::Progress { bytes, total });
                }
                // Ignore events that don't make sense for updating (like VerifyingChecksum)
                _ => {}
            }
        })?;

        on_event(UpdateEvent::Unpacking);

        let tar = GzDecoder::new(&content[..]);
        let mut archive = Archive::new(tar);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            if let Some(idx) = path.to_string_lossy().find("packages/") {
                let relative_path = &path.to_string_lossy()[idx..];
                let dest = self.registry_dir.join(relative_path);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                entry.unpack(dest)?;
            }
        }

        Ok(UpdateResult {
            source: source.clone(),
        })
    }

    /// Look up a specific package file (e.g. .../registry/packages/f/fzf.toml)
    pub fn find_package(&self, name: &str) -> Option<PackageManifest> {
        let prefix = name.chars().next()?;

        let path = self
            .registry_dir
            .join("packages")
            .join(prefix.to_string())
            .join(format!("{}.toml", name));

        // Read file -> Convert error to None -> Parse TOML -> Convert error to None
        fs::read_to_string(&path)
            .ok()
            .and_then(|content| toml::from_str(&content).ok())
    }

    /// Scan the folder structure to list all available packages
    pub fn list_available_packages(&self) -> Vec<(String, PackageManifest)> {
        let mut results = Vec::new();
        let packages_dir = self.registry_dir.join("packages");

        if !packages_dir.exists() {
            return results;
        }

        for entry in WalkDir::new(packages_dir)
            .min_depth(2)
            .max_depth(2)
            .into_iter()
            .flatten()
        {
            // Guard Clause 1: Must be a file
            if !entry.file_type().is_file() {
                continue;
            }

            // Guard Clause 2: Must have a valid filename
            let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) else {
                continue;
            };

            // Attempt to read and parse
            // We use unwrap_or_default/ok logic to skip bad files silently
            let content = fs::read_to_string(entry.path()).unwrap_or_default();
            if let Ok(manifest) = toml::from_str::<PackageManifest>(&content) {
                results.push((stem.to_string(), manifest));
            }
        }

        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }

    pub fn clean_trash(&self) -> Result<crate::models::CleanResult> {
        clean::clean_trash(self)
    }

    /// Developer Tool: Create/Update a local package manifest
    pub fn add_package_manual<F>(
        &self,
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
        let content = util::download_url(&self.client, &url, &mut on_event)?;

        on_event(InstallEvent::VerifyingChecksum);

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let sha256 = hex::encode(hasher.finalize());

        // Delegate to the logic we can test
        self.write_package_manifest(&name, &version, &target_arch, &url, bin_name, &sha256)
    }

    /// Internal helper: Updates the registry file. Separated for testing.
    /// This function uses self.registry_source to determine where to write.
    pub fn write_package_manifest(
        &self,
        name: &str,
        version: &str,
        target_arch: &str,
        url: &str,
        bin_name: Option<String>,
        sha256: &str,
    ) -> Result<()> {
        // 1. Dependecny injection
        let source_path = PathBuf::from(&self.registry_source);

        if self.registry_source.is_empty() || !source_path.exists() || !source_path.is_dir() {
            anyhow::bail!(
                "RUSH_REGISTRY_URL must be set to your local git repository path to use 'dev add'. Try 'export RUSH_REGISTRY_URL=\"$(pwd)\"'"
            );
        }

        // 2. Determine file path: e.g., packages/f/fzf.toml
        let prefix = name.chars().next().context("Package name empty")?;
        let package_dir = source_path.join("packages").join(prefix.to_string());
        let package_path = package_dir.join(format!("{}.toml", name));

        // 3. Load existing or create new manifest
        let mut manifest = if package_path.exists() {
            let content = std::fs::read_to_string(&package_path)?;
            toml::from_str::<PackageManifest>(&content).unwrap_or_else(|_| PackageManifest {
                version: version.to_string(),
                description: None,
                targets: std::collections::BTreeMap::new(),
            })
        } else {
            if !package_dir.exists() {
                std::fs::create_dir_all(&package_dir)?;
            }
            PackageManifest {
                version: version.to_string(),
                description: None,
                targets: std::collections::BTreeMap::new(),
            }
        };

        // 4. Update Struct
        manifest.version = version.to_string();
        manifest.targets.insert(
            target_arch.to_string(),
            TargetDefinition {
                url: url.to_string(),
                bin: bin_name.unwrap_or(name.to_string()),
                sha256: sha256.to_string(),
            },
        );

        // 5. Write back
        let toml_string = toml::to_string_pretty(&manifest)?;
        std::fs::write(&package_path, toml_string)?;

        Ok(())
    }

    /// Developer Tool: Interactive Import wizard from GitHub
    pub fn fetch_github_import_candidates(
        &self,
        repo: &str,
    ) -> Result<(String, String, Vec<ImportCandidate>)> {
        let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        let release: GitHubRelease = self
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
            // 1. Create a scored list of assets
            let mut scored_assets: Vec<ScoredAsset> = release
                .assets
                .iter()
                .map(|asset| ScoredAsset {
                    score: Self::calculate_asset_score(&asset.name, target_key),
                    asset: asset.clone(),
                })
                .collect();

            // 2. Sort by score descending (Best match first)
            scored_assets.sort_by(|a, b| b.score.cmp(&a.score));

            candidates.push(ImportCandidate {
                target_desc: desc.to_string(),
                target_slug: target_key.to_string(),
                assets: scored_assets,
            });
        }

        Ok((package_name, version, candidates))
    }

    /// Helper to rank assets based on how well they match the target architecture
    fn calculate_asset_score(name: &str, target_arch: &str) -> i32 {
        let name = name.to_lowercase();
        let mut score = 0;

        // --- GLOBAL PREFERENCES ---
        // We prefer tarballs because we have built-in extraction
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            score += 20;
        }
        // We dislike zips (for now) as we might not handle them perfectly on all OSes yet
        if name.ends_with(".zip") {
            score -= 10;
        }
        // We cannot handle system packages
        if name.ends_with(".deb") || name.ends_with(".rpm") || name.ends_with(".msi") {
            score -= 100;
        }
        // We don't want metadata files
        if name.contains("sha256") || name.contains("sum") || name.contains("sig") {
            score -= 100;
        }

        match target_arch {
            "x86_64-linux" => {
                // Good keywords
                if name.contains("linux") {
                    score += 10;
                }
                if name.contains("x86_64") || name.contains("amd64") {
                    score += 10;
                }
                if name.contains("musl") {
                    score += 5;
                } // Prefer static linking
                if name.contains("gnu") {
                    score += 3;
                }

                // Bad keywords (Wrong Arch/OS)
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
                // Good keywords
                if name.contains("apple") || name.contains("darwin") || name.contains("macos") {
                    score += 10;
                }
                if name.contains("aarch64") || name.contains("arm64") {
                    score += 10;
                }

                // Bad keywords
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
}

// --- TESTS ---
#[cfg(test)]
mod tests;
