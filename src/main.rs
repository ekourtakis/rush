use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::fs::{self, File};
use tar::Archive;

// CLI structure
#[derive(Parser)]
#[command(name = "rush")]
#[command(about = "A lightning-fast package manager", long_about = None)]
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
    /// List available packages
    List,
}

struct Package {
    url: &'static str,
    binary_name: &'static str,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Registry hardcoded for now
    let mut registry = HashMap::new();
    registry.insert("fzf", Package {
        url: "https://github.com/junegunn/fzf/releases/download/v0.56.3/fzf-0.56.3-linux_amd64.tar.gz",
        binary_name: "fzf",
    });
    registry.insert("ripgrep", Package {
        url: "https://github.com/BurntSushi/ripgrep/releases/download/14.1.0/ripgrep-14.1.0-x86_64-unknown-linux-musl.tar.gz",
        binary_name: "rg",
    });
    registry.insert("fd", Package {
        url: "https://github.com/sharkdp/fd/releases/download/v10.1.0/fd-v10.1.0-x86_64-unknown-linux-musl.tar.gz",
        binary_name: "fd",
    });

    match &cli.command {
        Commands::List => {
            println!("Available packages:");
            for (name, _) in registry {
                println!(" - {}", name);
            }
            Ok(())
        }
        Commands::Install { name } => {
            if let Some(pkg) = registry.get(name.as_str()) {
                install_package(pkg, name)
            } else {
                Err(anyhow::anyhow!("Package '{}' not found in registry.", name))
            }
        }
        Commands::Uninstall { name } => {
            if let Some(pkg) = registry.get(name.as_str()) {
                uninstall_package(pkg, name)
            } else {
                Err(anyhow::anyhow!(
                    "Package '{}' not found in registry. Cannot determine binary name to delete.",
                    name
                ))
            }
        }
    }
}

fn install_package(pkg: &Package, name: &str) -> Result<()> {
    println!("Rush: Installing {}...", name);

    let home_dir = dirs::home_dir().context("Could not find home directory")?;
    let install_dir = home_dir.join(".local/bin");
    if !install_dir.exists() {
        fs::create_dir_all(&install_dir)?;
    }

    println!("  Downloading...");
    let response = reqwest::blocking::get(pkg.url)?;
    let total_size = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
        .progress_chars("#>-"));

    let content = response.bytes()?;
    pb.finish_with_message("Downloaded");

    println!(" Extracting...");
    let tar = GzDecoder::new(&content[..]);
    let mut archive = Archive::new(tar);

    let mut found = false;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        // IMPROVED: Check if the filename matches (ignores folders in the tarball)
        if let Some(filename) = path.file_name() {
            if filename == pkg.binary_name {
                let target_path = install_dir.join(pkg.binary_name);

                let mut outfile = File::create(&target_path)?;
                std::io::copy(&mut entry, &mut outfile)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = fs::metadata(&target_path)?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&target_path, perms)?;
                }

                println!("✨ Installed to {:?}", target_path);
                found = true;
                break;
            }
        }
    }

    if !found {
        Err(anyhow::anyhow!(
            "Binary '{}' not found inside the archive",
            pkg.binary_name
        ))
    } else {
        Ok(())
    }
}

fn uninstall_package(pkg: &Package, name: &str) -> Result<()> {
    let home_dir = dirs::home_dir().context("Could not find home directory")?;
    let target_path = home_dir.join(".local/bin").join(pkg.binary_name);

    println!("Uninstalling {}...", name);

    if target_path.exists() {
        fs::remove_file(&target_path).context(format!("Failed to delete {:?}", target_path))?;

        println!("✅ Removed binary: {:?}", target_path);
        Ok(())
    } else {
        println!(
            "⚠️  Package '{}' was not installed (Binary {:?} not found).",
            name, target_path
        );
        Ok(())
    }
}
