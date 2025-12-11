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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InstalledPackage {
    pub version: String,
    pub binaries: Vec<String>,
}
