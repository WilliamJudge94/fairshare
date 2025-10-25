# Creating a GitHub Release

This document explains how to create a GitHub release with the pre-built binaries.

## Step 1: Build the Binaries

The binaries are already built and located in the `releases/` directory:

```bash
releases/
├── fairshare-x86_64    # Intel/AMD 64-bit
├── fairshare-aarch64   # ARM 64-bit (Apple Silicon, Raspberry Pi, etc.)
└── SHA256SUMS          # Checksums for verification
```

To rebuild them yourself:

```bash
# Build x86_64
cargo build --release
cp target/release/fairshare releases/fairshare-x86_64

# Build aarch64 (requires cross-compilation setup)
rustup target add aarch64-unknown-linux-gnu
apt-get install gcc-aarch64-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
cp target/aarch64-unknown-linux-gnu/release/fairshare releases/fairshare-aarch64

# Generate checksums
sha256sum releases/fairshare-* > releases/SHA256SUMS
```

## Step 2: Create the Release on GitHub

1. Go to https://github.com/WilliamJudge94/fairshare/releases
2. Click "Draft a new release"
3. Choose a tag version (e.g., `v0.2.0`)
4. Set the release title (e.g., "fairshare v0.2.0")
5. Write release notes describing changes
6. Upload the following files as release assets:
   - `releases/fairshare-x86_64`
   - `releases/fairshare-aarch64`
   - `releases/SHA256SUMS`

## Step 3: Test the Installation

After creating the release, test that users can install it:

```bash
# Test the install script
curl -sSL https://raw.githubusercontent.com/WilliamJudge94/fairshare/main/install.sh | sudo bash

# Or download and inspect first
wget https://raw.githubusercontent.com/WilliamJudge94/fairshare/main/install.sh
sudo bash install.sh
```

## Automated Releases (Future)

Consider setting up GitHub Actions to automatically build and release binaries when you create a new tag. This would eliminate manual building.

Example workflow file (`.github/workflows/release.yml`):
- Trigger on tag push (e.g., `v*`)
- Build for multiple architectures
- Automatically create GitHub release
- Upload binaries as release assets

This ensures consistent builds and makes the release process much easier.
