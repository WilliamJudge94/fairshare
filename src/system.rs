use std::process::Command;
use sysinfo::{System, SystemExt};

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
    sys.refresh_all();

    let total_mem_gb = sys.total_memory() as f64 / 1e6;
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
                "CPUQuota",
            ])
            .output()
            .unwrap();

        let out = String::from_utf8_lossy(&info.stdout);
        let mut mem_bytes = 0;
        let mut cpu_quota = 0.0;

        for l in out.lines() {
            if l.starts_with("MemoryMax=") {
                mem_bytes = l[10..].parse::<u64>().unwrap_or(0);
            } else if l.starts_with("CPUQuota=") {
                cpu_quota = l[9..].trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
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
    let used_mem: f64 = allocations.iter().map(|a| a.mem_bytes as f64 / 1e9).sum();

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
    let used_mem: f64 = allocations.iter().map(|a| a.mem_bytes as f64 / 1e9).sum();

    println!(
        "System total: {:.1} GB RAM / {} CPUs",
        totals.total_mem_gb, totals.total_cpu
    );
    println!(
        "Allocated: {:.1} GB RAM / {:.1} CPUs",
        used_mem, used_cpu
    );
    println!(
        "Available: {:.1} GB RAM / {:.1} CPUs\n",
        totals.total_mem_gb - used_mem,
        totals.total_cpu as f64 - used_cpu
    );

    println!("Per-user allocations:");
    for a in allocations {
        println!(
            "  UID {} → {:.1}% CPU, {:.1} GB RAM",
            a.uid,
            a.cpu_quota,
            a.mem_bytes as f64 / 1e9
        );
    }
}
