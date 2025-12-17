use super::*;
use tempfile::tempdir;

#[test]
fn test_engine_initialization() {
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let _engine = RushEngine::with_root(root.clone()).unwrap();
    assert!(root.join(".local/share/rush").exists());
}
