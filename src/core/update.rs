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
    if !source.starts_with("http") {
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
        let engine = RushEngine::with_root_and_registry(
            root.clone(),
            source_dir.to_str().unwrap().to_string(),
        )
        .unwrap();

        // Pass an empty callback that ignores all events
        engine.update_registry(|_| {}).unwrap();

        let found = engine.find_package("test-tool");
        assert!(found.is_some());
    }
}
