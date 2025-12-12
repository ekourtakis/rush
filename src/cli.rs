use clap::{Parser, Subcommand};

// --- CLI ---
#[derive(Parser, Debug)] // Added Debug
#[command(name = "rush")]
#[command(version)]
#[command(about = "A lightning-fast toy package manager.", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, PartialEq)] // Added Debug, PartialEq
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

// --- TESTS ---
#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli_configuration() {
        // This is a built-in Clap test.
        // It checks for conflicting arguments, missing help text, etc.
        Cli::command().debug_assert();
    }

    #[test]
    fn test_install_command_parsing() {
        // Simulate a user typing "rush install ripgrep"
        let args = vec!["rush", "install", "ripgrep"];
        let cli = Cli::parse_from(args);

        match cli.command {
            Commands::Install { name } => assert_eq!(name, "ripgrep"),
            _ => panic!("Parsed incorrect subcommand"),
        }
    }

    #[test]
    fn test_upgrade_command_parsing() {
        let args = vec!["rush", "upgrade"];
        let cli = Cli::parse_from(args);

        // We implemented PartialEq on the Enum so we can compare directly
        assert_eq!(cli.command, Commands::Upgrade);
    }
}
