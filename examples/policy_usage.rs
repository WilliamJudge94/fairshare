// Example usage of the Policy System
// This file demonstrates how to use the PolicyManager in the daemon

use anyhow::Result;
use fairshared::policy::PolicyManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Example 1: Load and validate a policy file
    example_load_policy()?;

    // Example 2: Validate user requests
    example_validate_requests()?;

    // Example 3: Get policy limits
    example_get_limits()?;

    // Example 4: Reload policies
    example_reload_policy()?;

    Ok(())
}

fn example_load_policy() -> Result<()> {
    println!("\n=== Example 1: Load and validate a policy file ===");

    // Create a policy manager with the path to the policy file
    let mut manager = PolicyManager::new("/etc/fairshare/policy.d/default.yaml");

    // Load the policies - this will read the YAML file and validate it
    match manager.load_policies() {
        Ok(_) => {
            println!("✓ Policy loaded successfully");

            // Get the configuration
            if let Some(config) = manager.get_config() {
                println!("  Defaults: {} CPUs, {}", config.defaults.cpu, config.defaults.mem);
                println!("  Max:      {} CPUs, {}", config.max.cpu, config.max.mem);
            }
        }
        Err(e) => {
            println!("✗ Failed to load policy: {}", e);
        }
    }

    Ok(())
}

fn example_validate_requests() -> Result<()> {
    println!("\n=== Example 2: Validate user requests ===");

    let mut manager = PolicyManager::new("/etc/fairshare/policy.d/default.yaml");
    manager.load_policies()?;

    // Test various requests
    let test_cases = vec![
        (4, "16G", "Valid request within limits"),
        (8, "32G", "Valid request at maximum limits"),
        (16, "8G", "Invalid - CPU exceeds limit"),
        (4, "64G", "Invalid - Memory exceeds limit"),
        (0, "8G", "Invalid - Zero CPU"),
    ];

    for (cpu, mem, description) in test_cases {
        print!("  Testing: {} ({} CPUs, {}) ... ", description, cpu, mem);

        match manager.validate_request(cpu, mem) {
            Ok(_) => println!("✓ Allowed"),
            Err(e) => println!("✗ Denied: {}", e),
        }
    }

    Ok(())
}

fn example_get_limits() -> Result<()> {
    println!("\n=== Example 3: Get policy limits ===");

    let mut manager = PolicyManager::new("/etc/fairshare/policy.d/default.yaml");
    manager.load_policies()?;

    // Get default limits
    let defaults = manager.get_defaults()?;
    println!("  Default allocation:");
    println!("    CPU: {} cores", defaults.cpu);
    println!("    Memory: {}", defaults.mem);

    // Get maximum limits
    let max = manager.get_max()?;
    println!("  Maximum allowed:");
    println!("    CPU: {} cores", max.cpu);
    println!("    Memory: {}", max.mem);

    Ok(())
}

fn example_reload_policy() -> Result<()> {
    println!("\n=== Example 4: Reload policies ===");

    let mut manager = PolicyManager::new("/etc/fairshare/policy.d/default.yaml");
    manager.load_policies()?;

    println!("  Initial load complete");

    // Simulate a policy file update (in real scenario, file would be modified externally)
    // Then reload the policies
    manager.reload_policies()?;

    println!("  ✓ Policy reloaded successfully");

    Ok(())
}

// Example: Integration with daemon request handler
async fn handle_user_request(
    manager: &PolicyManager,
    uid: u32,
    cpu: u32,
    mem: &str,
) -> Result<String> {
    // Validate the request against policy
    manager.validate_request(cpu, mem)?;

    // If validation passed, proceed with allocation
    // (This would call systemd_client::create_slice in the real daemon)
    println!("Creating slice for user {} with {} CPUs and {}", uid, cpu, mem);

    Ok(format!(
        "Successfully allocated {} CPUs and {} for user {}",
        cpu, mem, uid
    ))
}

// Example: Policy-aware default allocation
fn get_default_allocation(manager: &PolicyManager) -> Result<(u32, String)> {
    let defaults = manager.get_defaults()?;
    Ok((defaults.cpu, defaults.mem.clone()))
}

// Example: Check if a request would exceed limits before asking user
fn suggest_valid_allocation(
    manager: &PolicyManager,
    requested_cpu: u32,
    requested_mem: &str,
) -> Result<(u32, String)> {
    let max = manager.get_max()?;

    // Cap the CPU to maximum
    let cpu = requested_cpu.min(max.cpu);

    // For memory, we'd need to parse and compare
    // For now, return the max if requested exceeds it
    let mem = if manager.validate_request(requested_cpu, requested_mem).is_ok() {
        requested_mem.to_string()
    } else {
        max.mem.clone()
    };

    Ok((cpu, mem))
}
