# Security Fixes Task List for Fairshare

This document outlines security issues found in the fairshare codebase and provides a detailed task list for remediation.

## Legend
- 🔴 **CRITICAL**: Must fix before production use
- 🟠 **HIGH**: Should fix soon, significant security risk
- 🟡 **MEDIUM**: Should fix, moderate security risk
- 🟢 **LOW**: Nice to have, minor security improvement

---

## 🔴 CRITICAL Priority Tasks

### Task 1: Remove Command Injection Risk in get_user_allocations() ✅ COMPLETED
**File**: `src/system.rs:33-36`
**Issue**: Using `bash -c` with shell pipeline is unnecessary and potentially dangerous
**Status**: Fixed - Replaced bash pipeline with direct systemctl command, added UID validation, implemented proper error handling
**Current Code**:
```rust
let output = Command::new("bash")
    .arg("-c")
    .arg("systemctl list-units --type=slice --all | grep user- | awk '{print $1}'")
    .output()
    .expect("failed to list slices");
```

**Fix Steps**:
1. Replace bash pipeline with direct systemctl command
2. Use `--no-legend` and `--plain` flags for parseable output
3. Filter user slices in Rust code instead of grep/awk
4. Handle errors properly instead of `.expect()`

**Suggested Implementation**:
```rust
let output = Command::new("systemctl")
    .args(["list-units", "--type=slice", "--all", "--no-legend", "--plain"])
    .output()?;

for line in String::from_utf8_lossy(&output.stdout).lines() {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        continue;
    }
    let unit_name = parts[0];
    if !unit_name.starts_with("user-") || !unit_name.ends_with(".slice") {
        continue;
    }
    // Parse UID...
}
```

---

### Task 2: Add Input Validation and Bounds Checking
**File**: `src/cli.rs:16-21`
**Issue**: CPU and memory parameters accept unreasonably large u32 values

**Fix Steps**:
1. Add custom validation using clap's `value_parser`
2. Define reasonable maximum values (e.g., max 1000 CPUs, max 10000 GB)
3. Add minimum values (at least 1 CPU, at least 1 GB)
4. Add helpful error messages for out-of-range values

**Suggested Implementation**:
```rust
use clap::builder::RangedU64ValueParser;

Request {
    #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(1..=1000))]
    cpu: u32,
    #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(1..=10000))]
    mem: u32,
},
```

**Additional**:
- Add constants for max values: `const MAX_CPU: u32 = 1000;`
- Document why these limits exist in comments

---

### Task 3: Fix Integer Overflow in Memory Calculations
**File**: `src/systemd.rs:10, 142`
**Issue**: No overflow checking when converting GB to bytes

**Fix Steps**:
1. Use `checked_mul()` for all arithmetic operations
2. Return proper errors on overflow
3. Add tests for edge cases (very large values)
4. Consider using `u64` for memory values from the start

**Suggested Implementation**:
```rust
pub fn set_user_limits(cpu: u32, mem: u32) -> io::Result<()> {
    // Validate inputs first (should already be validated by clap)
    if cpu > MAX_CPU || mem > MAX_MEM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Values exceed maximum limits (cpu: {}, mem: {})", MAX_CPU, MAX_MEM)
        ));
    }

    let mem_bytes = (mem as u64).checked_mul(1_000_000_000)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            "Memory value too large, would cause overflow"
        ))?;

    let cpu_quota = cpu.checked_mul(100)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            "CPU value too large, would cause overflow"
        ))?;

    // Rest of implementation...
}
```

---

### Task 4: Validate and Sanitize UID Parsing
**File**: `src/system.rs:42-48`
**Issue**: UIDs extracted from systemctl are used without validation

**Fix Steps**:
1. Add regex validation for UID format (digits only)
2. Verify UID is a valid number
3. Check UID is within valid range (typically 0-65535 for system, higher for users)
4. Handle malformed input gracefully

**Suggested Implementation**:
```rust
fn parse_uid_from_slice(slice_name: &str) -> Option<u32> {
    // Expected format: "user-1000.slice"
    let parts: Vec<&str> = slice_name.split('-').collect();
    if parts.len() != 2 || parts[0] != "user" {
        return None;
    }

    let uid_str = parts[1].trim_end_matches(".slice");

    // Validate it's only digits
    if !uid_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    // Parse and validate range
    uid_str.parse::<u32>().ok().filter(|&uid| uid < 4294967295)
}

// In get_user_allocations():
let Some(uid) = parse_uid_from_slice(line.trim()) else {
    continue; // Skip invalid entries
};
```

