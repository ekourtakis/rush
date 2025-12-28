use crate::core::{RushEngine, util};
use crate::models::{UpdateEvent, UpdateResult};
use anyhow::Result;
use flate2::read::GzDecoder;
use std::fs;
use std::path::PathBuf;
use tar::Archive;
use walkdir::WalkDir;

/// Update the package registry from the configured source.
pub fn update_registry<F>(engine: &RushEngine, mut on_event: F) -> Result<UpdateResult>
where
    F: FnMut(UpdateEvent),
{
    let source = &engine.registry_source;

    on_event(UpdateEvent::Fetching {
        source: source.clone(),
    });

    // 1. Wipe old registry
    if engine.registry_dir.exists() {
        fs::remove_dir_all(&engine.registry_dir)?;
    }
    fs::create_dir_all(&engine.registry_dir)?;

    // 2. Handle Local Directory
    if !source.starts_with("http") && !source.starts_with("file://") {
        let source_path = PathBuf::from(source);
        if !source_path.exists() {
            anyhow::bail!("Local registry path not found: {:?}", source_path);
        }

        let pkg_source = source_path.join("packages");
        if !pkg_source.exists() {
            return Ok(UpdateResult {
                source: source.clone(),
            });
        }

        let pkg_dest = engine.registry_dir.join("packages");
        for entry in WalkDir::new(&pkg_source) {
            let entry = entry?;
            if let Ok(rel_path) = entry.path().strip_prefix(&pkg_source) {
                let dest_path = pkg_dest.join(rel_path);
                if entry.file_type().is_dir() {
                    fs::create_dir_all(&dest_path)?;
                } else {
                    fs::copy(entry.path(), &dest_path)?;
                }
            }
        }
        return Ok(UpdateResult {
            source: source.clone(),
        });
    }

    // 3. Handle Remote Tarball via util::download_url
    let content = util::download_url(&engine.client, source, &mut |event| {
        if let crate::models::InstallEvent::Progress { bytes, total } = event {
            on_event(UpdateEvent::Progress { bytes, total });
        }
    })?;

    on_event(UpdateEvent::Unpacking);

    let tar = GzDecoder::new(&content[..]);
    let mut archive = Archive::new(tar);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        if let Some(idx) = path.to_string_lossy().find("packages/") {
            let relative_path = &path.to_string_lossy()[idx..];
            let dest = engine.registry_dir.join(relative_path);

            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(dest)?;
        }
    }

    Ok(UpdateResult {
        source: source.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::fs::File;
    use tar::Builder;
    use tempfile::tempdir;

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
        std::fs::write(&dummy_toml, "content").unwrap();

        let engine = RushEngine::with_root_and_registry(
            root.clone(),
            source_dir.to_str().unwrap().to_string(),
        )
        .unwrap();

        engine.update_registry(|_| {}).unwrap();

        let expected_dest = engine.registry_dir.join("packages/t/test-tool.toml");
        assert!(expected_dest.exists());
    }

    #[test]
    fn test_update_missing_local_path() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        let engine =
            RushEngine::with_root_and_registry(root, "/path/that/does/not/exist".to_string())
                .unwrap();

        let result = engine.update_registry(|_| {});
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_update_from_tarball() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        let archive_path = temp_dir.path().join("registry.tar.gz");
        let file = File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_path("packages/z/zipped-tool.toml").unwrap();
        header.set_size(4);
        header.set_cksum();
        tar.append_data(
            &mut header,
            "packages/z/zipped-tool.toml",
            "data".as_bytes(),
        )
        .unwrap();

        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        let url = format!("file://{}", archive_path.to_str().unwrap());
        let engine = RushEngine::with_root_and_registry(root.clone(), url).unwrap();

        engine.update_registry(|_| {}).unwrap();

        let expected_file = engine
            .registry_dir
            .join("packages")
            .join("z")
            .join("zipped-tool.toml");
        assert!(
            expected_file.exists(),
            "Registry tarball was not extracted correctly"
        );
    }
}
