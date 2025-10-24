use std::process::Command;
use std::io::{self, Write};
use std::fs;
use std::path::Path;
use users;

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
    let mem_bytes = (mem as u64) * 1_000_000_000; // Convert GB to bytes
    writeln!(
        f,
        "[Slice]\nCPUQuota={}%\nMemoryMax={}\n",
        cpu * 100, mem_bytes
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
