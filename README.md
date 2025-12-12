# rush

**A lightning-fast toy package manager.**

rush is a proof-of-concept package manager written in Rust. It demonstrates how to manage static binaries, handle dependencies via a registry, and manage state.

## Prerequisites

The only prerequisite is Rust. **[Get Rust here](https://rustup.rs/)**.

I work only on x86 Linux, and cannot confirm rush works on any other platform.

## Installation

You can compile and install the `rush` binary directly into your local user path using Cargo.

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
| **`rush update`** | Reload the registry file |
| **`rush clean`** | Remove temporary files from failed installs |

If you haven't built the binary, you can use cargo run with all commands, e.g.: `cargo run -- install <name>`.

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
export RUSH_REGISTRY_URL="https://some/other/url/registry.toml"
rush update
```

`RUSH_REGISTRY_URL` may also be a valid path to a `toml` file.

## Testing

Unit tests located at the end of their corresponding files. Integration tests are within [`tests/`](./tests/).

Run tests:

```sh
cargo test
```
