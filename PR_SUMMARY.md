# Fix GLIBC 2.39 Compatibility Issue for RHEL 9

## Problem

Users on RHEL 9 and other systems with older GLIBC versions were unable to run fairshare binaries:

```bash
./fairshare-x86_64: /lib64/libc.so.6: version `GLIBC_2.39' not found
```

This occurred because binaries were being built on Ubuntu 24.04, which uses GLIBC 2.39, but RHEL 9 only has GLIBC 2.34.

## Solution

The release workflow now builds binaries in a **Debian 11 (Bullseye) Docker container** with **GLIBC 2.31**, ensuring maximum compatibility with older Linux distributions.

## What Changed

### 1. New Build Environment (`Dockerfile.release`)
- Based on Debian 11 with GLIBC 2.31
- Includes Rust toolchain and cross-compilation tools
- Supports both x86_64 and aarch64 builds

### 2. Updated Release Workflow (`.github/workflows/release.yml`)
- Builds Docker image from `Dockerfile.release`
- Compiles binaries inside the container
- Ensures consistent GLIBC 2.31 linking

### 3. Documentation Updates
- **README.md**: Added compatibility section listing supported distributions
- **RELEASE.md**: Added Docker build instructions and GLIBC compatibility notes
- **CHANGELOG.md**: Created changelog documenting the fix

### 4. Version Bump
- Updated to version **0.9.0** to reflect this significant improvement

### 5. Test Script
- Added `test-glibc-compat.sh` to verify binary GLIBC requirements

## Compatibility Matrix

Binaries built with this approach work on:

| Distribution | GLIBC Version | Status |
|--------------|---------------|--------|
| RHEL 9+ | 2.34 | ✅ Compatible |
| Debian 11+ | 2.31 | ✅ Compatible |
| Ubuntu 20.04+ | 2.31 | ✅ Compatible |
| Rocky Linux 9+ | 2.34 | ✅ Compatible |
| AlmaLinux 9+ | 2.34 | ✅ Compatible |

## Testing

To verify the GLIBC compatibility of built binaries:

```bash
./test-glibc-compat.sh
```

This script checks:
- Maximum GLIBC version required
- Binary file information
- All GLIBC versions referenced
- Compatibility with target systems

## Building Locally

### Option 1: Using Docker (Recommended)
```bash
docker build -t fairshare-builder -f Dockerfile.release .
docker run --rm -v $(pwd):/build -w /build fairshare-builder bash -c "
  mkdir -p .cargo
  echo '[target.aarch64-unknown-linux-gnu]' > .cargo/config.toml
  echo 'linker = \"aarch64-linux-gnu-gcc\"' >> .cargo/config.toml
  make compile-releases
"
```

### Option 2: Local Build (Uses Host GLIBC)
```bash
make compile-releases
```

## Files Changed

- `.github/workflows/release.yml` - Updated build process
- `Cargo.toml` - Version bump to 0.9.0
- `CHANGELOG.md` - New changelog
- `Dockerfile.release` - New build container
- `README.md` - Added compatibility information
- `RELEASE.md` - Added Docker build instructions
- `test-glibc-compat.sh` - New test script

## References

- Issue: #[issue_number]
- RHEL 9 GLIBC version: 2.34
- Debian 11 GLIBC version: 2.31
- Ubuntu 24.04 GLIBC version: 2.39

## Credits

Thanks to [@harsha89](https://github.com/harsha89) for reporting this issue and providing the initial workaround using the Zig toolchain.
