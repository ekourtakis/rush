//! # The "Orchestrator"
//!
//! This file acts as the bridge between the CLI arguments, the Core logic, and the UI.
//!
//! - **Core (`rush::core`):** Handles state, file I/O, network requests. Returns raw data.
//! - **UI (`rush::ui`):** Handles formatting, colors, progress bars, and user interaction.
//! - **Main:** connects the two. It fetches data from Core and passes it to UI.

use anyhow::Result;
use clap::Parser;
use colored::*;
use dialoguer::{Select, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};

use rush::cli::{Cli, Commands, DevCommands};
use rush::core::RushEngine;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize Engine
    let mut engine = RushEngine::new()?;

    // DETECT SYSTEM ARCHITECTURE
    let current_target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);

    match &cli.command {
        Commands::List => {
            rush::ui::print_installed_packages(&engine.state.packages);
        }

        Commands::Search => {
            let packages = engine.list_available_packages();
            rush::ui::print_available_packages(&packages, &current_target);
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
                    let event_handler = create_install_progress_handler(&mut pb);

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
                let event_handler = create_install_progress_handler(&mut pb);

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
            rush::ui::print_clean_result(&result);
        }

        Commands::Dev { command } => match command {
            DevCommands::Add {
                name,
                version,
                target,
                url,
                bin,
            } => {
                println!("{} {}", "Fetching and hashing:".cyan(), url);

                let mut pb: Option<ProgressBar> = None;
                let event_handler = create_install_progress_handler(&mut pb);

                engine.add_package_manual(
                    name.clone(),
                    version.clone(),
                    target.clone(),
                    url.clone(),
                    bin.clone(),
                    event_handler,
                )?;

                println!("{} Added {} to local registry.", "Success:".green(), name);
            }
            DevCommands::Import { repo } => {
                println!("{} metadata for {}...", "Fetching".cyan(), repo);

                // 1. Get Candidates from Core
                let (pkg_name, version, candidates) =
                    engine.fetch_github_import_candidates(repo)?;

                println!("Found Release: {}", version.green());

                // 2. Interactive Wizard
                for candidate in candidates {
                    // Create display strings
                    let mut menu_items: Vec<String> = candidate
                        .assets
                        .iter()
                        .map(|scored| {
                            // Visual hint for high scoring matches
                            if scored.score > 0 {
                                format!("{} (Recommended)", scored.asset.name)
                            } else {
                                scored.asset.name.clone()
                            }
                        })
                        .collect();

                    menu_items.push("Skip this target".to_string());

                    let selection = Select::with_theme(&ColorfulTheme::default())
                        .with_prompt(format!("Select asset for {}", candidate.target_desc.bold()))
                        .default(0)
                        .items(&menu_items)
                        .interact()?;

                    if selection == menu_items.len() - 1 {
                        println!("Skipping {}", candidate.target_slug);
                        continue;
                    }

                    let asset = &candidate.assets[selection].asset;
                    let url = asset.browser_download_url.clone();

                    println!("{} {}", "Fetching and hashing:".cyan(), url);

                    let mut pb: Option<ProgressBar> = None;
                    let event_handler = create_install_progress_handler(&mut pb);

                    engine.add_package_manual(
                        pkg_name.clone(),
                        version.clone(),
                        candidate.target_slug,
                        url,
                        None,
                        event_handler,
                    )?;
                }
                println!("{}", "Import wizard complete.".green());
            }
        },
    }

    Ok(())
}

/// Helper to create a closure for install progress events
fn create_install_progress_handler<'a>(
    pb: &'a mut Option<ProgressBar>,
) -> impl FnMut(rush::models::InstallEvent) + 'a {
    move |event: rush::models::InstallEvent| match event {
        rush::models::InstallEvent::Downloading { total_bytes } => {
            let b = ProgressBar::new(total_bytes);
            b.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            *pb = Some(b);
        }
        rush::models::InstallEvent::Progress { bytes, total: _ } => {
            if let Some(bar) = pb {
                bar.inc(bytes);
            }
        }
        rush::models::InstallEvent::VerifyingChecksum => {
            if let Some(bar) = pb {
                bar.finish_and_clear();
            }
            println!("{}", "Verifying checksum...".cyan());
        }
        rush::models::InstallEvent::Success => {
            println!("{}", "Checksum Verified.".green());
        }
        _ => {}
    }
}
