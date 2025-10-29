# Creating a GitHub Release

This document explains how to create a GitHub release with the pre-built binaries.

## Version 0.3.0 Changes

**New Features:**
- Added CPU reserve parameter (`--cpu-reserve`, default: 2 CPUs)
- Added memory reserve parameter (`--mem-reserve`, default: 4GB)
- System reserves are now subtracted from available resources for allocation
- Improved Makefile with simplified cross-compilation workflow

**What CPU/Memory Reserves Do:**
Reserves ensure that a portion of system resources are kept for the operating system and background processes. Users can only allocate up to (Total - Reserved) resources.

Example: On a system with 8 CPUs and 16GB RAM with defaults:
- Total: 8 CPUs, 16 GB
- Reserved: 2 CPUs, 4 GB
- Available for users: 6 CPUs, 12 GB

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
# Output: fairshare 0.3.0

# Check architecture
readelf -h releases/fairshare-x86_64 | grep Machine
readelf -h releases/fairshare-aarch64 | grep Machine

# Test help command with new reserve parameters
./releases/fairshare-x86_64 admin setup --help
```

You should see the new `--cpu-reserve` and `--mem-reserve` options in the help output.

## Step 3: Create the Release on GitHub

1. Go to https://github.com/WilliamJudge94/fairshare/releases
2. Click "Draft a new release"
3. Choose a tag version: `v0.3.0`
4. Set the release title: "fairshare v0.3.0 - System Resource Reserves"
5. Write release notes (see example below)
6. Upload the following files as release assets:
   - `releases/fairshare-x86_64`
   - `releases/fairshare-aarch64`
   - `releases/SHA256SUMS`
7. Click "Publish release"

### Example Release Notes

```markdown
# fairshare v0.3.0 - System Resource Reserves

## New Features

### System Resource Reserves
You can now reserve CPU and memory for the operating system and background processes. This ensures critical system services always have resources available.

```bash
# Setup with custom reserves (2 CPUs and 4GB RAM reserved by default)
sudo fairshare admin setup --cpu 1 --mem 2 --cpu-reserve 2 --mem-reserve 4

# The status command now shows reserved resources
fairshare status
```

**Example Output:**
```
╔═══════════════════════════════════════╗
║      SYSTEM RESOURCE OVERVIEW         ║
╚═══════════════════════════════════════╝

┌──────────────────┬──────┬──────────┐
│ Metric           │ CPUs │ RAM (GB) │
├──────────────────┼──────┼──────────┤
│ Total            │ 8    │ 16.00    │
│ Reserved (System)│ 2.00 │ 4.00     │
│ Allocated        │ 4.00 │ 6.00     │
│ Available        │ 2.00 │ 6.00     │
└──────────────────┴──────┴──────────┘
```

Users can now only allocate up to (Total - Reserved) resources, ensuring system stability.

## Improvements

- Simplified Makefile for easier cross-compilation
- Better error messages and user feedback
- Updated documentation for new reserve features

## Installation

### Quick Install
```bash
curl -sSL https://raw.githubusercontent.com/WilliamJudge94/fairshare/main/install.sh | sudo bash
```

### Verify Installation
```bash
fairshare --version  # Should show: fairshare 0.3.0
```

## Checksums

Verify your download:
```bash
sha256sum -c SHA256SUMS
```

## Full Changelog

See [CHANGELOG.md](CHANGELOG.md) for complete details.
```

## Step 4: Test the Installation

After creating the release, test that users can install it:

```bash
# Test the install script
curl -sSL https://raw.githubusercontent.com/WilliamJudge94/fairshare/main/install.sh | sudo bash

# Verify version
fairshare --version  # Should show: fairshare 0.3.0

# Test new reserve feature
sudo fairshare admin setup --cpu 1 --mem 2 --cpu-reserve 2 --mem-reserve 4
fairshare status  # Should show reserved resources row
```

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
