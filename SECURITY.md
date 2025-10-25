# Security Review Report: fairshare

**Review Date:** 2025-10-25
**Version Reviewed:** 0.1.0
**Review Type:** Pre-release security audit
**Overall Risk Assessment:** HIGH

## Executive Summary

This security review analyzed the fairshare Rust codebase, a systemd resource manager using pkexec/PolicyKit for privilege escalation. The codebase demonstrates strong foundational security practices in Rust, particularly around integer overflow protection, input validation, and memory safety. However, **critical PolicyKit configuration issues** were identified that grant all users unrestricted systemd management access, effectively bypassing the intended security model.

**Recommendation:** Address critical and high-severity issues before production deployment.

---

## Critical Vulnerabilities

### 1. Overly Permissive PolicyKit Configuration
**Severity:** CRITICAL
**Location:** `assets/50-fairshare.pkla` lines 15-27
**CVE:** N/A (Pre-release)

**Description:**

The PolicyKit localauthority file grants ALL users unrestricted access to systemd management operations:

```plaintext
[Allow systemd manage-units for all users]
Identity=unix-user:*
Action=org.freedesktop.systemd1.manage-units
ResultActive=yes

[Allow systemd manage-unit-files for all users]
Identity=unix-user:*
Action=org.freedesktop.systemd1.manage-unit-files
ResultActive=yes
```

**Impact:**
- Any user can manage ANY systemd unit (not just their own user-{UID}.slice)
- Privilege escalation vector: users could start/stop/modify critical system services
- Users could modify system units (sshd, networking, etc.)
- Completely bypasses intended resource isolation

**Remediation:**

Remove the overly broad permissions from `50-fairshare.pkla`. The PolicyKit configuration should only allow execution of the fairshare binary via pkexec, not grant blanket systemd privileges:

```diff
- [Allow systemd manage-units for all users]
- Identity=unix-user:*
- Action=org.freedesktop.systemd1.manage-units
- ResultActive=yes
- ResultInactive=yes
- ResultAny=yes
-
- [Allow systemd manage-unit-files for all users]
- Identity=unix-user:*
- Action=org.freedesktop.systemd1.manage-unit-files
- ResultActive=yes
- ResultInactive=yes
- ResultAny=yes
```

The fairshare binary itself (running as root via pkexec) should be the only entity manipulating systemd, not end users directly.

---

## High Severity Issues

### 2. World-Writable State File and Directory
**Severity:** HIGH
**Location:** `src/systemd.rs:333-355`

**Description:**

The admin setup creates world-writable permissions:

```rust
// Set directory permissions to 0777 (world-writable)
perms.set_mode(0o777);
fs::set_permissions("/var/lib/fairshare", perms)?;

// Set file permissions to 0666 (world-readable/writable)
perms.set_mode(0o666);
fs::set_permissions(state_file_path, perms)?;
```

**Impact:**
- Race conditions: Any user can modify allocation data
- Information disclosure: All users can read others' allocations
- File tampering: Users could inject false allocation data
- File deletion: Directory permissions allow removal
- Symlink attacks: Attacker could replace file with symlink

**Note:** The state file is currently unused (systemd is the source of truth), but the vulnerable infrastructure remains.

**Remediation:**

Option 1 (Recommended): Remove state file functionality entirely since systemd is already the authoritative source.

Option 2: Use restrictive permissions:
```rust
// Directory: 0755 (root write, others read/execute)
perms.set_mode(0o755);
fs::set_permissions("/var/lib/fairshare", perms)?;

// File: 0644 (root write, others read)
perms.set_mode(0o644);
fs::set_permissions(state_file_path, perms)?;
```

---

### 3. Insufficient UID Validation
**Severity:** HIGH
**Location:** `src/systemd.rs:15-27`

**Description:**

The UID from `PKEXEC_UID` is parsed but not validated against system boundaries:

```rust
pub fn get_calling_user_uid() -> io::Result<u32> {
    if let Ok(pkexec_uid_str) = env::var("PKEXEC_UID") {
        pkexec_uid_str.parse::<u32>()
            .map_err(|e| io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid PKEXEC_UID environment variable: {}", e)
            ))
    }
}
```

**Issues:**
- No validation that UID corresponds to a valid user
- No check against privileged UIDs (0-999)
- No verification of UID boundaries
- Potential manipulation of system slices (e.g., `user-0.slice` for root)

**Remediation:**

Add comprehensive UID validation:

```rust
pub fn get_calling_user_uid() -> io::Result<u32> {
    if let Ok(pkexec_uid_str) = env::var("PKEXEC_UID") {
        let uid = pkexec_uid_str.parse::<u32>()
            .map_err(|e| io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid PKEXEC_UID: {}", e)
            ))?;

        // Prevent root manipulation
        if uid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot modify root user slice"
            ));
        }

        // Standard user UID threshold (system users: 0-999)
        if uid < 1000 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot modify system user slice"
            ));
        }

        // Verify user exists
        if users::get_user_by_uid(uid).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("User with UID {} does not exist", uid)
            ));
        }

        Ok(uid)
    } else {
        Ok(users::get_current_uid())
    }
}
```

---

### 4. Binary Integrity Not Verified by PolicyKit
**Severity:** MEDIUM-HIGH
**Location:** `assets/50-fairshare.rules`, `assets/org.fairshare.policy`

**Description:**

PolicyKit rules only check the program path but don't verify:
- Binary integrity (checksum/signature)
- File permissions
- Binary hasn't been tampered with

```javascript
if (action.lookup("program") == "/usr/local/bin/fairshare") {
    return polkit.Result.YES;
}
```

