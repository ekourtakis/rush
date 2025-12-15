use crate::models::{InstalledPackage, PackageManifest, State, TargetDefinition};
use anyhow::{Context, Result};
use colored::*;
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::fs::{self};
use std::io::Read;
use std::path::PathBuf;
use tar::Archive;
use walkdir::WalkDir;

/// Default URL to fetch the registry from, overridable by env variable
const DEFAULT_REGISTRY_URL: &str =
    "https://github.com/ekourtakis/rush/archive/refs/heads/refactor/registry.tar.gz";

/// The core engine that handles state and I/O
pub struct RushEngine {
    pub state: State,
    state_path: PathBuf,               // ~/.local/share/rush/installed.json
    registry_dir: PathBuf,             // ~/.local/share/rush/registry/
    bin_path: PathBuf,                 // ~/.local/bin
    client: reqwest::blocking::Client, // HTTP Client
}

impl RushEngine {
    /// Standard constructor
    /// Load the engine and state from disk
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("No home dir")?;
        Self::init(home)
    }

    /// Test constructor (dependency injection)
    pub fn with_root(root: PathBuf) -> Result<Self> {
        Self::init(root)
    }

    /// Shared initialization logic
    fn init(root: PathBuf) -> Result<Self> {
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

        // Initialize Client
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("rush/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            state,
            state_path,
            registry_dir,
            bin_path,
            client,
        })
    }

    /// Save state to disk
    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_path, content)?;
        Ok(())
    }

    /// Download and Install a package
    pub fn install_package(
        &mut self,
        name: &str,
        version: &str,
        target: &TargetDefinition,
    ) -> Result<()> {
        println!("{} {} (v{})...", "Installing".cyan(), name, version);

        let content = self.download_with_progress(&target.url)?;

        println!("{}", "Verifying checksum...".cyan());
        Self::verify_checksum(&content, &target.sha256)?;
        println!("{}", "Checksum Verified.".green());

        // Extract
        let tar = GzDecoder::new(&content[..]);
        let mut archive = Archive::new(tar);
        let mut found = false;

        for entry in archive.entries()? {
            let mut entry = entry?;

            // Returns true if it extracted the file
            if self.try_extract_binary(&mut entry, &target.bin)? {
                found = true;
                break; // Stop scanning the tarball once we find the binary
            }
        }

        if !found {
            println!("{}", "Error: Binary not found in archive".red());
            anyhow::bail!("Binary missing in archive");
        }

        // Update State
        self.state.packages.insert(
            name.to_string(),
            InstalledPackage {
                version: version.to_string(),
                binaries: vec![target.bin.clone()],
            },
        );
        self.save()?;

        Ok(())
    }

    /// Helper for `install_package()`: Checks if the current tar entry is the binary we want.
    /// If yes, performs the atomic install and returns `true`.
    /// If no, returns `false`.
    fn try_extract_binary<R: std::io::Read>(
        &self,
        entry: &mut tar::Entry<R>,
        target_bin_name: &str,
    ) -> Result<bool> {
        let path = entry.path()?;

        // Guard Clause 1: Check if filename exists
        let fname = match path.file_name() {
            Some(f) => f,
            None => return Ok(false),
        };

        // Guard Clause 2: Check if filename matches target
        if fname != std::ffi::OsStr::new(target_bin_name) {
            return Ok(false);
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
        println!("{} Installed to {:?}", "Success:".green(), dest);

        Ok(true)
    }

    /// Verify checksum of given content against expected hash
    fn verify_checksum(content: &[u8], expected_hash: &str) -> Result<()> {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash = hex::encode(hasher.finalize());

        if hash != expected_hash {
            println!("{} Hash mismatch!", "Error:".red());
            println!("  Expected: {}", expected_hash);
            println!("  Got:      {}", hash);
            anyhow::bail!("Security check failed: Checksum mismatch");
        }
        Ok(())
    }

    pub fn uninstall_package(&mut self, name: &str) -> Result<()> {
        if let Some(pkg) = self.state.packages.get(name) {
            println!("{} {}...", "Uninstalling".cyan(), name);

            for binary in &pkg.binaries {
                let p = self.bin_path.join(binary);
                if p.exists() {
                    fs::remove_file(&p)?;
                    println!("   - Deleted {:?}", p);
                }
            }

            self.state.packages.remove(name);
            self.save()?;
            println!("{}", "Success: Uninstalled".green());
        } else {
            println!("{} Package '{}' is not installed", "Error:".red(), name);
        }
        Ok(())
    }

    /// Download the registry from the internet OR copy it from a local directory
    pub fn update_registry(&self) -> Result<()> {
        let source =
            std::env::var("RUSH_REGISTRY_URL").unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string());

        println!("{} from {}...", "Fetching registry".cyan(), source);

        // Wipe old registry to ensure deleted packages are removed
        if self.registry_dir.exists() {
            fs::remove_dir_all(&self.registry_dir)?;
        }
        fs::create_dir_all(&self.registry_dir)?;

        if source.starts_with("http") {
            let content = self.download_with_progress(&source)?;

            let tar = GzDecoder::new(&content[..]);
            let mut archive = Archive::new(tar);

            // GitHub archives usually start with "rush-refactor-registry/packages/..."
            // We need to find the "packages/" folder and extract it to our registry root.
            for entry in archive.entries()? {
                let mut entry = entry?;
                let path = entry.path()?;
                let path_str = path.to_string_lossy();

                // Look for "packages/" inside the tarball path
                if let Some(idx) = path_str.find("packages/") {
                    // Extract relative path: packages/f/fzf.toml
                    let relative_path = &path_str[idx..];
                    let dest = self.registry_dir.join(relative_path);

                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    entry.unpack(dest)?;
                }
            }
        } else {
            // LOCAL DIRECTORY MODE
            let source_path = PathBuf::from(&source);
            if !source_path.exists() {
                anyhow::bail!("Local registry path not found: {:?}", source_path);
            }

            let pkg_source = source_path.join("packages");
            let pkg_dest = self.registry_dir.join("packages");

            if !pkg_source.exists() {
                println!(
                    "{} No 'packages' directory found in {:?}",
                    "Warning:".yellow(),
                    source_path
                );
                return Ok(());
            }

            println!("Copying local registry from {:?}...", pkg_source);

            for entry in WalkDir::new(&pkg_source) {
                let entry = entry?;
                // Calculate relative path to preserve structure
                if let Ok(rel_path) = entry.path().strip_prefix(&pkg_source) {
                    let dest_path = pkg_dest.join(rel_path);
                    if entry.file_type().is_dir() {
                        fs::create_dir_all(&dest_path)?;
                    } else {
                        fs::copy(entry.path(), &dest_path)?;
                    }
                }
            }
        }

        println!("{} Registry updated.", "Success:".green());
        Ok(())
    }

    /// Look up a specific package file (e.g. .../registry/packages/f/fzf.toml)
    pub fn find_package(&self, name: &str) -> Option<PackageManifest> {
        let prefix = name.chars().next()?;

        let path = self
            .registry_dir
            .join("packages")
            .join(prefix.to_string())
            .join(format!("{}.toml", name));

        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(manifest) => Some(manifest),
                    Err(e) => {
                        println!("{} Failed to parse {:?}: {}", "Error:".red(), path, e);
                        None
                    }
                },
                Err(e) => {
                    println!("{} Failed to read {:?}: {}", "Error:".red(), path, e);
                    None
                }
            }
        } else {
            println!("DEBUG: Package file not found at {:?}", path);
            None
        }
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

    /// Remove temporary files from failed installs
    pub fn clean_trash(&self) -> Result<()> {
        // Read the bin directory
        // We use read_dir which returns an iterator over entries
        let bin_dir = std::fs::read_dir(&self.bin_path)?;
        let mut count = 0;

        for entry in bin_dir {
            let entry = entry?;
            let path = entry.path();

            if let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|n| n.starts_with(".rush-tmp-"))
            {
                std::fs::remove_file(&path)?;
                println!("{} {:?}", "Deleted trash:".yellow(), name);
                count += 1;
            }
        }

        if count == 0 {
            println!("{}", "No trash found. System is clean.".green());
        } else {
            println!("{} {} temporary files.", "Cleaned".green(), count);
        }
        Ok(())
    }

    /// Developer Tool: Create/Update a local package manifest
    pub fn add_package_manual(
        &self,
        name: String,
        version: String,
        target_arch: String,
        url: String,
        bin_name: Option<String>,
    ) -> Result<()> {
        // 1. Identify the local registry source
        // We MUST be pointing to a local directory to write files.
        let source = std::env::var("RUSH_REGISTRY_URL").unwrap_or_default();

        let source_path = PathBuf::from(&source);
        if source.is_empty() || !source_path.exists() || !source_path.is_dir() {
            anyhow::bail!(
                "RUSH_REGISTRY_URL must be set to your local git repository path to use 'dev add'."
            );
        }

        // 2. Determine file path: root/packages/f/fzf.toml
        let prefix = name.chars().next().context("Package name empty")?;
        let package_dir = source_path.join("packages").join(prefix.to_string());
        let package_path = package_dir.join(format!("{}.toml", name));

        // 3. Load existing or create new manifest
        let mut manifest = if package_path.exists() {
            let content = std::fs::read_to_string(&package_path)?;
            toml::from_str::<PackageManifest>(&content).unwrap_or_else(|_| PackageManifest {
                version: version.clone(),
                description: None,
                targets: std::collections::BTreeMap::new(),
            })
        } else {
            // Ensure parent dir exists (packages/f/)
            if !package_dir.exists() {
                std::fs::create_dir_all(&package_dir)?;
            }
            PackageManifest {
                version: version.clone(),
                description: None,
                targets: std::collections::BTreeMap::new(),
            }
        };

        // 4. Download and Hash
        // We reuse the private helper we wrote in the last PR!
        println!("{} {}", "Fetching and hashing:".cyan(), url);
        let content = self.download_with_progress(&url)?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let sha256 = hex::encode(hasher.finalize());
        println!("{} {}", "Calculated Hash:".green(), sha256);

        // 5. Update Struct
        manifest.version = version; // Always update version
        manifest.targets.insert(
            target_arch.clone(),
            TargetDefinition {
                url,
                bin: bin_name.unwrap_or(name.clone()),
                sha256,
            },
        );

        // 6. Write back
        let toml_string = toml::to_string_pretty(&manifest)?;
        std::fs::write(&package_path, toml_string)?;

        println!("{} Written to {:?}", "Success:".green(), package_path);
        Ok(())
    }

    /// Helper: Download with progress bar, returns content as Vec<u8>
    fn download_with_progress(&self, url: &str) -> Result<Vec<u8>> {
        let mut response = self.client.get(url).send()?.error_for_status()?;
        let total_size = response.content_length().unwrap_or(0);

        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
                .progress_chars("#>-"),
        );

        let mut content = Vec::with_capacity(total_size as usize);
        let mut buffer = [0; 8192];

        loop {
            let bytes_read = response.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            content.extend_from_slice(&buffer[..bytes_read]);
            pb.inc(bytes_read as u64);
        }
        pb.finish_with_message("Download complete");

        Ok(content)
    }
}

// --- TESTS ---
#[cfg(test)]
mod tests; // core/tests.rs
