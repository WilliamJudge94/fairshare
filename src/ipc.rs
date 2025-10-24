use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, debug, error};
use std::path::Path;
use std::os::unix::fs::PermissionsExt;
use std::fs;

/// IPC request types for resource allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Request resources for the calling user
    RequestResources { cpu: u32, mem: String },
    /// Release resources for the calling user
    Release,
    /// Get status of current allocation
    Status,
}

/// IPC response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Success response
    Success { message: String },
    /// Error response
    Error { error: String },
    /// Status information response
    StatusInfo { allocated_cpu: u32, allocated_mem: String },
}

/// Handler trait for processing IPC requests
/// This allows the daemon to inject its own logic for handling requests
#[async_trait::async_trait]
pub trait RequestHandler: Send + Sync {
    async fn handle_request(&self, request: Request, uid: u32) -> Response;
}

/// IPC server that listens on a Unix socket
pub struct IpcServer {
    socket_path: String,
    listener: Option<UnixListener>,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            listener: None,
        }
    }

    /// Start the IPC server
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting IPC server on: {}", self.socket_path);

        // Remove existing socket file if present
        if Path::new(&self.socket_path).exists() {
            debug!("Removing existing socket file: {}", self.socket_path);
            fs::remove_file(&self.socket_path)
                .with_context(|| format!("Failed to remove existing socket: {}", self.socket_path))?;
        }

        // Ensure parent directory exists
        if let Some(parent) = Path::new(&self.socket_path).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create socket directory: {:?}", parent))?;
            }
        }

        // Create Unix domain socket listener
        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("Failed to bind Unix socket: {}", self.socket_path))?;

        // Set appropriate permissions on socket (0666 - readable/writable by all)
        let metadata = fs::metadata(&self.socket_path)
            .with_context(|| format!("Failed to get socket metadata: {}", self.socket_path))?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o666);
        fs::set_permissions(&self.socket_path, permissions)
            .with_context(|| format!("Failed to set socket permissions: {}", self.socket_path))?;

        self.listener = Some(listener);

        info!("IPC server started successfully on: {}", self.socket_path);

        Ok(())
    }

    /// Accept and handle incoming connections
    pub async fn accept_connections<H>(&self, handler: std::sync::Arc<H>) -> Result<()>
    where
        H: RequestHandler + 'static,
    {
        info!("Accepting IPC connections");

        let listener = self.listener.as_ref()
            .ok_or_else(|| anyhow::anyhow!("IPC server not started"))?;

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    debug!("Accepted new IPC connection");
                    let handler_clone = handler.clone();

                    // Spawn a task to handle this connection
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, handler_clone).await {
                            error!("Error handling IPC client: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting IPC connection: {}", e);
                    // Continue accepting other connections
                }
            }
        }
    }

    /// Handle a single client connection
    async fn handle_client<H>(mut stream: UnixStream, handler: std::sync::Arc<H>) -> Result<()>
    where
        H: RequestHandler,
    {
        debug!("Handling new IPC client connection");

        // Get peer credentials (UID) for authentication
        let ucred = stream.peer_cred()
            .context("Failed to get peer credentials")?;
        let uid = ucred.uid();

        debug!("Client UID: {}", uid);

        // Read request from stream (one line of JSON)
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).await
            .context("Failed to read request from client")?;

        debug!("Received request: {}", line.trim());

        // Parse JSON request
        let request: Request = serde_json::from_str(&line)
            .context("Failed to parse JSON request")?;

        debug!("Parsed request: {:?}", request);

        // Process request using the handler
        let response = handler.handle_request(request, uid).await;

        debug!("Sending response: {:?}", response);

        // Send JSON response
        let response_json = serde_json::to_string(&response)
            .context("Failed to serialize response")?;

        stream.write_all(response_json.as_bytes()).await
            .context("Failed to write response to client")?;
        stream.write_all(b"\n").await
            .context("Failed to write newline to client")?;

        stream.flush().await
            .context("Failed to flush response to client")?;

        debug!("Response sent successfully");

        Ok(())
    }

    /// Stop the IPC server
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping IPC server");

        // Drop the listener to stop accepting new connections
        self.listener = None;

        // Remove socket file
        if Path::new(&self.socket_path).exists() {
            fs::remove_file(&self.socket_path)
                .with_context(|| format!("Failed to remove socket file: {}", self.socket_path))?;
            debug!("Removed socket file: {}", self.socket_path);
        }

        info!("IPC server stopped successfully");

        Ok(())
    }
}