---

## 🟠 HIGH Priority Tasks

### Task 5: Fix TOCTOU Race Condition in Resource Allocation
**File**: `src/main.rs:22-30`
**Issue**: Time gap between checking and allocating resources allows over-allocation

**Fix Steps**:
1. Implement a mutex/lock for resource allocation operations
2. Re-check availability after acquiring lock, before allocation
3. Consider using a resource reservation system
4. Add atomic operations or file-based locking

**Suggested Implementation**:
```rust
// Option 1: Use a lockfile
use std::fs::File;
use std::os::unix::fs::OpenOptionsExt;

fn acquire_allocation_lock() -> io::Result<File> {
    let lock_path = "/var/lock/fairshare-allocate.lock";
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .mode(0o600)
        .open(lock_path)
    // Note: Would need to implement proper file locking with flock
}

// In main.rs request handler:
let _lock = acquire_allocation_lock()?;

// Re-check after acquiring lock
let totals = get_system_totals();
let allocations = get_user_allocations();
if !check_request(&totals, &allocations, *cpu, &mem.to_string()) {
    eprintln!("{} {}", "✗".red().bold(), "Request exceeds available system resources.".red());
    std::process::exit(1);
}

if let Err(e) = set_user_limits(*cpu, *mem) {
    eprintln!("{} {}: {}", "✗".red().bold(), "Failed to set limits".red(), e);
    std::process::exit(1);
}
// Lock is released when _lock goes out of scope
```

**Alternative**: Create a daemon that manages allocations centrally

---

### Task 6: Replace .unwrap() and .expect() with Proper Error Handling
**Files**: `src/system.rs` (multiple locations)
**Issue**: Program panics instead of handling errors gracefully

**Fix Steps**:
1. Audit all uses of `.unwrap()` and `.expect()`
2. Replace with proper `?` operator or `match` statements
3. Return `Result` types from functions where needed
4. Add context to errors using `.map_err()` or custom error types

**Locations to Fix**:
- `src/system.rs:37` - `.expect("failed to list slices")`
- `src/system.rs:60` - `.unwrap()` on systemctl output
- `src/system.rs:69, 74` - `.unwrap_or(0)` silently hides errors
- `src/systemd.rs:74` - `.unwrap_or_else()` for username

**Suggested Implementation**:
```rust
// Change function signatures to return Result
pub fn get_user_allocations() -> io::Result<Vec<UserAlloc>> {
    let output = Command::new("systemctl")
        .args(["list-units", "--type=slice", "--all", "--no-legend", "--plain"])
        .output()
        .map_err(|e| io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to list systemd slices: {}", e)
        ))?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("systemctl command failed with exit code: {:?}", output.status.code())
        ));
    }

    // Rest of implementation with proper error handling...
}

// Update callers to handle Result
Commands::Status => {
    let totals = get_system_totals();
    let allocations = match get_user_allocations() {
        Ok(allocs) => allocs,
        Err(e) => {
            eprintln!("{} Failed to get user allocations: {}", "✗".red().bold(), e);
            std::process::exit(1);
        }
    };
    print_status(&totals, &allocations);
}
```

---

### Task 7: Add Privilege Validation for Admin Commands
**File**: `src/systemd.rs:136`
**Issue**: admin_setup_defaults doesn't verify root privileges upfront

**Fix Steps**:
1. Check if running as root (UID 0) at function entry
2. Return clear error message if not privileged
3. Add similar checks for any privilege-requiring operations
4. Document which functions require elevated privileges

**Suggested Implementation**:
```rust
pub fn admin_setup_defaults(cpu: u32, mem: u32) -> io::Result<()> {
    // Check for root privileges
    if users::get_current_uid() != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "admin setup requires root privileges. Please run with sudo."
        ));
    }

    // Validate inputs
    if cpu > MAX_CPU || mem > MAX_MEM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Values exceed system limits (max cpu: {}, max mem: {})", MAX_CPU, MAX_MEM)
        ));
    }

    // Rest of implementation...
}
```

