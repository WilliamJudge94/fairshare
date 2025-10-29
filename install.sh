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
if [[ $EUID -eq 0 ]]; then
    IS_ROOT=true
    SUDO=""
    echo "Running as root - will install directly without sudo"
else
    IS_ROOT=false
    SUDO="sudo"
    echo "This script will download and prepare fairshare for installation."
    echo "You will be shown the exact commands that need sudo before running them."
fi
echo

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

# Check for PolicyKit
NEEDS_POLKIT=false
POLKIT_INSTALL_CMD=""
POLKIT_UPDATE_CMD=""
if ! command -v pkexec &> /dev/null; then
    echo -e "${YELLOW}Warning: PolicyKit (pkexec) not found${NC}"
    echo "PolicyKit is required for fairshare to function properly."

    # Detect package manager and set install command
    if command -v apt &> /dev/null; then
        POLKIT_UPDATE_CMD="apt update"
        POLKIT_INSTALL_CMD="apt install -y policykit-1"
        echo "Will install using: sudo apt update && sudo apt install -y policykit-1"
    elif command -v dnf &> /dev/null; then
        POLKIT_INSTALL_CMD="dnf install -y polkit"
        echo "Will install using: sudo dnf install -y polkit"
    elif command -v pacman &> /dev/null; then
        POLKIT_INSTALL_CMD="pacman -S --noconfirm polkit"
        echo "Will install using: sudo pacman -S --noconfirm polkit"
    else
        echo "Could not detect package manager. Please install PolicyKit manually:"
        echo "  sudo apt install policykit-1     # Debian/Ubuntu"
        echo "  sudo dnf install polkit          # Fedora/RHEL"
        echo "  sudo pacman -S polkit            # Arch Linux"
    fi
    echo
    NEEDS_POLKIT=true
else
    echo -e "${GREEN}✓ PolicyKit found${NC}"
fi
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

# Prepare installation in staging directory
echo
echo "Preparing installation files..."

# Create staging directory
STAGING_DIR=$(mktemp -d -t fairshare-install.XXXXXXXXXX)
echo "Using staging directory: $STAGING_DIR"

# Copy binary
cp "$BINARY_PATH" "$STAGING_DIR/fairshare-bin"
chmod 0755 "$STAGING_DIR/fairshare-bin"
echo -e "${GREEN}✓${NC} Prepared binary"

# Copy wrapper
if [[ -f "$WRAPPER_PATH" ]]; then
    cp "$WRAPPER_PATH" "$STAGING_DIR/fairshare-wrapper"
    chmod 0755 "$STAGING_DIR/fairshare-wrapper"
    echo -e "${GREEN}✓${NC} Prepared wrapper script"
else
    echo -e "${RED}Error: Wrapper script not found at $WRAPPER_PATH${NC}"
    exit 1
fi

# Prepare PolicyKit files
echo -e "${GREEN}✓${NC} Preparing PolicyKit policies..."

if [[ -f "$ASSETS_DIR/org.fairshare.policy" ]]; then
    # Update the binary path in the policy file to match installation location
    sed "s|/usr/local/libexec/fairshare-bin|$INSTALL_DIR/libexec/fairshare-bin|g" \
        "$ASSETS_DIR/org.fairshare.policy" > "$STAGING_DIR/org.fairshare.policy"
    chmod 0644 "$STAGING_DIR/org.fairshare.policy"
fi

if [[ -f "$ASSETS_DIR/50-fairshare.rules" ]]; then
    cp "$ASSETS_DIR/50-fairshare.rules" "$STAGING_DIR/50-fairshare.rules"
    chmod 0644 "$STAGING_DIR/50-fairshare.rules"
fi

if [[ -f "$ASSETS_DIR/50-fairshare.pkla" ]]; then
    cp "$ASSETS_DIR/50-fairshare.pkla" "$STAGING_DIR/50-fairshare.pkla"
    chmod 0644 "$STAGING_DIR/50-fairshare.pkla"
fi

# Cleanup temp directory if used
if [[ "$USE_LOCAL" == false ]] && [[ -d "$TEMP_DIR" ]]; then
    rm -rf "$TEMP_DIR"
