use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::PathBuf;
use tar::Builder;
use tempfile::TempDir;

/// A helper to simulate a remote registry environment locally
pub struct MockEnvironment {
    pub _temp: TempDir,
    pub home: PathBuf,
    pub registry_source: PathBuf,
}

impl MockEnvironment {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().expect("Failed to create temp dir");
        let root = temp.path();

        let home = root.join("home");
        fs::create_dir(&home).expect("Failed to create mock home");

        let registry_source = root.join("registry_source");
        fs::create_dir(&registry_source).expect("Failed to create registry source");
        fs::create_dir(registry_source.join("packages")).expect("Failed to create packages dir");

        Self {
            _temp: temp,
            home,
            registry_source,
        }
    }

    /// Adds a valid package to the mock registry
    pub fn add_package(&self, name: &str, version: &str, bin_name: &str) {
        self.create_package_internal(name, version, bin_name, None)
    }

    /// Adds a package with a deliberately wrong checksum to test security
    pub fn add_malicious_package(&self, name: &str, version: &str, bin_name: &str) {
        self.create_package_internal(name, version, bin_name, Some("bad-checksum-123"))
    }

    fn create_package_internal(
        &self,
        name: &str,
        version: &str,
        bin_name: &str,
        checksum_override: Option<&str>,
    ) {
        // 1. Create Script
        let script_content = format!("#!/bin/sh\necho 'Hello from {} v{}'", name, version);
        let archive_name = format!("{}-{}.tar.gz", name, version);
        let archive_path = self.registry_source.join(&archive_name);

        // 2. Create Tarball
        let tar_file = File::create(&archive_path).expect("Failed to create tar file");
        let enc = GzEncoder::new(tar_file, Compression::default());
        let mut tar = Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_size(script_content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, bin_name, script_content.as_bytes())
            .unwrap();
        tar.finish().unwrap();

        // 3. Calculate (or fake) SHA256
        let sha256 = if let Some(bad_hash) = checksum_override {
            bad_hash.to_string()
        } else {
            let bytes = fs::read(&archive_path).unwrap();
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            hex::encode(hasher.finalize())
        };

        // 4. Write Manifest
        let target_arch = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
        let url = format!("file://{}", archive_path.to_str().unwrap());

        let toml_content = format!(
            r#"
            version = "{version}"
            description = "Mock package"
            [targets.{target_arch}]
            url = "{url}"
            bin = "{bin_name}"
            sha256 = "{sha256}"
            "#
        );

        let prefix = name.chars().next().unwrap();
        let package_dir = self
            .registry_source
            .join("packages")
            .join(prefix.to_string());
        fs::create_dir_all(&package_dir).unwrap();
        fs::write(package_dir.join(format!("{}.toml", name)), toml_content).unwrap();
    }

    pub fn envs(&self) -> Vec<(&str, String)> {
        vec![
            ("HOME", self.home.to_str().unwrap().to_string()),
            (
                "RUSH_REGISTRY_URL",
                self.registry_source.to_str().unwrap().to_string(),
            ),
        ]
    }
}
