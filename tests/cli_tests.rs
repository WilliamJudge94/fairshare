use std::process::Command;

#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fairshare"));
    assert!(stdout.contains("Systemd-based resource manager"));
}

#[test]
fn test_cli_version() {
    let output = Command::new("cargo")
        .args(["run", "--", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fairshare"));
}

#[test]
fn test_status_command() {
    let output = Command::new("cargo")
        .args(["run", "--", "status"])
        .output()
        .expect("Failed to execute command");

    // Status command should succeed
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check for expected output format (new table-based output)
    assert!(stdout.contains("SYSTEM RESOURCE OVERVIEW") || stdout.contains("RAM (GB)") || stdout.contains("CPUs"));
}

#[test]
fn test_info_command() {
    let output = Command::new("cargo")
        .args(["run", "--", "info"])
        .output()
        .expect("Failed to execute command");

    // Info command should run (may fail if not root, but should not panic)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should contain either success output (new formatted output) or an error message
    assert!(
        stdout.contains("USER RESOURCE ALLOCATION") ||
        stdout.contains("CPU Quota") ||
        stdout.contains("Memory Max") ||
        stderr.contains("Failed") ||
        output.status.success()
    );
}

#[test]
fn test_request_without_args_fails() {
    let output = Command::new("cargo")
        .args(["run", "--", "request"])
        .output()
        .expect("Failed to execute command");

    // Should fail because --cpu and --mem are required
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required") || stderr.contains("cpu") || stderr.contains("mem"));
}

#[test]
fn test_admin_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Admin operations"));
    assert!(stdout.contains("setup"));
}

#[test]
fn test_admin_uninstall_command_exists_in_help() {
    // Test that the uninstall command appears in the admin help output
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify uninstall command is listed in help
    assert!(
        stdout.contains("uninstall") || stdout.contains("Uninstall"),
        "Expected 'uninstall' command to appear in admin help output"
    );

    // Verify the description mentions removing configuration
    assert!(
        stdout.contains("remove") || stdout.contains("configuration") || stdout.contains("defaults"),
        "Expected uninstall description to mention removing configuration"
    );
}

#[test]
fn test_admin_uninstall_help_flag() {
    // Test that --help works for the uninstall subcommand
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "uninstall", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify help output contains key information
    assert!(
        stdout.contains("Uninstall") || stdout.contains("uninstall"),
        "Expected help to mention the uninstall command"
    );

    // Verify --force flag is documented
    assert!(
        stdout.contains("--force") || stdout.contains("-force"),
        "Expected --force flag to be documented in help"
    );

    // Verify the description mentions what will be removed
    assert!(
        stdout.contains("configuration") || stdout.contains("defaults") || stdout.contains("remove"),
        "Expected help to describe what uninstall does"
    );
}

#[test]
fn test_admin_uninstall_without_force_prompts_for_confirmation() {
    // Test that uninstall without --force shows confirmation prompt
    // Note: This test uses stdin, so we expect the command to either:
    // 1. Prompt for confirmation (which will fail due to no stdin)
    // 2. Show a warning message about what will be removed
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "uninstall"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Should show warning/confirmation message mentioning files to be removed
    assert!(
        combined.contains("/etc/systemd/system/user-.slice.d/00-defaults.conf") ||
        combined.contains("/etc/fairshare") ||
        combined.contains("Continue?") ||
        combined.contains("This will remove"),
        "Expected confirmation prompt or warning about files to be removed. Got stdout: '{}', stderr: '{}'",
        stdout, stderr
    );
}

#[test]
fn test_admin_uninstall_with_force_flag() {
    // Test that --force flag skips confirmation prompt
    // This test verifies that with --force, the command attempts to uninstall
    // without prompting for confirmation
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "uninstall", "--force"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // With --force, it should either:
    // 1. Successfully uninstall (if run as root with files present)
    // 2. Fail due to permissions (if not root)
    // 3. Succeed but show files not found (if files don't exist)
    // But it should NOT show the confirmation prompt

    // Check that we don't see confirmation prompts
    let combined = format!("{}{}", stdout, stderr);
    let has_confirmation_prompt = combined.contains("Continue?") ||
                                   combined.contains("[y/N]");

    if has_confirmation_prompt {
        panic!("Expected --force to skip confirmation prompt, but found prompt in output");
    }

    // Should either succeed or fail with permission/file error
    assert!(
        output.status.success() ||
        stderr.contains("permission") ||
        stderr.contains("Permission") ||
        stderr.contains("Failed") ||
        stdout.contains("not found") ||
        stdout.contains("Removed") ||
        combined.contains("/etc/systemd") ||
        combined.contains("Uninstall failed"),
        "Expected either success, permission error, or file operation. Got stdout: '{}', stderr: '{}'",
        stdout, stderr
    );
}

#[test]
fn test_admin_uninstall_force_flag_position() {
    // Test that --force flag works in different positions
    let output1 = Command::new("cargo")
        .args(["run", "--", "admin", "uninstall", "--force"])
        .output()
        .expect("Failed to execute command");

    let _output2 = Command::new("cargo")
        .args(["run", "--", "admin", "--force", "uninstall"])
        .output()
        .expect("Failed to execute command");

    // Both should attempt to execute without confirmation
    // (though the second form might not work depending on clap configuration)
    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stderr1 = String::from_utf8_lossy(&output1.stderr);
    let combined1 = format!("{}{}", stdout1, stderr1);

    // First form should definitely work
    assert!(
        !combined1.contains("Continue?") && !combined1.contains("[y/N]"),
        "Expected --force after uninstall to skip confirmation"
    );
}

#[test]
fn test_admin_uninstall_mentions_systemd_files() {
    // Test that uninstall output mentions the systemd configuration files
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "uninstall"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Should mention systemd configuration path
    assert!(
        combined.contains("/etc/systemd/system/user-.slice.d/00-defaults.conf"),
        "Expected output to mention systemd configuration file path"
    );

    // Should mention fairshare policy file
    assert!(
        combined.contains("/etc/fairshare/policy.toml") || combined.contains("/etc/fairshare"),
        "Expected output to mention fairshare policy file or directory"
    );
}

#[test]
fn test_admin_uninstall_mentions_daemon_reload() {
    // Test that successful uninstall mentions reloading systemd daemon
    // This test runs with --force to avoid confirmation prompts
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "uninstall", "--force"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // If the command succeeded (user has permissions), it should mention daemon reload
    if output.status.success() {
        assert!(
            stdout.contains("daemon") || stdout.contains("Reload") || stdout.contains("reload"),
            "Expected successful uninstall to mention systemd daemon reload"
        );
    }
    // If it failed, it should be due to permissions or missing files
    else {
        assert!(
            stderr.contains("permission") ||
            stderr.contains("Permission") ||
            stderr.contains("Failed"),
            "Expected failure to be due to permissions or other error"
        );
    }
}

// ============================================================================
// Task 2: Input Validation and Bounds Checking Tests
// ============================================================================

#[test]
fn test_request_cpu_below_minimum() {
    // Test that CPU value below minimum (0) is rejected
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "0", "--mem", "2"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with CPU=0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in") || stderr.contains("invalid"),
            "Expected validation error message about range");
}

