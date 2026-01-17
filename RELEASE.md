# Creating a GitHub Release

This document explains how to create a GitHub release with the pre-built binaries.

## Version 0.6.0 Changes

**New Admin Command: Force-Set User Resources**

Administrators can now directly set CPU and memory allocations for any user on the system, even if that user is not currently logged in:

```bash
# Set resources by username
sudo fairshare admin set-user --user alice --cpu 4 --mem 8

# Set resources by UID
sudo fairshare admin set-user --user 1000 --cpu 2 --mem 4

# Force set without confirmation prompt (for scripts)
sudo fairshare admin set-user --user bob --cpu 10 --mem 20 --force
```

**Features:**
- Accepts either username or UID for the `--user` parameter
- Validates that the user exists on the system
- Rejects modifications to root (UID 0) and system users (UID < 1000) for safety
- Checks resource availability using delta-based checking (same as regular user requests)
- Displays a warning prompt if the allocation would exceed available resources
- Optional `--force` flag to skip warning prompts for automated scripts
- Works even when the target user is signed out (modifies systemd user slice directly)

**Implementation Details:**
- Added `get_uid_from_user_string()` helper function in `system.rs` to convert username/UID to UID
- Added `admin_set_user_limits()` function in `systemd.rs` with full validation and safety checks
- Added resource availability checking with interactive warning prompts
- Comprehensive test coverage (13 new tests added)
- Updated documentation in README.md and CLAUDE.md

**Use Cases:**
- System administrators managing multi-user shared systems
- Automated resource allocation scripts
- Pre-allocating resources for specific users before they log in
- Adjusting user allocations without requiring the user to be logged in

## Step 1: Build the Binaries

### GLIBC Compatibility

**Important**: Starting from version 0.9.0, release binaries are built in a Debian 11 (Bullseye) container with GLIBC 2.31 to ensure maximum compatibility across Linux distributions.

This fixes the issue where binaries built on Ubuntu 24.04 (GLIBC 2.39) would not run on older systems like RHEL 9 (GLIBC 2.34).

**Compatible Systems:**
- RHEL 9+ (GLIBC 2.34)
- Debian 11+ (GLIBC 2.31)
- Ubuntu 20.04+ (GLIBC 2.31)
- Rocky Linux 9+ (GLIBC 2.34)
- AlmaLinux 9+ (GLIBC 2.34)
- Any distribution with GLIBC 2.31 or newer

### Using the Makefile (Recommended)

The simplest way to build both architectures:

```bash
# One-time setup (installs cross-compilation tools)
sudo make setup-cross

# Build both x86_64 and aarch64 binaries
make compile-releases
```

**Note**: The Makefile builds on your local system. For maximum compatibility, the GitHub Actions workflow uses Docker to build in a Debian 11 environment. See "Using Docker for GLIBC Compatibility" below.

This creates:
```
releases/
├── fairshare-x86_64    # Intel/AMD 64-bit (~1.9 MB, stripped)
├── fairshare-aarch64   # ARM 64-bit (~1.6 MB, stripped)
└── SHA256SUMS          # Checksums for verification
```

### Using Docker for GLIBC Compatibility (Recommended for Releases)

To ensure your binaries work on older systems, build inside the Debian 11 container:

```bash
# Build the Docker image
docker build -t fairshare-builder -f Dockerfile.release .

# Build binaries in the container
docker run --rm \
  -v $(pwd):/build \
  -w /build \
  fairshare-builder \
  bash -c "
    mkdir -p .cargo
    echo '[target.aarch64-unknown-linux-gnu]' > .cargo/config.toml
    echo 'linker = \"aarch64-linux-gnu-gcc\"' >> .cargo/config.toml
    make compile-releases
  "
```

This matches the GitHub Actions build environment and ensures GLIBC 2.31 compatibility.

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
# Output: fairshare 0.6.0

# Check architecture
readelf -h releases/fairshare-x86_64 | grep Machine
readelf -h releases/fairshare-aarch64 | grep Machine