fi

# Display sudo commands needed
echo
echo -e "${GREEN}✓ Installation files prepared successfully!${NC}"
echo
echo "=============================="
if [[ "$IS_ROOT" == true ]]; then
    echo "Installation commands:"
else
    echo "Commands that require sudo:"
fi
echo "=============================="
echo

if [[ "$NEEDS_POLKIT" == true ]] && [[ -n "$POLKIT_INSTALL_CMD" ]]; then
    echo "# Install PolicyKit (required for fairshare to work)"
    if [[ -n "$POLKIT_UPDATE_CMD" ]]; then
        if [[ -n "$SUDO" ]]; then
            echo "sudo $POLKIT_UPDATE_CMD"
        else
            echo "$POLKIT_UPDATE_CMD"
        fi
    fi
    if [[ -n "$SUDO" ]]; then
        echo "sudo $POLKIT_INSTALL_CMD"
    else
        echo "$POLKIT_INSTALL_CMD"
    fi
    echo
fi

echo "# Install binary and wrapper"
if [[ -n "$SUDO" ]]; then
    echo "sudo install -D -m 0755 $STAGING_DIR/fairshare-bin $INSTALL_DIR/libexec/fairshare-bin"
    echo "sudo install -D -m 0755 $STAGING_DIR/fairshare-wrapper $INSTALL_DIR/bin/fairshare"
else
    echo "install -D -m 0755 $STAGING_DIR/fairshare-bin $INSTALL_DIR/libexec/fairshare-bin"
    echo "install -D -m 0755 $STAGING_DIR/fairshare-wrapper $INSTALL_DIR/bin/fairshare"
fi
echo

if [[ -f "$STAGING_DIR/org.fairshare.policy" ]]; then
    echo "# Install PolicyKit policy"
    if [[ -n "$SUDO" ]]; then
        echo "sudo install -D -m 0644 $STAGING_DIR/org.fairshare.policy /usr/share/polkit-1/actions/org.fairshare.policy"
    else
        echo "install -D -m 0644 $STAGING_DIR/org.fairshare.policy /usr/share/polkit-1/actions/org.fairshare.policy"
    fi
fi

if [[ -f "$STAGING_DIR/50-fairshare.rules" ]]; then
    if [[ -n "$SUDO" ]]; then
        echo "sudo install -D -m 0644 $STAGING_DIR/50-fairshare.rules /etc/polkit-1/rules.d/50-fairshare.rules"
    else
        echo "install -D -m 0644 $STAGING_DIR/50-fairshare.rules /etc/polkit-1/rules.d/50-fairshare.rules"
    fi
fi

if [[ -f "$STAGING_DIR/50-fairshare.pkla" ]]; then
    if [[ -n "$SUDO" ]]; then
        echo "sudo install -D -m 0644 $STAGING_DIR/50-fairshare.pkla /var/lib/polkit-1/localauthority/10-vendor.d/50-fairshare.pkla"
    else
        echo "install -D -m 0644 $STAGING_DIR/50-fairshare.pkla /var/lib/polkit-1/localauthority/10-vendor.d/50-fairshare.pkla"
    fi
fi

echo
echo "# Configure default resource limits"
if [[ -n "$SUDO" ]]; then
    echo "sudo $INSTALL_DIR/bin/fairshare admin setup --cpu $DEFAULT_CPU --mem $DEFAULT_MEM"
else
    echo "$INSTALL_DIR/bin/fairshare admin setup --cpu $DEFAULT_CPU --mem $DEFAULT_MEM"
fi
echo
echo "=============================="
echo

# Optionally run them
# Check if stdin is a terminal (not piped from curl)
if [ -t 0 ]; then
    read -p "Run these commands now? (y/n): " -n 1 -r </dev/tty
    echo
else
    echo "Script is being piped - skipping automatic execution."
    echo "Please run the commands above manually, or download and run the script directly:"
    echo "  wget https://raw.githubusercontent.com/$REPO/main/install.sh"
    echo "  bash install.sh"
    REPLY="n"
