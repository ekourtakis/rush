use crate::models::{InstalledPackage, State, TargetDefinition};
use anyhow::{Context, Result};
use colored::*;
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{self, File};
use std::path::PathBuf;
use tar::Archive;

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

        // Check for HTTP errors (404, 403) ---
        let response = client.get(&target.url).send()?.error_for_status()?;

        let len = response.content_length().unwrap_or(0);

        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40}] {bytes}/{total_bytes}")?,
        );

        // Stream the bytes
        let content = response.bytes()?;
        pb.finish();

        // Extract
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

                    // Success Message
                    println!("{} Installed to {:?}", "Success:".green(), dest);
                    found = true;
                }
            }
        }

        if !found {
            // Error Message
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
}