/// IPC client for sending requests to the daemon
pub struct IpcClient {
    socket_path: String,
    timeout: std::time::Duration,
}

impl IpcClient {
    /// Create a new IPC client with default timeout (5 seconds)
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout: std::time::Duration::from_secs(5),
        }
    }

    /// Create a new IPC client with custom timeout
    pub fn with_timeout(socket_path: impl Into<String>, timeout: std::time::Duration) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout,
        }
    }

    /// Send a request to the daemon
    pub async fn send_request(&self, request: Request) -> Result<Response> {
        debug!("Sending IPC request: {:?}", request);

        // Wrap the entire operation in a timeout
        let result = tokio::time::timeout(self.timeout, async {
            // Connect to Unix socket
            let mut stream = UnixStream::connect(&self.socket_path)
                .await
                .with_context(|| format!("Failed to connect to Unix socket: {}", self.socket_path))?;

            // Serialize and send request
            let request_json = serde_json::to_string(&request)
                .context("Failed to serialize request")?;

            stream.write_all(request_json.as_bytes()).await
                .context("Failed to write request")?;
            stream.write_all(b"\n").await
                .context("Failed to write newline")?;

            stream.flush().await
                .context("Failed to flush request")?;

            debug!("Request sent, waiting for response");

            // Read and deserialize response
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            reader.read_line(&mut line).await
                .context("Failed to read response")?;

            debug!("Received response: {}", line.trim());

            let response: Response = serde_json::from_str(&line)
                .context("Failed to parse JSON response")?;

            Ok::<Response, anyhow::Error>(response)
        }).await;

        match result {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow::anyhow!("Request timed out after {} seconds", self.timeout.as_secs())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_request_serialization() {
        // Test RequestResources serialization
        let req = Request::RequestResources {
            cpu: 4,
            mem: "16G".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("request_resources"));
        assert!(json.contains("\"cpu\":4"));
        assert!(json.contains("\"mem\":\"16G\""));

        // Test deserialization
        let deserialized: Request = serde_json::from_str(&json).unwrap();
        match deserialized {
            Request::RequestResources { cpu, mem } => {
                assert_eq!(cpu, 4);
                assert_eq!(mem, "16G");
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_release_serialization() {
        let req = Request::Release;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("release"));

        let deserialized: Request = serde_json::from_str(&json).unwrap();
        match deserialized {
            Request::Release => {},
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_status_serialization() {
        let req = Request::Status;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("status"));

        let deserialized: Request = serde_json::from_str(&json).unwrap();
        match deserialized {
            Request::Status => {},
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_response_success_serialization() {
        let resp = Response::Success {
            message: "Resources allocated".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("success"));
        assert!(json.contains("Resources allocated"));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Success { message } => {
                assert_eq!(message, "Resources allocated");
            }
            _ => panic!("Wrong response type"),
        }
    }

    #[test]
    fn test_response_error_serialization() {
        let resp = Response::Error {
            error: "Insufficient resources".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("Insufficient resources"));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::Error { error } => {
                assert_eq!(error, "Insufficient resources");
            }
            _ => panic!("Wrong response type"),
        }
    }

    #[test]
    fn test_response_status_info_serialization() {
        let resp = Response::StatusInfo {
            allocated_cpu: 4,
            allocated_mem: "16G".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("status_info"));
        assert!(json.contains("\"allocated_cpu\":4"));
        assert!(json.contains("\"allocated_mem\":\"16G\""));

        let deserialized: Response = serde_json::from_str(&json).unwrap();
        match deserialized {
            Response::StatusInfo { allocated_cpu, allocated_mem } => {
                assert_eq!(allocated_cpu, 4);
                assert_eq!(allocated_mem, "16G");
            }
            _ => panic!("Wrong response type"),
        }
    }

    #[tokio::test]
    async fn test_ipc_server_creation() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        let socket_path_str = socket_path.to_str().unwrap();

        let mut server = IpcServer::new(socket_path_str);
        assert!(server.listener.is_none());

        // Start should succeed
        let result = server.start().await;
        assert!(result.is_ok(), "Failed to start server: {:?}", result.err());
        assert!(server.listener.is_some());

        // Socket file should exist
        assert!(socket_path.exists());

        // Stop should succeed
        let result = server.stop().await;
        assert!(result.is_ok());
        assert!(server.listener.is_none());

        // Socket file should be removed
        assert!(!socket_path.exists());
    }
}
