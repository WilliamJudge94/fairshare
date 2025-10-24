# Systemd DBus Client Implementation

## Overview

This document describes the implementation of the Systemd DBus Client (Deliverable #3) for the fairshared daemon. The implementation is located in `/workspaces/fairshare/src/systemd_client.rs`.

## Implementation Summary

### Core Components

#### 1. DBus Proxy Interfaces

**SystemdManager Proxy** (`#[proxy]` macro)
- Interface: `org.freedesktop.systemd1.Manager`
- Methods:
  - `start_transient_unit()` - Creates transient systemd units (slices/scopes)
  - `stop_unit()` - Stops and removes systemd units
  - `get_unit()` - Gets the DBus object path for a unit
  - `list_units()` - Lists all systemd units

**SystemdUnit Proxy** (`#[proxy]` macro)
- Interface: `org.freedesktop.systemd1.Unit`
- Properties:
  - `active_state` - Current active state (active, inactive, etc.)
  - `load_state` - Load state (loaded, not-found, etc.)
  - `sub_state` - Sub-state (running, dead, etc.)

#### 2. Data Structures

**SliceInfo**
```rust
pub struct SliceInfo {
    pub name: String,
    pub active_state: String,
    pub load_state: String,
    pub sub_state: String,
    pub cpu_quota: Option<u64>,
    pub memory_max: Option<u64>,
    pub tasks_max: Option<u64>,
}
```

**SystemdClient**
```rust
pub struct SystemdClient {
    connection: Connection,  // zbus::Connection to system DBus
}
```

### Required Functions Implementation

#### 1. `create_slice(uid: u32, cpu: u32, mem: &str) -> Result<()>`

Creates a transient systemd slice with resource limits.

**Parameters:**
- `uid` - User ID (used to generate slice name: `fairshare-{uid}.slice`)
- `cpu` - Number of CPU cores to allocate
- `mem` - Memory limit as string (e.g., "8G", "512M")

**Implementation Details:**
- Slice naming: `fairshare-{uid}.slice`
- CPU quota conversion: `cpu_count * 100_000` microseconds (e.g., 2 CPUs = 200,000us = 200%)
- Memory parsing: Uses `parse_memory_size()` from utils.rs to convert strings to bytes
- Tasks limit: Fixed at 4096 tasks per user
- Properties set:
  - `Description`: User-friendly description
  - `CPUQuota`: CPU limit in microseconds per 100ms
  - `MemoryMax`: Memory limit in bytes
  - `TasksMax`: Maximum number of tasks/threads
  - `DefaultDependencies`: Set to false for transient units

**DBus Method:** `StartTransientUnit` with mode "fail" (fails if already exists)

#### 2. `remove_slice(uid: u32) -> Result<()>`

Removes a systemd slice for a user.

**Parameters:**
- `uid` - User ID of the slice to remove

**Implementation Details:**
- Constructs slice name: `fairshare-{uid}.slice`
- Uses `StopUnit` DBus method with mode "replace"
- Returns job path for tracking

**DBus Method:** `StopUnit` with mode "replace"

#### 3. `get_slice_status(uid: u32) -> Result<SliceInfo>`

Gets comprehensive status information about a slice.

**Parameters:**
- `uid` - User ID of the slice to query

**Returns:** `SliceInfo` struct with:
- Slice name
- Active state, load state, sub state
- CPU quota (microseconds per second)
- Memory maximum (bytes)
- Tasks maximum (count)

**Implementation Details:**
- Gets unit object path via `GetUnit`
- Queries unit properties via `SystemdUnitProxy`
- Queries resource properties via DBus Properties interface
- Handles missing properties gracefully (returns None)

### Additional Helper Functions

#### `slice_exists(slice_name: &str) -> Result<bool>`
Checks if a slice exists by attempting to get its unit path.

#### `move_process_to_slice(pid: u32, slice_name: &str) -> Result<()>`
Moves a process to a slice by creating a scope unit as a child of the slice.

#### `list_slices() -> Result<Vec<String>>`
Lists all active slice units in the system.

#### `get_slice_properties(slice_name: &str) -> Result<HashMap<String, String>>`
Gets slice properties as a key-value map.

#### `delete_slice(slice_name: &str) -> Result<()>`
Deletes a slice by name (not UID).

### Technical Specifications

#### CPU Quota Calculation
```
CPU cores → Percentage → Microseconds per 100ms
1 CPU    → 100%      → 100,000 us
2 CPUs   → 200%      → 200,000 us
4 CPUs   → 400%      → 400,000 us
```

Formula: `cpu_quota_usec = cpu_count * 100_000`

#### Memory Conversion
Memory strings are parsed using `utils::parse_memory_size()`:
- "8G" → 8,589,934,592 bytes (8 * 1024^3)
- "512M" → 536,870,912 bytes (512 * 1024^2)
- "1024K" → 1,048,576 bytes (1024 * 1024)

#### Slice Naming Convention
Format: `fairshare-{uid}.slice`
Examples:
- UID 1001 → `fairshare-1001.slice`
- UID 9999 → `fairshare-9999.slice`

#### Transient Units
All slices created are **transient** (not persistent across reboots):
- Created using `StartTransientUnit` method
- Automatically removed when no processes remain
- Not saved to disk
- Ideal for dynamic resource allocation

## Error Handling

All functions return `anyhow::Result<T>` with context:
- DBus connection failures
- Unit not found errors
- Permission errors (requires root/systemd access)
- Invalid memory size formats
- Property retrieval failures

Example error contexts:
```rust
.context("Failed to connect to system DBus")
.context("Failed to create systemd manager proxy")
.context("Failed to start transient unit")
.context("Failed to parse memory size")
```

## Testing

### Unit Tests

Located in `#[cfg(test)] mod tests`:

1. **test_systemd_connection** - Tests DBus connection
2. **test_slice_name_format** - Validates slice naming
3. **test_cpu_quota_conversion** - Tests CPU quota calculations
4. **test_memory_parsing** - Tests memory size parsing
5. **test_scope_name_format** - Tests scope naming for process movement
6. **test_slice_info_creation** - Tests SliceInfo struct
7. **test_tasks_max_value** - Validates TasksMax default

### Integration Tests (marked with `#[ignore]`)

These require a running systemd instance and root privileges:

1. **test_create_and_remove_slice** - Full lifecycle test
   - Creates a slice
   - Verifies it exists
   - Gets status
   - Removes slice
   - Verifies removal

2. **test_list_slices** - Lists all system slices

3. **test_get_slice_properties** - Queries system.slice properties

### Running Tests

```bash
# Run unit tests
cargo test

# Run integration tests (requires systemd + root)
cargo test --ignored

# Run all tests
cargo test -- --include-ignored
```

## Dependencies

From `Cargo.toml`:
```toml
zbus = "5.0"          # DBus communication
anyhow = "1.0"        # Error handling
tracing = "0.1"       # Structured logging
tokio = { version = "1.41", features = ["full"] }  # Async runtime
```

## Usage Example

```rust
use fairshared::systemd_client::SystemdClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize client
    let client = SystemdClient::new().await?;

    // Create slice for user 1001 with 2 CPUs and 8GB RAM
    client.create_slice(1001, 2, "8G").await?;

    // Get slice status
    let status = client.get_slice_status(1001).await?;
    println!("Slice: {}", status.name);
    println!("State: {}", status.active_state);
    println!("CPU Quota: {:?}", status.cpu_quota);
    println!("Memory Max: {:?}", status.memory_max);

    // Remove slice
    client.remove_slice(1001).await?;

    Ok(())
}
```

## Manual Testing with busctl

Test the implementation manually using `busctl`:

```bash
# Create a transient slice
busctl call org.freedesktop.systemd1 \
  /org/freedesktop/systemd1 \
  org.freedesktop.systemd1.Manager \
  StartTransientUnit \
  "ssa(sv)a(sa(sv))" \
  "fairshare-1001.slice" "fail" \
  5 \
  "Description" s "Fairshare slice for UID 1001" \
  "CPUQuota" t 200000 \
  "MemoryMax" t 8589934592 \
  "TasksMax" t 4096 \
  "DefaultDependencies" b false \
  0

# Check slice status
systemctl status fairshare-1001.slice

# View slice properties
systemctl show fairshare-1001.slice

# Stop slice
systemctl stop fairshare-1001.slice
```

## DBus Interface Reference

### systemd Manager Interface

**Service:** `org.freedesktop.systemd1`
**Path:** `/org/freedesktop/systemd1`
**Interface:** `org.freedesktop.systemd1.Manager`

Key methods used:
- `StartTransientUnit(name, mode, properties, aux)`
- `StopUnit(name, mode)`
- `GetUnit(name)`
- `ListUnits()`

### systemd Unit Interface

**Interface:** `org.freedesktop.systemd1.Unit`

Key properties used:
- `ActiveState` (string)
- `LoadState` (string)
- `SubState` (string)
- `CPUQuotaPerSecUSec` (uint64)
- `MemoryMax` (uint64)
- `TasksMax` (uint64)

## Future Enhancements

Functions marked for future implementation:

1. **set_slice_properties()** - Dynamically update slice properties
   - Would require `SetUnitProperties` DBus method
   - Currently properties are set only during creation

2. **subscribe_to_changes()** - Monitor unit state changes
   - Would require DBus signal handlers
   - Listen for `UnitNew`, `UnitRemoved`, `PropertiesChanged` signals

## Compliance with Requirements

### Task 1 Requirements ✓

- ✅ Use `zbus` crate for DBus communication
- ✅ Use `StartTransientUnit` DBus method for creating slices
- ✅ Set properties: `CPUQuota`, `MemoryMax`, `TasksMax`
- ✅ Handle slice naming: `fairshare-{uid}.slice`
- ✅ Connect to systemd DBus interface (org.freedesktop.systemd1)
- ✅ CPU quota: Convert CPU count to percentage
- ✅ Memory: Convert from string using utils.rs
- ✅ TasksMax: Set to 4096 per user
- ✅ Make slices transient (not persistent across reboots)
- ✅ Implement required functions:
  - `create_slice(uid, cpu, mem)`
  - `remove_slice(uid)`
  - `get_slice_status(uid)`
- ✅ Proper error handling for DBus operations
- ✅ Comprehensive tests (unit + integration)

## Verification Checklist

- [x] All required functions implemented
- [x] DBus proxy interfaces defined
- [x] SliceInfo struct with all required fields
- [x] CPU quota conversion (cores to microseconds)
- [x] Memory parsing integration with utils.rs
- [x] TasksMax set to 4096
- [x] Slice naming convention followed
- [x] Transient units (not persistent)
- [x] Error handling with anyhow::Result
- [x] Logging with tracing macros
- [x] Unit tests for basic functionality
- [x] Integration tests for systemd interaction
- [x] Documentation and examples
- [x] Helper functions for additional operations

## Known Limitations

1. **Requires systemd**: Only works on Linux systems with systemd
2. **Requires root privileges**: Creating slices requires system DBus access
3. **Integration tests**: Marked as `#[ignore]` and require systemd to run
4. **No runtime property updates**: Properties are set during creation only
5. **No signal monitoring**: subscribe_to_changes() is not yet implemented

## Architecture Integration

The SystemdClient integrates with other modules:

```
┌─────────────────┐
│   daemon.rs     │
│  (main loop)    │
└────────┬────────┘
         │
         ├─── policy.rs (resource limits)
         │
         ├─── systemd_client.rs (THIS MODULE)
         │        │
         │        ├─── DBus ───> systemd
         │        └─── utils.rs (memory parsing)
         │
         └─── ipc.rs (Unix socket)
```

The daemon uses SystemdClient to:
1. Create slices when users request resources
2. Remove slices when users release resources
3. Query slice status for user info requests
4. Enforce policy limits via systemd cgroups

## Conclusion

The Systemd DBus Client implementation provides a complete, production-ready interface for managing systemd slices via DBus. It fulfills all requirements from Task 1, includes comprehensive error handling and testing, and is ready to be integrated into the fairshared daemon.
