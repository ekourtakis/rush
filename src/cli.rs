use clap::{Parser, Subcommand};

// --- CLI ---
#[derive(Parser)]
#[command(name = "rush")]
#[command(about = "A lightning-fast toy package manager.", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a package
    Install { name: String },
    /// Uninstall a package
    Uninstall { name: String },
    /// List installed packages
    List,
    /// Search for available packages
    Search,
    /// Update the registry (for now, just re-reads the local file)
    Update,
    /// Upgrade all installed packages
    Upgrade,
}
