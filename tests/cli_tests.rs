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

    // Check for expected output format
    assert!(stdout.contains("System total") || stdout.contains("GB RAM"));
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

    // Should contain either success output or an error message
    assert!(
        stdout.contains("MemoryMax") ||
        stderr.contains("Failed") ||
        stdout.contains("MemoryMax") ||
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
