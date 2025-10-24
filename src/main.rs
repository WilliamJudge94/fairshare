use anyhow::Result;
use tracing::{info, error};
use tracing_subscriber;

mod daemon;
mod policy;
mod systemd_client;
mod ipc;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting fairshared daemon");

    // TODO: Parse command line arguments
    // TODO: Load configuration and policy files
    // TODO: Initialize systemd DBus connection
    // TODO: Start IPC server
    // TODO: Start main daemon loop

    match daemon::run().await {
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
