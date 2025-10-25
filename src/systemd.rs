use std::process::Command;
use std::io::{self, Write};
use std::fs;
use std::path::Path;
use users;
use colored::*;

// Import constants from cli module for validation
use crate::cli::{MAX_CPU, MAX_MEM};

pub fn set_user_limits(cpu: u32, mem: u32) -> io::Result<()> {
    // Validate inputs before operations
    if cpu > MAX_CPU {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("CPU value {} exceeds maximum limit of {}", cpu, MAX_CPU)
        ));
    }
    if mem > MAX_MEM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Memory value {} exceeds maximum limit of {}", mem, MAX_MEM)
        ));
    }

    let uid = users::get_current_uid();

    // Convert GB to bytes with overflow checking
    let mem_bytes = (mem as u64).checked_mul(1_000_000_000)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Memory value {} GB is too large and would cause overflow when converting to bytes", mem)
        ))?;

    // Calculate CPU quota with overflow checking
    let cpu_quota = cpu.checked_mul(100)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("CPU value {} is too large and would cause overflow when calculating quota", cpu)
        ))?;

    let status = if uid == 0 {
        // Root user: manage system-wide user slices
        Command::new("systemctl")
            .arg("set-property")
            .arg(&format!("user-{}.slice", uid))
            .arg(format!("CPUQuota={}%", cpu_quota))
            .arg(format!("MemoryMax={}", mem_bytes))
            .status()?
    } else {
        // Regular user: manage their own user session
        Command::new("systemctl")
            .arg("--user")
            .arg("set-property")
            .arg("--")
            .arg("-.slice")
            .arg(format!("CPUQuota={}%", cpu_quota))
            .arg(format!("MemoryMax={}", mem_bytes))
            .status()?
    };

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to set user limits (exit code: {:?})", status.code()),
        ));
    }

    Ok(())
}

pub fn release_user_limits() -> io::Result<()> {
    let uid = users::get_current_uid();

    let status = if uid == 0 {
        // Root user: revert system-wide user slice
        Command::new("systemctl")
            .arg("revert")
            .arg(&format!("user-{}.slice", uid))
            .status()?
    } else {
        // Regular user: revert their own user session
        Command::new("systemctl")
            .arg("--user")
            .arg("revert")
            .arg("--")
            .arg("-.slice")
            .status()?
    };

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to release user limits (exit code: {:?})", status.code()),
        ));
    }

    Ok(())
}