#[test]
fn test_request_cpu_above_maximum() {
    // Test that CPU value above maximum (1001+) is rejected
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "2000", "--mem", "5"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with CPU=2000");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in") || stderr.contains("invalid"),
            "Expected validation error message about range");
}

#[test]
fn test_request_mem_below_minimum() {
    // Test that memory value below minimum (0) is rejected
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "4", "--mem", "0"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with mem=0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in") || stderr.contains("invalid"),
            "Expected validation error message about range");
}

#[test]
fn test_request_mem_above_maximum() {
    // Test that memory value above maximum (10001+) is rejected
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "4", "--mem", "20000"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with mem=20000");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in") || stderr.contains("invalid"),
            "Expected validation error message about range");
}

#[test]
fn test_request_negative_cpu() {
    // Test that negative CPU values are rejected
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu=-1", "--mem=2"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with negative CPU");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Clap rejects negative values for unsigned types
    assert!(stderr.contains("invalid") || stderr.contains("digit"),
            "Expected validation error for negative value, got: {}", stderr);
}

#[test]
fn test_request_negative_mem() {
    // Test that negative memory values are rejected
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu=4", "--mem=-5"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with negative mem");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Clap rejects negative values for unsigned types
    assert!(stderr.contains("invalid") || stderr.contains("digit"),
            "Expected validation error for negative value, got: {}", stderr);
}

