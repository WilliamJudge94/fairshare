use std::process::Command;
use std::io::{self, Write};
use std::fs;
use std::path::Path;
use users;

// Minimum default resource thresholds
const MIN_CPU_CORES: u32 = 1;
const MIN_MEM_BYTES: u64 = 5_000_000_000; // 5G

pub fn set_user_limits(cpu: u32, mem: u32) -> io::Result<()> {
    let uid = users::get_current_uid();
    let mem_bytes = (mem as u64) * 1_000_000_000; // Convert GB to bytes

    let status = if uid == 0 {
        // Root user: manage system-wide user slices
        Command::new("systemctl")
            .arg("set-property")
            .arg(&format!("user-{}.slice", uid))
            .arg(format!("CPUQuota={}%", cpu * 100))
            .arg(format!("MemoryMax={}", mem_bytes))
            .status()?
    } else {
        // Regular user: manage their own user session
        Command::new("systemctl")
            .arg("--user")
            .arg("set-property")
            .arg("--")
            .arg("-.slice")
            .arg(format!("CPUQuota={}%", cpu * 100))
            .arg(format!("MemoryMax={}", mem_bytes))
            .status()?
    };

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to set user limits (exit code: {:?})", status.code()),
        ));
    }

    Ok(())
}

pub fn release_user_limits() -> io::Result<()> {
    let uid = users::get_current_uid();

    // Get current user limits to check if they're at or below minimums
    let (current_cpu, current_mem) = get_current_limits()?;

    // Check if already at or below minimums
    if current_cpu <= MIN_CPU_CORES && current_mem <= MIN_MEM_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Cannot release below minimum thresholds: 1 CPU core and 5G RAM required".to_string(),
        ));
    }

    let status = if uid == 0 {
        // Root user: revert system-wide user slice
        Command::new("systemctl")
            .arg("revert")
            .arg(&format!("user-{}.slice", uid))
            .status()?
    } else {
        // Regular user: revert their own user session
        Command::new("systemctl")
            .arg("--user")
            .arg("revert")
            .arg("--")
            .arg("-.slice")
            .status()?
    };

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to release user limits (exit code: {:?})", status.code()),
        ));
    }

    Ok(())
}

fn get_current_limits() -> io::Result<(u32, u64)> {
    let uid = users::get_current_uid();

    let output = if uid == 0 {
        // Root user: show system-wide user slice
        Command::new("systemctl")
            .arg("show")
            .arg(&format!("user-{}.slice", uid))
            .arg("-p")
            .arg("MemoryMax")
            .arg("-p")
            .arg("CPUQuota")
            .output()?
    } else {
        // Regular user: show their own user session
        Command::new("systemctl")
            .arg("--user")
            .arg("show")
            .arg("-.slice")
            .arg("-pMemoryMax")
            .arg("-pCPUQuota")
            .output()?
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut cpu = 0u32;
    let mut mem = 0u64;

    for line in stdout.lines() {
        if line.starts_with("CPUQuota=") {
            if let Some(quota_str) = line.strip_prefix("CPUQuota=") {
                if let Some(pct_str) = quota_str.strip_suffix('%') {
                    if let Ok(pct) = pct_str.parse::<f64>() {
                        cpu = (pct / 100.0) as u32;
                    }
                }
            }
        } else if line.starts_with("MemoryMax=") {
            if let Some(mem_str) = line.strip_prefix("MemoryMax=") {
                mem = parse_mem_bytes(mem_str);
            }
        }
    }

    Ok((cpu, mem))
}

pub fn show_user_info() -> io::Result<()> {
    let uid = users::get_current_uid();

    let output = if uid == 0 {
        // Root user: show system-wide user slice
        Command::new("systemctl")
            .arg("show")
            .arg(&format!("user-{}.slice", uid))
            .arg("-p")
            .arg("MemoryMax")
            .arg("-p")
            .arg("CPUQuota")
            .output()?
    } else {
        // Regular user: show their own user session
        Command::new("systemctl")
            .arg("--user")
            .arg("show")
            .arg("-.slice")
            .arg("-pMemoryMax")
            .arg("-pCPUQuota")
            .output()?
    };

    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

pub fn admin_setup_defaults(cpu: u32, mem: u32) -> io::Result<()> {
    let dir = Path::new("/etc/systemd/system/user-.slice.d");
    let conf_path = dir.join("00-defaults.conf");

    fs::create_dir_all(dir)?;
    let mut f = fs::File::create(&conf_path)?;
    writeln!(
        f,
        "[Slice]\nCPUQuota={}%\nMemoryMax={}G\n",
        cpu, mem
    )?;

    println!("✔ Created {}", conf_path.display());

    Command::new("systemctl").arg("daemon-reload").status()?;
    println!("✔ Reloaded systemd daemon");

    fs::create_dir_all("/etc/fairshare")?;
    let mut policy = fs::File::create("/etc/fairshare/policy.toml")?;
    writeln!(
        policy,
        "[defaults]\ncpu = {}\nmem = {}\n\n[max_caps]\ncpu = {}\nmem = {}\n",
        cpu, mem, cpu * 10, mem
    )?;
    println!("✔ Created /etc/fairshare/policy.toml");

    Ok(())
}

fn parse_mem_bytes(mem: &str) -> u64 {
    let s = mem.trim().to_uppercase();
    if s.ends_with('G') {
        (s.trim_end_matches('G').parse::<f64>().unwrap_or(0.0) * 1e9) as u64
    } else if s.ends_with('M') {
        (s.trim_end_matches('M').parse::<f64>().unwrap_or(0.0) * 1e6) as u64
    } else {
        s.parse::<u64>().unwrap_or(0)
    }
}
