use crate::core::RushEngine;
use crate::models::PackageManifest;
use std::fs;
use walkdir::WalkDir;

/// Look up a specific package file (e.g. .../registry/packages/f/fzf.toml)
pub fn find_package(engine: &RushEngine, name: &str) -> Option<PackageManifest> {
    let prefix = name.chars().next()?;

    let path = engine
        .registry_dir
        .join("packages")
        .join(prefix.to_string())
        .join(format!("{}.toml", name));

    // Read file -> Convert error to None -> Parse TOML -> Convert error to None
    fs::read_to_string(&path)
        .ok()
        .and_then(|content| toml::from_str(&content).ok())
}

/// Scan the folder structure to list all available packages
pub fn list_available_packages(engine: &RushEngine) -> Vec<(String, PackageManifest)> {
    let mut results = Vec::new();
    let packages_dir = engine.registry_dir.join("packages");

    if !packages_dir.exists() {
        return results;
    }

    for entry in WalkDir::new(packages_dir)
        .min_depth(2)
        .max_depth(2)
        .into_iter()
        .flatten()
    {
        // Guard Clause 1: Must be a file
        if !entry.file_type().is_file() {
            continue;
        }

        // Guard Clause 2: Must have a valid filename
        let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        // Attempt to read and parse
        // We use unwrap_or_default/ok logic to skip bad files silently
        let content = fs::read_to_string(entry.path()).unwrap_or_default();
        if let Ok(manifest) = toml::from_str::<PackageManifest>(&content) {
            results.push((stem.to_string(), manifest));
        }
    }

    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}
