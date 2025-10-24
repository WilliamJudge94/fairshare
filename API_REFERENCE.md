# Policy System API Reference

## Module: `policy`

### Data Structures

#### `PolicyConfig`

Represents the complete policy configuration loaded from YAML.

```rust
pub struct PolicyConfig {
    pub defaults: ResourceSpec,
    pub max: ResourceSpec,
}
```

**Fields:**
- `defaults: ResourceSpec` - Default resource allocation
- `max: ResourceSpec` - Maximum resource limits

**Serialization:**
- Implements `Serialize`, `Deserialize` for YAML parsing
- Implements `Debug`, `Clone` for general use

---

#### `ResourceSpec`

Defines CPU and memory resource specifications.

```rust
pub struct ResourceSpec {
    pub cpu: u32,
    pub mem: String,
}
```

**Fields:**
- `cpu: u32` - Number of CPU cores
- `mem: String` - Memory size with unit (e.g., "8G", "512M")

**Example:**
```rust
let spec = ResourceSpec {
    cpu: 4,
    mem: "16G".to_string(),
};
```

---

#### `PolicyManager`

Manages policy loading, parsing, and validation.

```rust
pub struct PolicyManager {
    config: Option<PolicyConfig>,
    policy_path: String,
}
```

### Methods

#### `new(policy_path: impl Into<String>) -> Self`

Creates a new policy manager instance.

**Parameters:**
- `policy_path` - Path to the YAML policy file

**Returns:** A new `PolicyManager` instance

**Example:**
```rust
let manager = PolicyManager::new("/etc/fairshare/policy.d/default.yaml");
```

---

#### `load_policies(&mut self) -> Result<()>`

Loads and validates policies from the configured YAML file.

**Returns:**
- `Ok(())` on success
- `Err` if file cannot be read, YAML is invalid, or validation fails

**Errors:**
- File not found or permission denied
- Invalid YAML syntax
- Validation failures (max < defaults, zero values, invalid memory format)

**Example:**
```rust
let mut manager = PolicyManager::new("/etc/fairshare/policy.d/default.yaml");
manager.load_policies()?;
```

**Logging:**
- `INFO`: "Loading policies from: {path}"
- `INFO`: "Successfully loaded and validated policy configuration"
- `DEBUG`: Policy configuration details

---

#### `reload_policies(&mut self) -> Result<()>`

Reloads policies from disk, replacing the current configuration.

**Returns:**
- `Ok(())` on success
- `Err` if reload fails

**Example:**
```rust
manager.reload_policies()?;
```

**Logging:**
- `INFO`: "Reloading policies"
- `INFO`: "Policies reloaded successfully"

---

#### `get_config(&self) -> Option<&PolicyConfig>`

Returns a reference to the loaded policy configuration.

**Returns:**
- `Some(&PolicyConfig)` if policies are loaded
- `None` if policies haven't been loaded yet

**Example:**
```rust
if let Some(config) = manager.get_config() {
    println!("Defaults: {} CPUs", config.defaults.cpu);
}
```

---

#### `get_defaults(&self) -> Result<&ResourceSpec>`

Returns the default resource specification.

**Returns:**
- `Ok(&ResourceSpec)` with the default configuration
- `Err` if policies are not loaded

**Example:**
```rust
let defaults = manager.get_defaults()?;
println!("Default allocation: {} CPUs, {}", defaults.cpu, defaults.mem);
```

---

#### `get_max(&self) -> Result<&ResourceSpec>`

Returns the maximum resource specification.

**Returns:**
- `Ok(&ResourceSpec)` with the maximum limits
- `Err` if policies are not loaded

**Example:**
```rust
let max = manager.get_max()?;
println!("Maximum allowed: {} CPUs, {}", max.cpu, max.mem);
```

---

#### `validate_request(&self, cpu: u32, mem: &str) -> Result<()>`

Validates a resource request against the policy limits.

**Parameters:**
- `cpu: u32` - Requested number of CPU cores
- `mem: &str` - Requested memory size (e.g., "16G")

**Returns:**
- `Ok(())` if the request is valid
- `Err` with detailed error message if validation fails

**Validation Rules:**
1. CPU must be greater than 0
2. CPU must not exceed `max.cpu`
3. Memory must be parseable
4. Memory must not exceed `max.mem`

**Example:**
```rust
match manager.validate_request(4, "16G") {
    Ok(_) => println!("Request approved"),
    Err(e) => println!("Request denied: {}", e),
}
```

**Error Messages:**
- "CPU count must be greater than 0"
- "Requested CPU ({cpu}) exceeds maximum allowed ({max})"
- "Requested memory ({mem}) exceeds maximum allowed ({max})"
- Invalid memory format errors (from `parse_memory_size`)

---

## Module: `utils`

### Functions

#### `parse_memory_size(size_str: &str) -> Result<u64>`

Parses a human-readable memory size string to bytes.

