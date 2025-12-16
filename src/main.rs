use anyhow::Result;
use clap::Parser;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};

use rush::cli::{Cli, Commands, DevCommands};
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
                    println!(
                        "{} {} (v{})...",
                        "Installing".cyan(),
                        name,
                        manifest.version
                    );

                    // UI SETUP
                    let mut pb: Option<ProgressBar> = None;

                    let event_handler = |event: rush::models::InstallEvent| match event {
                        rush::models::InstallEvent::Downloading { total_bytes } => {
                            let b = ProgressBar::new(total_bytes);
                            b.set_style(
                                    ProgressStyle::default_bar()
                                        .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                                        .unwrap()
                                        .progress_chars("#>-"),
                                );
                            pb = Some(b);
                        }
                        rush::models::InstallEvent::Progress { bytes, total: _ } => {
                            if let Some(bar) = &pb {
                                bar.inc(bytes);
                            }
                        }
                        rush::models::InstallEvent::VerifyingChecksum => {
                            if let Some(bar) = &pb {
                                bar.finish_and_clear();
                            }
                            println!("{}", "Verifying checksum...".cyan());
                        }
                        rush::models::InstallEvent::Success => {
                            println!("{}", "Checksum Verified.".green());
                        }
                        _ => {}
                    };

                    // CALL ENGINE
                    match engine.install_package(name, &manifest.version, target, event_handler) {
                        Ok(result) => {
                            println!("{} Installed to {:?}", "Success:".green(), result.path);
                        }
                        Err(e) => {
                            println!("{} {}", "Error:".red(), e);
                        }
                    }
                } else {
                    println!(
                        "{} No compatible binary for {}",
                        "Error:".red(),
                        current_target
                    );
                }
            } else {
                println!("{} Package '{}' not found.", "Error:".red(), name);
            }
        }

        Commands::Uninstall { name } => {
            let result = engine.uninstall_package(name)?;

            if let Some(res) = result {
                println!("{} {}...", "Uninstalling".cyan(), res.package_name);
                for binary in res.binaries_removed {
                    println!("   - Deleted {:?}", binary);
                }
                println!("{}", "Success: Uninstalled".green());
            } else {
                println!("{} Package '{}' is not installed", "Error:".red(), name);
            }
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

                // --- Event Handler for Upgrade ---
                let mut pb: Option<ProgressBar> = None;
                let event_handler = |event: rush::models::InstallEvent| match event {
                    rush::models::InstallEvent::Downloading { total_bytes } => {
                        let b = ProgressBar::new(total_bytes);
                        b.set_style(
                                ProgressStyle::default_bar()
                                    .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                                    .unwrap()
                                    .progress_chars("#>-"),
                            );
                        pb = Some(b);
                    }
                    rush::models::InstallEvent::Progress { bytes, .. } => {
                        if let Some(bar) = &pb {
                            bar.inc(bytes);
                        }
                    }
                    rush::models::InstallEvent::VerifyingChecksum => {
                        if let Some(bar) = &pb {
                            bar.finish_and_clear();
                        }
                        println!("{}", "Verifying checksum...".cyan());
                    }
                    rush::models::InstallEvent::Success => {
                        println!("{}", "Checksum Verified.".green());
                    }
                    _ => {}
                };

                // Pass the handler
                engine.install_package(&name, &manifest.version, target, event_handler)?;
                count += 1;
            }

            println!("{} {} packages upgraded.", "Success:".green(), count);
        }

        Commands::Update => {
            // 1. Prepare UI elements (the progress bar)
            let mut pb: Option<ProgressBar> = None;

            // 2. Define the callback function
            let event_handler = |event: rush::models::UpdateEvent| {
                match event {
                    rush::models::UpdateEvent::Fetching { source } => {
                        println!("{} from {}...", "Fetching registry".cyan(), source);
                    }
                    rush::models::UpdateEvent::Progress { bytes, total } => {
                        // Create the progress bar on the first progress event
                        let bar = pb.get_or_insert_with(|| {
                            let b = ProgressBar::new(total);
                            b.set_style(
                                ProgressStyle::default_bar()
                                    .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                                    .unwrap()
                                    .progress_chars("#>-"),
                            );
                            b
                        });
                        bar.inc(bytes);
                    }
                    rush::models::UpdateEvent::Unpacking => {
                        if let Some(bar) = pb.take() {
                            bar.finish_with_message("Download complete");
                        }
                    }
                }
            };

            // 3. Call the silent core logic, passing the UI handler
            let result = engine.update_registry(event_handler)?;

            // 4. Print the final success message
            println!(
                "{} Registry updated from {}.",
                "Success:".green(),
                result.source
            );
        }

        Commands::Clean => {
            let result = engine.clean_trash()?;

            if result.files_cleaned.is_empty() {
                println!("{}", "No trash found. System is clean.".green());
            } else {
                for filename in &result.files_cleaned {
                    println!("{} {:?}", "Deleted trash:".yellow(), filename);
                }
                println!(
                    "{} {} temporary files.",
                    "Cleaned".green(),
                    result.files_cleaned.len()
                );
            }
        }

        Commands::Dev { command } => match command {
            DevCommands::Add {
                name,
                version,
                target,
                url,
                bin,
            } => {
                engine.add_package_manual(
                    name.clone(),
                    version.clone(),
                    target.clone(),
                    url.clone(),
                    bin.clone(),
                )?;
            }
            DevCommands::Import { repo } => {
                engine.import_github_package(repo)?;
            }
        },
    }

    Ok(())
}
