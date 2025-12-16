use super::RushEngine;
use crate::models::UninstallResult;
use anyhow::Result;
use std::fs;

pub fn uninstall_package(engine: &mut RushEngine, name: &str) -> Result<Option<UninstallResult>> {
    // Check if installed
    let Some(pkg) = engine.state.packages.get(name) else {
        return Ok(None); // Not installed
    };

    let mut removed_bins = Vec::new();

    // Delete binaries
    for binary in &pkg.binaries {
        let p = engine.bin_path.join(binary);
        if p.exists() {
            fs::remove_file(&p)?;
            removed_bins.push(binary.clone());
        }
    }

    // Update state
    engine.state.packages.remove(name);
    engine.save()?;

    Ok(Some(UninstallResult {
        package_name: name.to_string(),
        binaries_removed: removed_bins,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::InstalledPackage;
    use tempfile::tempdir;

    #[test]
    fn test_uninstall_deletes_files() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let bin_path = root.join(".local/bin");

        // 1. Setup: Create a fake installed package and a fake binary file
        let mut engine = RushEngine::with_root(root.clone()).unwrap();

        // Create the dummy binary file
        fs::create_dir_all(&bin_path).unwrap();
        let dummy_bin = bin_path.join("dummy-tool");
        fs::write(&dummy_bin, "binary content").unwrap();

        // Register it in state
        engine.state.packages.insert(
            "dummy-tool".to_string(),
            InstalledPackage {
                version: "1.0.0".to_string(),
                binaries: vec!["dummy-tool".to_string()],
            },
        );
        engine.save().unwrap();

        // 2. Action: Uninstall
        engine.uninstall_package("dummy-tool").unwrap();

        // 3. Assert: File should be gone
        assert!(!dummy_bin.exists(), "Binary file was not deleted!");

        // 4. Assert: State should be clean
        let reloaded_engine = RushEngine::with_root(root.clone()).unwrap();
        assert!(!reloaded_engine.state.packages.contains_key("dummy-tool"));
    }
}
