# TODO

## Phase 1: Reliability & Safety

1. **End-to-End Testing (Current Priority)**
    * Create `MockRegistry` and `MockArchive` helpers as traits.
    * Write `tests/cli.rs` integration tests that simulate a full install lifecycle without internet.
    * Stress Test: Write a script to generate 1,000 dummy packages and ensure `rush search` doesn't lag
2. **Registry Verification (`rush dev verify`)**
    * **Goal:** Prevent bad data in `registry.toml`.
    * **Task:** Implement a command that iterates through every target, downloads headers/files, validates checksums, and verifies binary existence.
    * **CI:** Run this on every PR that effects `packages/`.
3. **Binary Architecture Safety Check**
    * **Goal:** Prevent `Exec format error`.
    * **Task:** Inspect ELF/Mach-O headers before installation to verify CPU architecture matches host.

## Phase 2: Core Capabilities

1. **Interactive Binary Picking**
    * **Goal:** No more guessing binary names during `rush dev import`.
    * **Task:** Download archive to memory -> List files -> Fuzzy Select -> Save choice.
        * Cache archive listings during import to avoid re-downloads
2. **Universal Archive Support**
    * **Goal:** Support `.zip`, `.tar.xz`, etc.
    * **Task:** Abstract extraction logic via traits; add `zip` / `lzma` dependencies.
3. **Multiple Binaries Support**
    * **Goal:** Support packages like `llvm` (clang, lld).
    * **Task:** Update models to support `binaries: Vec<String>`.
4. **Version Pinning**
    * **Goal:** Allow installing specific versions (e.g., `rush install foo@1.2.0`).
    * **Task:**
        * Update Registry format to support multiple versions per package.
        * Update CLI parser to handle `@version` syntax.
    * TODO: Decide how upgrades behave when pinned?
        * skip?
        * warn?

## Phase 3: Lifecycle & Polish

1. **The Uninstaller (`scripts/uninstall.sh`)**
    * **Goal:** Clean removal of all traces of Rush.
    * **Task:**
        * Read `installed.js>on` to remove all managed binaries from `~/.local/bin`.
        * Remove `~/.local/share/rush` (Registry & State).
        * Remove Autocompletion files (`~/.zfunc/_rush`, `~/.config/fish/...`, etc).
        * Remove the `rush` binary itself.
2. **`rush outdated`**
    * **Goal:** See available updates without installing.
    * **Task:** Compare local vs. registry versions -> Print table.
3. **`rush self-update`**
    * **Goal:** One-command update.
    * **Task:** Check GitHub Releases -> Hot-swap binary.
4. **User Configuration (`config.toml`)**
    * **Goal:** persistent configuration without Environment Variables.
    * **Task:**
        * specific support for `~/.config/rush/config.toml`.
        * Support settings: `registry_url`, `install_path`, `color_output`.

## Phase 4: Ecosystem Expansion

1. **Populate the Registry**
    * **Goal:** Make Rush actually useful by having the tools people want.
    * **Task:** Add the top 20-50 most popular Rust/CLI tools (e.g., `bat`, `fd`, `exa`, `jq`, `yq`, `zoxide`, `starship`, `tokei`).
2. **Community Submissions**
    * **Goal:** Allow others to add packages easily.
    * **Task:** Create a `CONTRIBUTING.md` guide specifically for adding packages via `rush dev add`.