---

## 🟡 MEDIUM Priority Tasks

### Task 8: Fix Silent Parsing Failures
**File**: `src/system.rs:69, 74`
**Issue**: Invalid values silently become 0, hiding misconfigurations

**Fix Steps**:
1. Log warnings when parsing fails
2. Consider returning an error instead of defaulting to 0
3. Add validation that parsed values are reasonable
4. Track parsing failures for debugging

**Suggested Implementation**:
```rust
// Add logging dependency to Cargo.toml:
// log = "0.4"
// env_logger = "0.11"

for l in out.lines() {
    if l.starts_with("MemoryMax=") {
        if let Some(value_str) = l.strip_prefix("MemoryMax=") {
            match value_str.parse::<u64>() {
                Ok(bytes) => mem_bytes = bytes,
                Err(e) => {
                    log::warn!("Failed to parse MemoryMax '{}' for UID {}: {}", value_str, uid, e);
                    // Could choose to skip this allocation or use a sentinel value
                }
            }
        }
    } else if l.starts_with("CPUQuotaPerSecUSec=") {
        if let Some(quota_str) = l.strip_prefix("CPUQuotaPerSecUSec=") {
            if let Some(sec_str) = quota_str.strip_suffix('s') {
                match sec_str.parse::<f64>() {
                    Ok(seconds) => cpu_quota = seconds * 100.0,
                    Err(e) => {
                        log::warn!("Failed to parse CPUQuotaPerSecUSec '{}' for UID {}: {}", quota_str, uid, e);
                    }
                }
            }
        }
    }
}
```

---

### Task 9: Add Audit Logging
**Files**: All command handlers in `src/main.rs`
**Issue**: No logging of resource operations for security auditing

**Fix Steps**:
1. Add `log` and `env_logger` dependencies to Cargo.toml
2. Initialize logging in main()
3. Log all resource requests (who, what, when, success/failure)
4. Log all admin operations
5. Consider using syslog for production systems

**Suggested Implementation**:
```rust
// In Cargo.toml:
[dependencies]
log = "0.4"
env_logger = "0.11"
syslog = "6.1"  # Optional for system logging

// In main.rs:
use log::{info, warn, error};

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    match &cli.command {
        Commands::Request { cpu, mem } => {
            let uid = users::get_current_uid();
            let username = users::get_current_username()
                .and_then(|os_str| os_str.into_string().ok())
                .unwrap_or_else(|| uid.to_string());

            info!("User {} (UID {}) requesting {} CPU(s) and {}G RAM",
                  username, uid, cpu, mem);

            let totals = get_system_totals();
            let allocations = get_user_allocations();

            if !check_request(&totals, &allocations, *cpu, &mem.to_string()) {
                warn!("User {} (UID {}) request denied: insufficient resources", username, uid);
                eprintln!("{} {}", "✗".red().bold(), "Request exceeds available system resources.".red());
                std::process::exit(1);
            }

            if let Err(e) = set_user_limits(*cpu, *mem) {
                error!("User {} (UID {}) allocation failed: {}", username, uid, e);
                eprintln!("{} {}: {}", "✗".red().bold(), "Failed to set limits".red(), e);
                std::process::exit(1);
            }

            info!("User {} (UID {}) successfully allocated {} CPU(s) and {}G RAM",
                  username, uid, cpu, mem);
            println!("{} Allocated {} and {}.",
                "✓".green().bold(),
                format!("{} CPU(s)", cpu).bright_yellow().bold(),
                format!("{}G RAM", mem).bright_yellow().bold()
            );
        }

        // Similar logging for other commands...
    }
}
```

---

### Task 10: Implement Rate Limiting
**File**: New module `src/ratelimit.rs`
**Issue**: Users can spam requests causing system instability

**Fix Steps**:
1. Create a rate limiting module
2. Track requests per user with timestamps
3. Implement sliding window or token bucket algorithm
4. Store state in /var/lib/fairshare/ or similar
5. Add configurable limits (e.g., 10 requests per minute)