fi

if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo
    echo "Running installation commands..."
    echo

    # Install PolicyKit if needed
    if [[ "$NEEDS_POLKIT" == true ]] && [[ -n "$POLKIT_INSTALL_CMD" ]]; then
        if [[ -n "$POLKIT_UPDATE_CMD" ]]; then
            echo "Updating package lists..."
            $SUDO $POLKIT_UPDATE_CMD || {
                echo -e "${RED}Error: Failed to update package lists${NC}"
                echo "Please check your internet connection and try again."
                exit 1
            }
            echo -e "${GREEN}✓${NC} Updated package lists"
        fi

        echo "Installing PolicyKit..."
        $SUDO $POLKIT_INSTALL_CMD || {
            echo -e "${RED}Error: Failed to install PolicyKit${NC}"
            echo "Please install PolicyKit manually and re-run this script."
            exit 1
        }
        echo -e "${GREEN}✓${NC} Installed PolicyKit"
    fi

    $SUDO install -D -m 0755 "$STAGING_DIR/fairshare-bin" "$INSTALL_DIR/libexec/fairshare-bin" || {
        echo -e "${RED}Error: Failed to install binary${NC}"
        exit 1
    }
    echo -e "${GREEN}✓${NC} Installed binary to $INSTALL_DIR/libexec/fairshare-bin"

    $SUDO install -D -m 0755 "$STAGING_DIR/fairshare-wrapper" "$INSTALL_DIR/bin/fairshare" || {
        echo -e "${RED}Error: Failed to install wrapper${NC}"
        exit 1
    }
    echo -e "${GREEN}✓${NC} Installed wrapper to $INSTALL_DIR/bin/fairshare"

    if [[ -f "$STAGING_DIR/org.fairshare.policy" ]]; then
        $SUDO install -D -m 0644 "$STAGING_DIR/org.fairshare.policy" /usr/share/polkit-1/actions/org.fairshare.policy || {
            echo -e "${RED}Error: Failed to install PolicyKit action${NC}"
            exit 1
        }
        echo -e "${GREEN}✓${NC} Installed PolicyKit action"
    fi

    if [[ -f "$STAGING_DIR/50-fairshare.rules" ]]; then
        $SUDO install -D -m 0644 "$STAGING_DIR/50-fairshare.rules" /etc/polkit-1/rules.d/50-fairshare.rules || {
            echo -e "${RED}Error: Failed to install PolicyKit rules${NC}"
            exit 1
        }
        echo -e "${GREEN}✓${NC} Installed PolicyKit rules"
    fi

    if [[ -f "$STAGING_DIR/50-fairshare.pkla" ]]; then
        $SUDO install -D -m 0644 "$STAGING_DIR/50-fairshare.pkla" /var/lib/polkit-1/localauthority/10-vendor.d/50-fairshare.pkla || {
            echo -e "${RED}Error: Failed to install PolicyKit localauthority${NC}"
            exit 1
        }
        echo -e "${GREEN}✓${NC} Installed PolicyKit localauthority"
    fi

    echo
    echo "Setting up default resource limits..."
    if $SUDO "$INSTALL_DIR/bin/fairshare" admin setup --cpu "$DEFAULT_CPU" --mem "$DEFAULT_MEM"; then
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
        if [[ "$IS_ROOT" == true ]]; then
            echo "To uninstall, run: fairshare admin uninstall --force"
        else
            echo "To uninstall, run: sudo fairshare admin uninstall --force"
        fi
    else
        echo
        echo -e "${RED}Error: Admin setup failed${NC}"
        echo "Installation may be incomplete. You can try running:"
        echo "  $SUDO fairshare admin setup --cpu $DEFAULT_CPU --mem $DEFAULT_MEM"
        exit 1
    fi

    # Cleanup staging directory
    rm -rf "$STAGING_DIR"
else
    echo
    echo "Installation files are ready in: $STAGING_DIR"
    echo
    echo "To complete installation later, run the commands shown above."
    echo "The staging directory will be automatically cleaned up on next reboot."
    echo
fi
