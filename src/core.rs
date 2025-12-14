use crate::models::{InstalledPackage, Registry, State, TargetDefinition};
use anyhow::{Context, Result};
use colored::*;
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::fs::{self};
use std::path::PathBuf;
use tar::Archive;

/// Default URL to fetch the registry from, overridable by env variable
const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/ekourtakis/rush/main/registry.toml";

/// The core engine that handles state and I/O
pub struct RushEngine {
    pub state: State,
    state_path: PathBuf,               // ~/.local/share/rush/installed.json
    registry_path: PathBuf,            // ~/.local/share/rush/registry.toml
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
        let registry_path = state_dir.join("registry.toml");

        fs::create_dir_all(&state_dir)?;
        fs::create_dir_all(&bin_path)?;

        let state = if state_path.exists() {
            let content = fs::read_to_string(&state_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            State::default()
        };

        // Initialize Client ONCE
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("rush/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            state,
            state_path,
            registry_path,
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

        // 1. Download & Verify
        let response = self.client.get(&target.url).send()?.error_for_status()?;
        let len = response.content_length().unwrap_or(0);

        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40}] {bytes}/{total_bytes}")?,
        );

        // DOWNLOAD
        let content = response.bytes()?;
        pb.finish();

        println!("{}", "Verifying checksum...".cyan());
        Self::verify_checksum(&content, &target.sha256)?;
        println!("{}", "Checksum Verified.".green());

        // 2. Extract
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

        // 3. Update State
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

    /// Download the registry from the internet OR copy it from a local file
    pub fn update_registry(&self) -> Result<()> {
        let registry_url =
            std::env::var("RUSH_REGISTRY_URL").unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string());

        println!("{} from {}...", "Fetching registry".cyan(), registry_url);

        let content = if registry_url.starts_with("http") {
            // Case A: It's a URL (Download it)
            let response = self.client.get(&registry_url).send()?.error_for_status()?;
            response.text()?
        } else {
            // Case B: It's a Local File (Read it)
            let path = PathBuf::from(&registry_url);
            if !path.exists() {
                anyhow::bail!("Local registry file not found: {:?}", path);
            }
            fs::read_to_string(&path)?
        };

        fs::write(&self.registry_path, content)?;
        println!(
            "{} Registry saved to {:?}",
            "Success:".green(),
            self.registry_path
        );
        Ok(())
    }

    pub fn load_registry(&self) -> Result<Registry> {
        if !self.registry_path.exists() {
            anyhow::bail!("Registry not found");
        }

        let content = fs::read_to_string(&self.registry_path)?;
        let registry: Registry =
            toml::from_str(&content).context("Failed to parse registry.toml")?;

        Ok(registry)
    }

    pub fn get_registry_path(&self) -> PathBuf {
        self.registry_path.clone()
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
}

// --- TESTS ---
#[cfg(test)]
mod tests; // core/tests.rs
