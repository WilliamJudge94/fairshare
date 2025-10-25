# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

**fairshare** is a Rust-based systemd resource manager for multi-user Linux systems. It provides fair CPU and memory allocation management using systemd user slices, allowing users to request resources dynamically while preventing over-allocation.

## Installation

### Building and Installing

```bash
# Build release binary
cargo build --release

# Install wrapper and binary (requires sudo)
sudo make release

# Setup admin defaults and PolicyKit policies (REQUIRED for user commands)
sudo fairshare admin setup --cpu 1 --mem 2
```

### Installation Paths

- **Wrapper script**: `/usr/local/bin/fairshare` (what users run)
- **Real binary**: `/usr/local/libexec/fairshare-bin` (called by wrapper)
- **PolicyKit policies**: Installed via `admin setup` command
- **Systemd configuration**: `/etc/systemd/system/user-.slice.d/00-defaults.conf`

The wrapper script auto-detects the binary location, supporting both:
- Package installation: `/usr/libexec/fairshare-bin`
- Local installation: `/usr/local/libexec/fairshare-bin`

## Development Commands

### Building
- **Debug build**: `cargo build`
- **Release build**: `cargo build --release`
- **Install wrapper and binary**: `sudo make release` (installs to `/usr/local/bin/fairshare` and `/usr/local/libexec/fairshare-bin`)

### Testing
- **Run all tests**: `cargo test`
- **Run tests with output**: `cargo test -- --nocapture`
- **Run a specific test**: `cargo test test_cli_help`
- **Run integration tests only**: `cargo test --test integration_tests`
- **Run CLI tests only**: `cargo test --test cli_tests`

### Code Quality
- **Check code**: `cargo check`
- **Format code**: `cargo fmt`
- **Lint code**: `cargo clippy`

### Running
- **Show help**: `cargo run -- --help`
- **Show status**: `fairshare status` (wrapper handles pkexec automatically)
- **Request resources**: `fairshare request --cpu 4 --mem 8`
- **Show user info**: `fairshare info`
- **Release resources**: `fairshare release`
- **Admin setup** (requires root): `sudo fairshare admin setup --cpu 1 --mem 2`

**Note**: Regular user commands automatically use pkexec via the wrapper script at `/usr/local/bin/fairshare`. The real binary is at `/usr/local/libexec/fairshare-bin`. Admin commands require `sudo`.

## Architecture Overview

### Wrapper Architecture

The user-facing `fairshare` command is a shell script wrapper that:
1. Auto-detects binary location (supports package and local installation)
2. Detects admin commands (first arg is "admin") → executes directly (requires sudo)
3. Regular user commands → transparently calls `pkexec /usr/local/libexec/fairshare-bin`
4. Provides a simple UX without requiring users to type `pkexec`

This pattern is used by many system tools for privilege escalation (e.g., `systemctl`, `nmcli`).

### Module Structure

The codebase is organized into four main modules:

1. **`src/main.rs`** - Entry point that routes commands to appropriate handlers
2. **`src/cli.rs`** - Command-line interface definitions using `clap` with validation constraints:
   - CPU range: 1-1000 cores
   - Memory range: 1-10000 GB
3. **`src/system.rs`** - System information gathering and resource availability checking
4. **`src/systemd.rs`** - Systemd interaction for applying/reverting resource limits

### Core Data Flow

1. **Wrapper Detection** (shell script): Detects command type and invokes pkexec for user commands
2. **Command Parsing** (`cli.rs`): Clap validates input bounds before execution
3. **Resource Validation** (`system.rs`): Checks if requested resources are available
4. **Systemd Configuration** (`systemd.rs`): Applies limits via `systemctl set-property` or reverts via `systemctl revert`

### Key Functions

#### System Information (`system.rs`)
- `get_system_totals()` - Returns total system CPU and memory (uses `sysinfo` crate)
- `get_user_allocations()` - Queries systemd directly for all user slice allocations (systemd is the source of truth)
- `get_user_allocations_from_systemd()` - Reads user slice properties via `systemctl list-units` and `systemctl show`
- `check_request()` - Validates if requested resources are available using **delta-based checking** (considers existing user allocation)
- `print_status()` - Displays formatted system and per-user resource overview

#### Resource Management (`systemd.rs`)
- `get_calling_user_uid()` - Retrieves the UID of the user who invoked pkexec (reads `PKEXEC_UID` environment variable)
- `set_user_limits()` - Applies CPU quota and memory limits via `systemctl set-property` on the calling user's slice
- `release_user_limits()` - Reverts limits back to defaults via `systemctl revert` on the calling user's slice
- `show_user_info()` - Displays current user's resource allocation
- `admin_setup_defaults()` - Creates systemd config at `/etc/systemd/system/user-.slice.d/00-defaults.conf` and installs PolicyKit policies
- `admin_uninstall_defaults()` - Removes admin configuration files, PolicyKit policies, and reloads systemd

### Resource Units

- **CPU**: Represented as percentage quota (100% = 1 core, 400% = 4 cores)
- **Memory**: Stored internally as bytes (converted from GB: `GB * 1_000_000_000`)
- **Slice Names**: Format `user-1000.slice` (UID-based systemd user slices)

## Testing Strategy

### Unit Tests
Located in source modules (`src/system.rs` and `src/systemd.rs`), these test:
- Memory parsing (GB, MB formats)
- Resource availability checking
- CPU quota calculations
- Configuration format generation

Run with: `cargo test --lib`

### Integration Tests
Two test suites validate end-to-end functionality:

**`tests/cli_tests.rs`** - Comprehensive CLI validation:
- Help and version output
- Command parsing and validation
- Input bounds (min 1, max 1000 for CPU; min 1, max 10000 for memory)
- Admin setup/uninstall workflows

