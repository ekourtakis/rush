use crate::core::{RushEngine, util};
use crate::models::{InstallEvent, InstallResult, InstalledPackage, TargetDefinition};
use anyhow::Result;
use flate2::read::GzDecoder;
use std::path::{Path, PathBuf};
use tar::Archive;

pub fn install_package<F>(
    engine: &mut RushEngine,
    name: &str,
    version: &str,
    target: &TargetDefinition,
    mut on_event: F,
) -> Result<InstallResult>
where
    F: FnMut(InstallEvent),
{
    // 1. Download using shared utility
    let content = util::download_url(&engine.client, &target.url, &mut on_event)?;

    // 2. Verify Checksum using shared utility
    on_event(InstallEvent::VerifyingChecksum);
    util::verify_checksum(&content, &target.sha256)?;

    // 3. Extract
    on_event(InstallEvent::Extracting);
    let tar = GzDecoder::new(&content[..]);
    let mut archive = Archive::new(tar);
    let mut found = false;
    let mut final_path = PathBuf::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        // Call the local helper function
        if let Some(dest) = try_extract_binary(&mut entry, &engine.bin_path, &target.bin)? {
            final_path = dest;
            found = true;
            break;
        }
    }

    if !found {
        anyhow::bail!("Binary '{}' not found in archive", target.bin);
    }

    // 4. Update State
    engine.state.packages.insert(
        name.to_string(),
        InstalledPackage {
            version: version.to_string(),
            binaries: vec![target.bin.clone()],
        },
    );
    engine.save()?;

    on_event(InstallEvent::Success);

    Ok(InstallResult {
        package_name: name.to_string(),
        version: version.to_string(),
        path: final_path,
    })
}

/// Helper: Returns Some(path) if successful, None if skipped
/// We pass `bin_path` explicitly here instead of `&self`
fn try_extract_binary<R: std::io::Read>(
    entry: &mut tar::Entry<R>,
    bin_path: &Path,
    target_bin_name: &str,
) -> Result<Option<PathBuf>> {
    let path = entry.path()?;

    // Guard Clause 1: Check if filename exists
    let fname = match path.file_name() {
        Some(f) => f,
        None => return Ok(None),
    };

    // Guard Clause 2: Check if filename matches target
    if fname != std::ffi::OsStr::new(target_bin_name) {
        return Ok(None);
    }

    // --- ATOMIC INSTALL LOGIC ---
    let dest = bin_path.join(target_bin_name);

    let mut temp_file = tempfile::Builder::new()
        .prefix(".rush-tmp-")
        .tempfile_in(bin_path)?;

    std::io::copy(entry, &mut temp_file)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = temp_file.as_file().metadata()?.permissions();
        p.set_mode(0o755);
        temp_file.as_file().set_permissions(p)?;
    }

    temp_file.persist(&dest)?;

    Ok(Some(dest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempdir;

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

        let result = try_extract_binary(&mut entry, &engine.bin_path, "test-bin").unwrap();

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

        let result = try_extract_binary(&mut entry, &engine.bin_path, "test-bin").unwrap();

        assert!(
            result.is_none(),
            "Should not have extracted mismatched filename"
        );
        assert!(!root.join(".local/bin/test-bin").exists());
    }

    #[test]
    /// This confirms that if found == false, the logic handles it gracefully
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
        let cursor = Cursor::new(data);
        let mut archive = Archive::new(cursor);
        let mut found = false;

        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();

            if try_extract_binary(&mut entry, &engine.bin_path, "target_file")
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
}
