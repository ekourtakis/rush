#!/usr/bin/env bash

# Exit immediately if a command exits with a non-zero status
set -e

# Colors for pretty output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

print_step() {
        echo -e "\n${GREEN}==> $1${NC}"
}

error_handler() {
        echo -e "\n${RED}❌ Check failed! Fix the errors above before pushing.${NC}"
}

# Trap errors
trap 'error_handler' ERR

# 1. Format Check
print_step "Checking Code Formatting..."
cargo fmt --all -- --check

# 2. Clippy (Strict Mode)
print_step "Linting (Clippy)..."
# We use --all-targets to check tests/benches too
cargo clippy --all-targets --all-features -- -D warnings

# 3. Tests
print_step "Running Tests..."
# We run release tests too, just in case optimizations break logic (rare, but possible)
cargo test

# 4. Build Check
print_step "Verifying Build..."
cargo build
cargo build --release

# 5. Registry Verification
print_step "Checking Registry Integrity..."
CHANGES_DETECTED=false

# Check changes against main (if it exists)
if git rev-parse --verify main >/dev/null 2>&1; then
    if ! git diff --quiet main -- packages/; then
        CHANGES_DETECTED=true
    fi
fi

# Check unstaged changes
if ! git diff --quiet -- packages/; then
    CHANGES_DETECTED=true
fi

# Check staged changes
if ! git diff --quiet --cached -- packages/; then
    CHANGES_DETECTED=true
fi

# Check untracked files
if [ -n "$(git ls-files --others --exclude-standard -- packages/)" ]; then
    CHANGES_DETECTED=true
fi

if [ "$CHANGES_DETECTED" = true ]; then
    echo "📦 Detected changes in packages/. Verifying registry..."
    export RUSH_REGISTRY_URL="."
    cargo run -- dev verify
else
    echo "⏭️  No changes in packages/. Skipping registry verification."
fi

echo -e "\n${GREEN}✅ All checks passed! You are ready to open a PR.${NC}"