**Suggested Implementation**:
```rust
// src/ratelimit.rs
use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use std::sync::Mutex;
use lazy_static::lazy_static;

const MAX_REQUESTS_PER_MINUTE: usize = 10;
const WINDOW_DURATION: Duration = Duration::from_secs(60);

lazy_static! {
    static ref RATE_LIMITER: Mutex<RateLimiter> = Mutex::new(RateLimiter::new());
}

struct RateLimiter {
    requests: HashMap<u32, Vec<SystemTime>>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            requests: HashMap::new(),
        }
    }

    fn check_and_record(&mut self, uid: u32) -> Result<(), String> {
        let now = SystemTime::now();
        let entry = self.requests.entry(uid).or_insert_with(Vec::new);

        // Remove requests older than window
        entry.retain(|&time| {
            now.duration_since(time)
                .map(|d| d < WINDOW_DURATION)
                .unwrap_or(false)
        });

        if entry.len() >= MAX_REQUESTS_PER_MINUTE {
            return Err(format!(
                "Rate limit exceeded. Maximum {} requests per minute.",
                MAX_REQUESTS_PER_MINUTE
            ));
        }

        entry.push(now);
        Ok(())
    }
}

pub fn check_rate_limit(uid: u32) -> Result<(), String> {
    RATE_LIMITER
        .lock()
        .map_err(|e| format!("Failed to acquire rate limiter lock: {}", e))?
        .check_and_record(uid)
}
```

**Integration**:
```rust
// In main.rs before resource request:
if let Err(e) = ratelimit::check_rate_limit(users::get_current_uid()) {
    eprintln!("{} {}", "✗".red().bold(), e.red());
    std::process::exit(1);
}
```

---

### Task 11: Add Access Control for Status Command
**File**: `src/system.rs:120-193`
**Issue**: Any user can see all other users' resource allocations

**Fix Steps**:
1. Add option to show only current user's info in status
2. Require privilege to see all users' allocations
3. Add `--all` flag for admin users
4. Document privacy implications

**Suggested Implementation**:
```rust
// In cli.rs:
Status {
    /// Show all users (requires root, default: current user only)
    #[arg(long)]
    all: bool,
},

// In system.rs:
pub fn print_status(totals: &SystemTotals, allocations: &[UserAlloc], show_all: bool) {
    let current_uid = users::get_current_uid();

    // Filter allocations based on permissions
    let visible_allocations: Vec<&UserAlloc> = if show_all {
        if current_uid != 0 {
            eprintln!("Warning: --all flag requires root privileges, showing current user only");
            allocations.iter().filter(|a| a.uid == current_uid.to_string()).collect()
        } else {
            allocations.iter().collect()
        }
    } else {
        allocations.iter().filter(|a| a.uid == current_uid.to_string()).collect()
    };

    // Rest of print logic using visible_allocations...
}
```

---

## 🟢 LOW Priority Tasks

### Task 12: Fix Hardcoded Multiplier in Admin Setup
**File**: `src/systemd.rs:159`
**Issue**: Arbitrary "cpu * 10" multiplier for max caps

**Fix Steps**:
1. Add configuration parameter for cap multiplier
2. Validate cap doesn't exceed system totals
3. Document the purpose of caps
4. Make it configurable via CLI or config file

**Suggested Implementation**:
```rust
// In cli.rs AdminSubcommands::Setup:
Setup {
    #[arg(long, default_value_t = 1)]
    cpu: u32,
    #[arg(long, default_value_t = 2)]
    mem: u32,
    #[arg(long, default_value_t = 10, help = "Multiplier for max caps")]
    cap_multiplier: u32,
},

// In systemd.rs:
pub fn admin_setup_defaults(cpu: u32, mem: u32, cap_multiplier: u32) -> io::Result<()> {
    // Validate cap multiplier
    if cap_multiplier == 0 || cap_multiplier > 100 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cap_multiplier must be between 1 and 100"
        ));
    }

    let max_cpu = cpu.checked_mul(cap_multiplier)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            "CPU cap calculation overflow"
        ))?;

    let max_mem = mem.checked_mul(cap_multiplier)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            "Memory cap calculation overflow"
        ))?;

    // Verify against system totals
    let totals = get_system_totals();
    if max_cpu as usize > totals.total_cpu || max_mem as f64 > totals.total_mem_gb {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Max caps exceed system resources (system: {} CPU, {:.2}G RAM)",
                    totals.total_cpu, totals.total_mem_gb)
        ));
    }

    // Use max_cpu and max_mem in policy file...
}
```

