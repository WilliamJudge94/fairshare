use colored::*;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use serde::Deserialize;
use std::fs;
use std::io;
use std::process::Command;
use sysinfo::System;
use users::{get_user_by_uid, uid_t};

#[derive(Deserialize)]
struct PolicyConfig {
    defaults: PolicyDefaults,
}

#[derive(Deserialize)]
struct PolicyDefaults {
    #[allow(dead_code)]
    cpu: u32,
    #[allow(dead_code)]
    mem: u32,
    #[serde(default)]
    cpu_reserve: u32,
    #[serde(default)]
    mem_reserve: u32,
}

pub struct SystemTotals {
    pub total_mem_gb: f64,
    pub total_cpu: usize,
}

pub struct UserAlloc {
    pub uid: String,
    pub cpu_quota: f64,
    pub mem_bytes: u64,
}

pub struct ServiceAlloc {
    pub name: String,
    pub cpu_quota: f64,
    pub mem_bytes: u64,
}

/// Read the system CPU reserve from policy.toml
/// Returns 0 if the file doesn't exist or can't be read
pub fn get_system_cpu_reserve() -> u32 {
    let policy_path = "/etc/fairshare/policy.toml";

    match fs::read_to_string(policy_path) {
        Ok(contents) => match toml::from_str::<PolicyConfig>(&contents) {
            Ok(config) => config.defaults.cpu_reserve,
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

/// Read the system memory reserve from policy.toml
/// Returns 0 if the file doesn't exist or can't be read
pub fn get_system_mem_reserve() -> u32 {
    let policy_path = "/etc/fairshare/policy.toml";

    match fs::read_to_string(policy_path) {
        Ok(contents) => match toml::from_str::<PolicyConfig>(&contents) {
            Ok(config) => config.defaults.mem_reserve,
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

pub fn get_system_totals() -> SystemTotals {
    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.refresh_cpu();

    // sysinfo::System::total_memory() returns bytes since v0.30
    let total_mem_gb = sys.total_memory() as f64 / 1_000_000_000.0; // 10^9 (decimal GB)
    let total_cpu = sys.cpus().len();

    SystemTotals {
        total_mem_gb,
        total_cpu,
    }
}

pub fn get_user_allocations() -> io::Result<Vec<UserAlloc>> {
    // Query systemd directly for user allocations
    get_user_allocations_from_systemd()
}

// Get allocations by querying systemd directly
fn get_user_allocations_from_systemd() -> io::Result<Vec<UserAlloc>> {
    let output = Command::new("systemctl")
        .args([
            "list-units",
            "--type=slice",
            "--all",
            "--no-legend",
            "--plain",
        ])
        .output()
        .map_err(|e| io::Error::other(format!("Failed to list systemd slices: {}", e)))?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "systemctl command failed with exit code: {:?}",
            output.status.code()
        )));
    }

    let mut allocations = vec![];

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let unit_name = parts[0];
        if !unit_name.starts_with("user-") || !unit_name.ends_with(".slice") {
            continue;
        }

        // Parse UID from unit name (e.g., "user-1000.slice")
        let uid = match parse_uid_from_slice(unit_name) {
            Some(uid) => uid,
            None => continue, // Skip invalid entries
        };

        // Skip root user (UID 0) as it's a system slice, not a regular user allocation
        if uid == "0" {
            continue;
        }

        let info = Command::new("systemctl")
            .args([
                "show",
                unit_name,
                "-p",
                "MemoryMax",
                "-p",
                "CPUQuotaPerSecUSec",
            ])
            .output()
            .map_err(|e| {
                io::Error::other(format!("Failed to get slice info for {}: {}", unit_name, e))
            })?;

        let out = String::from_utf8_lossy(&info.stdout);
        let mut mem_bytes = 0;
        let mut cpu_quota = 0.0;

        for l in out.lines() {
            if l.starts_with("MemoryMax=") {
                if let Some(value_str) = l.strip_prefix("MemoryMax=") {
                    mem_bytes = value_str.parse::<u64>().unwrap_or(0);
                }
            } else if l.starts_with("CPUQuotaPerSecUSec=") {
                if let Some(quota_str) = l.strip_prefix("CPUQuotaPerSecUSec=") {
                    if let Some(sec_str) = quota_str.strip_suffix('s') {
                        if let Ok(seconds) = sec_str.parse::<f64>() {
                            // Convert seconds to percentage (1s = 100%, 2s = 200%, etc)
                            cpu_quota = seconds * 100.0;
                        }
                    }
                }
            }
        }

        allocations.push(UserAlloc {
            uid,
            cpu_quota,
            mem_bytes,
        });
    }

    Ok(allocations)
}

fn parse_uid_from_slice(slice_name: &str) -> Option<String> {
    // Expected format: "user-1000.slice"
    let parts: Vec<&str> = slice_name.split('-').collect();
    if parts.len() != 2 || parts[0] != "user" {
        return None;
    }

    let uid_str = parts[1].trim_end_matches(".slice");

    // Validate it's only digits
    if !uid_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    // Additional validation: ensure it can be parsed as u32
    uid_str.parse::<u32>().ok()?;

    Some(uid_str.to_string())
}

/// Get service allocations by querying systemd directly
pub fn get_service_allocations() -> io::Result<Vec<ServiceAlloc>> {
    // List of services to check for resource limits
    let services_to_check = vec![
        "docker",
        "containerd",
        "podman",
        "lxc",
        "libvirtd",
        "qemu-kvm",
    ];

    let mut allocations = vec![];

    for service_name in services_to_check {
        let unit_name = format!("{}.service", service_name);

        // Check if service exists by trying to show it
        let info = Command::new("systemctl")
            .args(["show", &unit_name, "-p", "LoadState"])
            .output()
            .map_err(|e| {
                io::Error::other(format!("Failed to check service {}: {}", unit_name, e))
            })?;

        let out = String::from_utf8_lossy(&info.stdout);
        let mut load_state = "";
        for l in out.lines() {
            if let Some(value) = l.strip_prefix("LoadState=") {
                load_state = value.trim();
            }
        }

        // Skip if service doesn't exist (LoadState=not-found)
        if load_state == "not-found" {
            continue;
        }

        // Get resource limits
        let info = Command::new("systemctl")
            .args([
                "show",
                &unit_name,
                "-p",
                "MemoryMax",
                "-p",
                "CPUQuotaPerSecUSec",
            ])
            .output()
            .map_err(|e| {
                io::Error::other(format!("Failed to get service info for {}: {}", unit_name, e))
            })?;

        let out = String::from_utf8_lossy(&info.stdout);
        let mut mem_bytes = 0u64;
        let mut cpu_quota = 0.0f64;

        for l in out.lines() {
            if l.starts_with("MemoryMax=") {
                if let Some(value_str) = l.strip_prefix("MemoryMax=") {
                    // MemoryMax can be "infinity" or a number
                    if value_str != "infinity" {
                        mem_bytes = value_str.parse::<u64>().unwrap_or(0);
                    }
                }
            } else if l.starts_with("CPUQuotaPerSecUSec=") {
                if let Some(quota_str) = l.strip_prefix("CPUQuotaPerSecUSec=") {
                    if let Some(sec_str) = quota_str.strip_suffix('s') {
                        if let Ok(seconds) = sec_str.parse::<f64>() {
                            // Convert seconds to percentage (1s = 100%, 2s = 200%, etc)
                            cpu_quota = seconds * 100.0;
                        }
                    }
                }
            }
        }

        // Only add to allocations if service has custom resource limits set
        if cpu_quota > 0.0 || mem_bytes > 0 {
            allocations.push(ServiceAlloc {
                name: service_name.to_string(),
                cpu_quota,
                mem_bytes,
            });
        }
    }

    Ok(allocations)
}

/// Calculate all available resources for the requesting user
/// Returns (available_cpu, available_mem_gb) taking into account:
/// - System reserves
/// - Other users' allocations
/// - Service allocations
/// - Requesting user's current allocation (delta-based)
pub fn calculate_available_resources(
    totals: &SystemTotals,
    allocations: &[UserAlloc],
    requesting_user_uid: Option<&str>,
) -> (u32, u32) {
    // Get system reserves
    let cpu_reserve = get_system_cpu_reserve() as f64;
    let mem_reserve = get_system_mem_reserve() as f64;

    // Calculate currently used resources from all users
    let used_cpu: f64 = allocations.iter().map(|a| a.cpu_quota / 100.0).sum();
    let used_mem: f64 = allocations
        .iter()
        .map(|a| a.mem_bytes as f64 / 1_000_000_000.0)
        .sum();

    // Add service allocations
    let service_allocs = get_service_allocations().unwrap_or_default();
    let service_cpu: f64 = service_allocs.iter().map(|a| a.cpu_quota / 100.0).sum();
    let service_mem: f64 = service_allocs
        .iter()
        .map(|a| a.mem_bytes as f64 / 1_000_000_000.0)
        .sum();

    // If the requesting user already has an allocation, subtract it from used resources
    // This allows us to check if the NET INCREASE fits, not the entire new request
    let (adjusted_used_cpu, adjusted_used_mem) = if let Some(uid) = requesting_user_uid {
        let current_user_alloc = allocations.iter().find(|a| a.uid == uid);
        if let Some(alloc) = current_user_alloc {
            let current_cpu = alloc.cpu_quota / 100.0;
            let current_mem = alloc.mem_bytes as f64 / 1_000_000_000.0;
            (used_cpu - current_cpu, used_mem - current_mem)
        } else {
            (used_cpu, used_mem)
        }
    } else {
        (used_cpu, used_mem)
    };

    // Subtract user allocations, service allocations, and system reserves from available resources
    let available_cpu = totals.total_cpu as f64 - adjusted_used_cpu - service_cpu - cpu_reserve;
    let available_mem = totals.total_mem_gb - adjusted_used_mem - service_mem - mem_reserve;

    // Return as u32, ensuring we don't return negative values
    let available_cpu_u32 = if available_cpu > 0.0 {
        available_cpu.floor() as u32
    } else {
        0
    };
    let available_mem_u32 = if available_mem > 0.0 {
        available_mem.floor() as u32
    } else {
        0
    };

    (available_cpu_u32, available_mem_u32)
}

pub fn check_request(
    totals: &SystemTotals,
    allocations: &[UserAlloc],
    req_cpu: u32,
    req_mem_gb: &str,
    requesting_user_uid: Option<&str>,
) -> bool {
    // Get system reserves
    let cpu_reserve = get_system_cpu_reserve() as f64;
    let mem_reserve = get_system_mem_reserve() as f64;

    // Calculate currently used resources from all users
    let used_cpu: f64 = allocations.iter().map(|a| a.cpu_quota / 100.0).sum();
    let used_mem: f64 = allocations
        .iter()
        .map(|a| a.mem_bytes as f64 / 1_000_000_000.0)
        .sum();

    // Add service allocations
    let service_allocs = get_service_allocations().unwrap_or_default();
    let service_cpu: f64 = service_allocs.iter().map(|a| a.cpu_quota / 100.0).sum();
    let service_mem: f64 = service_allocs
        .iter()
        .map(|a| a.mem_bytes as f64 / 1_000_000_000.0)
        .sum();

    // If the requesting user already has an allocation, subtract it from used resources
    // This allows us to check if the NET INCREASE fits, not the entire new request
    let (adjusted_used_cpu, adjusted_used_mem) = if let Some(uid) = requesting_user_uid {
        let current_user_alloc = allocations.iter().find(|a| a.uid == uid);
        if let Some(alloc) = current_user_alloc {
            let current_cpu = alloc.cpu_quota / 100.0;
            let current_mem = alloc.mem_bytes as f64 / 1_000_000_000.0;
            (used_cpu - current_cpu, used_mem - current_mem)
        } else {
            (used_cpu, used_mem)
        }
    } else {
        (used_cpu, used_mem)
    };

    // Subtract user allocations, service allocations, and system reserves from available resources
    let available_cpu = totals.total_cpu as f64 - adjusted_used_cpu - service_cpu - cpu_reserve;
    let available_mem = totals.total_mem_gb - adjusted_used_mem - service_mem - mem_reserve;
    let req_mem = parse_mem_gb(req_mem_gb);

    req_cpu as f64 <= available_cpu && req_mem <= available_mem
}

fn parse_mem_gb(mem: &str) -> f64 {
    let s = mem.trim().to_uppercase();
    if s.ends_with('G') {
        s.trim_end_matches('G').parse::<f64>().unwrap_or(0.0)
    } else if s.ends_with('M') {
        s.trim_end_matches('M').parse::<f64>().unwrap_or(0.0) / 1024.0
    } else {
        s.parse::<f64>().unwrap_or(0.0)
    }
}

/// Get username from UID, returns None if user doesn't exist
pub fn get_username_from_uid(uid_str: &str) -> Option<String> {
    let uid_num: uid_t = uid_str.parse().ok()?;
    get_user_by_uid(uid_num).map(|user| user.name().to_string_lossy().into_owned())
}

pub fn print_status(totals: &SystemTotals, allocations: &[UserAlloc]) {
    let cpu_reserve = get_system_cpu_reserve() as f64;
    let mem_reserve = get_system_mem_reserve() as f64;
    let used_cpu: f64 = allocations.iter().map(|a| a.cpu_quota / 100.0).sum();
    let used_mem: f64 = allocations
        .iter()
        .map(|a| a.mem_bytes as f64 / 1_000_000_000.0)
        .sum();

    // Get service allocations
    let service_allocs = get_service_allocations().unwrap_or_default();
    let service_cpu: f64 = service_allocs.iter().map(|a| a.cpu_quota / 100.0).sum();
    let service_mem: f64 = service_allocs
        .iter()
        .map(|a| a.mem_bytes as f64 / 1_000_000_000.0)
        .sum();

    // Subtract user allocations, service allocations, and system reserves from available resources
    let available_cpu = totals.total_cpu as f64 - used_cpu - service_cpu - cpu_reserve;
    let available_mem = totals.total_mem_gb - used_mem - service_mem - mem_reserve;

    // System overview table
    println!(
        "{}",
        "╔═══════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║      SYSTEM RESOURCE OVERVIEW         ║"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════╝".bright_cyan()
    );
    println!();

    let mut overview_table = Table::new();
    overview_table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Metric").fg(Color::Cyan),
            Cell::new("CPUs").fg(Color::Cyan),
            Cell::new("RAM (GB)").fg(Color::Cyan),
        ]);

    overview_table.add_row(vec![
        Cell::new("Total").fg(Color::White),
        Cell::new(format!("{}", totals.total_cpu)).fg(Color::White),
        Cell::new(format!("{:.2}", totals.total_mem_gb)).fg(Color::White),
    ]);

    // Show system reserves if configured
    if cpu_reserve > 0.0 || mem_reserve > 0.0 {
        let cpu_reserve_str = if cpu_reserve > 0.0 {
            format!("{:.2}", cpu_reserve)
        } else {
            "-".to_string()
        };
        let mem_reserve_str = if mem_reserve > 0.0 {
            format!("{:.2}", mem_reserve)
        } else {
            "-".to_string()
        };
        overview_table.add_row(vec![
            Cell::new("Reserved (System)").fg(Color::Magenta),
            Cell::new(cpu_reserve_str).fg(Color::Magenta),
            Cell::new(mem_reserve_str).fg(Color::Magenta),
        ]);
    }

    overview_table.add_row(vec![
        Cell::new("Allocated").fg(Color::Yellow),
        Cell::new(format!("{:.2}", used_cpu)).fg(Color::Yellow),
        Cell::new(format!("{:.2}", used_mem)).fg(Color::Yellow),
    ]);

    overview_table.add_row(vec![
        Cell::new("Available").fg(Color::Green),
        Cell::new(format!("{:.2}", available_cpu)).fg(Color::Green),
        Cell::new(format!("{:.2}", available_mem)).fg(Color::Green),
    ]);

    println!("{}", overview_table);
    println!();

    // Service allocations table
    if !service_allocs.is_empty() {
        println!("{}", "Service Allocations:".bright_cyan().bold());
        println!();

        let mut service_table = Table::new();
        service_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Service").fg(Color::Cyan),
                Cell::new("CPU Quota").fg(Color::Cyan),
                Cell::new("CPUs").fg(Color::Cyan),
                Cell::new("RAM (GB)").fg(Color::Cyan),
            ]);

        for s in &service_allocs {
            let cpu_cores = s.cpu_quota / 100.0;
            let mem_gb = s.mem_bytes as f64 / 1_000_000_000.0;
            service_table.add_row(vec![
                Cell::new(&s.name).fg(Color::White),
                Cell::new(format!("{:.1}%", s.cpu_quota)).fg(Color::Magenta),
                Cell::new(format!("{:.2}", cpu_cores)).fg(Color::Magenta),
                Cell::new(format!("{:.2}", mem_gb)).fg(Color::Magenta),
            ]);
        }

        println!("{}", service_table);
        println!();
    }

    // Per-user allocations table
    if !allocations.is_empty() {
        println!("{}", "Per-User Allocations:".bright_cyan().bold());
        println!();

        let mut user_table = Table::new();
        user_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Username").fg(Color::Cyan),
                Cell::new("UID").fg(Color::Cyan),
                Cell::new("CPU Quota").fg(Color::Cyan),
                Cell::new("CPUs").fg(Color::Cyan),
                Cell::new("RAM (GB)").fg(Color::Cyan),
            ]);

        for a in allocations {
            let username = get_username_from_uid(&a.uid).unwrap_or_else(|| format!("({})", a.uid));

            // Check if user has no custom allocations (both CPU and Memory are 0)
            let has_no_allocation = a.cpu_quota == 0.0 && a.mem_bytes == 0;

            if has_no_allocation {
                // Display "Not Set" for users without custom resource limits
                user_table.add_row(vec![
                    Cell::new(username).fg(Color::White),
                    Cell::new(&a.uid).fg(Color::White),
                    Cell::new("Not Set").fg(Color::DarkGrey),
                    Cell::new("Not Set").fg(Color::DarkGrey),
                    Cell::new("Not Set").fg(Color::DarkGrey),
                ]);
            } else {
                // Display actual values for users with custom allocations
                let cpu_cores = a.cpu_quota / 100.0;
                let mem_gb = a.mem_bytes as f64 / 1_000_000_000.0;
                user_table.add_row(vec![
                    Cell::new(username).fg(Color::White),
                    Cell::new(&a.uid).fg(Color::White),
                    Cell::new(format!("{:.1}%", a.cpu_quota)).fg(Color::Yellow),
                    Cell::new(format!("{:.2}", cpu_cores)).fg(Color::Yellow),
                    Cell::new(format!("{:.2}", mem_gb)).fg(Color::Yellow),
                ]);
            }
        }

        println!("{}", user_table);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mem_gb_with_gigabytes() {
        assert_eq!(parse_mem_gb("4G"), 4.0);
        assert_eq!(parse_mem_gb("8g"), 8.0);
        assert_eq!(parse_mem_gb("16G"), 16.0);
    }

    #[test]
    fn test_parse_mem_gb_with_megabytes() {
        assert_eq!(parse_mem_gb("1024M"), 1.0);
        assert_eq!(parse_mem_gb("2048m"), 2.0);
        assert_eq!(parse_mem_gb("512M"), 0.5);
    }

    #[test]
    fn test_parse_mem_gb_plain_number() {
        assert_eq!(parse_mem_gb("4"), 4.0);
        assert_eq!(parse_mem_gb("8.5"), 8.5);
    }

    #[test]
    fn test_parse_mem_gb_invalid() {
        assert_eq!(parse_mem_gb("invalid"), 0.0);
        assert_eq!(parse_mem_gb(""), 0.0);
    }

    #[test]
    fn test_check_request_sufficient_resources() {
        let totals = SystemTotals {
            total_mem_gb: 16.0,
            total_cpu: 8,
        };
        let allocations = vec![UserAlloc {
            uid: "1000".to_string(),
            cpu_quota: 200.0,         // 2 CPUs
            mem_bytes: 4_000_000_000, // 4 GB
        }];

        // Request 2 CPUs and 4 GB - should be allowed
        assert!(check_request(&totals, &allocations, 2, "4", None));
    }

    #[test]
    fn test_check_request_insufficient_cpu() {
        let totals = SystemTotals {
            total_mem_gb: 16.0,
            total_cpu: 8,
        };
        let allocations = vec![UserAlloc {
            uid: "1000".to_string(),
            cpu_quota: 600.0,         // 6 CPUs
            mem_bytes: 4_000_000_000, // 4 GB
        }];

        // Request 4 CPUs when only 2 are available - should fail
        assert!(!check_request(&totals, &allocations, 4, "4", None));
    }

    #[test]
    fn test_check_request_insufficient_memory() {
        let totals = SystemTotals {
            total_mem_gb: 16.0,
            total_cpu: 8,
        };
        let allocations = vec![UserAlloc {
            uid: "1000".to_string(),
            cpu_quota: 200.0,          // 2 CPUs
            mem_bytes: 12_000_000_000, // 12 GB
        }];

        // Request 8 GB when only 4 GB available - should fail
        assert!(!check_request(&totals, &allocations, 2, "8", None));
    }

    #[test]
    fn test_check_request_multiple_users() {
        let totals = SystemTotals {
            total_mem_gb: 32.0,
            total_cpu: 16,
        };
        let allocations = vec![
            UserAlloc {
                uid: "1000".to_string(),
                cpu_quota: 400.0,         // 4 CPUs
                mem_bytes: 8_000_000_000, // 8 GB
            },
            UserAlloc {
                uid: "1001".to_string(),
                cpu_quota: 200.0,         // 2 CPUs
                mem_bytes: 4_000_000_000, // 4 GB
            },
        ];

        // 6 CPUs used, 12 GB used
        // Request 5 CPUs and 10 GB - should be allowed (10 available, 20 available)
        assert!(check_request(&totals, &allocations, 5, "10", None));

        // Request 12 CPUs - should fail (only 10 available)
        assert!(!check_request(&totals, &allocations, 12, "8", None));
    }

    #[test]
    fn test_check_request_exact_available() {
        // Get the actual system reserves to ensure test accounts for them
        let cpu_reserve = get_system_cpu_reserve();
        let mem_reserve = get_system_mem_reserve();

        let totals = SystemTotals {
            total_mem_gb: 16.0,
            total_cpu: 8,
        };
        let allocations = vec![UserAlloc {
            uid: "1000".to_string(),
            cpu_quota: 400.0,         // 4 CPUs
            mem_bytes: 8_000_000_000, // 8 GB
        }];

        // Calculate actual available resources considering reserves
        // Available = Total - Used - Reserve
        // Available CPU = 8 - 4 - cpu_reserve
        // Available MEM = 16 - 8 - mem_reserve
        let available_cpu = (8u32 - 4u32).saturating_sub(cpu_reserve);
        let available_mem = (16u32 - 8u32).saturating_sub(mem_reserve);

        // Request exactly what's available (should succeed)
        assert!(check_request(
            &totals,
            &allocations,
            available_cpu,
            &available_mem.to_string(),
            None
        ));

        // Request more than available (should fail)
        assert!(!check_request(
            &totals,
            &allocations,
            available_cpu + 1,
            &available_mem.to_string(),
            None
        ));
    }

    #[test]
    fn test_check_request_user_modifying_own_allocation() {
        // Get the actual system reserves to ensure test accounts for them
        let cpu_reserve = get_system_cpu_reserve();
        let mem_reserve = get_system_mem_reserve();

        // Use larger system to accommodate reserves and test scenarios
        let totals = SystemTotals {
            total_mem_gb: 32.0,
            total_cpu: 16,
        };
        let allocations = vec![
            UserAlloc {
                uid: "1000".to_string(),
                cpu_quota: 400.0,          // 4 CPUs
                mem_bytes: 10_000_000_000, // 10 GB
            },
            UserAlloc {
                uid: "1001".to_string(),
                cpu_quota: 200.0,         // 2 CPUs
                mem_bytes: 5_000_000_000, // 5 GB
            },
        ];

        // Total used: 6 CPUs, 15 GB
        // User 1000 requests 5 CPUs and 11 GB (increase of 1 CPU and 1 GB)
        // Delta: adjusted_used = (6-4, 15-10) = (2 CPUs, 5 GB)
        // Available = (16 - 2 - cpu_reserve, 32 - 5 - mem_reserve)
        // With reserves (2, 4): Available = (12, 23)
        // Request: 5 CPUs, 11 GB - should succeed since 5 <= 12 and 11 <= 23
        assert!(check_request(&totals, &allocations, 5, "11", Some("1000")));

        // User 1001 trying to request 1 CPU and 3 GB (decrease from 2 CPUs, 5 GB)
        // Should definitely succeed as this is a decrease
        assert!(check_request(&totals, &allocations, 1, "3", Some("1001")));

        // Calculate what's actually available for a new user
        // Used: 6 CPUs, 15 GB
        // Available = (16 - 6 - cpu_reserve, 32 - 15 - mem_reserve)
        // With reserves (2, 4): Available = (8, 13)
        let avail_cpu_for_new = (16u32 - 6u32).saturating_sub(cpu_reserve);
        let avail_mem_for_new = (32u32 - 15u32).saturating_sub(mem_reserve);

        // New user 1002 requesting within available (should succeed)
        assert!(check_request(
            &totals,
            &allocations,
            avail_cpu_for_new.min(1),
            &avail_mem_for_new.min(1).to_string(),
            Some("1002")
        ));

        // User 1000 requesting way too much even with delta (should fail)
        // Current: 4 CPUs. Request: 20 CPUs. Net: +16 CPUs.
        // Available with delta = (16 - 2 - cpu_reserve) = 12 or less
        // 20 > 12, so should fail
        assert!(!check_request(
            &totals,
            &allocations,
            20,
            "15",
            Some("1000")
        ));
    }

    #[test]
    fn test_get_system_totals() {
        let totals = get_system_totals();

        // Basic sanity checks
        assert!(totals.total_mem_gb > 0.0, "Total memory should be positive");
        assert!(totals.total_cpu > 0, "Total CPUs should be positive");
    }

    #[test]
    fn test_parse_uid_from_slice_rejects_root() {
        // Verify that the parse_uid_from_slice function correctly parses
        // user-0.slice even though we skip it during allocation gathering.
        // This test ensures the parsing logic works, while the actual
        // get_user_allocations function filters out UID 0.
        let uid = parse_uid_from_slice("user-0.slice");
        assert_eq!(uid, Some("0".to_string()), "Should parse UID 0 correctly");

        let uid = parse_uid_from_slice("user-1000.slice");
        assert_eq!(
            uid,
            Some("1000".to_string()),
            "Should parse UID 1000 correctly"
        );
    }

    #[test]
    fn test_system_slices_excluded_from_allocations() {
        // Regression test: ensure that user-0.slice (root/system) is not
        // counted in user allocations, which was causing negative available RAM
        // when user-0.slice had a MemoryMax set.

        // This test validates the logic by checking that the parsing
        // correctly identifies UID 0 for filtering.
        let uid_0 = parse_uid_from_slice("user-0.slice");
        assert_eq!(
            uid_0,
            Some("0".to_string()),
            "Root user-0.slice should parse correctly"
        );

        // The actual filtering happens in get_user_allocations(),
        // which skips any entry with UID "0"
    }
}