# Test help command
./releases/fairshare-x86_64 --help
```

## Step 3: Create the Release on GitHub

1. Go to https://github.com/WilliamJudge94/fairshare/releases
2. Click "Draft a new release"
3. Choose a tag version: `v0.6.0`
4. Set the release title: "fairshare v0.6.0 - Admin Force-Set User Resources"
5. Write release notes (see example below)
6. Upload the following files as release assets:
   - `releases/fairshare-x86_64`
   - `releases/fairshare-aarch64`
   - `releases/SHA256SUMS`
7. Click "Publish release"

### Example Release Notes

```markdown
# fairshare v0.6.0 - Admin Force-Set User Resources

## What's New

### New Admin Command: Force-Set User Resources

Administrators can now directly set CPU and memory allocations for any user on the system, even if that user is not currently logged in:

```bash
# Set resources by username
sudo fairshare admin set-user --user alice --cpu 4 --mem 8

# Set resources by UID
sudo fairshare admin set-user --user 1000 --cpu 2 --mem 4

# Force set without confirmation prompt (for scripts)
sudo fairshare admin set-user --user bob --cpu 10 --mem 20 --force
```

**Key Features:**
- **Flexible User Identification**: Accepts either username (e.g., "alice") or UID (e.g., "1000")
- **Resource Availability Checking**: Warns if allocation exceeds available resources and prompts for confirmation
- **Safety First**: Cannot modify root (UID 0) or system users (UID < 1000)
- **Offline Users**: Works even when the target user is not logged in
- **Automation-Friendly**: Optional `--force` flag skips confirmation prompts for scripts

**How It Works:**
When an admin sets resources that would exceed available capacity:
1. Displays warning: "WARNING: This allocation exceeds available system resources!"
2. Warns about potential resource contention or system instability
3. Prompts for confirmation: "Proceed anyway? [y/N]"
4. Only proceeds if admin confirms with 'y' or 'yes'
5. With `--force` flag, skips prompt but still displays a warning

**Use Cases:**
- System administrators managing multi-user shared systems
- Automated resource allocation scripts
- Pre-allocating resources for specific users before they log in
- Adjusting user allocations without requiring the user to be logged in

### Technical Details

- Added comprehensive input validation and safety checks
- Implements delta-based resource checking (same as regular user requests)
- Full test coverage with 13 new tests
- Updated documentation across README.md and CLAUDE.md

## Bug Fixes

- None (this is a feature release)

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
fairshare --version  # Should show: fairshare 0.6.0
```

## Checksums

Verify your download:
```bash
sha256sum -c SHA256SUMS
```

## Full Changelog

### Added
- **New admin command**: `fairshare admin set-user` to force-set resources for any user
- Flexible user identification (accepts username or UID)
- Resource availability checking with warning prompts for over-allocation
- Optional `--force` flag to skip confirmation prompts
- Comprehensive validation and safety checks (rejects root and system users)
- 13 new unit tests for the new functionality

### Changed
- Updated README.md with new admin command documentation
- Updated CLAUDE.md with implementation details and usage examples

## Upgrading from v0.5.0

Simply run the new installer or build from source. No configuration changes needed:

```bash
curl -sSL https://raw.github.com/WilliamJudge94/fairshare/main/install.sh | bash
```

**New Command Available After Upgrade:**
```bash
sudo fairshare admin set-user --user <username|UID> --cpu <N> --mem <N>
```
```

## Step 4: Test the Installation

After creating the release, test that users can install it:

```bash
# Test the install script
curl -sSL https://raw.github.com/WilliamJudge94/fairshare/main/install.sh | bash

# Verify version
fairshare --version  # Should show: fairshare 0.6.0

# Test basic functionality
fairshare status
fairshare info

# Test new admin command (requires sudo and a valid username)
sudo fairshare admin set-user --user testuser --cpu 2 --mem 4
```

## Step 5: Update Version References

After the release, update version numbers in the codebase for the next release:

1. Update `Cargo.toml` version to `0.7.0` (or appropriate next version)
2. Update any hardcoded version strings in documentation
3. Commit with message: "chore: bump version to 0.7.0-dev"

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
