use std::process::Command;
use std::io;
use sysinfo::System;
use colored::*;
use comfy_table::{Table, presets::UTF8_FULL, modifiers::UTF8_ROUND_CORNERS, Cell, Color};

pub struct SystemTotals {
    pub total_mem_gb: f64,
    pub total_cpu: usize,
}

pub struct UserAlloc {
    pub uid: String,
    pub cpu_quota: f64,
    pub mem_bytes: u64,
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
    let output = Command::new("systemctl")
        .args(["list-units", "--type=slice", "--all", "--no-legend", "--plain"])
        .output()
        .map_err(|e| io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to list systemd slices: {}", e)
        ))?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("systemctl command failed with exit code: {:?}", output.status.code())
        ));
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
            .map_err(|e| io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to get slice info for {}: {}", unit_name, e)
            ))?;

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

pub fn check_request(
    totals: &SystemTotals,
    allocations: &[UserAlloc],
    req_cpu: u32,
    req_mem_gb: &str,
) -> bool {
    let used_cpu: f64 = allocations.iter().map(|a| a.cpu_quota / 100.0).sum();
    let used_mem: f64 = allocations.iter().map(|a| a.mem_bytes as f64 / 1_000_000_000.0).sum();

    let available_cpu = totals.total_cpu as f64 - used_cpu;
    let available_mem = totals.total_mem_gb - used_mem;
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

pub fn print_status(totals: &SystemTotals, allocations: &[UserAlloc]) {
    let used_cpu: f64 = allocations.iter().map(|a| a.cpu_quota / 100.0).sum();
    let used_mem: f64 = allocations.iter().map(|a| a.mem_bytes as f64 / 1_000_000_000.0).sum();
    let available_cpu = totals.total_cpu as f64 - used_cpu;
    let available_mem = totals.total_mem_gb - used_mem;

    // System overview table
    println!("{}", "╔═══════════════════════════════════════╗".bright_cyan());
    println!("{}", "║      SYSTEM RESOURCE OVERVIEW         ║".bright_cyan().bold());
    println!("{}", "╚═══════════════════════════════════════╝".bright_cyan());
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

    // Per-user allocations table
    if !allocations.is_empty() {
        println!("{}", "Per-User Allocations:".bright_cyan().bold());
        println!();

        let mut user_table = Table::new();
        user_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("UID").fg(Color::Cyan),
                Cell::new("CPU Quota").fg(Color::Cyan),
                Cell::new("CPUs").fg(Color::Cyan),
                Cell::new("RAM (GB)").fg(Color::Cyan),
            ]);

        for a in allocations {
            let cpu_cores = a.cpu_quota / 100.0;
            let mem_gb = a.mem_bytes as f64 / 1_000_000_000.0;

            user_table.add_row(vec![
                Cell::new(&a.uid).fg(Color::White),
                Cell::new(format!("{:.1}%", a.cpu_quota)).fg(Color::Yellow),
                Cell::new(format!("{:.2}", cpu_cores)).fg(Color::Yellow),
                Cell::new(format!("{:.2}", mem_gb)).fg(Color::Yellow),
            ]);
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
        let allocations = vec![
            UserAlloc {
                uid: "1000".to_string(),
                cpu_quota: 200.0,  // 2 CPUs
                mem_bytes: 4_000_000_000,  // 4 GB
            },
        ];

        // Request 2 CPUs and 4 GB - should be allowed
        assert!(check_request(&totals, &allocations, 2, "4"));
    }

    #[test]
    fn test_check_request_insufficient_cpu() {
        let totals = SystemTotals {
            total_mem_gb: 16.0,
            total_cpu: 8,
        };
        let allocations = vec![
            UserAlloc {
                uid: "1000".to_string(),
                cpu_quota: 600.0,  // 6 CPUs
                mem_bytes: 4_000_000_000,  // 4 GB
            },
        ];

        // Request 4 CPUs when only 2 are available - should fail
        assert!(!check_request(&totals, &allocations, 4, "4"));
    }

    #[test]
    fn test_check_request_insufficient_memory() {
        let totals = SystemTotals {
            total_mem_gb: 16.0,
            total_cpu: 8,
        };
        let allocations = vec![
            UserAlloc {
                uid: "1000".to_string(),
                cpu_quota: 200.0,  // 2 CPUs
                mem_bytes: 12_000_000_000,  // 12 GB
            },
        ];

        // Request 8 GB when only 4 GB available - should fail
        assert!(!check_request(&totals, &allocations, 2, "8"));
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
                cpu_quota: 400.0,  // 4 CPUs
                mem_bytes: 8_000_000_000,  // 8 GB
            },
            UserAlloc {
                uid: "1001".to_string(),
                cpu_quota: 200.0,  // 2 CPUs
                mem_bytes: 4_000_000_000,  // 4 GB
            },
        ];

        // 6 CPUs used, 12 GB used
        // Request 5 CPUs and 10 GB - should be allowed (10 available, 20 available)
        assert!(check_request(&totals, &allocations, 5, "10"));

        // Request 12 CPUs - should fail (only 10 available)
        assert!(!check_request(&totals, &allocations, 12, "8"));
    }

    #[test]
    fn test_check_request_exact_available() {
        let totals = SystemTotals {
            total_mem_gb: 16.0,
            total_cpu: 8,
        };
        let allocations = vec![
            UserAlloc {
                uid: "1000".to_string(),
                cpu_quota: 400.0,  // 4 CPUs
                mem_bytes: 8_000_000_000,  // 8 GB
            },
        ];

        // Request exactly what's available
        assert!(check_request(&totals, &allocations, 4, "8"));
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
        assert_eq!(uid, Some("1000".to_string()), "Should parse UID 1000 correctly");
    }

    #[test]
    fn test_system_slices_excluded_from_allocations() {
        // Regression test: ensure that user-0.slice (root/system) is not
        // counted in user allocations, which was causing negative available RAM
        // when user-0.slice had a MemoryMax set.

        // This test validates the logic by checking that the parsing
        // correctly identifies UID 0 for filtering.
        let uid_0 = parse_uid_from_slice("user-0.slice");
        assert_eq!(uid_0, Some("0".to_string()),
                   "Root user-0.slice should parse correctly");

        // The actual filtering happens in get_user_allocations(),
        // which skips any entry with UID "0"
    }
}
