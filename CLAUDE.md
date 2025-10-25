# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

**fairshare** is a Rust-based systemd resource manager for multi-user Linux systems. It provides fair CPU and memory allocation management using systemd user slices, allowing users to request resources dynamically while preventing over-allocation.

## Development Commands

### Building
- **Debug build**: `cargo build`
- **Release build**: `cargo build --release`
- **Install release binary**: `make release` (copies to `/usr/local/bin/fairshare`)

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
- **Show status**: `cargo run -- status`
- **Request resources**: `cargo run -- request --cpu 4 --mem 8`
- **Show user info**: `cargo run -- info`
- **Release resources**: `cargo run -- release`
- **Admin setup** (requires root): `cargo run -- admin setup --cpu 1 --mem 2`

## Architecture Overview

### Module Structure

The codebase is organized into four main modules:

1. **`src/main.rs`** - Entry point that routes commands to appropriate handlers
2. **`src/cli.rs`** - Command-line interface definitions using `clap` with validation constraints:
   - CPU range: 1-1000 cores
   - Memory range: 1-10000 GB
3. **`src/system.rs`** - System information gathering and resource availability checking
4. **`src/systemd.rs`** - Systemd interaction for applying/reverting resource limits

### Core Data Flow

1. **Command Parsing** (`cli.rs`): Clap validates input bounds before execution
2. **Resource Validation** (`system.rs`): Checks if requested resources are available
3. **Systemd Configuration** (`systemd.rs`): Applies limits via `systemctl set-property` or reverts via `systemctl revert`

### Key Functions

#### System Information (`system.rs`)
- `get_system_totals()` - Returns total system CPU and memory (uses `sysinfo` crate)
- `get_user_allocations()` - Reads all user slice allocations via `systemctl list-units`
- `check_request()` - Validates if requested resources are available
- `print_status()` - Displays formatted system and per-user resource overview

#### Resource Management (`systemd.rs`)
- `set_user_limits()` - Applies CPU quota and memory limits via `systemctl --user set-property`
- `release_user_limits()` - Reverts limits back to defaults via `systemctl --user revert`
- `show_user_info()` - Displays current user's resource allocation
- `admin_setup_defaults()` - Creates systemd config at `/etc/systemd/system/user-.slice.d/00-defaults.conf`
- `admin_uninstall_defaults()` - Removes admin configuration files and reloads systemd

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

### User Privileges

The tool handles both regular and root users differently:

**Regular users** (non-root):
- Manage their own user session: `systemctl --user set-property -.slice`
- Cannot affect other users
- No elevated privileges needed

**Root users**:
- Manage system-wide settings via `systemctl set-property user-0.slice`
- Create/modify global defaults in `/etc/systemd/system/`
- Can affect all user sessions

### Resource Calculation

When checking availability:
1. Sum all current allocations from `systemctl show` output
2. Subtract from system totals (from `sysinfo`)
3. Compare against requested resources

Critical parsing logic in `system.rs:88-101` handles `CPUQuotaPerSecUSec` conversion to percentage.

### Systemd Interaction

The tool uses these `systemctl` commands:
- `systemctl list-units --type=slice` - List all user slices
- `systemctl show` - Get slice properties (MemoryMax, CPUQuotaPerSecUSec)
- `systemctl --user set-property` - Apply limits to user session
- `systemctl --user revert` - Remove custom limits
- `systemctl daemon-reload` - Reload after config changes

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
- Ensure systemd user session is running: `systemctl --user status`
- Root operations require `sudo`

### Resource Allocation Fails
- Check available resources: `cargo run -- status`
- May exceed system limits during concurrent allocations

### Configuration Not Applied
- Always run `systemctl daemon-reload` after modifying `/etc/systemd/`
- Verify file ownership and permissions

## Project State

The project uses semantic versioning (currently 0.1.0) and includes comprehensive test coverage for:
- CLI input validation and bounds checking
- Admin setup/uninstall workflows
- Full end-to-end resource allocation workflows

Recent security and stability improvements documented in `SECURITY_FIXES.md`.
