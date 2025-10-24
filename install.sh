#!/bin/bash

set -e

echo "=== fairshare installation script ==="
echo

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Error: This script must be run as root (use sudo)"
    exit 1
fi

# Check for polkit (warn but continue)
if ! command -v pkexec &> /dev/null; then
    echo "Warning: pkexec not found. Users will need to install polkit:"
    echo
    if command -v apt &> /dev/null; then
        echo "  sudo apt install policykit-1"
    elif command -v dnf &> /dev/null; then
        echo "  sudo dnf install polkit"
    elif command -v pacman &> /dev/null; then
        echo "  sudo pacman -S polkit"
    else
        echo "  Install the 'polkit' package for your distribution"
    fi
    echo
fi

# Build the project
echo "Building fairshare..."
cargo build --release

# Install binary
echo "Installing binary to /usr/local/bin/..."
cp target/release/fairshare /usr/local/bin/
chmod 755 /usr/local/bin/fairshare

# Install polkit policy
echo "Installing polkit policy..."
mkdir -p /usr/share/polkit-1/actions/
cp com.fairshare.policy /usr/share/polkit-1/actions/
chmod 644 /usr/share/polkit-1/actions/com.fairshare.policy

echo
echo "=== Installation complete! ==="
echo
echo "Users can now run 'fairshare request' and 'fairshare release' without sudo."
echo "They will be prompted for authentication via polkit when needed."
echo
echo "Next steps:"
echo "1. Ensure polkit is installed (fairshare will check and prompt users)"
echo "2. Set up global defaults:"
echo "     sudo fairshare admin setup --cpu 10 --mem 512M"
