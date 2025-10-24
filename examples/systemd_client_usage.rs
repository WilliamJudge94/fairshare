// Example usage of SystemdClient
//
// This example demonstrates how to use the SystemdClient to manage
// systemd slices for resource allocation.
//
// Run with: cargo run --example systemd_client_usage
//
// NOTE: Requires root privileges and systemd

use anyhow::Result;
use tracing::{info, error};
use tracing_subscriber;

// Import the systemd client (in real usage, this would be from the crate)
// use fairshared::systemd_client::{SystemdClient, SliceInfo};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("Systemd Client Example");
    info!("======================");

    // Check if running as root
    if unsafe { libc::geteuid() } != 0 {
        error!("This example requires root privileges to manage systemd units");
        error!("Please run with: sudo cargo run --example systemd_client_usage");
        std::process::exit(1);
    }

    // Since we can't import the module in examples without it being a library,
    // this is a template showing how to use it:

    println!("
Example Usage Pattern:
=====================

1. Create a SystemdClient
   let client = SystemdClient::new().await?;

2. Create a slice for user 1001 with 2 CPUs and 8GB RAM
   client.create_slice(1001, 2, \"8G\").await?;

3. Get slice status
   let status = client.get_slice_status(1001).await?;
   println!(\"Slice: {{}}\", status.name);
   println!(\"Active: {{}}\", status.active_state);
   println!(\"CPU Quota: {{:?}}\", status.cpu_quota);
   println!(\"Memory Max: {{:?}}\", status.memory_max);

4. List all slices
   let slices = client.list_slices().await?;
   for slice in slices {{
       println!(\"  - {{}}\", slice);
   }}

5. Check if slice exists
   if client.slice_exists(\"fairshare-1001.slice\").await? {{
       println!(\"Slice exists\");
   }}

6. Move a process to the slice
   client.move_process_to_slice(12345, \"fairshare-1001.slice\").await?;

7. Remove the slice
   client.remove_slice(1001).await?;
");

    // Example data structures
    println!("
Data Structures:
================

SliceInfo {{
    name: String,              // e.g., \"fairshare-1001.slice\"
    active_state: String,      // e.g., \"active\", \"inactive\"
    load_state: String,        // e.g., \"loaded\", \"not-found\"
    sub_state: String,         // e.g., \"running\", \"dead\"
    cpu_quota: Option<u64>,    // CPU quota in microseconds (200000 = 2 CPUs)
    memory_max: Option<u64>,   // Memory limit in bytes
    tasks_max: Option<u64>,    // Maximum tasks (typically 4096)
}}
");

    // Example CPU quota calculations
    println!("
CPU Quota Conversions:
=====================
1 CPU  = 100,000 microseconds per 100ms (100%)
2 CPUs = 200,000 microseconds per 100ms (200%)
4 CPUs = 400,000 microseconds per 100ms (400%)
8 CPUs = 800,000 microseconds per 100ms (800%)
");

    // Example memory conversions
    println!("
Memory Size Conversions:
=======================
\"1G\"   -> 1,073,741,824 bytes
\"8G\"   -> 8,589,934,592 bytes
\"512M\" -> 536,870,912 bytes
\"1024K\"-> 1,048,576 bytes
\"2048\" -> 2,048 bytes
");

    // Example error handling
    println!("
Error Handling:
==============
All functions return Result<T> with context:

match client.create_slice(1001, 2, \"8G\").await {{
    Ok(_) => println!(\"Slice created successfully\"),
    Err(e) => {{
        eprintln!(\"Failed to create slice: {{}}\", e);
        // Error will include context chain:
        // - Failed to start transient unit
        // - Failed to create systemd manager proxy
        // - Failed to connect to system DBus
    }}
}}
");

    println!("
Manual Testing with busctl:
==========================
# Create a slice
busctl call org.freedesktop.systemd1 \\
  /org/freedesktop/systemd1 \\
  org.freedesktop.systemd1.Manager \\
  StartTransientUnit \\
  \"ssa(sv)a(sa(sv))\" \\
  \"fairshare-1001.slice\" \"fail\" \\
  5 \\
  \"Description\" s \"Fairshare slice for UID 1001\" \\
  \"CPUQuota\" t 200000 \\
  \"MemoryMax\" t 8589934592 \\
  \"TasksMax\" t 4096 \\
  \"DefaultDependencies\" b false \\
  0

# Check status
systemctl status fairshare-1001.slice

# View properties
systemctl show fairshare-1001.slice | grep -E '(CPU|Memory|Tasks)'

# Stop slice
systemctl stop fairshare-1001.slice
");

    Ok(())
}