---

### Task 13: Add Dependency Security Scanning
**File**: CI/CD configuration
**Issue**: No automated vulnerability checking for dependencies

**Fix Steps**:
1. Add `cargo-audit` to CI pipeline
2. Set up Dependabot or Renovate for automated updates
3. Add `cargo-deny` for policy enforcement
4. Document security update process

**Suggested Implementation**:
```bash
# Install cargo-audit
cargo install cargo-audit

# Run in CI
cargo audit

# Create .github/dependabot.yml:
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 10
```

---

### Task 14: Add Comprehensive Security Tests
**File**: New test file `tests/security_tests.rs`
**Issue**: No tests specifically for security scenarios

**Fix Steps**:
1. Test integer overflow scenarios
2. Test race conditions (if possible)
3. Test invalid input handling
4. Test privilege escalation attempts
5. Test rate limiting
6. Test injection attempts

**Suggested Implementation**:
```rust
// tests/security_tests.rs
#[test]
fn test_integer_overflow_memory() {
    // Test that very large memory values are rejected
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "1", "--mem", "4294967295"])
        .output()
        .expect("Failed to run request");

    // Should fail gracefully, not overflow
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("exceeds") || stderr.contains("overflow") || stderr.contains("invalid"));
}

#[test]
fn test_uid_injection_attempt() {
    // Test that malicious UID patterns don't cause issues
    // This would be tested at the unit level in parse_uid_from_slice
}

#[test]
fn test_negative_values_rejected() {
    // Clap should reject negative values for u32
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "-1", "--mem", "2"])
        .output()
        .expect("Failed to run request");

    assert!(!output.status.success());
}

#[test]
fn test_zero_values_rejected() {
    // Should require at least 1 CPU and 1GB
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "0", "--mem", "2"])
        .output()
        .expect("Failed to run request");

    assert!(!output.status.success());
}

#[test]
fn test_rate_limiting() {
    // Test that excessive requests are rate limited
    // Would need to mock the rate limiter or use a test instance
}
```

---

## Implementation Order Recommendation

1. **Phase 1 - Critical Fixes (Week 1)**
   - Task 2: Input validation and bounds checking
   - Task 3: Integer overflow fixes
   - Task 1: Remove command injection risk
   - Task 4: UID validation

2. **Phase 2 - High Priority (Week 2)**
   - Task 6: Replace unwrap/expect with error handling
   - Task 7: Privilege validation
   - Task 5: TOCTOU race condition fix

3. **Phase 3 - Medium Priority (Week 3)**
   - Task 9: Audit logging
   - Task 8: Fix silent parsing failures
   - Task 10: Rate limiting

4. **Phase 4 - Low Priority & Testing (Week 4)**
   - Task 11: Access control improvements
   - Task 12: Fix hardcoded multiplier
   - Task 14: Security tests
   - Task 13: Dependency scanning

---

## Testing Strategy

After each fix:
1. Run existing tests: `cargo test`
2. Build and test manually: `cargo build --release && ./target/release/fairshare status`
3. Test edge cases specific to the fix
4. Update integration tests as needed
5. Run `cargo clippy` for additional warnings
6. Run `cargo audit` for dependency vulnerabilities

---

## Additional Resources

- [Rust Security Best Practices](https://anssi-fr.github.io/rust-guide/)
- [OWASP Secure Coding Practices](https://owasp.org/www-project-secure-coding-practices-quick-reference-guide/)
- [CWE Top 25 Most Dangerous Software Weaknesses](https://cwe.mitre.org/top25/)
- [Cargo Security Advisory Database](https://rustsec.org/)

---

## Sign-off Checklist

Before considering security fixes complete:
- [ ] All CRITICAL tasks completed
- [ ] All HIGH tasks completed
- [ ] Security tests pass
- [ ] `cargo audit` shows no vulnerabilities
- [ ] `cargo clippy` shows no warnings
- [ ] Code review by security-aware developer
- [ ] Manual penetration testing completed
- [ ] Documentation updated with security considerations
- [ ] CHANGELOG.md updated with security fixes
