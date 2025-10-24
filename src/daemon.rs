use anyhow::{Result, Context};
use tokio::sync::RwLock;
use tracing::{info, debug, warn, error};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;

use crate::policy::PolicyManager;
use crate::systemd_client::SystemdClient;
use crate::ipc::{IpcServer, Request, Response, RequestHandler};

/// Represents an active resource allocation for a user
#[derive(Debug, Clone)]
pub struct Allocation {
    pub uid: u32,
    pub cpu: u32,
    pub mem: String,
}

/// Main daemon structure that coordinates all components
pub struct Daemon {
    policy_manager: Arc<RwLock<PolicyManager>>,
    systemd_client: Arc<SystemdClient>,
    ipc_server: IpcServer,
    allocations: Arc<RwLock<HashMap<u32, Allocation>>>,
}

impl Daemon {
    /// Create a new daemon instance
    pub async fn new(policy_path: PathBuf, socket_path: PathBuf) -> Result<()> {
        info!("Initializing daemon components");

        // Initialize policy manager
        let mut policy_manager = PolicyManager::new(policy_path.to_str().unwrap());
        policy_manager.load_policies()
            .context("Failed to load policies")?;
        let policy_manager = Arc::new(RwLock::new(policy_manager));

        // Initialize systemd DBus client
        let systemd_client = SystemdClient::new()
            .await
            .context("Failed to initialize systemd client")?;
        let systemd_client = Arc::new(systemd_client);

        // Initialize IPC server
        let mut ipc_server = IpcServer::new(socket_path.to_str().unwrap());
        ipc_server.start()
            .await
            .context("Failed to start IPC server")?;

        // Initialize allocations tracking
        let allocations = Arc::new(RwLock::new(HashMap::new()));

        // Create the daemon instance
        let mut daemon = Daemon {
            policy_manager,
            systemd_client,
            ipc_server,
            allocations,
        };

        // Start the daemon's main event loop
        daemon.start().await?;

        Ok(())
    }

    /// Start the daemon's main event loop
    async fn start(&mut self) -> Result<()> {
        info!("Starting daemon event loop");

        // Create a handler for IPC requests
        let handler = DaemonRequestHandler {
            policy_manager: self.policy_manager.clone(),
            systemd_client: self.systemd_client.clone(),
            allocations: self.allocations.clone(),
        };

        let handler = Arc::new(handler);

        // Start accepting IPC connections
        // This runs indefinitely until the process is terminated
        self.ipc_server.accept_connections(handler).await?;

        Ok(())
    }

    /// Gracefully shutdown the daemon
    async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down daemon");

        // Stop IPC server
        self.ipc_server.stop().await
            .context("Failed to stop IPC server")?;

        // Clean up all active slices
        let allocations = self.allocations.read().await;
        for (uid, _allocation) in allocations.iter() {
            info!("Cleaning up slice for UID: {}", uid);
            if let Err(e) = self.systemd_client.remove_slice(*uid).await {
                warn!("Failed to remove slice for UID {}: {}", uid, e);
            }
        }

        info!("Daemon shutdown complete");

        Ok(())
    }
}

/// Implementation of RequestHandler trait for the daemon
struct DaemonRequestHandler {
    policy_manager: Arc<RwLock<PolicyManager>>,
    systemd_client: Arc<SystemdClient>,
    allocations: Arc<RwLock<HashMap<u32, Allocation>>>,
}

#[async_trait::async_trait]
impl RequestHandler for DaemonRequestHandler {
    async fn handle_request(&self, request: Request, uid: u32) -> Response {
        debug!("Handling request {:?} for UID {}", request, uid);

        match request {
            Request::RequestResources { cpu, mem } => {
                self.handle_request_resources(uid, cpu, mem).await
            }
            Request::Release => {
                self.handle_release(uid).await
            }
            Request::Status => {
                self.handle_status(uid).await
            }
        }
    }
}

