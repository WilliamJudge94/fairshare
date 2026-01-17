# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0] - 2026-01-17

### Fixed
- **GLIBC Compatibility**: Release binaries are now built in a Debian 11 container with GLIBC 2.31 instead of Ubuntu 24.04 with GLIBC 2.39
  - Fixes `GLIBC_2.39' not found` error on RHEL 9 and other older systems
  - Binaries now work on any Linux distribution with GLIBC 2.31+ including:
    - RHEL 9+ (GLIBC 2.34)
    - Debian 11+ (GLIBC 2.31)
    - Ubuntu 20.04+ (GLIBC 2.31)
    - Rocky Linux 9+ (GLIBC 2.34)
    - AlmaLinux 9+ (GLIBC 2.34)

### Added
- `Dockerfile.release` for building binaries with GLIBC 2.31 compatibility
- Documentation about GLIBC compatibility in README.md and RELEASE.md

### Changed
- GitHub Actions release workflow now builds binaries in Docker container for consistent GLIBC 2.31 linking

## [0.8.0] - Previous Release

### Added
- Admin force-set user resources command
- Flexible user identification (username or UID)
- Resource availability checking with warning prompts
- Safety checks (rejects root and system users)