**Parameters:**
- `size_str: &str` - Memory size string (e.g., "8G", "512M")

**Returns:**
- `Ok(u64)` - Size in bytes
- `Err` - If format is invalid, unit is unknown, or value is zero

**Supported Units:**
- `B` - Bytes
- `K`, `KB` - Kilobytes (1024 bytes)
- `M`, `MB` - Megabytes (1024^2 bytes)
- `G`, `GB` - Gigabytes (1024^3 bytes)
- `T`, `TB` - Terabytes (1024^4 bytes)

**Features:**
- Case-insensitive
- Supports decimal values (e.g., "1.5G")
- Handles whitespace
- Validates positive values

**Examples:**
```rust
assert_eq!(parse_memory_size("8G")?, 8 * 1024 * 1024 * 1024);
assert_eq!(parse_memory_size("512M")?, 512 * 1024 * 1024);
assert_eq!(parse_memory_size("1.5G")?, (1.5 * 1024.0 * 1024.0 * 1024.0) as u64);
```

**Error Messages:**
- "Invalid memory size format: {input}"
- "Failed to parse number from: {input}"
- "Unknown memory unit: {unit}. Supported units: B, K, KB, M, MB, G, GB, T, TB"
- "Memory size must be greater than 0"

---

#### `format_memory_size(bytes: u64) -> String`

Formats a byte count as a human-readable string.

**Parameters:**
- `bytes: u64` - Size in bytes

**Returns:** Formatted string with appropriate unit

**Examples:**
```rust
assert_eq!(format_memory_size(8 * 1024 * 1024 * 1024), "8.00G");
assert_eq!(format_memory_size(512 * 1024 * 1024), "512.00M");
assert_eq!(format_memory_size(1024), "1.00K");
assert_eq!(format_memory_size(512), "512B");
```

---

## Error Handling

All functions use the `anyhow` crate for error handling, providing rich context:

```rust
use anyhow::{Result, Context};

// Errors include context about what failed
manager.load_policies()
    .context("Failed to initialize policy system")?;
```

## Usage Pattern

### Typical Integration Flow

```rust
use fairshared::policy::PolicyManager;
use anyhow::Result;

async fn daemon_main() -> Result<()> {
    // 1. Initialize policy manager
    let mut policy_manager = PolicyManager::new(
        "/etc/fairshare/policy.d/default.yaml"
    );

    // 2. Load policies at startup
    policy_manager.load_policies()?;

    // 3. In request handler
    async fn handle_request(
        manager: &PolicyManager,
        cpu: u32,
        mem: &str,
    ) -> Result<()> {
        // Validate request
        manager.validate_request(cpu, mem)?;

        // If valid, proceed with allocation
        create_systemd_slice(cpu, mem).await?;

        Ok(())
    }

    Ok(())
}
```

### Configuration Reload

```rust
// Handle SIGHUP or admin command to reload config
async fn reload_configuration(manager: &mut PolicyManager) -> Result<()> {
    manager.reload_policies()?;
    Ok(())
}
```

### Default Allocation

```rust
// Use defaults when user doesn't specify resources
fn get_default_allocation(manager: &PolicyManager) -> Result<(u32, String)> {
    let defaults = manager.get_defaults()?;
    Ok((defaults.cpu, defaults.mem.clone()))
}
```

## Thread Safety

`PolicyManager` is **not** thread-safe by default. For concurrent access:

```rust
use std::sync::{Arc, RwLock};

let policy_manager = Arc::new(RwLock::new(PolicyManager::new(path)));

// Read access
let config = policy_manager.read().unwrap().get_config();

// Write access (for reload)
policy_manager.write().unwrap().reload_policies()?;
```

## Performance Considerations

- **Loading**: File I/O and YAML parsing are relatively expensive. Load once at startup.
- **Validation**: Request validation is cheap (just comparisons). Safe to call on every request.
- **Memory Parsing**: String parsing has overhead. Consider caching parsed values if needed.

## Best Practices

1. **Load Early**: Load policies during daemon initialization, not on first request
2. **Validate Always**: Always validate requests before creating slices
3. **Handle Errors**: Provide clear error messages to users
4. **Log Operations**: Use tracing for policy load/reload events
5. **Reload Safely**: Consider the impact of reload on in-flight requests

## YAML Schema

```yaml
# Schema for /etc/fairshare/policy.d/default.yaml
defaults:
  cpu: <positive integer>     # Default CPU cores
  mem: <size string>          # Default memory (e.g., "8G")

max:
  cpu: <positive integer>     # Maximum CPU cores (>= defaults.cpu)
  mem: <size string>          # Maximum memory (>= defaults.mem)
```

**Constraints:**
- All values must be positive
- `max.cpu >= defaults.cpu`
- `max.mem >= defaults.mem` (after parsing to bytes)
- Memory strings must use valid units (B, K, KB, M, MB, G, GB, T, TB)
