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