impl DaemonRequestHandler {
    /// Handle resource allocation request
    async fn handle_request_resources(&self, uid: u32, cpu: u32, mem: String) -> Response {
        info!("Processing resource request for UID {}: cpu={}, mem={}", uid, cpu, mem);

        // Validate request against policy
        let policy_manager = self.policy_manager.read().await;
        if let Err(e) = policy_manager.validate_request(cpu, &mem) {
            error!("Resource request validation failed for UID {}: {}", uid, e);
            return Response::Error {
                error: format!("Request validation failed: {}", e),
            };
        }
        drop(policy_manager); // Release the read lock

        // Check if user already has an allocation
        let mut allocations = self.allocations.write().await;
        if allocations.contains_key(&uid) {
            warn!("UID {} already has an active allocation", uid);
            return Response::Error {
                error: "User already has an active resource allocation. Release it first.".to_string(),
            };
        }

        // Create systemd slice
        if let Err(e) = self.systemd_client.create_slice(uid, cpu, &mem).await {
            error!("Failed to create systemd slice for UID {}: {}", uid, e);
            return Response::Error {
                error: format!("Failed to create systemd slice: {}", e),
            };
        }

        // Track the allocation
        let allocation = Allocation {
            uid,
            cpu,
            mem: mem.clone(),
        };
        allocations.insert(uid, allocation);

        info!("Successfully allocated resources for UID {}: cpu={}, mem={}", uid, cpu, mem);

        Response::Success {
            message: format!("Resources allocated: {} CPUs, {} memory", cpu, mem),
        }
    }

    /// Handle resource release request
    async fn handle_release(&self, uid: u32) -> Response {
        info!("Processing resource release for UID {}", uid);

        // Check if user has an allocation
        let mut allocations = self.allocations.write().await;
        if !allocations.contains_key(&uid) {
            warn!("UID {} has no active allocation to release", uid);
            return Response::Error {
                error: "No active resource allocation found for this user".to_string(),
            };
        }

        // Remove systemd slice
        if let Err(e) = self.systemd_client.remove_slice(uid).await {
            error!("Failed to remove systemd slice for UID {}: {}", uid, e);
            return Response::Error {
                error: format!("Failed to remove systemd slice: {}", e),
            };
        }

        // Remove allocation tracking
        allocations.remove(&uid);

        info!("Successfully released resources for UID {}", uid);

        Response::Success {
            message: "Resources released successfully".to_string(),
        }
    }

    /// Handle status request
    async fn handle_status(&self, uid: u32) -> Response {
        debug!("Processing status request for UID {}", uid);

        // Check if user has an allocation
        let allocations = self.allocations.read().await;
        if let Some(allocation) = allocations.get(&uid) {
            info!("Status for UID {}: cpu={}, mem={}", uid, allocation.cpu, allocation.mem);
            Response::StatusInfo {
                allocated_cpu: allocation.cpu,
                allocated_mem: allocation.mem.clone(),
            }
        } else {
            debug!("No active allocation found for UID {}", uid);
            Response::Error {
                error: "No active resource allocation found for this user".to_string(),
            }
        }
    }
}

/// Entry point for the daemon
pub async fn run(policy_path: PathBuf, socket_path: PathBuf) -> Result<()> {
    info!("Starting fairshared daemon");

    Daemon::new(policy_path, socket_path).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_creation() {
        let allocation = Allocation {
            uid: 1000,
            cpu: 4,
            mem: "16G".to_string(),
        };

        assert_eq!(allocation.uid, 1000);
        assert_eq!(allocation.cpu, 4);
        assert_eq!(allocation.mem, "16G");
    }

    #[tokio::test]
    async fn test_allocations_tracking() {
        let allocations: Arc<RwLock<HashMap<u32, Allocation>>> = Arc::new(RwLock::new(HashMap::new()));

        // Add allocation
        {
            let mut allocs = allocations.write().await;
            allocs.insert(1000, Allocation {
                uid: 1000,
                cpu: 2,
                mem: "8G".to_string(),
            });
        }

        // Check allocation exists
        {
            let allocs = allocations.read().await;
            assert!(allocs.contains_key(&1000));
            let allocation = allocs.get(&1000).unwrap();
            assert_eq!(allocation.cpu, 2);
            assert_eq!(allocation.mem, "8G");
        }

        // Remove allocation
        {
            let mut allocs = allocations.write().await;
            allocs.remove(&1000);
        }

        // Check allocation is gone
        {
            let allocs = allocations.read().await;
            assert!(!allocs.contains_key(&1000));
        }
    }
}
