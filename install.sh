#!/bin/bash

set -e

echo "=== fairshare installation script ==="
echo

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Error: This script must be run as root (use sudo)"
    exit 1
fi

# Build the project
echo "Building fairshare..."
cargo build --release

# Install binary
echo "Installing binary to /usr/local/bin/..."
cp target/release/fairshare /usr/local/bin/
chmod 755 /usr/local/bin/fairshare

echo
echo "=== Installation complete! ==="
echo
echo "Users can now run 'fairshare request' and 'fairshare release' without sudo."
echo "Each user manages their own systemd user session resources."
echo
echo "To set up global defaults (optional):"
echo "  sudo fairshare admin setup --cpu 10 --mem 512M"
