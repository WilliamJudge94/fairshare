// Integration tests that test the overall behavior of fairshare
// without requiring actual system privileges

use std::process::Command;

#[test]
fn test_full_workflow_help_and_status() {
    // Test that basic commands work end-to-end
    let help_output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to run help");

    assert!(help_output.status.success());

    let status_output = Command::new("cargo")
        .args(["run", "--", "status"])
        .output()
        .expect("Failed to run status");

    if !status_output.status.success() {
        let stderr = String::from_utf8_lossy(&status_output.stderr);
        
        // check only for linux
        #[cfg(not(target_os = "linux"))]
        if stderr.contains("Failed to get user allocations") || stderr.contains("No such file") {
            return;
        }
    }
    assert!(status_output.status.success());
}

#[test]
fn test_request_validation() {
    // Test that request command validates arguments properly
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "1", "--mem", "2", "--disk", "1"])
        .output()
        .expect("Failed to run request");

    // May succeed or fail depending on permissions, but should not panic
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should either succeed with allocation or fail with permission/resource error
    assert!(
        stdout.contains("Allocated")
            || stderr.contains("Failed")
            || stderr.contains("exceeds")
            || stderr.contains("permission")
    );
}

#[test]
fn test_request_with_invalid_resources() {
    // Test requesting unreasonably large resources
    let output = Command::new("cargo")
        .args(["run", "--", "request", "--cpu", "999999", "--mem", "999999"])
        .output()
        .expect("Failed to run request");

    // Should fail due to exceeding system resources
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should indicate failure (either resource limit or permission)
    assert!(
        !output.status.success()
            || stdout.contains("exceeds")
            || stderr.contains("Failed")
            || stderr.contains("exceeds")
    );
}

#[test]
fn test_multiple_command_execution() {
    // Test that we can run multiple commands in sequence
    let commands = vec!["status", "info"];

    for cmd in commands {
        let output = Command::new("cargo")
            .args(["run", "--", cmd])
            .output()
            .expect(&format!("Failed to run {}", cmd));

        // Commands should execute without panicking
        // They may fail due to permissions, but should not crash
        assert!(
            output.status.success()
                || String::from_utf8_lossy(&output.stderr).contains("Failed")
                || String::from_utf8_lossy(&output.stdout).contains("MemoryMax")
        );
    }
}

#[test]
fn test_admin_setup_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "admin", "setup", "--help"])
        .output()
        .expect("Failed to run admin setup help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Setup global baseline"));
}
