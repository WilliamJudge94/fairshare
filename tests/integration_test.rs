use fairshare::cli::{Cli, Command};
use fairshare::ipc::{IpcClient, Request, Response};
use clap::Parser;

/// Test CLI argument parsing for request command
#[test]
fn test_cli_request_parsing() {
    let args = vec!["fairshare", "request", "--cpu", "4", "--mem", "8G"];
    let cli = Cli::parse_from(args);

    assert_eq!(cli.socket, "/run/fairshare.sock");

    match cli.command {
        Command::Request { cpu, mem } => {
            assert_eq!(cpu, 4);
            assert_eq!(mem, "8G");
        }
        _ => panic!("Expected Request command"),
    }
}

/// Test CLI argument parsing for release command
#[test]
fn test_cli_release_parsing() {
    let args = vec!["fairshare", "release"];
    let cli = Cli::parse_from(args);

    matches!(cli.command, Command::Release);
}

/// Test CLI argument parsing for status command
#[test]
fn test_cli_status_parsing() {
    let args = vec!["fairshare", "status"];
    let cli = Cli::parse_from(args);

    matches!(cli.command, Command::Status);
}

/// Test CLI argument parsing for exec command
#[test]
fn test_cli_exec_parsing() {
    let args = vec!["fairshare", "exec", "bash", "-c", "echo test"];
    let cli = Cli::parse_from(args);

    match cli.command {
        Command::Exec { command } => {
            assert_eq!(command.len(), 4);
            assert_eq!(command[0], "bash");
            assert_eq!(command[1], "-c");
            assert_eq!(command[2], "echo");
            assert_eq!(command[3], "test");
        }
        _ => panic!("Expected Exec command"),
    }
}

/// Test CLI with custom socket path
#[test]
fn test_cli_custom_socket() {
    let args = vec!["fairshare", "--socket", "/tmp/custom.sock", "status"];
    let cli = Cli::parse_from(args);

    assert_eq!(cli.socket, "/tmp/custom.sock");
}

/// Test IPC request serialization - RequestResources
#[test]
fn test_request_resources_serialization() {
    let request = Request::RequestResources {
        cpu: 8,
        mem: "16G".to_string(),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("request_resources"));
    assert!(json.contains("\"cpu\":8"));
    assert!(json.contains("\"mem\":\"16G\""));

    // Test deserialization round-trip
    let deserialized: Request = serde_json::from_str(&json).unwrap();
    match deserialized {
        Request::RequestResources { cpu, mem } => {
            assert_eq!(cpu, 8);
            assert_eq!(mem, "16G");
        }
        _ => panic!("Wrong request type"),
    }
}

/// Test IPC response serialization - Success
#[test]
fn test_response_success_serialization() {
    let response = Response::Success {
        message: "Resources allocated successfully".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("success"));
    assert!(json.contains("Resources allocated successfully"));

    // Test deserialization round-trip
    let deserialized: Response = serde_json::from_str(&json).unwrap();
    match deserialized {
        Response::Success { message } => {
            assert_eq!(message, "Resources allocated successfully");
        }
        _ => panic!("Wrong response type"),
    }
}

/// Test IPC response serialization - Error
#[test]
fn test_response_error_serialization() {
    let response = Response::Error {
        error: "Insufficient resources available".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("error"));
    assert!(json.contains("Insufficient resources available"));

    // Test deserialization round-trip
    let deserialized: Response = serde_json::from_str(&json).unwrap();
    match deserialized {
        Response::Error { error } => {
            assert_eq!(error, "Insufficient resources available");
        }
        _ => panic!("Wrong response type"),
    }
}

/// Test IPC response serialization - StatusInfo
#[test]
fn test_response_status_info_serialization() {
    let response = Response::StatusInfo {
        allocated_cpu: 4,
        allocated_mem: "8G".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("status_info"));
    assert!(json.contains("\"allocated_cpu\":4"));
    assert!(json.contains("\"allocated_mem\":\"8G\""));

    // Test deserialization round-trip
    let deserialized: Response = serde_json::from_str(&json).unwrap();
    match deserialized {
        Response::StatusInfo { allocated_cpu, allocated_mem } => {
            assert_eq!(allocated_cpu, 4);
            assert_eq!(allocated_mem, "8G");
        }
        _ => panic!("Wrong response type"),
    }
}

/// Test that IpcClient can be created with custom timeout
#[test]
fn test_ipc_client_with_timeout() {
    let client = IpcClient::with_timeout("/tmp/test.sock", std::time::Duration::from_secs(10));
    // Just verify it can be created - actual connection test requires running daemon
    drop(client);
}

/// Test that IpcClient can be created with default timeout
#[test]
fn test_ipc_client_default() {
    let client = IpcClient::new("/tmp/test.sock");
    // Just verify it can be created - actual connection test requires running daemon
    drop(client);
}

// Note: End-to-end tests that require a running daemon and systemd are documented
// but not implemented here. These should be run manually or in a properly configured
// test environment with systemd support.
//
// Manual end-to-end test procedure:
// 1. Start the daemon: sudo fairshared /etc/fairshare/policy.d/default.yaml
// 2. Request resources: fairshare request --cpu 4 --mem 8G
// 3. Check status: fairshare status
// 4. Execute command: fairshare exec -- stress-ng --cpu 2 --timeout 10s
// 5. Release resources: fairshare release
// 6. Verify cleanup: fairshare status (should show no allocation)

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    /// Test that connecting to non-existent socket produces proper error
    #[tokio::test]
    async fn test_connection_to_missing_socket() {
        let client = IpcClient::new("/tmp/nonexistent_fairshare_test.sock");
        let request = Request::Status;

        let result = client.send_request(request).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Failed to connect") ||
            error_msg.contains("No such file or directory")
        );
    }
}

#[cfg(test)]
mod cli_validation_tests {
    use super::*;

    /// Test that CPU values are properly parsed
    #[test]
    fn test_cpu_values() {
        for cpu_val in [1, 2, 4, 8, 16, 32] {
            let cpu_str = cpu_val.to_string();
            let args = vec!["fairshare", "request", "--cpu", &cpu_str, "--mem", "8G"];
            let cli = Cli::parse_from(args);

            match cli.command {
                Command::Request { cpu, .. } => {
                    assert_eq!(cpu, cpu_val);
                }
                _ => panic!("Expected Request command"),
            }
        }
    }

    /// Test various memory formats
    #[test]
    fn test_memory_formats() {
        let test_cases = vec![
            "512M",
            "1G",
            "8G",
            "16G",
            "1024M",
            "2048M",
        ];

        for mem_val in test_cases {
            let args = vec!["fairshare", "request", "--cpu", "4", "--mem", mem_val];
            let cli = Cli::parse_from(args);

            match cli.command {
                Command::Request { mem, .. } => {
                    assert_eq!(mem, mem_val);
                }
                _ => panic!("Expected Request command"),
            }
        }
    }

    /// Test exec with complex commands
    #[test]
    fn test_exec_complex_commands() {
        let args = vec![
            "fairshare",
            "exec",
            "python3",
            "-c",
            "print('hello world')"
        ];
        let cli = Cli::parse_from(args);

        match cli.command {
            Command::Exec { command } => {
                assert_eq!(command[0], "python3");
                assert_eq!(command[1], "-c");
                assert_eq!(command[2], "print('hello");
                assert_eq!(command[3], "world')");
            }
            _ => panic!("Expected Exec command"),
        }
    }
}
