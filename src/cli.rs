use clap::{Parser, Subcommand};
use clap_complete::Shell;

// --- CLI ---
#[derive(Parser, Debug)] // Added Debug
#[command(name = "rush")]
#[command(version)]
#[command(about = "A lightning-fast toy package manager.", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, PartialEq)]
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
    /// Remove temporary files from failed installs
    Clean,

    #[command(hide = true)]
    /// Developer commands (hidden from help)
    Dev {
        #[command(subcommand)]
        command: DevCommands,
    },

    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand, Debug, PartialEq)]
/// Developer commands
pub enum DevCommands {
    /// Add a package target to the local registry file
    Add {
        /// Package name (e.g. "fzf")
        name: String,
        /// Package version (e.g. "0.56.3")
        version: String,
        /// System target (e.g. "x86_64-linux")
        target: String,
        /// Download URL
        url: String,
        /// Binary name inside the archive (defaults to package name)
        #[arg(long)]
        bin: Option<String>,
    },
    /// Interactive wizard to import a package from GitHub
    Import {
        /// Repository (e.g. "sharkdp/bat")
        repo: String,
    },
    Verify,
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

    #[test]
    fn test_dev_add_command_parsing() {
        let args = vec![
            "rush",
            "dev",
            "add",
            "my-tool",
            "1.2.3",
            "x86_64-linux",
            "http://example.com/tool.tar.gz",
            "--bin",
            "tool-bin",
        ];
        let cli = Cli::parse_from(args);

        match cli.command {
            Commands::Dev { command } => match command {
                DevCommands::Add {
                    name,
                    version,
                    target,
                    url,
                    bin,
                } => {
                    assert_eq!(name, "my-tool");
                    assert_eq!(version, "1.2.3");
                    assert_eq!(target, "x86_64-linux");
                    assert_eq!(url, "http://example.com/tool.tar.gz");
                    assert_eq!(bin, Some("tool-bin".to_string()));
                }
                _ => panic!("Parsed incorrect dev subcommand"),
            },
            _ => panic!("Parsed incorrect top-level command"),
        }
    }
}