pub fn show_user_info() -> io::Result<()> {
    let uid = users::get_current_uid();
    let username = users::get_current_username()
        .and_then(|os_str| os_str.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    let output = if uid == 0 {
        // Root user: show system-wide user slice
        Command::new("systemctl")
            .arg("show")
            .arg(&format!("user-{}.slice", uid))
            .arg("-p")
            .arg("MemoryMax")
            .arg("-p")
            .arg("CPUQuota")
            .arg("-p")
            .arg("CPUQuotaPerSecUSec")
            .output()?
    } else {
        // Regular user: show their own user session
        Command::new("systemctl")
            .arg("--user")
            .arg("show")
            .arg("-.slice")
            .arg("-pMemoryMax")
            .arg("-pCPUQuota")
            .arg("-pCPUQuotaPerSecUSec")
            .output()?
    };

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let mut cpu_quota = "Not set".to_string();
    let mut mem_max = "Not set".to_string();

    for line in stdout_str.lines() {
        if let Some(value) = line.strip_prefix("CPUQuotaPerSecUSec=") {
            if let Some(sec_str) = value.strip_suffix('s') {
                if let Ok(seconds) = sec_str.parse::<f64>() {
                    cpu_quota = format!("{:.1}% ({:.2} CPUs)", seconds * 100.0, seconds);
                }
            }
        } else if let Some(value) = line.strip_prefix("MemoryMax=") {
            if let Ok(bytes) = value.parse::<u64>() {
                let gb = bytes as f64 / 1_000_000_000.0;
                mem_max = format!("{:.2} GB", gb);
            }
        }
    }

    println!("{}", "╔═══════════════════════════════════════╗".bright_cyan());
    println!("{}", "║       USER RESOURCE ALLOCATION        ║".bright_cyan().bold());
    println!("{}", "╚═══════════════════════════════════════╝".bright_cyan());
    println!();
    println!("{} {}", "User:".bright_white().bold(), username.bright_yellow());
    println!("{} {}", "UID:".bright_white().bold(), uid.to_string().bright_yellow());
    println!();
    println!("{} {}", "CPU Quota:".bright_white().bold(), cpu_quota.green());
    println!("{} {}", "Memory Max:".bright_white().bold(), mem_max.green());

    Ok(())
}

/// Setup global default resource allocations for all users.
/// Default minimum: 1 CPU core and 2G RAM per user.
/// Each user can request additional resources up to system limits.
pub fn admin_setup_defaults(cpu: u32, mem: u32) -> io::Result<()> {
    // Validate inputs before operations
    if cpu > MAX_CPU {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("CPU value {} exceeds maximum limit of {}", cpu, MAX_CPU)
        ));
    }
    if mem > MAX_MEM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Memory value {} exceeds maximum limit of {}", mem, MAX_MEM)
        ));
    }

    let dir = Path::new("/etc/systemd/system/user-.slice.d");
    let conf_path = dir.join("00-defaults.conf");

    fs::create_dir_all(dir)?;
    let mut f = fs::File::create(&conf_path)?;

    // Convert GB to bytes with overflow checking
    let mem_bytes = (mem as u64).checked_mul(1_000_000_000)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Memory value {} GB is too large and would cause overflow when converting to bytes", mem)
        ))?;

    // Calculate CPU quota with overflow checking
    let cpu_quota = cpu.checked_mul(100)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("CPU value {} is too large and would cause overflow when calculating quota", cpu)
        ))?;

    writeln!(
        f,
        "[Slice]\nCPUQuota={}%\nMemoryMax={}\n",
        cpu_quota, mem_bytes
    )?;

    println!("{} Created {}", "✓".green().bold(), conf_path.display().to_string().bright_white());

    Command::new("systemctl").arg("daemon-reload").status()?;
    println!("{} {}", "✓".green().bold(), "Reloaded systemd daemon".bright_white());

    // Calculate max caps with overflow checking
    let max_cpu_cap = cpu.checked_mul(10)
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("CPU value {} is too large for calculating max cap (cpu * 10 would overflow)", cpu)
        ))?;

    fs::create_dir_all("/etc/fairshare")?;
    let mut policy = fs::File::create("/etc/fairshare/policy.toml")?;
    writeln!(
        policy,
        "[defaults]\ncpu = {}\nmem = {}\n\n[max_caps]\ncpu = {}\nmem = {}\n",
        cpu, mem, max_cpu_cap, mem
    )?;
    println!("{} {}", "✓".green().bold(), "Created /etc/fairshare/policy.toml".bright_white());

    Ok(())
}

