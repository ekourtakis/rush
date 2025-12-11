# rush

**A lightning-fast toy package manager.**

rush is a proof-of-concept package manager written in Rust. It can manage static binaries, handle dependencies via a registry, and manage package state.

## Prerequisites

The only prerequisit is Rust. **[Get Rust here](https://rustup.rs/)**.

I work only on x86 Linux, and cannot confirm rush works on any other platform.

## How to Build & Run

1. **Clone the repository:**

    ```bash
    git clone https://github.com/ekourtakis/rush.git
    cd rush
    ```

2. **Run directly via Cargo:**

    ```bash
    cargo run -- search
    ```

3. **Build a release binary:**

    ```bash
    cargo build --release
    ```

    The binary will be located at `./target/release/rush`.

> **Note:** Rush currently relies on a local `registry.toml` file in the working directory to find packages. Ensure this file exists before running commands.

## Usage

Rush installs binaries to `~/.local/bin`. Ensure this directory is in your `PATH`.

### Core Commands

| Command | Description |
| :--- | :--- |
| **`search`** | List all packages available in `registry.toml` |
| **`install <name>`** | Download and install a package (e.g., `rush install fzf`) |
| **`list`** | Show packages currently installed on your system |
| **`upgrade`** | Check for newer versions in the registry and upgrade installed tools |
| **`uninstall <name>`** | Remove a package and delete its binary |
| **`update`** | Reload the registry file (currently a placeholder for remote fetching) |

### Example Workflow

```bash
# Check what is available
rush search

# Install a tool
rush install ripgrep

# Verify it works
rg --version

# Remove it
rush uninstall ripgrep
```
