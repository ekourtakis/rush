#!/bin/sh
set -e

# 1. Detect OS and Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

echo "Detected OS: $OS"
echo "Detected Arch: $ARCH"

# Map to GitHub Release Asset Names
# (Make sure these match what you put in your .github/workflows/release.yml)
case "$OS" in
Linux)
	if [ "$ARCH" = "x86_64" ]; then
		ASSET="rush-linux-amd64"
	else
		echo "Unsupported Linux architecture: $ARCH"
		exit 1
	fi
	;;
Darwin)
	if [ "$ARCH" = "arm64" ]; then
		ASSET="rush-macos-arm64"
	elif [ "$ARCH" = "x86_64" ]; then
		ASSET="rush-macos-intel"
	else
		echo "Unsupported macOS architecture: $ARCH"
		exit 1
	fi
	;;
*)
	echo "Unsupported OS: $OS"
	exit 1
	;;
esac

# 2. Define Paths
INSTALL_DIR="$HOME/.local/bin"
BINARY_PATH="$INSTALL_DIR/rush"
RELEASE_URL="https://github.com/ekourtakis/rush/releases/latest/download/$ASSET"

# 3. Download
echo "Downloading rush from $RELEASE_URL..."
mkdir -p "$INSTALL_DIR"

if command -v curl >/dev/null 2>&1; then
	curl -fsSL -o "$BINARY_PATH" "$RELEASE_URL"
elif command -v wget >/dev/null 2>&1; then
	wget -qO "$BINARY_PATH" "$RELEASE_URL"
else
	echo "Error: Neither curl nor wget found. Please install one."
	exit 1
fi

# 4. Make Executable
chmod +x "$BINARY_PATH"

echo ""
echo "✅ Rush installed successfully to $BINARY_PATH"
echo ""
echo "To get started, run:"
echo "  rush update"
echo "  rush search"
echo ""
# Check PATH
case ":$PATH:" in
*":$INSTALL_DIR:"*) ;;
*) echo "⚠️  Warning: $INSTALL_DIR is not in your PATH." ;;
esac
