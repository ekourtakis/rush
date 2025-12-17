# rush

**A lightning-fast toy package manager.**

rush is a proof-of-concept package manager written in Rust. It demonstrates how to manage static binaries, handle dependencies via a registry, and manage state.

## Prerequisites

I work only on x86 Linux, and cannot confirm rush works on any other platform.

## Installation

Use the installer script: 

```bash
curl -fsSL https://raw.githubusercontent.com/ekourtakis/rush/main/scripts/install.sh | sh
```

### Build and Install

You must have Rust installed to build. **[Get Rust here](https://rustup.rs/)**.

You can compile and install the `rush` binary directly into `~/.local/bin` using Cargo.

1. **Clone the repository:**

    ```bash
    git clone https://github.com/ekourtakis/rush.git
    cd rush
    ```

2. **Install to `~/.local/bin`:**
    This command compiles the project in release mode and places the binary in your local bin folder.

    ```bash
    cargo install --path . --root ~/.local
    ```

3. **Update your PATH (if needed):**
    Ensure `~/.local/bin` is in your shell's path so you can run `rush` and packages you install with it from anywhere.

    ```bash
    export PATH="$HOME/.local/bin:$PATH"
    ```

## Usage

Once installed, you can use the `rush` command.

| Command | Description |
| :--- | :--- |
| **`rush search`** | List all packages available in `registry.toml` |
| **`rush install <name>`** | Download and install a package (e.g., `rush install fzf`) |
| **`rush list`** | Show packages currently installed on your system |
| **`rush upgrade`** | Check for newer versions in the registry and upgrade installed tools |
| **`rush uninstall <name>`** | Remove a package and delete its binary |
| **`rush update`** | Reload the registry |
| **`rush clean`** | Remove temporary files from failed installs |
| **`rush --help`** | Show help message |

If you haven't built the binary, you can use cargo run with all commands, e.g.: `cargo run -- install <name>`.

There are developer commands hidden from the default help message. See [Developer Commands](#developer-commands).

### Example Workflow

```bash
# Update registry
rush update

# Search for tools
rush search

# Install a tool
rush install ripgrep

# Verify it works (rush installs tools to ~/.local/bin)
rg --version

# Upgrade packages
rush upgrade

# Remove it
rush uninstall ripgrep
```

### Configuration

By default, rush uses the default registry hosted on this GitHub repo. You can override this by setting the `RUSH_REGISTRY_URL` environment variable.

```bash
export RUSH_REGISTRY_URL="https://github.com/username/repo/archive/main.tar.gz"
rush update
```

`RUSH_REGISTRY_URL` may also be a valid path to a directory containing a registry directory, e.g.:

```sh
# In "rush" directory
export RUSH_REGISTRY_URL="$(pwd)"
```

## Development

### Developer Commands

To use developer commands that modify the registry, you must set `RUSH_REGISTRY_URL` to your local git repository path.

| Command | Description |
| :--- | :--- |
| **`rush dev add`** | Add or update a package target in the local registry. [Usage](#developer-examples). |
| **`rush dev import`** | Interactive wizard to import packages from GitHub |
| **`rush dev --help`** | Show help message. |

#### Developer Examples

```bash
# 1. Point to your local registry source (must be writable)
export RUSH_REGISTRY_URL="$(pwd)"

# 2. Use the Wizard (Recommended)
rush dev import sharkdp/bat

# 3. Or use the Manual Command
rush dev add bat 0.24.0 x86_64-linux https://github.com/.../bat.tar.gz --bin bat
```

### Pre-PR Check

Before opening a Pull Request, run the local CI script to ensure formatting, linting, and tests all pass:

```bash
./scripts/pre-pr.sh
```

If you use the GitHub CLI (`gh`), you can create an alias to run checks automatically before submitting:

```bash
gh alias set submit '!./scripts/pre-pr.sh && gh pr create "$@"'
# Usage: gh submit --web
```

### Testing

Unit tests are either located at the end of source files. Integration tests are in `tests/`.

Run all tests:

```sh
cargo test
```

### Linting

Use:

```sh
cargo clippy -- -D warnings
```

### Formatting

Use:

```sh
cargo fmt --check
```