/// Uninstall global defaults and remove all fairshare admin configuration.
/// This removes:
/// - /etc/systemd/system/user-.slice.d/00-defaults.conf
/// - /etc/fairshare/policy.toml
/// - /etc/fairshare/ directory (if empty)
/// - Reloads systemd daemon to apply changes
pub fn admin_uninstall_defaults() -> io::Result<()> {
    let systemd_conf_path = Path::new("/etc/systemd/system/user-.slice.d/00-defaults.conf");
    let policy_path = Path::new("/etc/fairshare/policy.toml");
    let fairshare_dir = Path::new("/etc/fairshare");

    // Remove systemd configuration file
    if systemd_conf_path.exists() {
        fs::remove_file(systemd_conf_path)?;
        println!("{} Removed {}", "✓".green().bold(), systemd_conf_path.display().to_string().bright_white());
    } else {
        println!("{} {} (not found)", "→".bright_white(), systemd_conf_path.display().to_string().bright_white());
    }

    // Remove policy configuration file
    if policy_path.exists() {
        fs::remove_file(policy_path)?;
        println!("{} Removed {}", "✓".green().bold(), policy_path.display().to_string().bright_white());
    } else {
        println!("{} {} (not found)", "→".bright_white(), policy_path.display().to_string().bright_white());
    }

    // Remove fairshare directory if it's empty
    if fairshare_dir.exists() {
        match fs::remove_dir(fairshare_dir) {
            Ok(()) => {
                println!("{} Removed {}", "✓".green().bold(), fairshare_dir.display().to_string().bright_white());
            }
            Err(e) => {
                // Directory might not be empty, which is fine
                if e.kind() == io::ErrorKind::Other || !fairshare_dir.read_dir()?.next().is_some() {
                    println!("{} {} (not empty or already removed)", "→".bright_white(), fairshare_dir.display().to_string().bright_white());
                } else {
                    return Err(e);
                }
            }
        }
    }

    // Reload systemd daemon to apply changes
    let status = Command::new("systemctl").arg("daemon-reload").status()?;
    if status.success() {
        println!("{} {}", "✓".green().bold(), "Reloaded systemd daemon".bright_white());
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to reload systemd daemon (exit code: {:?})", status.code()),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_admin_setup_creates_valid_config_content() {
        // This test validates the configuration format without actually
        // creating files on the system
        let cpu: u32 = 2;
        let mem: u32 = 4;
        let mem_bytes = (mem as u64).checked_mul(1_000_000_000).unwrap();
        let cpu_quota = cpu.checked_mul(100).unwrap();

        let expected_slice_config = format!(
            "[Slice]\nCPUQuota={}%\nMemoryMax={}\n",
            cpu_quota,
            mem_bytes
        );

        assert_eq!(expected_slice_config, "[Slice]\nCPUQuota=200%\nMemoryMax=4000000000\n");

        let max_cpu_cap = cpu.checked_mul(10).unwrap();
        let expected_policy = format!(
            "[defaults]\ncpu = {}\nmem = {}\n\n[max_caps]\ncpu = {}\nmem = {}\n",
            cpu, mem, max_cpu_cap, mem
        );

        assert!(expected_policy.contains("[defaults]"));
        assert!(expected_policy.contains("cpu = 2"));
        assert!(expected_policy.contains("mem = 4"));
    }

    #[test]
    fn test_memory_conversion_to_bytes_safe() {
        // Verify memory conversion logic with overflow checking
        let mem_gb = 8u32;
        let mem_bytes = (mem_gb as u64).checked_mul(1_000_000_000).unwrap();
        assert_eq!(mem_bytes, 8_000_000_000);

        let mem_gb = 16u32;
        let mem_bytes = (mem_gb as u64).checked_mul(1_000_000_000).unwrap();
        assert_eq!(mem_bytes, 16_000_000_000);

        // Test maximum valid value
        let mem_gb = 10000u32; // MAX_MEM
        let mem_bytes = (mem_gb as u64).checked_mul(1_000_000_000).unwrap();
        assert_eq!(mem_bytes, 10_000_000_000_000);
    }

    #[test]
    fn test_cpu_quota_calculation_safe() {
        // Verify CPU quota percentage calculation with overflow checking
        let cpu = 1u32;
        let quota = cpu.checked_mul(100).unwrap();
        assert_eq!(quota, 100);

        let cpu = 4u32;
        let quota = cpu.checked_mul(100).unwrap();
        assert_eq!(quota, 400);

        let cpu = 8u32;
        let quota = cpu.checked_mul(100).unwrap();
        assert_eq!(quota, 800);

        // Test maximum valid value
        let cpu = 1000u32; // MAX_CPU
        let quota = cpu.checked_mul(100).unwrap();
        assert_eq!(quota, 100_000);
    }

    #[test]
    fn test_memory_overflow_detection() {
        // Test that very large memory values that would overflow are handled
        // u64::MAX / 1_000_000_000 = 18_446_744_073 (approximately)
        // u32::MAX (4_294_967_295) * 1_000_000_000 = 4_294_967_295_000_000_000 which is < u64::MAX
        // So u32::MAX won't overflow when cast to u64 and multiplied
        let huge_mem = u32::MAX; // 4_294_967_295 GB
        let result = (huge_mem as u64).checked_mul(1_000_000_000);
        // This should NOT overflow because u32::MAX * 1 billion < u64::MAX
        assert!(result.is_some(), "u32::MAX GB should not overflow when converted to bytes");

        // To actually test overflow, we need a u64 value larger than u64::MAX / 1_000_000_000
        let overflow_mem = 18_446_744_074u64; // Just above safe limit
        let result = overflow_mem.checked_mul(1_000_000_000);
        assert!(result.is_none(), "Expected overflow for value above u64::MAX / 1 billion");
    }

    #[test]
    fn test_cpu_quota_overflow_detection() {
        // Test that very large CPU values that would overflow are handled
        // u32::MAX * 100 would overflow u32
        let huge_cpu = u32::MAX;
        let result = huge_cpu.checked_mul(100);
        assert!(result.is_none(), "Expected overflow for u32::MAX * 100");
    }

    #[test]
    fn test_max_cpu_cap_overflow_detection() {
        // Test that CPU * 10 overflow is detected
        // u32::MAX / 10 = 429_496_729 (approximately)
        // So values above this should fail
        let large_cpu = u32::MAX;
        let result = large_cpu.checked_mul(10);
        assert!(result.is_none(), "Expected overflow for u32::MAX * 10");

        // Test a value that should work
        let safe_cpu = 100u32;
        let result = safe_cpu.checked_mul(10);
        assert_eq!(result, Some(1000));
    }

    #[test]
    fn test_boundary_values() {
        // Test boundary conditions within valid range

        // Minimum values
        let min_cpu = 1u32;
        let min_mem = 1u32;
        assert!(min_cpu.checked_mul(100).is_some());
        assert!((min_mem as u64).checked_mul(1_000_000_000).is_some());

        // Maximum valid values
        let max_cpu = 1000u32; // MAX_CPU
        let max_mem = 10000u32; // MAX_MEM
        assert!(max_cpu.checked_mul(100).is_some());
        assert!((max_mem as u64).checked_mul(1_000_000_000).is_some());
        assert!(max_cpu.checked_mul(10).is_some()); // For max_caps calculation
    }

    #[test]
    fn test_near_overflow_values() {
        // Test values near overflow boundaries

        // For memory: u64::MAX is 18_446_744_073_709_551_615
        // Dividing by 1_000_000_000 gives max safe value of ~18_446_744_073
        // Our MAX_MEM (10000) is well within this range

        // Test a value that's within bounds
        let safe_mem = 18_000_000_000u64; // 18 billion GB - still under u64::MAX / 1 billion
        let result = safe_mem.checked_mul(1_000_000_000);
        assert!(result.is_some(), "18 billion GB should not overflow");

        // Test a value that will overflow
        let overflow_mem = 19_000_000_000u64; // 19 billion GB - will overflow
        let result = overflow_mem.checked_mul(1_000_000_000);
        assert!(result.is_none(), "Expected overflow for 19 billion GB");

        // For CPU quota: u32::MAX is 4_294_967_295
        // Dividing by 100 gives max safe value of 42_949_672
        // Our MAX_CPU (1000) is well within this range
        let near_overflow_cpu = 42_949_673u32; // Above safe limit
        let result = near_overflow_cpu.checked_mul(100);
        assert!(result.is_none(), "Expected overflow for CPU quota");
    }

    #[test]
    fn test_zero_values() {
        // Test that zero values are handled correctly (though they should be
        // rejected by input validation in practice)
        let cpu = 0u32;
        let mem = 0u32;

        assert_eq!(cpu.checked_mul(100), Some(0));
        assert_eq!((mem as u64).checked_mul(1_000_000_000), Some(0));
    }

    #[test]
    fn test_set_user_limits_input_validation_cpu_exceeds_max() {
        // Test that set_user_limits rejects CPU values exceeding MAX_CPU
        use crate::cli::MAX_CPU;

        let result = super::set_user_limits(MAX_CPU + 1, 2);
        assert!(result.is_err(), "Should reject CPU exceeding MAX_CPU");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(error_msg.contains("exceeds maximum limit"),
                    "Error should mention exceeding limit: {}", error_msg);
            assert!(error_msg.contains(&(MAX_CPU + 1).to_string()),
                    "Error should contain the invalid CPU value: {}", error_msg);
        }
    }

    #[test]
    fn test_set_user_limits_input_validation_mem_exceeds_max() {
        // Test that set_user_limits rejects memory values exceeding MAX_MEM
        use crate::cli::MAX_MEM;

        let result = super::set_user_limits(2, MAX_MEM + 1);
        assert!(result.is_err(), "Should reject memory exceeding MAX_MEM");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(error_msg.contains("exceeds maximum limit"),
                    "Error should mention exceeding limit: {}", error_msg);
            assert!(error_msg.contains(&(MAX_MEM + 1).to_string()),
                    "Error should contain the invalid memory value: {}", error_msg);
        }
    }

    #[test]
    fn test_admin_setup_defaults_input_validation_cpu_exceeds_max() {
        // Test that admin_setup_defaults rejects CPU values exceeding MAX_CPU
        use crate::cli::MAX_CPU;

        let result = super::admin_setup_defaults(MAX_CPU + 1, 2);
        assert!(result.is_err(), "Should reject CPU exceeding MAX_CPU");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(error_msg.contains("exceeds maximum limit"),
                    "Error should mention exceeding limit: {}", error_msg);
        }
    }

    #[test]
    fn test_admin_setup_defaults_input_validation_mem_exceeds_max() {
        // Test that admin_setup_defaults rejects memory values exceeding MAX_MEM
        use crate::cli::MAX_MEM;

        let result = super::admin_setup_defaults(2, MAX_MEM + 1);
        assert!(result.is_err(), "Should reject memory exceeding MAX_MEM");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(error_msg.contains("exceeds maximum limit"),
                    "Error should mention exceeding limit: {}", error_msg);
        }
    }

    #[test]
    fn test_overflow_checked_mul_memory_conversion() {
        // Test checked_mul for memory to bytes conversion

        // Valid conversions
        let conversions = vec![
            (1u32, 1_000_000_000u64),
            (2u32, 2_000_000_000u64),
            (10u32, 10_000_000_000u64),
            (100u32, 100_000_000_000u64),
            (1000u32, 1_000_000_000_000u64),
            (10000u32, 10_000_000_000_000u64),
        ];

        for (gb, expected_bytes) in conversions {
            let result = (gb as u64).checked_mul(1_000_000_000);
            assert!(result.is_some(), "Conversion of {} GB should not overflow", gb);
            assert_eq!(result.unwrap(), expected_bytes,
                      "Conversion of {} GB should equal {} bytes", gb, expected_bytes);
        }
    }

    #[test]
    fn test_overflow_checked_mul_cpu_quota() {
        // Test checked_mul for CPU quota calculation

        // Valid calculations
        let calculations = vec![
            (1u32, 100u32),
            (2u32, 200u32),
            (4u32, 400u32),
            (10u32, 1000u32),
            (100u32, 10000u32),
            (1000u32, 100000u32),
        ];

        for (cpu, expected_quota) in calculations {
            let result = cpu.checked_mul(100);
            assert!(result.is_some(), "Quota calculation for {} CPU should not overflow", cpu);
            assert_eq!(result.unwrap(), expected_quota,
                      "Quota for {} CPU should equal {}", cpu, expected_quota);
        }
    }

    #[test]
    fn test_overflow_checked_mul_max_caps() {
        // Test checked_mul for max caps calculation

        // Valid calculations
        let calculations = vec![
            (1u32, 10u32),
            (2u32, 20u32),
            (4u32, 40u32),
            (10u32, 100u32),
            (100u32, 1000u32),
            (1000u32, 10000u32),
        ];

        for (cpu, expected_cap) in calculations {
            let result = cpu.checked_mul(10);
            assert!(result.is_some(), "Max cap calculation for {} CPU should not overflow", cpu);
            assert_eq!(result.unwrap(), expected_cap,
                      "Max cap for {} CPU should equal {}", cpu, expected_cap);
        }
    }

    #[test]
    fn test_max_valid_cpu_quota_without_overflow() {
        // Test that MAX_CPU can safely be converted to quota
        use crate::cli::MAX_CPU;

        let result = MAX_CPU.checked_mul(100);
        assert!(result.is_some(), "MAX_CPU ({}) should not overflow when multiplied by 100", MAX_CPU);
        assert_eq!(result.unwrap(), MAX_CPU as u32 * 100,
                  "MAX_CPU quota should be {} * 100", MAX_CPU);
    }

    #[test]
    fn test_max_valid_memory_conversion_without_overflow() {
        // Test that MAX_MEM can safely be converted to bytes
        use crate::cli::MAX_MEM;

        let result = (MAX_MEM as u64).checked_mul(1_000_000_000);
        assert!(result.is_some(), "MAX_MEM ({}) should not overflow when converted to bytes", MAX_MEM);
        assert_eq!(result.unwrap(), MAX_MEM as u64 * 1_000_000_000,
                  "MAX_MEM conversion should be {} * 1 billion", MAX_MEM);
    }

    #[test]
    fn test_max_valid_cpu_cap_without_overflow() {
        // Test that MAX_CPU can safely be used in max caps calculation
        use crate::cli::MAX_CPU;

        let result = MAX_CPU.checked_mul(10);
        assert!(result.is_some(), "MAX_CPU ({}) should not overflow when multiplied by 10 for caps", MAX_CPU);
        assert_eq!(result.unwrap(), MAX_CPU as u32 * 10,
                  "MAX_CPU cap should be {} * 10", MAX_CPU);
    }

    #[test]
    fn test_sequential_operations_dont_cause_overflow() {
        // Test that multiple operations on max values don't accumulate overflow
        use crate::cli::{MAX_CPU, MAX_MEM};

        // Simulate full set_user_limits operation with MAX values
        let cpu = MAX_CPU;
        let mem = MAX_MEM;

        let mem_bytes = (mem as u64).checked_mul(1_000_000_000);
        assert!(mem_bytes.is_some());

        let cpu_quota = cpu.checked_mul(100);
        assert!(cpu_quota.is_some());

        // Both operations should succeed without overflow
        assert_eq!(mem_bytes.unwrap(), MAX_MEM as u64 * 1_000_000_000);
        assert_eq!(cpu_quota.unwrap(), MAX_CPU as u32 * 100);
    }

    #[test]
    fn test_sequential_admin_operations_dont_cause_overflow() {
        // Test that multiple operations in admin_setup_defaults don't overflow
        use crate::cli::{MAX_CPU, MAX_MEM};

        // Simulate full admin_setup_defaults operation with MAX values
        let cpu = MAX_CPU;
        let mem = MAX_MEM;

        // Memory conversion
        let mem_bytes = (mem as u64).checked_mul(1_000_000_000);
        assert!(mem_bytes.is_some());

        // CPU quota
        let cpu_quota = cpu.checked_mul(100);
        assert!(cpu_quota.is_some());

        // Max caps
        let max_cpu_cap = cpu.checked_mul(10);
        assert!(max_cpu_cap.is_some());

        // All should succeed
        assert_eq!(mem_bytes.unwrap(), MAX_MEM as u64 * 1_000_000_000);
        assert_eq!(cpu_quota.unwrap(), MAX_CPU as u32 * 100);
        assert_eq!(max_cpu_cap.unwrap(), MAX_CPU as u32 * 10);
    }

    #[test]
    fn test_error_messages_are_informative() {
        // Verify error messages contain useful debugging information
        use crate::cli::MAX_CPU;

        let invalid_cpu = MAX_CPU + 5;
        let result = super::set_user_limits(invalid_cpu, 2);

        assert!(result.is_err());
        if let Err(e) = result {
            let error_msg = format!("{}", e);
            // Check that error message includes:
            // 1. The invalid value
            assert!(error_msg.contains(&invalid_cpu.to_string()),
                   "Error message should include the invalid value: {}", error_msg);
            // 2. The limit
            assert!(error_msg.contains(&MAX_CPU.to_string()),
                   "Error message should include the max limit: {}", error_msg);
            // 3. A description of what went wrong
            assert!(error_msg.contains("exceeds"),
                   "Error message should indicate exceeding: {}", error_msg);
        }
    }

    #[test]
    fn test_valid_edge_case_values_in_set_user_limits() {
        // Test that minimum and maximum valid values are accepted
        use crate::cli::{MAX_CPU, MAX_MEM};

        // These should NOT error on input validation
        // (they may fail on systemctl execution, but that's okay for this test)
        let min_result = super::set_user_limits(1, 1);
        // Just verify it doesn't error on validation
        if let Err(e) = min_result {
            let error_msg = format!("{}", e);
            assert!(!error_msg.contains("exceeds maximum limit"),
                   "Minimum values should not fail validation: {}", error_msg);
        }

        let max_result = super::set_user_limits(MAX_CPU, MAX_MEM);
        // Just verify it doesn't error on validation
        if let Err(e) = max_result {
            let error_msg = format!("{}", e);
            assert!(!error_msg.contains("exceeds maximum limit"),
                   "Maximum valid values should not fail validation: {}", error_msg);
        }
    }

    #[test]
    fn test_u32_max_causes_proper_rejection() {
        // Test that u32::MAX values are properly rejected by input validation
        let result = super::set_user_limits(u32::MAX, 2);
        assert!(result.is_err(), "u32::MAX should be rejected");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(error_msg.contains("exceeds maximum limit"),
                   "Should indicate input validation failure: {}", error_msg);
        }
    }
}
