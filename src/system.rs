use std::process::Command;
use sysinfo::System;

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

pub fn get_user_allocations() -> Vec<UserAlloc> {
    let output = Command::new("bash")
        .arg("-c")
        .arg("systemctl list-units --type=slice --all | grep user- | awk '{print $1}'")
        .output()
        .expect("failed to list slices");

    let mut allocations = vec![];

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let uid = line
            .trim()
            .split('-')
            .nth(1)
            .unwrap_or("")
            .trim_end_matches(".slice")
            .to_string();

        let info = Command::new("systemctl")
            .args([
                "show",
                &format!("user-{}.slice", uid),
                "-p",
                "MemoryMax",
                "-p",
                "CPUQuotaPerSecUSec",
            ])
            .output()
            .unwrap();

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

    allocations
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

    println!(
        "System total: {:.2} GB RAM / {} CPUs",
        totals.total_mem_gb, totals.total_cpu
    );
    println!(
        "Allocated: {:.2} GB RAM / {:.2} CPUs",
        used_mem, used_cpu
    );
    println!(
        "Available: {:.2} GB RAM / {:.2} CPUs\n",
        totals.total_mem_gb - used_mem,
        totals.total_cpu as f64 - used_cpu
    );

    println!("Per-user allocations:");
    for a in allocations {
        println!(
            "  UID {} → {:.1}% CPU, {:.2} GB RAM",
            a.uid,
            a.cpu_quota,
            a.mem_bytes as f64 / 1_000_000_000.0
        );
    }
}
