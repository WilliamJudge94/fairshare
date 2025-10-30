# Creating a GitHub Release

This document explains how to create a GitHub release with the pre-built binaries.

## Version 0.3.1 Changes

**Installation Improvements:**
- The installation script now automatically detects if PolicyKit is missing
- Automatically installs PolicyKit on apt/dnf/pacman-based systems
- For Debian/Ubuntu systems, runs `apt update` before installing PolicyKit
- Shows PolicyKit installation commands in the "Commands that require sudo" section
- Users no longer need to manually install PolicyKit before running the installer

**Documentation Updates:**
- Updated README.md with PolicyKit auto-installation information
- Updated CLAUDE.md with installation and troubleshooting guidance
- Added new troubleshooting section for PolicyKit issues

**What This Fixes:**
Previously, users had to manually install PolicyKit before fairshare would work. Now the installer handles this automatically, providing a smoother installation experience.

## Step 1: Build the Binaries

### Using the Makefile (Recommended)

The simplest way to build both architectures:

```bash
# One-time setup (installs cross-compilation tools)
sudo make setup-cross

# Build both x86_64 and aarch64 binaries
make compile-releases
```

This creates:
```
releases/
├── fairshare-x86_64    # Intel/AMD 64-bit (~1.9 MB, stripped)
├── fairshare-aarch64   # ARM 64-bit (~1.6 MB, stripped)
└── SHA256SUMS          # Checksums for verification
```

### Build x86_64 Only (Faster)

If you only need x86_64:

```bash
make compile-x86_64
```

### Manual Build (Advanced)

If you prefer to build manually:

```bash
# Setup (one-time)
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
sudo apt-get update && sudo apt-get install -y gcc-aarch64-linux-gnu

# Configure cargo
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
EOF

# Build x86_64
cargo build --release --target x86_64-unknown-linux-gnu
cp target/x86_64-unknown-linux-gnu/release/fairshare releases/fairshare-x86_64
strip releases/fairshare-x86_64

# Build aarch64
cargo build --release --target aarch64-unknown-linux-gnu
cp target/aarch64-unknown-linux-gnu/release/fairshare releases/fairshare-aarch64
aarch64-linux-gnu-strip releases/fairshare-aarch64

# Generate checksums
cd releases && sha256sum fairshare-* > SHA256SUMS
```

## Step 2: Verify the Binaries

Test that the binaries work correctly:

```bash
# Check version
./releases/fairshare-x86_64 --version
# Output: fairshare 0.3.1

# Check architecture
readelf -h releases/fairshare-x86_64 | grep Machine
readelf -h releases/fairshare-aarch64 | grep Machine

# Test help command
./releases/fairshare-x86_64 --help
```

## Step 3: Create the Release on GitHub

1. Go to https://github.com/WilliamJudge94/fairshare/releases
2. Click "Draft a new release"
3. Choose a tag version: `v0.3.1`
4. Set the release title: "fairshare v0.3.1 - Improved Installation"
5. Write release notes (see example below)
6. Upload the following files as release assets:
   - `releases/fairshare-x86_64`
   - `releases/fairshare-aarch64`
   - `releases/SHA256SUMS`
7. Click "Publish release"

### Example Release Notes

```markdown
# fairshare v0.3.1 - Improved Installation

## What's New

### Automatic PolicyKit Installation

The installation script now automatically handles PolicyKit installation, making setup even easier:

```bash
# Just run the installer - it handles everything
curl -sSL https://raw.github.com/WilliamJudge94/fairshare/main/install.sh | bash
```

**What the installer does:**
- Detects if PolicyKit (pkexec) is installed
- If missing, automatically installs it using your package manager:
  - Debian/Ubuntu: Runs `apt update` then `apt install policykit-1`
  - Fedora/RHEL: Runs `dnf install polkit`
  - Arch Linux: Runs `pacman -S polkit`
- Shows all commands that will be run before executing them
- Continues with fairshare installation

**Before (v0.3.0):**
```bash
# Users had to manually install PolicyKit first
sudo apt install policykit-1
curl -sSL https://raw.github.com/WilliamJudge94/fairshare/main/install.sh | bash
```

**Now (v0.3.1):**
```bash
# Installer handles everything automatically
curl -sSL https://raw.github.com/WilliamJudge94/fairshare/main/install.sh | bash
```

### Documentation Improvements

- Added PolicyKit auto-installation details to README.md
- Updated CLAUDE.md with comprehensive installation guidance
- Added troubleshooting section for PolicyKit issues
- Clarified manual installation steps for users building from source

## Bug Fixes

- None (this is a patch release focused on installation improvements)

## Installation

### Quick Install (Recommended)
```bash
curl -sSL https://raw.github.com/WilliamJudge94/fairshare/main/install.sh | bash
```

### Build from Source
```bash
cargo build --release
bash install.sh
```

### Verify Installation
```bash
fairshare --version  # Should show: fairshare 0.3.1
```

## Checksums

Verify your download:
```bash
sha256sum -c SHA256SUMS
```

## Full Changelog

### Changed
- Installation script now automatically detects and installs PolicyKit if missing
- For apt-based systems, runs `apt update` before installing PolicyKit
- Updated documentation to reflect automatic PolicyKit installation

### Added
- New troubleshooting section for PolicyKit issues
- Clear guidance for manual PolicyKit installation

## Upgrading from v0.3.0

Simply run the new installer or build from source. No configuration changes needed:

```bash
curl -sSL https://raw.github.com/WilliamJudge94/fairshare/main/install.sh | bash
```
```

## Step 4: Test the Installation

After creating the release, test that users can install it:

```bash
# Test the install script (on a system without PolicyKit if possible)
curl -sSL https://raw.github.com/WilliamJudge94/fairshare/main/install.sh | bash

# Verify version
fairshare --version  # Should show: fairshare 0.3.1

# Test basic functionality
fairshare status
fairshare info
```

## Step 5: Update Version References

After the release, update version numbers in the codebase for the next release:

1. Update `Cargo.toml` version to `0.3.2` (or `0.4.0` depending on next changes)
2. Update any hardcoded version strings in documentation
3. Commit with message: "chore: bump version to 0.3.2-dev"

## Automated Releases (Future)

Consider setting up GitHub Actions to automatically build and release binaries when you create a new tag.

Example workflow file (`.github/workflows/release.yml`):
```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Setup cross-compilation
        run: sudo make setup-cross

      - name: Build releases
        run: make compile-releases

      - name: Create Release
        uses: softprops/action-gh-release@v1
        with:
          files: |
            releases/fairshare-x86_64
            releases/fairshare-aarch64
            releases/SHA256SUMS
```

This eliminates manual building and ensures consistent releases.
