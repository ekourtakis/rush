use anyhow::Result;
use clap::Parser;
use colored::*;

use rush::cli::{Cli, Commands};
use rush::core::RushEngine;
use rush::models::Registry;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize Engine
    let mut engine = RushEngine::new()?;

    // Load Registry
    let registry = match engine.load_registry() {
        Ok(reg) => reg,
        Err(_) => {
            // If the file is missing, handle it gracefully
            if matches!(cli.command, Commands::Update | Commands::Clean) {
                // If the user is running 'update', we don't need the old registry.
                // Return an empty one to satisfy the compiler.
                Registry {
                    packages: std::collections::HashMap::new(),
                }
            } else {
                println!(
                    "{} Registry not found. Please run 'rush update' first.",
                    "Warning:".yellow()
                );
                return Ok(());
            }
        }
    };

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
            let mut keys: Vec<_> = registry.packages.keys().collect();
            keys.sort();

            for name in keys {
                let pkg = registry.packages.get(name).unwrap();
                // Only show packages compatible with THIS computer
                if pkg.targets.contains_key(&current_target) {
                    println!(" - {} (v{})", name.bold(), pkg.version);
                }
            }
        }

        Commands::Install { name } => {
            if engine.state.packages.contains_key(name) {
                println!("{} {} is already installed", "Warning:".yellow(), name);
                return Ok(());
            }

            // Find package in registry
            if let Some(pkg_def) = registry.packages.get(name) {
                // Find compatible target for THIS computer
                if let Some(target) = pkg_def.targets.get(&current_target) {
                    engine.install_package(name, &pkg_def.version, target)?;
                } else {
                    println!(
                        "{} No compatible binary for {}",
                        "Error:".red(),
                        current_target
                    );
                }
            } else {
                println!("{} Package '{}' not found", "Error:".red(), name);
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

                // 1. Does package exist in registry?
                let Some(reg_pkg) = registry.packages.get(&name) else {
                    continue;
                };

                // 2. Does it support this OS?
                let Some(target) = reg_pkg.targets.get(&current_target) else {
                    continue;
                };

                // 3. Is the version new?
                if reg_pkg.version == current_ver {
                    continue;
                }

                // --- ACTION ---
                println!(
                    "{} {} (v{} -> v{})...",
                    "Upgrading".cyan(),
                    name,
                    current_ver,
                    reg_pkg.version
                );
                engine.install_package(&name, &reg_pkg.version, target)?;
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
