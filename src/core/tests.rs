use super::*;
use std::io::Cursor;
use tempfile::tempdir;

#[test]
fn test_engine_initialization() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let _engine = RushEngine::with_root(root.clone()).unwrap();
    assert!(root.join(".local/share/rush").exists());
}

// -- install_package() tests --

#[test]
fn test_verify_checksum_logic() {
    let data = b"hello world";
    let correct_hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
    let wrong_hash = "literally-anything-else";
    assert!(RushEngine::verify_checksum(data, correct_hash).is_ok());
    assert!(RushEngine::verify_checksum(data, wrong_hash).is_err());
}

#[test]
fn test_try_extract_binary_success() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let engine = RushEngine::with_root(root.clone()).unwrap();

    let mut header = tar::Header::new_gnu();
    header.set_size(12);
    header.set_path("test-bin").unwrap();
    header.set_cksum();

    let mut data = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut data);
        builder.append(&header, &b"fake content"[..]).unwrap();
        builder.finish().unwrap();
    }

    let cursor = Cursor::new(data);
    let mut archive = Archive::new(cursor);
    let mut entries = archive.entries().unwrap();
    let mut entry = entries.next().unwrap().unwrap();

    let result = engine.try_extract_binary(&mut entry, "test-bin").unwrap();

    assert!(result, "Should have extracted the binary");
    assert!(root.join(".local/bin/test-bin").exists());
}

#[test]
fn test_try_extract_binary_mismatch() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let engine = RushEngine::with_root(root.clone()).unwrap();

    // Create a tarball with a different filename
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_path("wrong-name").unwrap();
    header.set_cksum();

    let mut data = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut data);
        builder.append(&header, &b""[..]).unwrap();
        builder.finish().unwrap();
    }

    let cursor = Cursor::new(data);
    let mut archive = Archive::new(cursor);
    let mut entries = archive.entries().unwrap();
    let mut entry = entries.next().unwrap().unwrap();

    let result = engine.try_extract_binary(&mut entry, "test-bin").unwrap();

    assert!(!result, "Should not have extracted mismatched filename");
    assert!(!root.join(".local/bin/test-bin").exists());
}

#[test]
fn test_state_persistence() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();

    {
        let mut engine = RushEngine::with_root(root.clone()).unwrap();
        engine.state.packages.insert(
            "fake-pkg".to_string(),
            InstalledPackage {
                version: "1.0.0".to_string(),
                binaries: vec!["fake-bin".to_string()],
            },
        );
        engine.save().unwrap();
    }

    let engine = RushEngine::with_root(root.clone()).unwrap();
    assert!(engine.state.packages.contains_key("fake-pkg"));
}

// -- uninstall_package() tests --

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

// -- update_registry() tests --

#[test]
fn test_local_registry_update() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();

    // Create a dummy registry SOURCE structure
    // mimic packages/t/test-tool.toml
    let source_dir = temp_dir.path().join("source");
    let pkg_dir = source_dir.join("packages").join("t");
    std::fs::create_dir_all(&pkg_dir).unwrap();

    let dummy_toml = pkg_dir.join("test-tool.toml");
    std::fs::write(
        &dummy_toml,
        r#"
            version = "0.1.0"
            description = "Test package"
            [targets.x86_64-linux]
            url = "http://example.com"
            bin = "test"
            sha256 = "123"
        "#,
    )
    .unwrap();

    // Create dummy engine and update
    let engine =
        RushEngine::with_root_and_registry(root.clone(), source_dir.to_str().unwrap().to_string())
            .unwrap();

    // Pass an empty callback that ignores all events
    engine.update_registry(|_| {}).unwrap();

    let found = engine.find_package("test-tool");
    assert!(found.is_some());
}

#[test]
fn test_write_package_manifest() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();

    let engine =
        RushEngine::with_root_and_registry(root.clone(), root.to_str().unwrap().to_string())
            .unwrap();

    engine
        .write_package_manifest(
            "test-tool",
            "1.0.0",
            "x86_64-linux",
            "http://example.com",
            Some("binary-name".to_string()),
            "fake-hash-123",
        )
        .unwrap();

    let expected_path = root.join("packages").join("t").join("test-tool.toml");
    assert!(expected_path.exists());
}

#[test]
fn test_clean_trash() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let bin_path = root.join(".local/bin");

    // 1. Initialize Engine (creates folders)
    let engine = RushEngine::with_root(root.clone()).unwrap();

    let real_bin = bin_path.join("ripgrep");
    fs::write(&real_bin, "I am a real program").unwrap();

    // 3. Create "Trash" files (SHOULD be deleted)
    let trash1 = bin_path.join(".rush-tmp-12345");
    let trash2 = bin_path.join(".rush-tmp-abcde");
    fs::write(&trash1, "junk data").unwrap();
    fs::write(&trash2, "more junk").unwrap();

    // 4. Run the cleaner
    engine.clean_trash().unwrap();

    // 5. Verify results
    assert!(
        real_bin.exists(),
        "The real binary was accidentally deleted!"
    );
    assert!(!trash1.exists(), "Trash file 1 still exists!");
    assert!(!trash2.exists(), "Trash file 2 still exists!");
}

#[test]
/// This confirms that if found == false, your install_package function
/// will trigger the "Binary missing in archive" error.
fn test_install_fails_gracefully_if_binary_missing() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let engine = RushEngine::with_root(root.clone()).unwrap();

    // 1. Create a tarball that contains "wrong_file", NOT "target_file"
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_path("wrong_file").unwrap();
    header.set_cksum();

    let mut data = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut data);
        builder.append(&header, &b""[..]).unwrap();
        builder.finish().unwrap();
    }

    // 2. Run the extraction logic manually to simulate the install loop
    // (We can't call install_package directly easily without mocking HTTP,
    // but we can verify the loop logic using the helper)
    let cursor = Cursor::new(data);
    let mut archive = Archive::new(cursor);
    let mut found = false;

    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        // We are looking for "target_file", but tarball has "wrong_file"
        if engine
            .try_extract_binary(&mut entry, "target_file")
            .unwrap()
        {
            found = true;
            break;
        }
    }

    // 3. Assert failure
    assert!(!found, "Should not have found binary");
}
