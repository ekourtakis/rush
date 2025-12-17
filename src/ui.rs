use crate::models::{
    CleanResult, ImportCandidate, InstallEvent, InstalledPackage, PackageManifest, UninstallResult,
    UpdateEvent,
};
use anyhow::Result;
use colored::*;
use dialoguer::{Select, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;

// -- GENERAL UI FUNCTIONS --

/// Display an error message
pub fn print_error(msg: &str) {
    println!("{} {}", "Error:".red(), msg);
}

/// Display a warning message
pub fn print_warning(msg: &str) {
    println!("{} {}", "Warning:".yellow(), msg);
}

// -- INTERNAL HELPERS --

/// Creates a progress bar
fn make_progress_bar(total_bytes: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb
}

// -- LIST FUNCTIONS --

/// Display the list of installed packages given
pub fn print_installed_packages(packages: &HashMap<String, InstalledPackage>) {
    println!("{}", "Installed Packages:".bold());

    if packages.is_empty() {
        println!("   (No packages installed)");
    } else {
        // We might want to sort them for consistent display
        let mut sorted_keys: Vec<_> = packages.keys().collect();
        sorted_keys.sort();

        for name in sorted_keys {
            let pkg = &packages[name];
            println!(" - {} (v{})", name.bold(), pkg.version);
        }
    }
}

// -- SEARCH FUNCTIONS --

/// Display the list of available packages given
pub fn print_available_packages(packages: &[(String, PackageManifest)], target: &str) {
    println!("{} ({}):", "Available Packages".bold(), target);

    if packages.is_empty() {
        println!("   (Registry empty or not found. Run 'rush update')");
        return;
    }

    for (name, manifest) in packages {
        // The View decides to only show packages compatible with the current system
        if manifest.targets.contains_key(target) {
            println!(" - {} (v{})", name.bold(), manifest.version);
        }
    }
}

// -- UNINSTALL FUNCTIONS --

/// Display the result of an uninstall operation
pub fn print_uninstall_result(result: &Option<UninstallResult>, requested_name: &str) {
    if let Some(res) = result {
        println!("{} {}...", "Uninstalling".cyan(), res.package_name);
        for binary in &res.binaries_removed {
            println!("   - Deleted {:?}", binary);
        }
        println!("{}", "Success: Uninstalled".green());
    } else {
        println!(
            "{} Package '{}' is not installed",
            "Error:".red(),
            requested_name
        );
    }
}

// -- CLEAN FUNCTIONS --

/// Display the result of a cleaning operation
pub fn print_clean_result(result: &CleanResult) {
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

// --- INSTALLATION FUNCTIONS ---

pub fn print_install_start(name: &str, version: &str) {
    println!("{} {} (v{})...", "Installing".cyan(), name, version);
}

pub fn print_install_success(path: &std::path::Path) {
    println!("{} Installed to {:?}", "Success:".green(), path);
}

/// Factory: Creates a closure that handles InstallEvents and updates the progress bar
pub fn create_install_handler() -> impl FnMut(InstallEvent) {
    let mut pb: Option<ProgressBar> = None;

    move |event: InstallEvent| match event {
        InstallEvent::Downloading { total_bytes } => {
            pb = Some(make_progress_bar(total_bytes));
        }
        InstallEvent::Progress { bytes, total: _ } => {
            if let Some(bar) = &pb {
                bar.inc(bytes);
            }
        }
        InstallEvent::VerifyingChecksum => {
            if let Some(bar) = &pb {
                bar.finish_and_clear();
            }
            println!("{}", "Verifying checksum...".cyan());
        }
        InstallEvent::Success => {
            println!("{}", "Checksum Verified.".green());
        }
        _ => {}
    }
}

// --- UPDATE FUNCTIONS ---

/// Display the successful result of an update operation
pub fn print_update_success(source: &str) {
    println!("{} Registry updated from {}.", "Success:".green(), source);
}

/// Factory: Creates a closure that handles UpdateEvents
pub fn create_update_handler() -> impl FnMut(UpdateEvent) {
    let mut pb: Option<ProgressBar> = None;

    move |event: UpdateEvent| match event {
        UpdateEvent::Fetching { source } => {
            println!("{} from {}...", "Fetching registry".cyan(), source);
        }
        UpdateEvent::Progress { bytes, total } => {
            let bar = pb.get_or_insert_with(|| make_progress_bar(total));
            bar.inc(bytes);
        }
        UpdateEvent::Unpacking => {
            if let Some(bar) = pb.take() {
                bar.finish_with_message("Download complete");
            }
        }
    }
}

// --- UPGRADE UI ---

pub fn print_upgrade_check() {
    println!("{}", "Checking for upgrades...".cyan());
}

pub fn print_upgrade_start(name: &str, old_v: &str, new_v: &str) {
    println!(
        "{} {} (v{} -> v{})...",
        "Upgrading".cyan(),
        name,
        old_v,
        new_v
    );
}

pub fn print_upgrade_summary(count: usize) {
    println!("{} {} packages upgraded.", "Success:".green(), count);
}

// --- DEV / WIZARD UI ---

pub fn print_fetching_msg(url: &str) {
    println!("{} {}", "Fetching and hashing:".cyan(), url);
}

pub fn print_dev_add_success(name: &str) {
    println!("{} Added {} to local registry.", "Success:".green(), name);
}

pub fn print_fetching_metadata(repo: &str) {
    println!("{} metadata for {}...", "Fetching".cyan(), repo);
}

pub fn print_found_release(version: &str) {
    println!("Found Release: {}", version.green());
}

pub fn print_wizard_complete() {
    println!("{}", "Import wizard complete.".green());
}

pub fn print_skipping_target(target: &str) {
    println!("Skipping {}", target);
}

/// Interactive Prompt: Asks the user to select an asset from a list.
/// Returns Ok(Some(index)) if an asset was selected.
/// Returns Ok(None) if the user chose to skip.
pub fn prompt_select_asset(candidate: &ImportCandidate) -> Result<Option<usize>> {
    // 1. Build the menu items
    let mut menu_items: Vec<String> = candidate
        .assets
        .iter()
        .map(|scored| {
            if scored.score > 0 {
                format!("{} (Recommended)", scored.asset.name)
            } else {
                scored.asset.name.clone()
            }
        })
        .collect();

    // 2. Add the "Skip" option
    menu_items.push("Skip this target".to_string());

    // 3. Render the menu
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Select asset for {}", candidate.target_desc.bold()))
        .default(0)
        .items(&menu_items)
        .interact()?;

    // 4. Return result
    if selection == menu_items.len() - 1 {
        Ok(None) // User selected "Skip"
    } else {
        Ok(Some(selection))
    }
}
