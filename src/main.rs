//! # The "Orchestrator"
//!
//! This file acts as the bridge between the CLI arguments, the Core logic, and the UI.
//!
//! - **Core (`rush::core`):** Handles state, file I/O, network requests. Returns raw data.
//! - **UI (`rush::ui`):** Handles formatting, colors, progress bars, and user interaction.
//! - **Main:** connects the two. It fetches data from Core and passes it to UI.

use anyhow::Result;
use clap::Parser;

use rush::cli::{Cli, Commands, DevCommands};
use rush::core::RushEngine;
use rush::ui;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize Engine
    let mut engine = RushEngine::new()?;

    // DETECT SYSTEM ARCHITECTURE
    let current_target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);

    match &cli.command {
        Commands::List => {
            ui::print_installed_packages(&engine.state.packages);
        }

        Commands::Search => {
            let packages = engine.list_available_packages();
            ui::print_available_packages(&packages, &current_target);
        }

        Commands::Install { name } => {
            if engine.state.packages.contains_key(name) {
                ui::print_warning(&format!("{} is already installed", name));
                return Ok(());
            }

            if let Some(manifest) = engine.find_package(name) {
                if let Some(target) = manifest.targets.get(&current_target) {
                    ui::print_install_start(name, &manifest.version);

                    let event_handler = ui::create_install_handler();

                    match engine.install_package(name, &manifest.version, target, event_handler) {
                        Ok(result) => ui::print_install_success(&result.path),
                        Err(e) => ui::print_error(&e.to_string()),
                    }
                } else {
                    ui::print_error(&format!("No compatible binary for {}", current_target));
                }
            } else {
                ui::print_error(&format!("Package '{}' not found.", name));
            }
        }

        Commands::Uninstall { name } => {
            let result = engine.uninstall_package(name)?;
            ui::print_uninstall_result(&result, name);
        }

        Commands::Upgrade => {
            ui::print_upgrade_check();

            let installed_names: Vec<String> = engine.state.packages.keys().cloned().collect();
            let mut count = 0;

            for name in installed_names {
                let current_ver = engine.state.packages.get(&name).unwrap().version.clone();

                // Logic to find update
                let Some(manifest) = engine.find_package(&name) else {
                    continue;
                };
                let Some(target) = manifest.targets.get(&current_target) else {
                    continue;
                };

                if manifest.version == current_ver {
                    continue;
                }

                ui::print_upgrade_start(&name, &current_ver, &manifest.version);

                let event_handler = ui::create_install_handler();

                engine.install_package(&name, &manifest.version, target, event_handler)?;
                count += 1;
            }
            ui::print_upgrade_summary(count);
        }

        Commands::Update => {
            let event_handler = ui::create_update_handler();
            let result = engine.update_registry(event_handler)?;
            ui::print_update_success(&result.source);
        }

        Commands::Clean => {
            let result = engine.clean_trash()?;
            ui::print_clean_result(&result);
        }

        Commands::Dev { command } => match command {
            DevCommands::Add {
                name,
                version,
                target,
                url,
                bin,
            } => {
                ui::print_fetching_msg(url);
                let event_handler = ui::create_install_handler();

                engine.add_package_manual(
                    name.clone(),
                    version.clone(),
                    target.clone(),
                    url.clone(),
                    bin.clone(),
                    event_handler,
                )?;
                ui::print_dev_add_success(name);
            }
            DevCommands::Import { repo } => {
                ui::print_fetching_metadata(repo);

                // 1. Get Candidates from Core
                let (pkg_name, version, candidates) =
                    engine.fetch_github_import_candidates(repo)?;
                ui::print_found_release(&version);

                // 2. Interactive Wizard
                for candidate in candidates {
                    // Ask UI to prompt the user
                    let selection_index = ui::prompt_select_asset(&candidate)?;

                    match selection_index {
                        Some(idx) => {
                            let asset = &candidate.assets[idx].asset;
                            let url = asset.browser_download_url.clone();

                            ui::print_fetching_msg(&url);
                            let event_handler = ui::create_install_handler();

                            engine.add_package_manual(
                                pkg_name.clone(),
                                version.clone(),
                                candidate.target_slug.clone(),
                                url,
                                None,
                                event_handler,
                            )?;
                        }
                        None => {
                            ui::print_skipping_target(&candidate.target_slug);
                        }
                    }
                }
                ui::print_wizard_complete();
            }
        },
    }

    Ok(())
}
