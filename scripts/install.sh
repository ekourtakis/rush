#!/bin/sh
set -e

# 0. Parse Arguments
AUTOCOMPLETE="false"

for arg in "$@"; do
	case $arg in
	--autocomplete)
		AUTOCOMPLETE="true"
		shift
		;;
	esac
done

# 1. Detect OS and Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

echo "Detected OS: $OS"
echo "Detected Arch: $ARCH"

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

# Allow overriding URL for local testing
DEFAULT_URL="https://github.com/ekourtakis/rush/releases/latest/download/$ASSET"
RELEASE_URL="${RUSH_INSTALL_OVERRIDE:-$DEFAULT_URL}"

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

# 5. Helper Function for Completions
install_completions() {
	echo "Installing shell completions..."

	# Detect the shell name (e.g., /bin/zsh -> zsh)
	USER_SHELL=$(basename "$SHELL")

	case "$USER_SHELL" in
	bash)
		# Standard XDG path for user-specific bash completions
		COMP_DIR="$HOME/.local/share/bash-completion/completions"
		mkdir -p "$COMP_DIR"
		"$BINARY_PATH" completions bash >"$COMP_DIR/rush"
		echo "   ✅ Bash completions installed to $COMP_DIR/rush"
		;;
	zsh)
		# We use ~/.zfunc as a common convention.
		COMP_DIR="$HOME/.zfunc"
		mkdir -p "$COMP_DIR"
		"$BINARY_PATH" completions zsh >"$COMP_DIR/_rush"
		echo "   ✅ Zsh completions installed to $COMP_DIR/_rush"
		echo "      (NOTE: Ensure 'fpath+=~/.zfunc' is in your .zshrc before compinit)"
		;;
	fish)
		# Fish standard user completion path
		COMP_DIR="$HOME/.config/fish/completions"
		mkdir -p "$COMP_DIR"
		"$BINARY_PATH" completions fish >"$COMP_DIR/rush.fish"
		echo "   ✅ Fish completions installed to $COMP_DIR/rush.fish"
		;;
	*)
		echo "   ℹ️  Shell '$USER_SHELL' not automatically supported."
		echo "      Run 'rush completions <shell> > <path>' to install manually."
		;;
	esac
}

# 6. Handle Completion Logic
if [ "$AUTOCOMPLETE" = "true" ]; then
	install_completions
else
	# Check if we are running in a terminal to allow interaction
	# We read from /dev/tty to support 'curl | sh' piping
	if [ -t 1 ]; then
		echo ""
		printf "Do you want to install shell completions? [y/N] "
		if read -r response < /dev/tty; then
			case "$response" in
			[yY][eE][sS] | [yY])
				install_completions
				;;
			*)
				echo "Skipping completions."
				;;
			esac
		fi
	fi
fi

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
