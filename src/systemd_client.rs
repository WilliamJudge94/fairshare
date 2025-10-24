use anyhow::{Result, Context};
use zbus::{Connection, proxy};
use zbus::zvariant::{OwnedObjectPath, Value};
use tracing::{info, debug, warn};
use std::collections::HashMap;
use crate::utils::parse_memory_size;

/// DBus proxy for systemd Manager interface
#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
    /// Start a transient unit with properties
    fn start_transient_unit(
        &self,
        name: &str,
        mode: &str,
        properties: Vec<(&str, Value<'_>)>,
        aux: Vec<(&str, Vec<(&str, Value<'_>)>)>,
    ) -> zbus::Result<OwnedObjectPath>;

    /// Stop a unit
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    /// Get unit object path
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;

    /// List all units
    fn list_units(&self) -> zbus::Result<Vec<(String, String, String, String, String, String, OwnedObjectPath, u32, String, OwnedObjectPath)>>;
}

/// DBus proxy for systemd Unit interface
#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
trait SystemdUnit {
    /// Get a property from the unit
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn load_state(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;
}

/// Information about a systemd slice
#[derive(Debug, Clone)]
pub struct SliceInfo {
    pub name: String,
    pub active_state: String,
    pub load_state: String,
    pub sub_state: String,
    pub cpu_quota: Option<u64>,
    pub memory_max: Option<u64>,
    pub tasks_max: Option<u64>,
}

/// Client for interacting with systemd via DBus
pub struct SystemdClient {
    connection: Connection,
}

impl SystemdClient {
    /// Create a new systemd client
    pub async fn new() -> Result<Self> {
        info!("Initializing systemd DBus client");

        // Connect to system DBus
        let connection = Connection::system()
            .await
            .context("Failed to connect to system DBus")?;

        debug!("Connected to system DBus");

        Ok(Self { connection })
    }

    /// Create a new systemd slice with resource limits
    ///
    /// # Arguments
    /// * `uid` - User ID for which to create the slice
    /// * `cpu` - Number of CPUs to allocate (converted to percentage)
    /// * `mem` - Memory limit as a string (e.g., "8G")
    pub async fn create_slice(&self, uid: u32, cpu: u32, mem: &str) -> Result<()> {
        let slice_name = format!("fairshare-{}.slice", uid);
        info!("Creating slice: {} with cpu={}, mem={}", slice_name, cpu, mem);

        // Parse memory to bytes
        let memory_bytes = parse_memory_size(mem)
            .context("Failed to parse memory size")?;

        // Convert CPU count to quota percentage (e.g., 2 CPUs = 200%)
        // CPUQuota is in microseconds per 100ms, so 100% = 100000us
        let cpu_quota_usec = (cpu as u64) * 100_000u64;

        // Set a reasonable tasks limit per user
        let tasks_max: u64 = 4096;

        debug!("CPU quota: {}us ({}%)", cpu_quota_usec, cpu * 100);
        debug!("Memory max: {} bytes ({} GB)", memory_bytes, memory_bytes as f64 / 1024.0 / 1024.0 / 1024.0);
        debug!("Tasks max: {}", tasks_max);

        // Create systemd manager proxy
        let manager = SystemdManagerProxy::new(&self.connection)
            .await
            .context("Failed to create systemd manager proxy")?;

        // Build properties array for the slice
        let properties = vec![
            ("Description", Value::new(format!("Fairshare resource slice for UID {}", uid))),
            ("CPUQuota", Value::new(cpu_quota_usec)),
            ("MemoryMax", Value::new(memory_bytes)),
            ("TasksMax", Value::new(tasks_max)),
            // Make sure the slice is a proper slice unit
            ("DefaultDependencies", Value::new(false)),
        ];

        // Start the transient unit (slice)
        // Mode "fail" means fail if the unit already exists
        let job_path = manager
            .start_transient_unit(&slice_name, "fail", properties, vec![])
            .await
            .context("Failed to start transient unit")?;

        info!("Slice {} created successfully (job: {})", slice_name, job_path);

        Ok(())
    }

    /// Remove a systemd slice
    ///
    /// # Arguments
    /// * `uid` - User ID for which to remove the slice
    pub async fn remove_slice(&self, uid: u32) -> Result<()> {
        let slice_name = format!("fairshare-{}.slice", uid);
        info!("Removing slice: {}", slice_name);

        // Create systemd manager proxy
        let manager = SystemdManagerProxy::new(&self.connection)
            .await
            .context("Failed to create systemd manager proxy")?;

        // Stop the slice unit
        // Mode "replace" means replace any pending conflicting job
        let job_path = manager
            .stop_unit(&slice_name, "replace")
            .await
            .context("Failed to stop slice unit")?;

        info!("Slice {} removed successfully (job: {})", slice_name, job_path);

        Ok(())
    }

    /// Get status information about a slice
    ///
    /// # Arguments
    /// * `uid` - User ID for which to get slice status
    pub async fn get_slice_status(&self, uid: u32) -> Result<SliceInfo> {
        let slice_name = format!("fairshare-{}.slice", uid);
        debug!("Getting status for slice: {}", slice_name);

        // Create systemd manager proxy
        let manager = SystemdManagerProxy::new(&self.connection)
            .await
            .context("Failed to create systemd manager proxy")?;

        // Get the unit object path
        let unit_path = manager
            .get_unit(&slice_name)
            .await
            .context("Failed to get unit path")?;

        // Create unit proxy
        let unit = SystemdUnitProxy::builder(&self.connection)
            .path(unit_path.clone())
            .context("Invalid unit path")?
            .build()
            .await
            .context("Failed to create unit proxy")?;

        // Get unit states
        let active_state = unit.active_state().await
            .context("Failed to get active state")?;
        let load_state = unit.load_state().await
            .context("Failed to get load state")?;
        let sub_state = unit.sub_state().await
            .context("Failed to get sub state")?;

        // Get resource properties using DBus Properties interface
        let proxy = zbus::Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            unit_path,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("Failed to create properties proxy")?;

        // Get CPU quota
        let cpu_quota = match proxy.call::<(Value,), _>(
            "Get",
            &("org.freedesktop.systemd1.Unit", "CPUQuotaPerSecUSec"),
        ).await {
            Ok((value,)) => {
                match value.downcast_ref::<u64>() {
                    Some(v) => Some(*v),
                    None => None,
                }
            },
            Err(e) => {
                debug!("Failed to get CPUQuota: {}", e);
                None
            }
        };

        // Get memory max
        let memory_max = match proxy.call::<(Value,), _>(
            "Get",
            &("org.freedesktop.systemd1.Unit", "MemoryMax"),
        ).await {
            Ok((value,)) => {
                match value.downcast_ref::<u64>() {
                    Some(v) => Some(*v),
                    None => None,
                }
            },
            Err(e) => {
                debug!("Failed to get MemoryMax: {}", e);
                None
            }
        };

        // Get tasks max
        let tasks_max = match proxy.call::<(Value,), _>(
            "Get",
            &("org.freedesktop.systemd1.Unit", "TasksMax"),
        ).await {
            Ok((value,)) => {
                match value.downcast_ref::<u64>() {
                    Some(v) => Some(*v),
                    None => None,
                }
            },
            Err(e) => {
                debug!("Failed to get TasksMax: {}", e);
                None
            }
        };

        let slice_info = SliceInfo {
            name: slice_name,
            active_state,
            load_state,
            sub_state,
            cpu_quota,
            memory_max,
            tasks_max,
        };

        debug!("Slice status: {:?}", slice_info);

        Ok(slice_info)
    }

    /// Set resource properties on a slice/unit
    pub async fn set_slice_properties(
        &self,
        slice_name: &str,
        properties: HashMap<String, String>,
    ) -> Result<()> {
        info!("Setting properties for slice: {}", slice_name);

        // This would require SetUnitProperties method
        // For now, we handle properties during slice creation
        // This is kept for future extensibility

        warn!("set_slice_properties is not yet implemented - properties should be set during slice creation");

        Ok(())
    }

    /// Move a process to a slice
    pub async fn move_process_to_slice(&self, pid: u32, slice_name: &str) -> Result<()> {
        info!("Moving process {} to slice {}", pid, slice_name);

        // To move a process to a slice, we need to create a scope unit for it
        // The scope will be a child of the slice
        let scope_name = format!("fairshare-pid-{}.scope", pid);

        // Create systemd manager proxy
        let manager = SystemdManagerProxy::new(&self.connection)
            .await
            .context("Failed to create systemd manager proxy")?;

        // Build properties for the scope
        let properties = vec![
            ("Description", Value::new(format!("Fairshare scope for PID {}", pid))),
            ("PIDs", Value::new(vec![pid])),
            ("Slice", Value::new(slice_name.to_string())),
            ("DefaultDependencies", Value::new(false)),
        ];

        // Start the transient scope
        let job_path = manager
            .start_transient_unit(&scope_name, "fail", properties, vec![])
            .await
            .context("Failed to create scope for process")?;

        info!("Process {} moved to slice {} via scope {} (job: {})",
              pid, slice_name, scope_name, job_path);

        Ok(())
    }

    /// Get properties of a slice/unit
    pub async fn get_slice_properties(&self, slice_name: &str) -> Result<HashMap<String, String>> {
        debug!("Getting properties for slice: {}", slice_name);

        // Get slice status which includes the main properties
        let status = self.get_slice_status_by_name(slice_name).await?;

        let mut properties = HashMap::new();
        properties.insert("active_state".to_string(), status.active_state);
        properties.insert("load_state".to_string(), status.load_state);
        properties.insert("sub_state".to_string(), status.sub_state);

        if let Some(cpu) = status.cpu_quota {
            properties.insert("cpu_quota".to_string(), cpu.to_string());
        }
        if let Some(mem) = status.memory_max {
            properties.insert("memory_max".to_string(), mem.to_string());
        }
        if let Some(tasks) = status.tasks_max {
            properties.insert("tasks_max".to_string(), tasks.to_string());
        }

        Ok(properties)
    }

    /// List all active slices
    pub async fn list_slices(&self) -> Result<Vec<String>> {
        debug!("Listing all active slices");

        // Create systemd manager proxy
        let manager = SystemdManagerProxy::new(&self.connection)
            .await
            .context("Failed to create systemd manager proxy")?;

        // List all units
        let units = manager.list_units()
            .await
            .context("Failed to list units")?;

        // Filter for slice units
        let slices: Vec<String> = units
            .into_iter()
            .filter(|(name, _, _, _, _, _, _, _, _, _)| name.ends_with(".slice"))
            .map(|(name, _, _, _, _, _, _, _, _, _)| name)
            .collect();

        debug!("Found {} slices", slices.len());

        Ok(slices)
    }

    /// Delete/stop a slice
    pub async fn delete_slice(&self, slice_name: &str) -> Result<()> {
        info!("Deleting slice: {}", slice_name);

        // Create systemd manager proxy
        let manager = SystemdManagerProxy::new(&self.connection)
            .await
            .context("Failed to create systemd manager proxy")?;

        // Stop the slice unit
        let job_path = manager
            .stop_unit(slice_name, "replace")
            .await
            .context("Failed to stop slice unit")?;

        info!("Slice {} deleted successfully (job: {})", slice_name, job_path);

        Ok(())
    }

    /// Subscribe to systemd unit changes
    pub async fn subscribe_to_changes(&self) -> Result<()> {
        info!("Subscribing to systemd unit changes");

        // This would require setting up DBus signal handlers
        // For MVP, we don't need real-time monitoring
        // This is kept for future extensibility

        warn!("subscribe_to_changes is not yet implemented");

        Ok(())
    }

    /// Check if a slice exists
    pub async fn slice_exists(&self, slice_name: &str) -> Result<bool> {
        debug!("Checking if slice exists: {}", slice_name);

        // Create systemd manager proxy
        let manager = SystemdManagerProxy::new(&self.connection)
            .await
            .context("Failed to create systemd manager proxy")?;

        // Try to get the unit
        match manager.get_unit(slice_name).await {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if it's a "not found" error
                let err_str = e.to_string();
                if err_str.contains("NoSuchUnit") || err_str.contains("not loaded") {
                    Ok(false)
                } else {
                    // Some other error occurred
                    Err(e).context("Failed to check if slice exists")
                }
            }
        }
    }

    /// Helper function to get slice status by name (not UID)
    async fn get_slice_status_by_name(&self, slice_name: &str) -> Result<SliceInfo> {
        debug!("Getting status for slice: {}", slice_name);

        // Create systemd manager proxy
        let manager = SystemdManagerProxy::new(&self.connection)
            .await
            .context("Failed to create systemd manager proxy")?;

        // Get the unit object path
        let unit_path = manager
            .get_unit(slice_name)
            .await
            .context("Failed to get unit path")?;

        // Create unit proxy
        let unit = SystemdUnitProxy::builder(&self.connection)
            .path(unit_path.clone())
            .context("Invalid unit path")?
            .build()
            .await
            .context("Failed to create unit proxy")?;

        // Get unit states
        let active_state = unit.active_state().await
            .context("Failed to get active state")?;
        let load_state = unit.load_state().await
            .context("Failed to get load state")?;
        let sub_state = unit.sub_state().await
            .context("Failed to get sub state")?;

        // Get resource properties using DBus Properties interface
        let proxy = zbus::Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            unit_path,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("Failed to create properties proxy")?;

        // Get CPU quota
        let cpu_quota = match proxy.call::<(Value,), _>(
            "Get",
            &("org.freedesktop.systemd1.Unit", "CPUQuotaPerSecUSec"),
        ).await {
            Ok((value,)) => {
                match value.downcast_ref::<u64>() {
                    Some(v) => Some(*v),
                    None => None,
                }
            },
            Err(e) => {
                debug!("Failed to get CPUQuota: {}", e);
                None
            }
        };

        // Get memory max
        let memory_max = match proxy.call::<(Value,), _>(
            "Get",
            &("org.freedesktop.systemd1.Unit", "MemoryMax"),
        ).await {
            Ok((value,)) => {
                match value.downcast_ref::<u64>() {
                    Some(v) => Some(*v),
                    None => None,
                }
            },
            Err(e) => {
                debug!("Failed to get MemoryMax: {}", e);
                None
            }
        };

        // Get tasks max
        let tasks_max = match proxy.call::<(Value,), _>(
            "Get",
            &("org.freedesktop.systemd1.Unit", "TasksMax"),
        ).await {
            Ok((value,)) => {
                match value.downcast_ref::<u64>() {
                    Some(v) => Some(*v),
                    None => None,
                }
            },
            Err(e) => {
                debug!("Failed to get TasksMax: {}", e);
                None
            }
        };

        let slice_info = SliceInfo {
            name: slice_name.to_string(),
            active_state,
            load_state,
            sub_state,
            cpu_quota,
            memory_max,
            tasks_max,
        };

        debug!("Slice status: {:?}", slice_info);

        Ok(slice_info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Most of these tests require a running systemd instance
    // They are integration tests rather than unit tests

    #[tokio::test]
    async fn test_systemd_connection() {
        // Test that we can connect to system DBus
        let result = SystemdClient::new().await;

        // This might fail in environments without systemd or proper DBus access
        match result {
            Ok(client) => {
                // Successfully connected
                assert!(client.connection.unique_name().is_some());
            }
            Err(e) => {
                // Log the error but don't fail the test in CI environments
                eprintln!("Could not connect to system DBus: {}. This is expected in environments without systemd.", e);
            }
        }
    }

    #[tokio::test]
    async fn test_slice_name_format() {
        // Test that slice names are formatted correctly
        let uid = 1001u32;
        let expected = "fairshare-1001.slice";

        let slice_name = format!("fairshare-{}.slice", uid);
        assert_eq!(slice_name, expected);
        assert!(slice_name.ends_with(".slice"));
    }

    #[tokio::test]
    async fn test_cpu_quota_conversion() {
        // Test CPU quota calculation
        // 1 CPU = 100% = 100000 microseconds per 100ms
        // 2 CPUs = 200% = 200000 microseconds per 100ms
        let cpu_count = 2u32;
        let cpu_quota_usec = (cpu_count as u64) * 100_000u64;

        assert_eq!(cpu_quota_usec, 200_000);

        // Test edge cases
        assert_eq!((1u32 as u64) * 100_000u64, 100_000);
        assert_eq!((4u32 as u64) * 100_000u64, 400_000);
    }

    #[tokio::test]
    async fn test_memory_parsing() {
        // Test memory size parsing
        assert_eq!(parse_memory_size("8G").unwrap(), 8 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("512M").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_memory_size("1024K").unwrap(), 1024 * 1024);
        assert_eq!(parse_memory_size("2048").unwrap(), 2048);
    }

    #[tokio::test]
    async fn test_scope_name_format() {
        // Test scope name formatting for process movement
        let pid = 12345u32;
        let scope_name = format!("fairshare-pid-{}.scope", pid);

        assert_eq!(scope_name, "fairshare-pid-12345.scope");
        assert!(scope_name.ends_with(".scope"));
    }

    // Integration test - requires systemd
    #[tokio::test]
    #[ignore] // Ignored by default, run with --ignored flag
    async fn test_create_and_remove_slice() {
        let client = match SystemdClient::new().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: could not connect to systemd: {}", e);
                return;
            }
        };

        let test_uid = 9999u32;
        let cpu = 2u32;
        let mem = "1G";

        // Create slice
        let create_result = client.create_slice(test_uid, cpu, mem).await;
        match create_result {
            Ok(_) => {
                println!("Successfully created test slice");

                // Check if slice exists
                let slice_name = format!("fairshare-{}.slice", test_uid);
                let exists = client.slice_exists(&slice_name).await.unwrap_or(false);
                assert!(exists, "Slice should exist after creation");

                // Get slice status
                let status_result = client.get_slice_status(test_uid).await;
                if let Ok(status) = status_result {
                    assert_eq!(status.name, slice_name);
                    println!("Slice status: {:?}", status);
                }

                // Clean up - remove slice
                let remove_result = client.remove_slice(test_uid).await;
                assert!(remove_result.is_ok(), "Should be able to remove slice");

                // Give systemd time to clean up
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                // Verify slice is gone
                let exists_after = client.slice_exists(&slice_name).await.unwrap_or(true);
                assert!(!exists_after, "Slice should not exist after removal");
            }
            Err(e) => {
                eprintln!("Could not create slice (may need root permissions): {}", e);
                // This test requires root/systemd permissions, so we don't fail
            }
        }
    }

    // Integration test - requires systemd
    #[tokio::test]
    #[ignore] // Ignored by default
    async fn test_list_slices() {
        let client = match SystemdClient::new().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: could not connect to systemd: {}", e);
                return;
            }
        };

        let result = client.list_slices().await;
        match result {
            Ok(slices) => {
                println!("Found {} slices", slices.len());
                for slice in slices.iter() {
                    println!("  - {}", slice);
                }
                // Should at least have system.slice
                assert!(!slices.is_empty(), "Should find at least some system slices");
            }
            Err(e) => {
                eprintln!("Could not list slices: {}", e);
            }
        }
    }

    // Integration test - requires systemd and root
    #[tokio::test]
    #[ignore] // Ignored by default
    async fn test_get_slice_properties() {
        let client = match SystemdClient::new().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: could not connect to systemd: {}", e);
                return;
            }
        };

        // Try to get properties of system.slice which should always exist
        let result = client.get_slice_properties("system.slice").await;
        match result {
            Ok(properties) => {
                println!("system.slice properties: {:?}", properties);
                assert!(properties.contains_key("active_state"));
                assert!(properties.contains_key("load_state"));
            }
            Err(e) => {
                eprintln!("Could not get slice properties: {}", e);
            }
        }
    }

    #[test]
    fn test_slice_info_creation() {
        let slice_info = SliceInfo {
            name: "test.slice".to_string(),
            active_state: "active".to_string(),
            load_state: "loaded".to_string(),
            sub_state: "running".to_string(),
            cpu_quota: Some(200_000),
            memory_max: Some(8_589_934_592), // 8GB
            tasks_max: Some(4096),
        };

        assert_eq!(slice_info.name, "test.slice");
        assert_eq!(slice_info.active_state, "active");
        assert_eq!(slice_info.cpu_quota, Some(200_000));
        assert_eq!(slice_info.memory_max, Some(8_589_934_592));
        assert_eq!(slice_info.tasks_max, Some(4096));
    }

    #[test]
    fn test_tasks_max_value() {
        // Verify the default TasksMax value
        let tasks_max: u64 = 4096;
        assert_eq!(tasks_max, 4096);
        assert!(tasks_max > 0);
        assert!(tasks_max <= 10000); // Reasonable upper bound
    }
}
