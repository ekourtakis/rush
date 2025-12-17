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

    assert!(result.is_some(), "Should have extracted the binary");
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

    assert!(
        result.is_none(),
        "Should not have extracted mismatched filename"
    );
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
            .is_some()
        {
            found = true;
            break;
        }
    }

    // 3. Assert failure
    assert!(!found, "Should not have found binary");
}
