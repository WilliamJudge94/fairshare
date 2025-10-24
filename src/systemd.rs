use std::process::Command;
use std::io::{self, Write};
use std::fs;
use std::path::Path;
use users;

pub fn set_user_limits(cpu: u32, mem: &str) -> io::Result<()> {
    let uid = users::get_current_uid();
    let mem_bytes = parse_mem_bytes(mem);

    Command::new("sudo")
        .args([
            "systemctl",
            "set-property",
            &format!("user-{}.slice", uid),
            &format!("CPUQuota={}%", cpu * 100),
            &format!("MemoryMax={}", mem_bytes),
        ])
        .status()?;

    Ok(())
}

pub fn release_user_limits() -> io::Result<()> {
    let uid = users::get_current_uid();
    Command::new("sudo")
        .args([
            "systemctl",
            "revert",
            &format!("user-{}.slice", uid),
        ])
        .status()?;
    Ok(())
}

pub fn show_user_info() -> io::Result<()> {
    let uid = users::get_current_uid();
    let output = Command::new("systemctl")
        .args([
            "show",
            &format!("user-{}.slice", uid),
            "-p",
            "MemoryMax",
            "-p",
            "CPUQuota",
        ])
        .output()?;

    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

pub fn admin_setup_defaults(cpu: u32, mem: &str) -> io::Result<()> {
    let dir = Path::new("/etc/systemd/system/user-.slice.d");
    let conf_path = dir.join("00-defaults.conf");

    fs::create_dir_all(dir)?;
    let mut f = fs::File::create(&conf_path)?;
    writeln!(
        f,
        "[Slice]\nCPUQuota={}%\nMemoryMax={}\n",
        cpu, mem
    )?;

    println!("✔ Created {}", conf_path.display());

    Command::new("systemctl").arg("daemon-reload").status()?;
    println!("✔ Reloaded systemd daemon");

    fs::create_dir_all("/etc/fairshare")?;
    let mut policy = fs::File::create("/etc/fairshare/policy.toml")?;
    writeln!(
        policy,
        "[defaults]\ncpu = {}\nmem = \"{}\"\n\n[max_caps]\ncpu = {}\nmem = \"{}\"\n",
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
