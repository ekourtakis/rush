use crate::models::{CleanResult, InstalledPackage, PackageManifest};
use colored::*;
use std::collections::HashMap;

/// Display the list of installed packages
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
