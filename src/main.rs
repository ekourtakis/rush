use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::PathBuf;
use tar::Archive;

// CLI structure
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
}

struct PackageRecipe {
    url: &'static str,
    binary_name: &'static str,
    version: &'static str,
}

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

    // Registry hardcoded for now
    let mut registry = HashMap::new();
    registry.insert("fzf", PackageRecipe {
        url: "https://github.com/junegunn/fzf/releases/download/v0.56.3/fzf-0.56.3-linux_amd64.tar.gz",
        binary_name: "fzf",
        version: "0.56.3",
    });
    registry.insert("ripgrep", PackageRecipe {
        url: "https://github.com/BurntSushi/ripgrep/releases/download/14.1.0/ripgrep-14.1.0-x86_64-unknown-linux-musl.tar.gz",
        binary_name: "rg",
        version: "14.1.0",
    });

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
            println!("🔍 Available Packages in Registry:");
            // We iterate over the REGISTRY, not the state
            let mut packages: Vec<_> = registry.keys().collect();
            packages.sort(); // Sort them alphabetically

            for name in packages {
                let recipe = registry.get(name).unwrap();
                println!(" - {} (v{})", name, recipe.version);
            }
        }

        Commands::Install { name } => {
            if state.data.packages.contains_key(name) {
                println!("⚠️  {} is already installed!", name);
                return Ok(());
            }

            if let Some(recipe) = registry.get(name.as_str()) {
                install_package(recipe, name)?;
                // SAVE STATE
                state.data.packages.insert(
                    name.clone(),
                    InstalledPackage {
                        version: recipe.version.to_string(),
                        binaries: vec![recipe.binary_name.to_string()],
                    },
                );
                state.save()?;
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

// --- STATE MANAGEMENT ---
struct StateManager {
    data: State,
    path: PathBuf,
}

impl StateManager {
    fn load() -> Result<Self> {
        let home = dirs::home_dir().context("No home dir")?;
        let state_dir = home.join(".local/share/rush");
        let state_file = state_dir.join("installed.json");

        if !state_dir.exists() {
            fs::create_dir_all(&state_dir)?;
        }

        let data = if state_file.exists() {
            let content = fs::read_to_string(&state_file)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            State::default()
        };

        Ok(Self {
            data,
            path: state_file,
        })
    }

    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}

// --- INSTALL/UNINSTALL LOGIC ---

fn install_package(recipe: &PackageRecipe, name: &str) -> Result<()> {
    println!("Installing {} (v{})...", name, recipe.version);

    let home_dir = dirs::home_dir().unwrap();
    let install_dir = home_dir.join(".local/bin");
    fs::create_dir_all(&install_dir)?;

    // Download
    let response = reqwest::blocking::get(recipe.url)?;
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
            if fname == recipe.binary_name {
                let target = install_dir.join(recipe.binary_name);
                let mut out = File::create(&target)?;
                std::io::copy(&mut entry, &mut out)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut p = fs::metadata(&target)?.permissions();
                    p.set_mode(0o755);
                    fs::set_permissions(&target, p)?;
                }
                println!("Installed to {:?}", target);
                return Ok(());
            }
        }
    }
    Err(anyhow::anyhow!("Binary not found"))
}

fn uninstall_package(pkg: &InstalledPackage, name: &str) -> Result<()> {
    let home_dir = dirs::home_dir().unwrap();
    let bin_dir = home_dir.join(".local/bin");

    println!("Uninstalling {}...", name);

    for binary in &pkg.binaries {
        let target = bin_dir.join(binary);
        if target.exists() {
            fs::remove_file(&target)?;
            println!("   - Deleted {:?}", target);
        }
    }
    println!("Uninstalled complete.");
    Ok(())
}
