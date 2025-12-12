use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- REGISTRY DATA ---
#[derive(Deserialize, Debug, Clone)]
pub struct Registry {
    pub packages: HashMap<String, PackageDefinition>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PackageDefinition {
    pub version: String,
    pub targets: HashMap<String, TargetDefinition>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TargetDefinition {
    pub url: String,
    pub bin: String,
    pub sha256: String,
}

// --- STATE DATA ---
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct State {
    pub packages: HashMap<String, InstalledPackage>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct InstalledPackage {
    pub version: String,
    pub binaries: Vec<String>,
}

// --- TESTS ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_toml_contract() {
        // This simulates the actual TOML file on GitHub.
        // If this test fails, it means we broke compatibility with our own registry.
        let toml_input = r#"
            [packages.test-tool]
            version = "1.0.0"
            
            [packages.test-tool.targets.x86_64-linux]
            url = "https://example.com/tool.tar.gz"
            bin = "tool"
            sha256 = "abc123456"
        "#;

        let registry: Registry = toml::from_str(toml_input).expect("Failed to parse registry TOML");

        assert!(registry.packages.contains_key("test-tool"));
        let pkg = &registry.packages["test-tool"];
        assert_eq!(pkg.version, "1.0.0");

        let target = &pkg.targets["x86_64-linux"];
        assert_eq!(target.bin, "tool");
    }

    #[test]
    fn test_state_json_contract() {
        // Verify we can parse an existing installed.json format
        // If this test fails, it means we broke compatibility with our existing state files.
        let json_input = r#"
        {
            "packages": {
                "grep": {
                    "version": "2.0",
                    "binaries": ["grep"]
                }
            }
        }
        "#;

        let state: State = serde_json::from_str(json_input).expect("Failed to parse state JSON");
        assert_eq!(state.packages["grep"].version, "2.0");
    }

    #[test]
    fn test_state_round_trip() {
        // Verify that Saving -> Loading gives the exact same data
        let mut original = State::default();
        original.packages.insert(
            "foo".to_string(),
            InstalledPackage {
                version: "1.0".to_string(),
                binaries: vec!["bar".to_string()],
            },
        );

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: State = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original.packages["foo"], deserialized.packages["foo"]);
    }
}
