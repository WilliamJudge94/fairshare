#!/bin/bash
# fairshare installation script
# Installs fairshare system-wide with proper permissions

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default resource limits
DEFAULT_CPU=1
DEFAULT_MEM=2

# GitHub repository
REPO="WilliamJudge94/fairshare"
INSTALL_DIR="/usr/local"

echo -e "${GREEN}fairshare Installation Script${NC}"
echo "=============================="
echo

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}Error: This script must be run as root${NC}"
   echo "Please run: sudo $0"
   exit 1
fi

# Detect architecture
ARCH=$(uname -m)
case $ARCH in
    x86_64)
        ARCH_NAME="x86_64"
        ;;
    aarch64)
        ARCH_NAME="aarch64"
        ;;
    *)
        echo -e "${RED}Error: Unsupported architecture: $ARCH${NC}"
        echo "Supported architectures: x86_64, aarch64"
        exit 1
        ;;
esac

echo -e "Detected architecture: ${GREEN}$ARCH_NAME${NC}"
echo

# Check for dependencies
echo "Checking dependencies..."
MISSING_DEPS=()

if ! command -v systemctl &> /dev/null; then
    MISSING_DEPS+=("systemd")
fi

# Check if systemd is missing (critical dependency)
if [ ${#MISSING_DEPS[@]} -ne 0 ]; then
    echo -e "${RED}Error: Missing required dependencies:${NC}"
    for dep in "${MISSING_DEPS[@]}"; do
        echo "  - $dep"
    done
    exit 1
fi

# Check for PolicyKit and offer to install if missing
if ! command -v pkexec &> /dev/null; then
    echo -e "${YELLOW}PolicyKit (pkexec) not found${NC}"
    echo
    echo "PolicyKit is required to allow regular users to manage their systemd user slices"
    echo "without requiring full root access. This enables users to safely request and"
    echo "release CPU and memory resources for their own sessions."
    echo
    read -p "Would you like to install PolicyKit now? (y/n): " -n 1 -r
    echo

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo -e "${YELLOW}Installing PolicyKit...${NC}"
        echo "Running: apt-get update"
        apt-get update || {
            echo -e "${RED}Error: Failed to run apt-get update${NC}"
            exit 1
        }

        echo "Running: apt install -y policykit-1"
        apt install -y policykit-1 || {
            echo -e "${RED}Error: Failed to install policykit-1${NC}"
            echo "You may need to install it manually or use a different package manager."
            exit 1
        }

        echo -e "${GREEN}✓ PolicyKit installed successfully${NC}"
        echo
    else
        echo -e "${RED}Error: PolicyKit is required for fairshare to function${NC}"
        echo "Please install it manually:"
        echo "  apt install policykit-1     # Debian/Ubuntu"
        echo "  dnf install polkit          # Fedora/RHEL"
        echo "  pacman -S polkit            # Arch Linux"
        exit 1
    fi
fi

echo -e "${GREEN}All dependencies found${NC}"
echo

# Determine installation source
BINARY_PATH=""
WRAPPER_PATH="assets/fairshare-wrapper.sh"
ASSETS_DIR="assets"

# Check if we're in the source directory with a built binary
if [[ -f "target/release/fairshare" ]] && [[ -f "$WRAPPER_PATH" ]]; then
    echo -e "${YELLOW}Local build detected${NC}"
    BINARY_PATH="target/release/fairshare"
    USE_LOCAL=true
else
    echo -e "${YELLOW}No local build found, will download from GitHub releases${NC}"
    USE_LOCAL=false

    # Get latest release
    echo "Fetching latest release information..."
    LATEST_RELEASE=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

    if [[ -z "$LATEST_RELEASE" ]]; then
        echo -e "${RED}Error: Could not fetch latest release from GitHub${NC}"
        echo "Please check your internet connection or build from source:"
        echo "  cargo build --release"
        echo "  sudo ./install.sh"
        exit 1
    fi

    echo -e "Latest release: ${GREEN}$LATEST_RELEASE${NC}"

    # Download binary
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_RELEASE/fairshare-$ARCH_NAME"
    echo "Downloading binary from $DOWNLOAD_URL..."

    TEMP_DIR=$(mktemp -d)
    if ! curl -L "$DOWNLOAD_URL" -o "$TEMP_DIR/fairshare" 2>/dev/null; then
        echo -e "${RED}Error: Failed to download binary${NC}"
        echo "Build from source instead:"
        echo "  cargo build --release"
        echo "  sudo ./install.sh"
        rm -rf "$TEMP_DIR"
        exit 1
    fi

    chmod +x "$TEMP_DIR/fairshare"
    BINARY_PATH="$TEMP_DIR/fairshare"

    # Download wrapper and assets
    echo "Downloading wrapper script and assets..."
    mkdir -p "$TEMP_DIR/assets"

    for file in fairshare-wrapper.sh org.fairshare.policy 50-fairshare.rules 50-fairshare.pkla; do
        if ! curl -L "https://raw.githubusercontent.com/$REPO/$LATEST_RELEASE/assets/$file" -o "$TEMP_DIR/assets/$file" 2>/dev/null; then
            echo -e "${YELLOW}Warning: Could not download $file${NC}"
        fi
    done

    WRAPPER_PATH="$TEMP_DIR/assets/fairshare-wrapper.sh"
    ASSETS_DIR="$TEMP_DIR/assets"
fi

# Install binary
echo
echo "Installing fairshare..."
install -D -m 0755 "$BINARY_PATH" "$INSTALL_DIR/libexec/fairshare-bin"
echo -e "${GREEN}✓${NC} Installed binary to $INSTALL_DIR/libexec/fairshare-bin"

# Install wrapper
if [[ -f "$WRAPPER_PATH" ]]; then
    install -D -m 0755 "$WRAPPER_PATH" "$INSTALL_DIR/bin/fairshare"
    echo -e "${GREEN}✓${NC} Installed wrapper to $INSTALL_DIR/bin/fairshare"
else
    echo -e "${RED}Error: Wrapper script not found at $WRAPPER_PATH${NC}"
    exit 1
fi

# Install PolicyKit files
echo
echo "Installing PolicyKit policies..."

if [[ -f "$ASSETS_DIR/org.fairshare.policy" ]]; then
    # Update the binary path in the policy file to match installation location
    sed "s|/usr/local/libexec/fairshare-bin|$INSTALL_DIR/libexec/fairshare-bin|g" \
        "$ASSETS_DIR/org.fairshare.policy" > /tmp/org.fairshare.policy
    install -D -m 0644 /tmp/org.fairshare.policy /usr/share/polkit-1/actions/org.fairshare.policy
    rm /tmp/org.fairshare.policy
    echo -e "${GREEN}✓${NC} Installed PolicyKit action"
fi

if [[ -f "$ASSETS_DIR/50-fairshare.rules" ]]; then
    install -D -m 0644 "$ASSETS_DIR/50-fairshare.rules" /etc/polkit-1/rules.d/50-fairshare.rules
    echo -e "${GREEN}✓${NC} Installed PolicyKit rules"
fi

if [[ -f "$ASSETS_DIR/50-fairshare.pkla" ]]; then
    install -D -m 0644 "$ASSETS_DIR/50-fairshare.pkla" /var/lib/polkit-1/localauthority/10-vendor.d/50-fairshare.pkla
    echo -e "${GREEN}✓${NC} Installed PolicyKit localauthority"
fi

# Cleanup temp directory if used
if [[ "$USE_LOCAL" == false ]] && [[ -d "$TEMP_DIR" ]]; then
    rm -rf "$TEMP_DIR"
fi

# Run admin setup
echo
echo "Setting up default resource limits..."
echo -e "${YELLOW}This will configure default limits of ${DEFAULT_CPU} CPU core(s) and ${DEFAULT_MEM} GB memory per user${NC}"
echo

if "$INSTALL_DIR/bin/fairshare" admin setup --cpu "$DEFAULT_CPU" --mem "$DEFAULT_MEM"; then
    echo
    echo -e "${GREEN}✓ Installation complete!${NC}"
    echo
    echo "fairshare is now installed and configured."
    echo
    echo "Try these commands:"
    echo "  fairshare status    # View system resource usage"
    echo "  fairshare info      # View your current allocation"
    echo "  fairshare request --cpu 4 --mem 8  # Request resources"
    echo
    echo "To uninstall, run: sudo $INSTALL_DIR/bin/fairshare admin uninstall --force"
else
    echo
    echo -e "${RED}Error: Admin setup failed${NC}"
    echo "Installation may be incomplete. You can try running:"
    echo "  sudo fairshare admin setup --cpu $DEFAULT_CPU --mem $DEFAULT_MEM"
    exit 1
fi
