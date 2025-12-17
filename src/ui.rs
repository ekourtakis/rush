use crate::models::{
    CleanResult, InstallEvent, InstalledPackage, PackageManifest, UninstallResult, UpdateEvent,
};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;

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

// --- INSTALLATION UI ---

pub fn print_install_start(name: &str, version: &str) {
    println!("{} {} (v{})...", "Installing".cyan(), name, version);
}

pub fn print_install_success(path: &std::path::Path) {
    println!("{} Installed to {:?}", "Success:".green(), path);
}

pub fn print_error(msg: &str) {
    println!("{} {}", "Error:".red(), msg);
}

/// Factory: Creates a closure that handles InstallEvents and updates the progress bar
pub fn create_install_handler<'a>(
    pb: &'a mut Option<ProgressBar>,
) -> impl FnMut(InstallEvent) + 'a {
    move |event: InstallEvent| match event {
        InstallEvent::Downloading { total_bytes } => {
            let b = ProgressBar::new(total_bytes);
            b.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            *pb = Some(b);
        }
        InstallEvent::Progress { bytes, total: _ } => {
            if let Some(bar) = pb {
                bar.inc(bytes);
            }
        }
        InstallEvent::VerifyingChecksum => {
            if let Some(bar) = pb {
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

// --- UPDATE UI ---

pub fn print_update_success(source: &str) {
    println!("{} Registry updated from {}.", "Success:".green(), source);
}

/// Factory: Creates a closure that handles UpdateEvents
pub fn create_update_handler<'a>(pb: &'a mut Option<ProgressBar>) -> impl FnMut(UpdateEvent) + 'a {
    move |event: UpdateEvent| match event {
        UpdateEvent::Fetching { source } => {
            println!("{} from {}...", "Fetching registry".cyan(), source);
        }
        UpdateEvent::Progress { bytes, total } => {
            let bar = pb.get_or_insert_with(|| {
                let b = ProgressBar::new(total);
                b.set_style(
                    ProgressStyle::default_bar()
                        .template(
                            "{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                        )
                        .unwrap()
                        .progress_chars("#>-"),
                );
                b
            });
            bar.inc(bytes);
        }
        UpdateEvent::Unpacking => {
            if let Some(bar) = pb.take() {
                bar.finish_with_message("Download complete");
            }
        }
    }
}
