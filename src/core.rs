pub mod clean;
pub mod dev;
pub mod install;
pub mod query;
pub mod uninstall;
pub mod update;
pub mod util;

use crate::models::{
    ImportCandidate, InstallEvent, InstallResult, PackageManifest, State, TargetDefinition,
    UninstallResult, UpdateEvent, UpdateResult,
};
use anyhow::{Context, Result};
use std::fs::{self};
use std::path::PathBuf;

/// Default URL to fetch the registry from, overridable by env variable
const DEFAULT_REGISTRY_URL: &str =
    "https://github.com/ekourtakis/rush/archive/refs/heads/main.tar.gz";

/// The core engine that handles state and I/O
pub struct RushEngine {
    pub state: State,
    pub(crate) state_path: PathBuf, // ~/.local/share/rush/installed.json
    pub(crate) registry_dir: PathBuf, // ~/.local/share/rush/registry/
    pub(crate) bin_path: PathBuf,   // ~/.local/bin
    pub(crate) client: reqwest::blocking::Client, // HTTP Client
    pub(crate) registry_source: String,
}

impl RushEngine {
    /// Standard constructor
    /// Reads HOME and Env Vars automatically.
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("No home dir")?;
        let source =
            std::env::var("RUSH_REGISTRY_URL").unwrap_or_else(|_| DEFAULT_REGISTRY_URL.to_string());
        Self::init(home, source)
    }

    /// Test constructor: Isolated Root + Default Registry
    pub fn with_root(root: PathBuf) -> Result<Self> {
        Self::init(root, DEFAULT_REGISTRY_URL.to_string())
    }

    /// Test constructor: Isolated Root + Custom Registry Source
    pub fn with_root_and_registry(root: PathBuf, registry_source: String) -> Result<Self> {
        Self::init(root, registry_source)
    }

    /// Shared initialization logic
    fn init(root: PathBuf, registry_source: String) -> Result<Self> {
        let state_dir = root.join(".local/share/rush");
        let bin_path = root.join(".local/bin");
        let state_path = state_dir.join("installed.json");
        let registry_dir = state_dir.join("registry");

        fs::create_dir_all(&state_dir)?;
        fs::create_dir_all(&bin_path)?;

        let state = if state_path.exists() {
            let content = fs::read_to_string(&state_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            State::default()
        };

        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("rush/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            state,
            state_path,
            registry_dir,
            bin_path,
            client,
            registry_source,
        })
    }

    /// Save state to disk
    pub(crate) fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_path, content)?;
        Ok(())
    }

    /// Download and Install a package.
    pub fn install_package<F>(
        &mut self,
        name: &str,
        version: &str,
        target: &TargetDefinition,
        on_event: F,
    ) -> Result<InstallResult>
    where
        F: FnMut(InstallEvent),
    {
        install::install_package(self, name, version, target, on_event)
    }

    /// Uninstall a package.
    pub fn uninstall_package(&mut self, name: &str) -> Result<Option<UninstallResult>> {
        uninstall::uninstall_package(self, name)
    }

    /// Download the registry from the internet OR copy it from a local directory
    pub fn update_registry<F>(&self, on_event: F) -> Result<UpdateResult>
    where
        F: FnMut(UpdateEvent),
    {
        update::update_registry(self, on_event)
    }

    /// Look up a specific package file (e.g. .../registry/packages/f/fzf.toml)
    pub fn find_package(&self, name: &str) -> Option<PackageManifest> {
        query::find_package(self, name)
    }

    /// Scan the folder structure to list all available packages
    pub fn list_available_packages(&self) -> Vec<(String, PackageManifest)> {
        query::list_available_packages(self)
    }

    /// Clean up old temorary files from atomic installs
    pub fn clean_trash(&self) -> Result<crate::models::CleanResult> {
        clean::clean_trash(self)
    }

    /// Developer Tool: Create/Update a local package manifest
    pub fn add_package_manual<F>(
        &self,
        name: String,
        version: String,
        target_arch: String,
        url: String,
        bin_name: Option<String>,
        on_event: F,
    ) -> Result<()>
    where
        F: FnMut(InstallEvent),
    {
        dev::add_package_manual(self, name, version, target_arch, url, bin_name, on_event)
    }

    /// Developer Tool: Interactive Import wizard from GitHub
    pub fn fetch_github_import_candidates(
        &self,
        repo: &str,
    ) -> Result<(String, String, Vec<ImportCandidate>)> {
        dev::fetch_github_import_candidates(self, repo)
    }
}

// --- TESTS ---
#[cfg(test)]
mod tests;
