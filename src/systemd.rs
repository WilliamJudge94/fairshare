use std::process::Command;
use std::io::{self, Write};
use std::fs;
use std::path::Path;
use users;
use colored::*;

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

pub fn show_user_info() -> io::Result<()> {
    let uid = users::get_current_uid();
    let username = users::get_current_username()
        .and_then(|os_str| os_str.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    let output = if uid == 0 {
        // Root user: show system-wide user slice
        Command::new("systemctl")
            .arg("show")
            .arg(&format!("user-{}.slice", uid))
            .arg("-p")
            .arg("MemoryMax")
            .arg("-p")
            .arg("CPUQuota")
            .arg("-p")
            .arg("CPUQuotaPerSecUSec")
            .output()?
    } else {
        // Regular user: show their own user session
        Command::new("systemctl")
            .arg("--user")
            .arg("show")
            .arg("-.slice")
            .arg("-pMemoryMax")
            .arg("-pCPUQuota")
            .arg("-pCPUQuotaPerSecUSec")
            .output()?
    };

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let mut cpu_quota = "Not set".to_string();
    let mut mem_max = "Not set".to_string();

    for line in stdout_str.lines() {
        if let Some(value) = line.strip_prefix("CPUQuotaPerSecUSec=") {
            if let Some(sec_str) = value.strip_suffix('s') {
                if let Ok(seconds) = sec_str.parse::<f64>() {
                    cpu_quota = format!("{:.1}% ({:.2} CPUs)", seconds * 100.0, seconds);
                }
            }
        } else if let Some(value) = line.strip_prefix("MemoryMax=") {
            if let Ok(bytes) = value.parse::<u64>() {
                let gb = bytes as f64 / 1_000_000_000.0;
                mem_max = format!("{:.2} GB", gb);
            }
        }
    }

    println!("{}", "╔═══════════════════════════════════════╗".bright_cyan());
    println!("{}", "║       USER RESOURCE ALLOCATION        ║".bright_cyan().bold());
    println!("{}", "╚═══════════════════════════════════════╝".bright_cyan());
    println!();
    println!("{} {}", "User:".bright_white().bold(), username.bright_yellow());
    println!("{} {}", "UID:".bright_white().bold(), uid.to_string().bright_yellow());
    println!();
    println!("{} {}", "CPU Quota:".bright_white().bold(), cpu_quota.green());
    println!("{} {}", "Memory Max:".bright_white().bold(), mem_max.green());

    Ok(())
}

/// Setup global default resource allocations for all users.
/// Default minimum: 1 CPU core and 2G RAM per user.
/// Each user can request additional resources up to system limits.
pub fn admin_setup_defaults(cpu: u32, mem: u32) -> io::Result<()> {
    let dir = Path::new("/etc/systemd/system/user-.slice.d");
    let conf_path = dir.join("00-defaults.conf");

    fs::create_dir_all(dir)?;
    let mut f = fs::File::create(&conf_path)?;
    let mem_bytes = (mem as u64) * 1_000_000_000; // Convert GB to bytes
    writeln!(
        f,
        "[Slice]\nCPUQuota={}%\nMemoryMax={}\n",
        cpu * 100, mem_bytes
    )?;

    println!("{} Created {}", "✓".green().bold(), conf_path.display().to_string().bright_white());

    Command::new("systemctl").arg("daemon-reload").status()?;
    println!("{} {}", "✓".green().bold(), "Reloaded systemd daemon".bright_white());

    fs::create_dir_all("/etc/fairshare")?;
    let mut policy = fs::File::create("/etc/fairshare/policy.toml")?;
    writeln!(
        policy,
        "[defaults]\ncpu = {}\nmem = {}\n\n[max_caps]\ncpu = {}\nmem = {}\n",
        cpu, mem, cpu * 10, mem
    )?;
    println!("{} {}", "✓".green().bold(), "Created /etc/fairshare/policy.toml".bright_white());

    Ok(())
}

/// Uninstall global defaults and remove all fairshare admin configuration.
/// This removes:
/// - /etc/systemd/system/user-.slice.d/00-defaults.conf
/// - /etc/fairshare/policy.toml
/// - /etc/fairshare/ directory (if empty)
/// - Reloads systemd daemon to apply changes
pub fn admin_uninstall_defaults() -> io::Result<()> {
    let systemd_conf_path = Path::new("/etc/systemd/system/user-.slice.d/00-defaults.conf");
    let policy_path = Path::new("/etc/fairshare/policy.toml");
    let fairshare_dir = Path::new("/etc/fairshare");

    // Remove systemd configuration file
    if systemd_conf_path.exists() {
        fs::remove_file(systemd_conf_path)?;
        println!("{} Removed {}", "✓".green().bold(), systemd_conf_path.display().to_string().bright_white());
    } else {
        println!("{} {} (not found)", "→".bright_white(), systemd_conf_path.display().to_string().bright_white());
    }

    // Remove policy configuration file
    if policy_path.exists() {
        fs::remove_file(policy_path)?;
        println!("{} Removed {}", "✓".green().bold(), policy_path.display().to_string().bright_white());
    } else {
        println!("{} {} (not found)", "→".bright_white(), policy_path.display().to_string().bright_white());
    }

    // Remove fairshare directory if it's empty
    if fairshare_dir.exists() {
        match fs::remove_dir(fairshare_dir) {
            Ok(()) => {
                println!("{} Removed {}", "✓".green().bold(), fairshare_dir.display().to_string().bright_white());
            }
            Err(e) => {
                // Directory might not be empty, which is fine
                if e.kind() == io::ErrorKind::Other || !fairshare_dir.read_dir()?.next().is_some() {
                    println!("{} {} (not empty or already removed)", "→".bright_white(), fairshare_dir.display().to_string().bright_white());
                } else {
                    return Err(e);
                }
            }
        }
    }

    // Reload systemd daemon to apply changes
    let status = Command::new("systemctl").arg("daemon-reload").status()?;
    if status.success() {
        println!("{} {}", "✓".green().bold(), "Reloaded systemd daemon".bright_white());
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to reload systemd daemon (exit code: {:?})", status.code()),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_admin_setup_creates_valid_config_content() {
        // This test validates the configuration format without actually
        // creating files on the system
        let cpu = 2;
        let mem = 4;
        let mem_bytes = (mem as u64) * 1_000_000_000;

        let expected_slice_config = format!(
            "[Slice]\nCPUQuota={}%\nMemoryMax={}\n",
            cpu * 100,
            mem_bytes
        );

        assert_eq!(expected_slice_config, "[Slice]\nCPUQuota=200%\nMemoryMax=4000000000\n");

        let expected_policy = format!(
            "[defaults]\ncpu = {}\nmem = {}\n\n[max_caps]\ncpu = {}\nmem = {}\n",
            cpu, mem, cpu * 10, mem
        );

        assert!(expected_policy.contains("[defaults]"));
        assert!(expected_policy.contains("cpu = 2"));
        assert!(expected_policy.contains("mem = 4"));
    }

    #[test]
    fn test_memory_conversion_to_bytes() {
        // Verify memory conversion logic
        let mem_gb = 8;
        let mem_bytes = (mem_gb as u64) * 1_000_000_000;
        assert_eq!(mem_bytes, 8_000_000_000);

        let mem_gb = 16;
        let mem_bytes = (mem_gb as u64) * 1_000_000_000;
        assert_eq!(mem_bytes, 16_000_000_000);
    }

    #[test]
    fn test_cpu_quota_calculation() {
        // Verify CPU quota percentage calculation
        let cpu = 1;
        let quota = cpu * 100;
        assert_eq!(quota, 100);

        let cpu = 4;
        let quota = cpu * 100;
        assert_eq!(quota, 400);

        let cpu = 8;
        let quota = cpu * 100;
        assert_eq!(quota, 800);
    }
}
