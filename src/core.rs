use crate::models::{InstalledPackage, Registry, State, TargetDefinition};
use anyhow::{Context, Result};
use colored::*;
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::PathBuf;
use tar::Archive;

/// Default URL to fetch the registry from, overridable by env variable
const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/ekourtakis/rush/main/registry.toml";

/// The core engine that handles state and I/O
pub struct RushEngine {
    pub state: State,
    state_path: PathBuf,
}

impl RushEngine {
    /// Load the engine and state from disk
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("No home dir")?;
        let state_dir = home.join(".local/share/rush");
        fs::create_dir_all(&state_dir)?;

        let state_path = state_dir.join("installed.json");
        let state = if state_path.exists() {
            let content = fs::read_to_string(&state_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            State::default()
        };

        Ok(Self { state, state_path })
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

        let home = dirs::home_dir().context("No home dir")?;
        let install_dir = home.join(".local/bin");
        fs::create_dir_all(&install_dir)?;

        // Create a proper Client with a User-Agent
        let client = reqwest::blocking::Client::builder()
            .user_agent("rush/1.0")
            .build()?;

        // Check for HTTP errors (404, 403)
        let response = client.get(&target.url).send()?.error_for_status()?;
        let len = response.content_length().unwrap_or(0);

        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40}] {bytes}/{total_bytes}")?,
        );

        // DOWNLOAD
        let content = response.bytes()?;
        pb.finish();

        // VERIFY CHECKSUM
        println!("{}", "Verifying checksum...".cyan());

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let hash = hex::encode(hasher.finalize());

        if hash != target.sha256 {
            println!("{} Hash mismatch!", "Error:".red());
            println!("  Expected: {}", target.sha256);
            println!("  Got:      {}", hash);
            anyhow::bail!("Security check failed: Checksum mismatch");
        }

        println!("{}", "Checksum Verified.".green());

        // EXTRACT
        let tar = GzDecoder::new(&content[..]);
        let mut archive = Archive::new(tar);
        let mut found = false;

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            if let Some(fname) = path.file_name() {
                if fname == std::ffi::OsStr::new(&target.bin) {
                    let dest = install_dir.join(&target.bin);
                    let mut out = File::create(&dest)?;
                    std::io::copy(&mut entry, &mut out)?;

                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut p = fs::metadata(&dest)?.permissions();
                        p.set_mode(0o755);
                        fs::set_permissions(&dest, p)?;
                    }

                    println!("{} Installed to {:?}", "Success:".green(), dest);
                    found = true;
                }
            }
        }

        if !found {
            println!("{}", "Error: Binary not found in archive".red());
            anyhow::bail!("Binary missing");
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

    pub fn uninstall_package(&mut self, name: &str) -> Result<()> {
        if let Some(pkg) = self.state.packages.get(name) {
            println!("{} {}...", "Uninstalling".cyan(), name);
            let home = dirs::home_dir().unwrap();
            let bin_dir = home.join(".local/bin");

            for binary in &pkg.binaries {
                let p = bin_dir.join(binary);
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
        // 1. Determine the source (Env var or Default)
        let registry_url =
            std::env::var("RUSH_REGISTRY_URL").unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string());

        println!("{} from {}...", "Fetching registry".cyan(), registry_url);

        let content = if registry_url.starts_with("http") {
            // Case A: It's a URL (Download it)
            let client = reqwest::blocking::Client::builder()
                .user_agent("rush/1.0")
                .build()?;

            let response = client.get(&registry_url).send()?.error_for_status()?;
            response.text()?
        } else {
            // Case B: It's a Local File (Read it)
            let path = PathBuf::from(&registry_url);
            if !path.exists() {
                anyhow::bail!("Local registry file not found: {:?}", path);
            }
            fs::read_to_string(&path)?
        };

        // 2. Save it to our cache (~/.local/share/rush/registry.toml)
        let registry_path = self.state_path.parent().unwrap().join("registry.toml");
        fs::write(&registry_path, content)?;

        println!(
            "{} Registry saved to {:?}",
            "Success:".green(),
            registry_path
        );
        Ok(())
    }

    /// Attempt to load the registry from disk.
    /// Returns an error if the file doesn't exist.
    pub fn load_registry(&self) -> Result<Registry> {
        let path = self.get_registry_path();

        if !path.exists() {
            anyhow::bail!("Registry not found");
        }

        let content = fs::read_to_string(&path)?;
        let registry: Registry =
            toml::from_str(&content).context("Failed to parse registry.toml")?;

        Ok(registry)
    }

    /// Helper to get the path where the registry is cached
    pub fn get_registry_path(&self) -> PathBuf {
        self.state_path.parent().unwrap().join("registry.toml")
    }
}
