use anyhow::Result;
use tracing::{info, error};
use tracing_subscriber;
use std::path::PathBuf;

mod daemon;
mod policy;
mod systemd_client;
mod ipc;
mod utils;

// Default configuration paths
const DEFAULT_POLICY_PATH: &str = "/etc/fairshare/policy.d/default.yaml";
const DEFAULT_SOCKET_PATH: &str = "/run/fairshare.sock";

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with environment variable support
    // Use RUST_LOG=debug for verbose output
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    info!("Starting fairshared daemon");

    // Parse command line arguments
    let policy_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_POLICY_PATH));

    let socket_path = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));

    info!("Policy path: {:?}", policy_path);
    info!("Socket path: {:?}", socket_path);

    // Check if policy file exists
    if !policy_path.exists() {
        error!("Policy file does not exist: {:?}", policy_path);
        error!("Please create a policy file or specify a valid path as the first argument");
        error!("Example policy file location: /etc/fairshare/policy.d/default.yaml");
        return Err(anyhow::anyhow!("Policy file not found: {:?}", policy_path));
    }

    // Run the daemon
    match daemon::run(policy_path, socket_path).await {
        Ok(_) => {
            info!("fairshared daemon stopped gracefully");
            Ok(())
        }
        Err(e) => {
            error!("fairshared daemon error: {}", e);
            Err(e)
        }
    }
}
