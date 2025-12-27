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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_find_package_success() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let engine = RushEngine::with_root(root.clone()).unwrap();

        // Setup: Create a fake package 'test-pkg' inside registry/packages/t/
        let prefix_dir = engine.registry_dir.join("packages").join("t");
        fs::create_dir_all(&prefix_dir).unwrap();

        let toml_path = prefix_dir.join("test-pkg.toml");
        fs::write(
            &toml_path,
            r#"
            version = "1.2.3"
            description = "A test package"
            [targets.x86_64-linux]
            url = "http://example.com"
            bin = "test"
            sha256 = "abc"
            "#,
        )
        .unwrap();

        // Act
        let manifest = find_package(&engine, "test-pkg");

        // Assert
        assert!(manifest.is_some());
        let m = manifest.unwrap();
        assert_eq!(m.version, "1.2.3");
        assert_eq!(m.description, Some("A test package".to_string()));
    }

    #[test]
    fn test_find_package_missing() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let engine = RushEngine::with_root(root).unwrap();

        let manifest = find_package(&engine, "ghost-pkg");
        assert!(manifest.is_none());
    }

    #[test]
    fn test_list_available_packages() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let engine = RushEngine::with_root(root).unwrap();

        // Setup: Create packages 'a-pkg' and 'b-pkg'
        let packages_root = engine.registry_dir.join("packages");
        
        // A
        let dir_a = packages_root.join("a");
        fs::create_dir_all(&dir_a).unwrap();
        fs::write(
            dir_a.join("a-pkg.toml"),
            r#"version="1.0"
               [targets.x]
               url=""
               bin=""
               sha256="""#,
        )
        .unwrap();

        // B
        let dir_b = packages_root.join("b");
        fs::create_dir_all(&dir_b).unwrap();
        fs::write(
            dir_b.join("b-pkg.toml"),
            r#"version="2.0"
               [targets.x]
               url=""
               bin=""
               sha256="""#,
        )
        .unwrap();

        // Act
        let list = list_available_packages(&engine);

        // Assert
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].0, "a-pkg"); // Should be sorted alphabetically
        assert_eq!(list[0].1.version, "1.0");
        assert_eq!(list[1].0, "b-pkg");
    }

    #[test]
    fn test_find_package_empty_string() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let engine = RushEngine::with_root(root).unwrap();

        // Should return None safely, not panic on .chars().next()
        assert!(find_package(&engine, "").is_none());
    }

    #[test]
    fn test_find_package_corrupted_file() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let engine = RushEngine::with_root(root.clone()).unwrap();

        let prefix_dir = engine.registry_dir.join("packages").join("c");
        fs::create_dir_all(&prefix_dir).unwrap();
        
        // Write garbage data
        fs::write(prefix_dir.join("corrupt.toml"), "This is not TOML").unwrap();

        // Should treat it as missing/invalid rather than crashing
        assert!(find_package(&engine, "corrupt").is_none());
    }

    #[test]
    fn test_list_skips_bad_files() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let engine = RushEngine::with_root(root).unwrap();

        let packages_root = engine.registry_dir.join("packages");
        
        // Good Package
        let dir_g = packages_root.join("g");
        fs::create_dir_all(&dir_g).unwrap();
        fs::write(
            dir_g.join("good.toml"), 
            r#"version="1.0"
               [targets.x]
               url=""
               bin=""
               sha256="""#
        ).unwrap();

        // Bad Package
        let dir_b = packages_root.join("b");
        fs::create_dir_all(&dir_b).unwrap();
        fs::write(dir_b.join("bad.toml"), "INVALID DATA").unwrap();

        let list = list_available_packages(&engine);

        // Should only have the good one
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "good");
    }

    #[test]
    fn test_list_empty_registry_dir() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let engine = RushEngine::with_root(root).unwrap();

        // Do not create the 'packages' directory here
        // simulating a fresh install before 'rush update'

        let list = list_available_packages(&engine);
        assert!(list.is_empty());
    }
}
