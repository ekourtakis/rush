use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::PathBuf;
use tar::Archive;

// --- CLI ---
#[derive(Parser)]
#[command(name = "rush")]
#[command(about = "A lightning-fast toy package manager.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install a package
    Install { name: String },
    /// Uninstall a package
    Uninstall { name: String },
    /// List installed packages
    List,
    /// Search for available packages
    Search,
    /// Update the registry (for now, just re-reads the local file)
    Update,
}

// --- DATA STRUCTURES ---

// The Registry (TOML format)
#[derive(Deserialize, Debug)]
struct Registry {
    packages: HashMap<String, PackageDefinition>,
}

#[derive(Deserialize, Debug)]
struct PackageDefinition {
    version: String,
    // description: String
    targets: HashMap<String, TargetDefinition>,
}

#[derive(Deserialize, Debug, Clone)]
struct TargetDefinition {
    url: String,
    bin: String,
}

// The Local State
#[derive(Serialize, Deserialize, Debug, Clone)]
struct InstalledPackage {
    version: String,
    binaries: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct State {
    packages: HashMap<String, InstalledPackage>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut state = StateManager::load()?;

    // LOAD REGISTRY FROM TOML FILE
    let registry_content = fs::read_to_string("registry.toml")
        .context("Could not read registry.toml. Make sure it exists in the current folder.")?;
    let registry: Registry = toml::from_str(&registry_content)?;

    // DETECT SYSTEM ARCHITECTURE
    // e.g., "x86_64-linux" or "aarch64-apple-darwin"
    let current_target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);

    match &cli.command {
        Commands::List => {
            println!("Installed Packages:");
            if state.data.packages.is_empty() {
                println!("   (No packages installed)");
            } else {
                for (name, pkg) in &state.data.packages {
                    println!(" - {} (v{})", name, pkg.version);
                }
            }
        }
        Commands::Search => {
            println!("Available Packages (for {}):", current_target);
            let mut keys: Vec<_> = registry.packages.keys().collect();
            keys.sort();
            for name in keys {
                let pkg = registry.packages.get(name).unwrap();
                // Only show packages compatible with THIS computer
                if pkg.targets.contains_key(&current_target) {
                    println!(" - {} (v{})", name, pkg.version);
                }
            }
        }
        Commands::Update => {
            println!("Registry reloaded from registry.toml");
        }
        Commands::Install { name } => {
            if state.data.packages.contains_key(name) {
                println!("⚠️  {} is already installed!", name);
                return Ok(());
            }

            // Find package in registry
            if let Some(pkg_def) = registry.packages.get(name) {
                // Find compatible target for THIS computer
                if let Some(target) = pkg_def.targets.get(&current_target) {
                    install_package(target, name, &pkg_def.version)?;

                    // Save to state
                    state.data.packages.insert(
                        name.clone(),
                        InstalledPackage {
                            version: pkg_def.version.clone(),
                            binaries: vec![target.bin.clone()],
                        },
                    );
                    state.save()?;
                } else {
                    println!(
                        "❌ Package '{}' exists, but no binary found for your system ({})",
                        name, current_target
                    );
                }
            } else {
                println!("❌ Package '{}' not found in registry.", name);
            }
        }

        Commands::Uninstall { name } => {
            // LOOK UP STATE FIRST
            if let Some(installed) = state.data.packages.get(name) {
                uninstall_package(installed, name)?;
                // REMOVE FROM STATE
                state.data.packages.remove(name);
                state.save()?;
            } else {
                println!("❌ Package '{}' is not installed.", name);
            }
        }
    }
    Ok(())
}

// --- HELPERS ---
struct StateManager {
    data: State,
    path: PathBuf,
}

impl StateManager {
    fn load() -> Result<Self> {
        let home = dirs::home_dir().context("No home dir")?;
        let state_dir = home.join(".local/share/rush");

        if !state_dir.exists() {
            fs::create_dir_all(&state_dir)?;
        }

        let state_file = state_dir.join("installed.json");

        let data = if state_file.exists() {
            let c = fs::read_to_string(&state_file)?;
            serde_json::from_str(&c).unwrap_or_default()
        } else {
            State::default()
        };

        Ok(Self {
            data,
            path: state_file,
        })
    }

    fn save(&self) -> Result<()> {
        let c = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.path, c)?;
        Ok(())
    }
}

fn install_package(target: &TargetDefinition, name: &str, version: &str) -> Result<()> {
    println!("Installing {} (v{})...", name, version);
    let home = dirs::home_dir().unwrap();
    let install_dir = home.join(".local/bin");
    fs::create_dir_all(&install_dir)?;

    let response = reqwest::blocking::get(&target.url)?;
    let total_size = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes}")?,
    );

    let content = response.bytes()?;
    pb.finish();

    // Extract
    let tar = GzDecoder::new(&content[..]);
    let mut archive = Archive::new(tar);

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
                println!("✅ Installed to {:?}", dest);
                return Ok(());
            }
        }
    }
    Err(anyhow::anyhow!(
        "Binary '{}' not found in archive",
        target.bin
    ))
}

fn uninstall_package(pkg: &InstalledPackage, name: &str) -> Result<()> {
    let home = dirs::home_dir().unwrap();
    let bin_dir = home.join(".local/bin");
    println!("Uninstalling {}...", name);

    for binary in &pkg.binaries {
        let p = bin_dir.join(binary);
        if p.exists() {
            fs::remove_file(&p)?;
            println!("   - Deleted {:?}", p);
        }
    }
    println!("✅ Uninstalled.");
    Ok(())
}
