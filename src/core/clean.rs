use super::RushEngine;
use crate::models::CleanResult;
use anyhow::Result;
use std::fs;

pub fn clean_trash(engine: &RushEngine) -> Result<CleanResult> {
    let bin_dir = fs::read_dir(&engine.bin_path)?;
    let mut deleted_files = Vec::new();

    for entry in bin_dir {
        let entry = entry?;
        let path = entry.path();

        if let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| n.starts_with(".rush-tmp-"))
        {
            fs::remove_file(&path)?;
            deleted_files.push(name.to_string());
        }
    }

    Ok(CleanResult {
        files_cleaned: deleted_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
}
