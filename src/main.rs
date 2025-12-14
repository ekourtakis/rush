use anyhow::Result;
use clap::Parser;
use colored::*;

use rush::cli::{Cli, Commands};
use rush::core::RushEngine;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize Engine
    let mut engine = RushEngine::new()?;

    // DETECT SYSTEM ARCHITECTURE
    // e.g., "x86_64-linux" or "aarch64-apple-darwin"
    let current_target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);

    match &cli.command {
        Commands::List => {
            println!("{}", "Installed Packages:".bold());
            if engine.state.packages.is_empty() {
                println!("   (No packages installed)");
            } else {
                for (name, pkg) in &engine.state.packages {
                    println!(" - {} (v{})", name.bold(), pkg.version);
                }
            }
        }

        Commands::Search => {
            println!("{} ({}):", "Available Packages".bold(), current_target);

            let packages = engine.list_available_packages();

            if packages.is_empty() {
                println!("   (Registry empty or not found. Run 'rush update')");
            }

            for (name, manifest) in packages {
                if manifest.targets.contains_key(&current_target) {
                    println!(" - {} (v{})", name.bold(), manifest.version);
                }
            }
        }

        Commands::Install { name } => {
            if engine.state.packages.contains_key(name) {
                println!("{} {} is already installed", "Warning:".yellow(), name);
                return Ok(());
            }

            if let Some(manifest) = engine.find_package(name) {
                if let Some(target) = manifest.targets.get(&current_target) {
                    engine.install_package(name, &manifest.version, target)?;
                } else {
                    println!(
                        "{} No compatible binary for {}",
                        "Error:".red(),
                        current_target
                    );
                }
            } else {
                println!(
                    "{} Package '{}' not found. Run 'rush update'?",
                    "Error:".red(),
                    name
                );
            }
        }

        Commands::Uninstall { name } => {
            engine.uninstall_package(name)?;
        }

        Commands::Upgrade => {
            println!("{}", "Checking for upgrades...".cyan());
            let installed_names: Vec<String> = engine.state.packages.keys().cloned().collect();
            let mut count = 0;

            for name in installed_names {
                let current_ver = engine.state.packages.get(&name).unwrap().version.clone();

                let Some(manifest) = engine.find_package(&name) else {
                    continue;
                };

                let Some(target) = manifest.targets.get(&current_target) else {
                    continue;
                };

                if manifest.version == current_ver {
                    continue;
                }

                println!(
                    "{} {} (v{} -> v{})...",
                    "Upgrading".cyan(),
                    name,
                    current_ver,
                    manifest.version
                );
                engine.install_package(&name, &manifest.version, target)?;
                count += 1;
            }

            if count == 0 {
                println!("{}", "All packages are up to date.".green());
            } else {
                println!("{} Upgraded {} packages", "Success:".green(), count);
            }
        }

        Commands::Update => {
            engine.update_registry()?;
        }

        Commands::Clean => engine.clean_trash()?,
    }

    Ok(())
}