**Impact:**

If `/usr/local/bin/fairshare` is misconfigured (e.g., world-writable), an attacker could:
- Replace the binary with malicious code
- PolicyKit would still allow passwordless execution

**Remediation:**

1. Ensure proper installation permissions in Makefile
2. Document required permissions: `/usr/local/bin/fairshare` must be root-owned, mode 0755
3. Add installation verification step
4. Consider adding checksum validation to PolicyKit policy (advanced)

---

## Medium Severity Issues

### 5. Lack of Rate Limiting
**Severity:** MEDIUM
**Location:** All request handling code

**Description:**

No rate limiting on resource allocation requests. A user could:
- Rapidly request/release resources in a loop
- Cause systemd to thrash with property changes
- Create DoS conditions via excessive systemctl invocations

**Remediation:**

Implement rate limiting:
- Track request timestamps per user
- Limit requests to N per minute (e.g., 10 requests/minute)
- Add cooldown period between changes (e.g., 5 seconds)
- Return clear error when rate limit exceeded

---

### 6. Potential Information Leakage in Error Messages
**Severity:** LOW-MEDIUM
**Location:** Multiple locations with detailed error output

**Description:**

Error messages expose internal system details:

```rust
format!("Failed to list systemd slices: {}", e)
format!("systemctl command failed with exit code: {:?}", output.status.code())
```

**Impact:**

- Reveals system configuration details to unprivileged users
- Could aid reconnaissance for further attacks

**Remediation:**

Sanitize user-facing error messages:
```rust
// User-facing
eprintln!("Failed to retrieve system information");

// Detailed logging (syslog/journal)
log::error!("Failed to list systemd slices: {}", e);
```

---

## Low Severity Issues

### 7. Silent Parse Failures
**Severity:** LOW
**Location:** `src/system.rs:102, 182, 184, 186`

**Description:**

Parsing failures default to zero:

```rust
mem_bytes = value_str.parse::<u64>().unwrap_or(0);
```

**Impact:**

- Silent failures could cause incorrect resource calculations
- Zero values might be treated as "no limit" by systemd

**Remediation:**

Propagate parse errors or log warnings:
```rust
mem_bytes = value_str.parse::<u64>().unwrap_or_else(|e| {
    log::warn!("Failed to parse memory value '{}': {}", value_str, e);
    0
});
```

---

## Security Best Practices Observed

The following security measures are properly implemented:

### Integer Overflow Protection
Excellent use of `checked_mul()` throughout:

```rust
let mem_bytes = (mem as u64).checked_mul(1_000_000_000)
    .ok_or_else(|| io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("Memory value {} GB would cause overflow", mem)
    ))?;
```

### Input Validation at CLI Boundary
Strong validation using `clap` with range constraints:

```rust
#[arg(long, value_parser = RangedU64ValueParser::<u32>::new()
    .range(MIN_CPU as u64..=MAX_CPU as u64))]
cpu: u32,
```

### No Unsafe Code
Zero `unsafe` blocks found - excellent Rust safety practices.

### Proper Command Construction
Uses `Command::new().arg()` pattern instead of shell interpolation, preventing injection attacks:

```rust
Command::new("systemctl")
    .arg("set-property")
    .arg(&format!("user-{}.slice", uid))  // uid is u32, safe
```

### Delta-Based Resource Checking
Smart resource validation accounts for existing allocations, preventing gaming:

```rust
if let Some(current_alloc) = user_current_allocation {
    adjusted_used = used - current_alloc;
}
```

### Root UID Filtering
System properly filters UID 0 from user allocations:

```rust
if uid == "0" {
    continue;  // Skip root user slice
}
```

### Comprehensive Test Coverage
Extensive unit tests for overflow conditions, boundary values, and arithmetic operations.

---

## Remediation Timeline

### Immediate (Before Release)
1. Fix PolicyKit configuration (Issue #1) - CRITICAL
2. Fix world-writable permissions (Issue #2) - HIGH
3. Add UID validation (Issue #3) - HIGH
4. Document binary installation security (Issue #4) - MEDIUM-HIGH

### Short-term (Next Release)
5. Implement rate limiting (Issue #5) - MEDIUM
6. Sanitize error messages (Issue #6) - LOW-MEDIUM

### Long-term (Future Enhancement)
7. Add comprehensive security logging
8. Implement audit trail for resource changes
9. Add security documentation and threat model

---

## Responsible Disclosure Policy

If you discover a security vulnerability in fairshare, please report it responsibly:

1. **Do not** open a public GitHub issue
2. Email security reports to: [MAINTAINER EMAIL - TO BE ADDED]
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if available)

We will respond within 48 hours and provide a timeline for fixes.

---

## Security Contact

**Security Team:** [TO BE DEFINED]
**PGP Key:** [TO BE ADDED]
**Response Time:** 48 hours for initial response

---

## Changelog

- **2025-10-25:** Initial security review (v0.1.0 pre-release)

---

## Conclusion

The fairshare codebase demonstrates strong Rust security practices with excellent overflow protection, input validation, and memory safety. However, **critical PolicyKit misconfigurations must be addressed before production deployment** as they present immediate privilege escalation risks.

**Key Strengths:**
- No unsafe code blocks
- Excellent overflow protection
- Strong input validation
- Proper command construction (no shell injection)
- Good test coverage

**Key Weaknesses:**
- Critical PolicyKit privilege escalation vector
- World-writable state file infrastructure
- Insufficient UID validation
- Missing rate limiting

All critical and high-severity issues should be resolved before public release.