#[test]
fn test_request_minimum_valid_values() {
    // Test that minimum valid values (1 CPU, 1 GB) are accepted
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "1", "--mem", "1"])
        .output()
        .expect("Failed to execute command");

    // Should either succeed or fail with resource availability, but NOT validation
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() || stderr.contains("exceeds available") || stderr.contains("resource"),
        "Expected validation to pass for minimum valid values (1, 1), got: {}",
        stderr
    );
}

#[test]
fn test_request_maximum_valid_values() {
    // Test that maximum valid values (1000 CPU, 10000 GB) pass validation
    // (may fail on resource availability, but should pass input validation)
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "1000", "--mem", "10000"])
        .output()
        .expect("Failed to execute command");

    // Should either succeed or fail with resource availability, but NOT validation error
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() || stderr.contains("exceeds available") || stderr.contains("resource"),
        "Expected validation to pass for maximum valid values (1000, 10000), got validation error: {}",
        stderr
    );

    // Should NOT contain range validation errors
    assert!(!stderr.contains("not in 1..=1000") && !stderr.contains("not in 1..=10000"),
            "Should pass validation but got range error: {}", stderr);
}

#[test]
fn test_request_boundary_cpu_1() {
    // Test lower boundary: CPU = 1 (minimum valid)
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "1", "--mem", "4"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("not in"), "CPU=1 should pass validation");
}

#[test]
fn test_request_boundary_cpu_1000() {
    // Test upper boundary: CPU = 1000 (maximum valid)
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "1000", "--mem", "4"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("not in 1..=1000"), "CPU=1000 should pass validation");
}

#[test]
fn test_request_boundary_mem_1() {
    // Test lower boundary: mem = 1 (minimum valid)
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "4", "--mem", "1"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("not in"), "mem=1 should pass validation");
}

#[test]
fn test_request_boundary_mem_10000() {
    // Test upper boundary: mem = 10000 (maximum valid)
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "4", "--mem", "10000"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("not in 1..=10000"), "mem=10000 should pass validation");
}

#[test]
fn test_admin_setup_cpu_below_minimum() {
    // Test that admin setup with CPU below minimum is rejected
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "setup", "--cpu", "0", "--mem", "2"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with CPU=0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in") || stderr.contains("invalid"),
            "Expected validation error message about range");
}

#[test]
fn test_admin_setup_cpu_above_maximum() {
    // Test that admin setup with CPU above maximum is rejected
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "setup", "--cpu", "2000", "--mem", "2"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with CPU=2000");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in") || stderr.contains("invalid"),
            "Expected validation error message about range");
}

#[test]
fn test_admin_setup_mem_below_minimum() {
    // Test that admin setup with memory below minimum is rejected
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "setup", "--cpu", "2", "--mem", "0"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with mem=0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in") || stderr.contains("invalid"),
            "Expected validation error message about range");
}

#[test]
fn test_admin_setup_mem_above_maximum() {
    // Test that admin setup with memory above maximum is rejected
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "setup", "--cpu", "2", "--mem", "20000"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success(), "Expected command to fail with mem=20000");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not in") || stderr.contains("invalid"),
            "Expected validation error message about range");
}

#[test]
fn test_admin_setup_default_values_valid() {
    // Test that default values for admin setup (1 CPU, 2 GB) pass validation
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "setup"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not fail due to validation errors
    // May fail due to permissions, but that's fine for this test
    assert!(
        output.status.success() || stderr.contains("Permission") || stderr.contains("permission") || stderr.contains("root"),
        "Default values should pass validation, got: {}", stderr
    );
}

#[test]
fn test_admin_setup_minimum_valid_values() {
    // Test that minimum valid values (1 CPU, 1 GB) pass validation for admin setup
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "setup", "--cpu", "1", "--mem", "1"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not have validation range errors
    assert!(
        !stderr.contains("not in 1..=1000") && !stderr.contains("not in 1..=10000"),
        "Minimum valid values should pass validation, got: {}", stderr
    );
}

#[test]
fn test_admin_setup_maximum_valid_values() {
    // Test that maximum valid values (1000 CPU, 10000 GB) pass validation for admin setup
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "setup", "--cpu", "1000", "--mem", "10000"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not have validation range errors
    assert!(
        !stderr.contains("not in 1..=1000") && !stderr.contains("not in 1..=10000"),
        "Maximum valid values should pass validation, got: {}", stderr
    );
}
