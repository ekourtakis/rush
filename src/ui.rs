use crate::models::InstalledPackage;
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
