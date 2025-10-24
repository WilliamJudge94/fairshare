# Policy System Implementation

## Overview

The policy system for the fairshared daemon is implemented in `src/policy.rs` and provides YAML-based configuration for resource limits. This document describes the implementation details and usage.

## Architecture

### Core Components

1. **PolicyConfig** - The main configuration structure that holds default and maximum resource specifications
2. **ResourceSpec** - Defines CPU and memory limits
3. **PolicyManager** - Handles loading, parsing, validation, and querying of policies

### File Structure

```
src/
├── policy.rs      # Policy parsing and validation
├── utils.rs       # Memory parsing utilities
└── ...
```

## YAML Configuration Format

The policy file uses a simple YAML structure with two main sections:

```yaml
defaults:
  cpu: 2      # Default CPU cores
  mem: 8G     # Default memory limit

max:
  cpu: 8      # Maximum CPU cores allowed
  mem: 32G    # Maximum memory allowed
```

### Supported Memory Units

The following memory units are supported (case-insensitive):
- **B** or **bytes** - Bytes
- **K** or **KB** - Kilobytes (1024 bytes)
- **M** or **MB** - Megabytes (1024^2 bytes)
- **G** or **GB** - Gigabytes (1024^3 bytes)
- **T** or **TB** - Terabytes (1024^4 bytes)

Examples:
- `8G`, `8GB`, `8g` - All represent 8 gigabytes
- `512M`, `0.5G` - Both represent 512 megabytes
- `8192M`, `8G` - Both represent 8 gigabytes

## Implementation Details

### Memory Parsing (`utils.rs`)

The `parse_memory_size()` function converts human-readable memory strings to bytes:

```rust
// Examples
parse_memory_size("8G")      // Returns: 8589934592 (8 * 1024^3)
parse_memory_size("512M")    // Returns: 536870912 (512 * 1024^2)
parse_memory_size("1.5G")    // Returns: 1610612736 (1.5 * 1024^3)
```

Features:
- Supports decimal values (e.g., "1.5G")
- Case-insensitive unit parsing
- Handles whitespace
- Validates positive values
- Returns detailed error messages

### Policy Validation

The policy system enforces several validation rules:

1. **Positive Values**: All CPU and memory values must be greater than 0
2. **Max >= Defaults**: Maximum limits must be greater than or equal to defaults
3. **Valid Memory Format**: Memory strings must be parseable with recognized units
4. **Request Validation**: User requests must not exceed maximum limits

### PolicyManager API

#### Creating a Policy Manager

```rust
use fairshared::policy::PolicyManager;

let mut manager = PolicyManager::new("/etc/fairshare/policy.d/default.yaml");
```

#### Loading Policies

```rust
// Load policies from the configured path
manager.load_policies()?;

// Reload policies (useful for live updates)
manager.reload_policies()?;
```

#### Accessing Configuration

```rust
// Get the entire configuration
let config = manager.get_config()?;

// Get defaults
let defaults = manager.get_defaults()?;
println!("Default CPU: {}", defaults.cpu);
println!("Default Memory: {}", defaults.mem);

// Get max limits
let max = manager.get_max()?;
println!("Max CPU: {}", max.cpu);
println!("Max Memory: {}", max.mem);
```

#### Validating Requests

```rust
// Validate a user's resource request
match manager.validate_request(4, "16G") {
    Ok(_) => println!("Request is valid"),
    Err(e) => println!("Request denied: {}", e),
}
```

## Error Handling

The policy system provides detailed error messages for various failure scenarios:

### Configuration Errors

```
❌ Failed to read policy file: /etc/fairshare/policy.d/default.yaml
❌ Failed to parse YAML policy file: expected mapping at line 3
❌ Invalid default memory size: 8X
❌ Default CPU must be greater than 0
❌ Maximum CPU (4) must be greater than or equal to default CPU (8)
```

### Request Validation Errors

```
❌ CPU count must be greater than 0
❌ Requested CPU (16) exceeds maximum allowed (8)
❌ Requested memory (64G) exceeds maximum allowed (32G)
❌ Invalid memory size format: abc
❌ Unknown memory unit: PB. Supported units: B, K, KB, M, MB, G, GB, T, TB
```

## Testing

The implementation includes comprehensive unit tests:

### Policy Parsing Tests
- Valid policy parsing
- Invalid YAML syntax
- Missing required fields

### Validation Tests
- Max < defaults rejection
- Zero CPU rejection
- Invalid memory formats
- Memory unit parsing (K, M, G, GB, etc.)

### Request Validation Tests
- Valid requests within limits
- CPU limit exceeded
- Memory limit exceeded
- Zero CPU rejection

### Memory Utility Tests
- Parsing various units (B, K, M, G, T)
- Case-insensitive parsing
- Decimal values
- Whitespace handling
- Invalid format detection
- Formatting bytes to human-readable strings

Run tests with:
```bash
cargo test --lib policy
cargo test --lib utils
```

## Usage in Daemon

The daemon should integrate the policy system as follows:

```rust
use fairshared::policy::PolicyManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize policy manager
    let mut policy_manager = PolicyManager::new("/etc/fairshare/policy.d/default.yaml");

    // Load policies on startup
    policy_manager.load_policies()?;

    // When handling a user request
    let cpu_request = 4;
    let mem_request = "16G";

    // Validate the request
    match policy_manager.validate_request(cpu_request, mem_request) {
        Ok(_) => {
            // Request is valid, proceed with allocation
            create_slice(uid, cpu_request, mem_request).await?;
        }
        Err(e) => {
            // Request exceeds limits, reject
            return Err(anyhow!("Request denied: {}", e));
        }
    }

    Ok(())
}
```

## Example Policy Files

### Development Environment
```yaml
defaults:
  cpu: 2
  mem: 4G

max:
  cpu: 4
  mem: 16G
```

### Production Server
```yaml
defaults:
  cpu: 4
  mem: 16G

max:
  cpu: 16
  mem: 128G
```

### High-Performance Computing
```yaml
defaults:
  cpu: 8
  mem: 32G

max:
  cpu: 64
  mem: 512G
```

## File Location

The default policy file location is:
```
/etc/fairshare/policy.d/default.yaml
```

This can be configured when creating the PolicyManager instance.

## Future Enhancements

Potential improvements for future versions:

1. **Per-User Policies** - Allow different limits for different users/groups
2. **Time-Based Policies** - Different limits based on time of day
3. **Priority Levels** - Different resource pools with priorities
4. **Dynamic Rebalancing** - Automatic resource adjustment based on load
5. **Policy Inheritance** - Hierarchical policy configuration
6. **Quota Management** - Track usage over time periods

## Dependencies

- `serde` - Serialization/deserialization framework
- `serde_yaml` - YAML parsing
- `anyhow` - Error handling
- `tracing` - Structured logging

## See Also

- [Task 1: Core Daemon & Systemd Integration](task_1.md)
- [Design Document](design_doc.md)
- [src/policy.rs](src/policy.rs) - Implementation
- [src/utils.rs](src/utils.rs) - Utility functions