**`tests/integration_tests.rs`** - Full workflow tests:
- Help → Status command flow
- Request validation with various constraints
- Multiple command execution
- Admin setup documentation

### Running Tests
- Full suite: `cargo test`
- With output: `cargo test -- --nocapture --test-threads=1`
- Specific test file: `cargo test --test cli_tests`

## Configuration

### Runtime Configuration Files

Created by `admin setup`:

1. **`/etc/systemd/system/user-.slice.d/00-defaults.conf`**
   - Sets CPUQuota and MemoryMax for all user slices
   - Format: systemd slice configuration
   - Requires: `systemctl daemon-reload` after modification

2. **`/etc/fairshare/policy.toml`**
   - Policy configuration (currently placeholder for future features)
   - Stores default CPU/memory and max caps

### Input Validation

Configured in `src/cli.rs`:
- `MIN_CPU = 1`, `MAX_CPU = 1000`
- `MIN_MEM = 1`, `MAX_MEM = 10000`
- Validation enforced by `clap`'s `RangedU64ValueParser`

## Important Implementation Details

### User Privileges and pkexec Integration

The tool uses **pkexec** (PolicyKit) for privilege escalation, allowing regular users to modify their own resource limits without full root access:

**Regular users**:
- Run commands via wrapper: `fairshare status` (wrapper transparently calls `pkexec /usr/local/libexec/fairshare-bin status`)
- pkexec grants root privileges but preserves the calling user's UID in `PKEXEC_UID` environment variable
- Commands modify system-level user slices: `systemctl set-property user-{UID}.slice`
- PolicyKit policies allow users to manage their own resources without entering admin password
- Cannot affect other users' allocations

**Root/Admin users**:
- Run admin commands with `sudo` (e.g., `sudo fairshare admin setup --cpu 1 --mem 2`)
- Create/modify global defaults in `/etc/systemd/system/`
- Install PolicyKit policies in `/usr/share/polkit-1/actions/` and `/etc/polkit-1/rules.d/`
- Can affect all user sessions

### Wrapper Script Pattern

To provide a seamless user experience, fairshare uses a wrapper script pattern:
- **User-facing command**: `/usr/local/bin/fairshare` (shell script)
- **Real binary**: `/usr/local/libexec/fairshare-bin` (Rust executable)
- **Wrapper transparently invokes pkexec** for regular commands
- **Admin commands bypass pkexec** (user must use sudo)

This allows users to type `fairshare status` instead of `pkexec fairshare status`.

### Resource Calculation and Delta-Based Checking

**Source of Truth**: Systemd is the authoritative source for all resource allocations. The system queries systemd directly via `systemctl` commands - no persistent state file is used.

**Delta-based resource checking**: When a user requests resources, the system intelligently calculates the **net increase** needed:

1. Query all current allocations from systemd via `systemctl show`
2. If the requesting user already has an allocation, subtract it from total used resources
3. Calculate available resources: `total - (used - user's_current_allocation)`
4. Check if the requested amount fits in the available pool

**Example**: User has 9GB allocated, requests 10GB, and only 2GB is free system-wide:
- Old behavior (would fail): Check if 10GB ≤ 2GB available → **FAIL**
- New behavior (succeeds): Net increase = 10GB - 9GB = 1GB; Check if 1GB ≤ 2GB available → **SUCCESS**

This allows users to adjust their allocations up or down without being blocked by their own existing allocation.

Critical parsing logic in `system.rs` handles `CPUQuotaPerSecUSec` conversion to percentage (1s = 100%, 4s = 400%, etc.).

### Systemd Interaction

The tool uses these `systemctl` commands (when run via pkexec, these operate at the system level):
- `systemctl list-units --type=slice --all --no-legend --plain` - List all user slices
- `systemctl show user-{UID}.slice` - Get slice properties (MemoryMax, CPUQuotaPerSecUSec)
- `systemctl set-property user-{UID}.slice` - Apply limits to specific user slice
- `systemctl revert user-{UID}.slice` - Remove custom limits and restore defaults
- `systemctl daemon-reload` - Reload systemd after config file changes

## Dependencies

- `clap` (4.5) - CLI argument parsing with validation
- `serde` (1.0) - Serialization (for TOML)
- `toml` (0.8) - TOML configuration parsing
- `sysinfo` (0.30) - System CPU/memory information
- `humansize` (2.1) - Human-readable size formatting
- `users` (0.11) - Current user UID/name lookup
- `colored` (2.1) - Terminal color output
- `comfy-table` (7.1) - Formatted table display

## Common Issues

### Systemd Commands Fail
- Ensure the wrapper script is installed: `which fairshare` should show `/usr/local/bin/fairshare`
- Ensure the binary is installed: `ls -l /usr/local/libexec/fairshare-bin`
- Admin operations require `sudo`: `sudo fairshare admin setup --cpu 1 --mem 2`
- PolicyKit policies must be installed (automatic during `admin setup`)

### Resource Allocation Fails
- Check available resources: `fairshare status`
- Remember: The system uses delta-based checking, so you can modify your existing allocation
- Requests may fail if the net increase exceeds available resources

### Configuration Not Applied
- Always run `systemctl daemon-reload` after modifying `/etc/systemd/`
- Verify file ownership and permissions

## Project State

The project uses semantic versioning (currently 0.1.0) and includes comprehensive test coverage for:
- CLI input validation and bounds checking
- Admin setup/uninstall workflows
- Full end-to-end resource allocation workflows

Recent security and stability improvements documented in `SECURITY_FIXES.md`.
