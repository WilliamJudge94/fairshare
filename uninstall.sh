#!/bin/bash
# fairshare uninstallation script
# Removes fairshare from the system and reverts configuration

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

INSTALL_DIR="/usr/local"

echo -e "${YELLOW}fairshare Uninstallation Script${NC}"
echo "================================"
echo

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}Error: This script must be run as root${NC}"
   echo "Please run: sudo $0"
   exit 1
fi

# Check if fairshare is installed
if [[ ! -f "$INSTALL_DIR/bin/fairshare" ]] && [[ ! -f "$INSTALL_DIR/libexec/fairshare-bin" ]]; then
    echo -e "${YELLOW}fairshare does not appear to be installed${NC}"
    echo "Nothing to uninstall."
    exit 0
fi

echo "This will remove fairshare from your system and revert all configuration."
echo
read -p "Are you sure you want to continue? [y/N] " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Uninstallation cancelled."
    exit 0
fi

echo

# Run admin uninstall if binary exists
if [[ -x "$INSTALL_DIR/bin/fairshare" ]]; then
    echo "Running fairshare admin uninstall..."
    if "$INSTALL_DIR/bin/fairshare" admin uninstall --force 2>/dev/null; then
        echo -e "${GREEN}✓${NC} Removed fairshare configuration"
    else
        echo -e "${YELLOW}Warning: Could not run admin uninstall (may already be removed)${NC}"
    fi
    echo
fi

# Remove binary files
echo "Removing installed files..."

if [[ -f "$INSTALL_DIR/libexec/fairshare-bin" ]]; then
    rm -f "$INSTALL_DIR/libexec/fairshare-bin"
    echo -e "${GREEN}✓${NC} Removed $INSTALL_DIR/libexec/fairshare-bin"
fi

if [[ -f "$INSTALL_DIR/bin/fairshare" ]]; then
    rm -f "$INSTALL_DIR/bin/fairshare"
    echo -e "${GREEN}✓${NC} Removed $INSTALL_DIR/bin/fairshare"
fi

# Remove PolicyKit files (in case admin uninstall didn't run)
echo
echo "Removing PolicyKit policies..."

if [[ -f "/usr/share/polkit-1/actions/org.fairshare.policy" ]]; then
    rm -f /usr/share/polkit-1/actions/org.fairshare.policy
    echo -e "${GREEN}✓${NC} Removed PolicyKit action"
fi

if [[ -f "/etc/polkit-1/rules.d/50-fairshare.rules" ]]; then
    rm -f /etc/polkit-1/rules.d/50-fairshare.rules
    echo -e "${GREEN}✓${NC} Removed PolicyKit rules"
fi

if [[ -f "/var/lib/polkit-1/localauthority/10-vendor.d/50-fairshare.pkla" ]]; then
    rm -f /var/lib/polkit-1/localauthority/10-vendor.d/50-fairshare.pkla
    echo -e "${GREEN}✓${NC} Removed PolicyKit localauthority"
fi

# Remove systemd configuration (in case admin uninstall didn't run)
echo
echo "Removing systemd configuration..."

if [[ -f "/etc/systemd/system/user-.slice.d/00-defaults.conf" ]]; then
    rm -f /etc/systemd/system/user-.slice.d/00-defaults.conf
    # Remove directory if empty
    rmdir /etc/systemd/system/user-.slice.d 2>/dev/null || true
    echo -e "${GREEN}✓${NC} Removed systemd defaults"

    echo "Reloading systemd..."
    systemctl daemon-reload
    echo -e "${GREEN}✓${NC} Reloaded systemd"
fi

# Remove policy configuration
if [[ -f "/etc/fairshare/policy.toml" ]]; then
    rm -f /etc/fairshare/policy.toml
    rmdir /etc/fairshare 2>/dev/null || true
    echo -e "${GREEN}✓${NC} Removed policy configuration"
fi

echo
echo -e "${GREEN}✓ Uninstallation complete!${NC}"
echo
echo "fairshare has been removed from your system."
echo "User resource limits have been reverted to system defaults."
