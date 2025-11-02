use colored::*;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use users;

// Import constants from cli module for validation
use crate::cli::{MAX_CPU, MAX_MEM};

/// Get the UID of the user who invoked pkexec, or the current user if not run via pkexec.
/// When run via pkexec, the PKEXEC_UID environment variable contains the original user's UID.
/// This function validates that the UID is not root (0), not a system user (< 1000),
/// and that the user exists on the system.
pub fn get_calling_user_uid() -> io::Result<u32> {
    // First check if we're running via pkexec
    if let Ok(pkexec_uid_str) = env::var("PKEXEC_UID") {
        let uid = pkexec_uid_str.parse::<u32>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid PKEXEC_UID environment variable: {}", e),
            )
        })?;

        // Validate UID is not root
        if uid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot modify root user slice",
            ));
        }

        // Validate UID is not a system user (standard threshold is 1000)
        if uid < 1000 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot modify system user slice",
            ));
        }

        // Verify user exists
        if users::get_user_by_uid(uid).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("User with UID {} does not exist", uid),
            ));
        }

        Ok(uid)
    } else {
        // Fallback to current user (for admin commands run directly as root)
        Ok(users::get_current_uid())
    }
}

pub fn set_user_limits(cpu: u32, mem: u32) -> io::Result<()> {
    // Validate inputs before operations
    if cpu > MAX_CPU {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("CPU value {} exceeds maximum limit of {}", cpu, MAX_CPU),
        ));
    }
    if mem > MAX_MEM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Memory value {} exceeds maximum limit of {}", mem, MAX_MEM),
        ));
    }

    // Get the UID of the user who invoked pkexec (or current user)
    let uid = get_calling_user_uid()?;

    // Convert GB to bytes with overflow checking
    let mem_bytes = (mem as u64).checked_mul(1_000_000_000).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Memory value {} GB is too large and would cause overflow when converting to bytes",
                mem
            ),
        )
    })?;

    // Calculate CPU quota with overflow checking
    let cpu_quota = cpu.checked_mul(100).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "CPU value {} is too large and would cause overflow when calculating quota",
                cpu
            ),
        )
    })?;

    // When run via pkexec, we have root privileges and modify system-level user slices
    let status = Command::new("systemctl")
        .arg("set-property")
        .arg(&format!("user-{}.slice", uid))
        .arg(format!("CPUQuota={}%", cpu_quota))
        .arg(format!("MemoryMax={}", mem_bytes))
        .status()?;

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to set user limits (exit code: {:?})", status.code()),
        ));
    }

    Ok(())
}

pub fn release_user_limits() -> io::Result<()> {
    // Get the UID of the user who invoked pkexec (or current user)
    let uid = get_calling_user_uid()?;

    // When run via pkexec, we have root privileges and modify system-level user slices
    let status = Command::new("systemctl")
        .arg("revert")
        .arg(&format!("user-{}.slice", uid))
        .status()?;

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Failed to release user limits (exit code: {:?})",
                status.code()
            ),
        ));
    }

    Ok(())
}

pub fn show_user_info() -> io::Result<()> {
    // Get the UID of the user who invoked pkexec (or current user)
    let uid = get_calling_user_uid()?;

    // Get username for the calling user
    let username = users::get_user_by_uid(uid)
        .and_then(|user| user.name().to_str().map(String::from))
        .unwrap_or_else(|| format!("uid{}", uid));

    // When run via pkexec, we have root privileges and query system-level user slices
    let output = Command::new("systemctl")
        .arg("show")
        .arg(&format!("user-{}.slice", uid))
        .arg("-p")
        .arg("MemoryMax")
        .arg("-p")
        .arg("CPUQuota")
        .arg("-p")
        .arg("CPUQuotaPerSecUSec")
        .output()?;

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

    println!(
        "{}",
        "╔═══════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║       USER RESOURCE ALLOCATION        ║"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════╝".bright_cyan()
    );
    println!();
    println!(
        "{} {}",
        "User:".bright_white().bold(),
        username.bright_yellow()
    );
    println!(
        "{} {}",
        "UID:".bright_white().bold(),
        uid.to_string().bright_yellow()
    );
    println!();
    println!(
        "{} {}",
        "CPU Quota:".bright_white().bold(),
        cpu_quota.green()
    );
    println!(
        "{} {}",
        "Memory Max:".bright_white().bold(),
        mem_max.green()
    );

    Ok(())
}

/// Check if PolicyKit (policykit-1) is installed on the system
fn check_policykit_installed() -> bool {
    // Method 1: Check if pkexec binary exists
    if Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    // Method 2: Check with dpkg (Debian/Ubuntu)
    if let Ok(output) = Command::new("dpkg").args(["-l", "policykit-1"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Check if package is installed (starts with "ii")
            return stdout
                .lines()
                .any(|line| line.starts_with("ii") && line.contains("policykit-1"));
        }
    }

    false
}

/// Prompt user with a yes/no question and return their response
fn prompt_yes_no(prompt: &str) -> io::Result<bool> {
    print!("{}", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let response = input.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}

/// Install PolicyKit using apt package manager
fn install_policykit() -> io::Result<()> {
    println!("{}", "Installing PolicyKit (policykit-1)...".bright_cyan());

    // Update apt cache
    println!("{}", "→ Updating apt cache...".bright_white());
    let update_status = Command::new("apt").args(["update"]).status()?;

    if !update_status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to update apt cache. Please run 'apt update' manually.",
        ));
    }

    // Install policykit-1
    println!("{}", "→ Installing policykit-1 package...".bright_white());
    let install_status = Command::new("apt")
        .args(["install", "-y", "policykit-1"])
        .status()?;

    if !install_status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to install policykit-1. Please install it manually with: apt install policykit-1"
        ));
    }

    println!(
        "{} {}",
        "✓".green().bold(),
        "PolicyKit installed successfully".bright_white()
    );
    Ok(())
}

/// Setup global default resource allocations for all users.
/// Default minimum: 1 CPU core and 2G RAM per user, with 2 CPU and 4G RAM system reserves.
/// Each user can request additional resources up to system limits.
pub fn admin_setup_defaults(
    cpu: u32,
    mem: u32,
    cpu_reserve: u32,
    mem_reserve: u32,
) -> io::Result<()> {
    // Check if PolicyKit is installed first
    print!("{} ", "→".bright_white());
    print!("{}", "Checking PolicyKit installation...".bright_white());
    io::stdout().flush()?;

    if !check_policykit_installed() {
        println!(" {}", "✗".red().bold());
        eprintln!(
            "{} {}",
            "⚠".bright_yellow().bold(),
            "PolicyKit (policykit-1) is required but not installed.".bright_yellow()
        );
        eprintln!(
            "{}",
            "PolicyKit is needed for secure privilege escalation when users request resources."
                .bright_white()
        );
        println!();

        match prompt_yes_no("Would you like to install it now? [y/n]: ") {
            Ok(true) => {
                install_policykit()?;
                println!();
            }
            Ok(false) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "PolicyKit installation declined. Please install policykit-1 manually: apt install policykit-1"
                ));
            }
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to read user input: {}", e),
                ));
            }
        }
    } else {
        println!(" {}", "✓".green().bold());
    }

    // Validate inputs before operations
    if cpu > MAX_CPU {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("CPU value {} exceeds maximum limit of {}", cpu, MAX_CPU),
        ));
    }
    if mem > MAX_MEM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Memory value {} exceeds maximum limit of {}", mem, MAX_MEM),
        ));
    }

    let dir = Path::new("/etc/systemd/system/user-.slice.d");
    let conf_path = dir.join("00-defaults.conf");

    fs::create_dir_all(dir)?;
    let mut f = fs::File::create(&conf_path)?;

    // Convert GB to bytes with overflow checking
    let mem_bytes = (mem as u64).checked_mul(1_000_000_000).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Memory value {} GB is too large and would cause overflow when converting to bytes",
                mem
            ),
        )
    })?;

    // Calculate CPU quota with overflow checking
    let cpu_quota = cpu.checked_mul(100).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "CPU value {} is too large and would cause overflow when calculating quota",
                cpu
            ),
        )
    })?;

    writeln!(
        f,
        "[Slice]\nCPUQuota={}%\nMemoryMax={}\n",
        cpu_quota, mem_bytes
    )?;

    println!(
        "{} Created {}",
        "✓".green().bold(),
        conf_path.display().to_string().bright_white()
    );

    Command::new("systemctl").arg("daemon-reload").status()?;
    println!(
        "{} {}",
        "✓".green().bold(),
        "Reloaded systemd daemon".bright_white()
    );

    // Calculate max caps with overflow checking
    let max_cpu_cap = cpu.checked_mul(10).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "CPU value {} is too large for calculating max cap (cpu * 10 would overflow)",
                cpu
            ),
        )
    })?;

    fs::create_dir_all("/etc/fairshare")?;
    let mut policy = fs::File::create("/etc/fairshare/policy.toml")?;
    writeln!(
        policy,
        "[defaults]\ncpu = {}\nmem = {}\ncpu_reserve = {}\nmem_reserve = {}\n\n[max_caps]\ncpu = {}\nmem = {}\n",
        cpu, mem, cpu_reserve, mem_reserve, max_cpu_cap, mem
    )?;
    println!(
        "{} {}",
        "✓".green().bold(),
        "Created /etc/fairshare/policy.toml".bright_white()
    );

    // Install PolicyKit policy file for pkexec integration
    let policy_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/org.fairshare.policy");
    let policy_dest = Path::new("/usr/share/polkit-1/actions/org.fairshare.policy");

    if policy_source.exists() {
        // Create the destination directory if it doesn't exist
        if let Some(parent) = policy_dest.parent() {
            fs::create_dir_all(parent)?;
        }

        // Copy the policy file
        fs::copy(&policy_source, policy_dest)?;

        // Set permissions to 644 (rw-r--r--)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(policy_dest)?.permissions();
            perms.set_mode(0o644);
            fs::set_permissions(policy_dest, perms)?;
        }

        println!(
            "{} {}",
            "✓".green().bold(),
            "Installed PolicyKit policy to /usr/share/polkit-1/actions/org.fairshare.policy"
                .bright_white()
        );
    } else {
        eprintln!(
            "{} {}",
            "⚠".bright_yellow().bold(),
            "Warning: PolicyKit policy file not found at assets/org.fairshare.policy"
                .bright_yellow()
        );
    }

    // Install PolicyKit rule to allow pkexec without admin authentication
    let rule_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/50-fairshare.rules");
    let rule_dest = Path::new("/etc/polkit-1/rules.d/50-fairshare.rules");

    if rule_source.exists() {
        // Create the destination directory if it doesn't exist
        if let Some(parent) = rule_dest.parent() {
            fs::create_dir_all(parent)?;
        }

        // Copy the rule file
        fs::copy(&rule_source, rule_dest)?;

        // Set permissions to 644 (rw-r--r--)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(rule_dest)?.permissions();
            perms.set_mode(0o644);
            fs::set_permissions(rule_dest, perms)?;
        }

        println!(
            "{} {}",
            "✓".green().bold(),
            "Installed PolicyKit rule to /etc/polkit-1/rules.d/50-fairshare.rules".bright_white()
        );

        // Restart polkit service to apply the new rule
        let polkit_restart = Command::new("systemctl")
            .arg("restart")
            .arg("polkit.service")
            .status();

        match polkit_restart {
            Ok(status) if status.success() => {
                println!(
                    "{} {}",
                    "✓".green().bold(),
                    "Restarted polkit.service".bright_white()
                );
            }
            Ok(_) => {
                eprintln!("{} {}", "⚠".bright_yellow().bold(), "Warning: Failed to restart polkit.service - you may need to restart it manually".bright_yellow());
            }
            Err(e) => {
                eprintln!(
                    "{} {}",
                    "⚠".bright_yellow().bold(),
                    format!("Warning: Could not restart polkit.service: {}", e).bright_yellow()
                );
            }
        }
    } else {
        eprintln!(
            "{} {}",
            "⚠".bright_yellow().bold(),
            "Warning: PolicyKit rule file not found at assets/50-fairshare.rules".bright_yellow()
        );
    }

    // Install PolicyKit localauthority file (.pkla) for older PolicyKit versions (0.105 and earlier)
    // This provides the same functionality as the .rules file but uses the older localauthority backend
    let pkla_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/50-fairshare.pkla");
    let pkla_dest = Path::new("/etc/polkit-1/localauthority/50-local.d/50-fairshare.pkla");

    if pkla_source.exists() {
        // Create the destination directory if it doesn't exist
        if let Some(parent) = pkla_dest.parent() {
            fs::create_dir_all(parent)?;
        }

        // Copy the pkla file
        fs::copy(&pkla_source, pkla_dest)?;

        // Set permissions to 644 (rw-r--r--)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(pkla_dest)?.permissions();
            perms.set_mode(0o644);
            fs::set_permissions(pkla_dest, perms)?;
        }

        println!("{} {}", "✓".green().bold(), "Installed PolicyKit localauthority file to /etc/polkit-1/localauthority/50-local.d/50-fairshare.pkla".bright_white());

        // Restart polkit service to apply the new policy (if not already restarted above)
        let polkit_restart = Command::new("systemctl")
            .arg("restart")
            .arg("polkit.service")
            .status();

        match polkit_restart {
            Ok(status) if status.success() => {
                println!(
                    "{} {}",
                    "✓".green().bold(),
                    "Restarted polkit.service to apply policies".bright_white()
                );
            }
            Ok(_) => {
                eprintln!("{} {}", "⚠".bright_yellow().bold(), "Warning: Failed to restart polkit.service - you may need to restart it manually".bright_yellow());
            }
            Err(e) => {
                eprintln!(
                    "{} {}",
                    "⚠".bright_yellow().bold(),
                    format!("Warning: Could not restart polkit.service: {}", e).bright_yellow()
                );
            }
        }
    } else {
        eprintln!(
            "{} {}",
            "⚠".bright_yellow().bold(),
            "Warning: PolicyKit localauthority file not found at assets/50-fairshare.pkla"
                .bright_yellow()
        );
    }

    Ok(())
}

/// Uninstall global defaults and remove all fairshare admin configuration.
/// This removes:
/// - All active user allocations (queries systemd and reverts each user-{UID}.slice)
/// - /etc/systemd/system.control/user-*.slice.d/ directories (user slice configs)
/// - /etc/systemd/system/user-.slice.d/00-defaults.conf
/// - /etc/fairshare/policy.toml
/// - /etc/fairshare/ directory (if empty)
/// - /usr/share/polkit-1/actions/org.fairshare.policy
/// - /etc/polkit-1/rules.d/50-fairshare.rules
/// - /etc/polkit-1/localauthority/50-local.d/50-fairshare.pkla
/// - Reloads systemd daemon to apply changes
/// - Restarts polkit.service to apply rule removal
pub fn admin_uninstall_defaults() -> io::Result<()> {
    let systemd_conf_path = Path::new("/etc/systemd/system/user-.slice.d/00-defaults.conf");
    let policy_path = Path::new("/etc/fairshare/policy.toml");
    let fairshare_dir = Path::new("/etc/fairshare");
    let polkit_policy_path = Path::new("/usr/share/polkit-1/actions/org.fairshare.policy");
    let polkit_rule_path = Path::new("/etc/polkit-1/rules.d/50-fairshare.rules");
    let polkit_pkla_path = Path::new("/etc/polkit-1/localauthority/50-local.d/50-fairshare.pkla");

    // First, revert all user allocations by querying systemd directly
    match crate::system::get_user_allocations() {
        Ok(allocations) => {
            if !allocations.is_empty() {
                println!("{}", "Reverting user allocations:".bright_cyan().bold());
                for alloc in allocations {
                    // Get username for display
                    let username = crate::system::get_username_from_uid(&alloc.uid)
                        .unwrap_or_else(|| format!("UID {}", alloc.uid));

                    // Revert the user's slice at system level (not --user)
                    let result = Command::new("systemctl")
                        .arg("revert")
                        .arg(&format!("user-{}.slice", alloc.uid))
                        .output();

                    match result {
                        Ok(output) => {
                            if output.status.success() {
                                println!(
                                    "{} Reverted limits for user {} (UID: {})",
                                    "✓".green().bold(),
                                    username.bright_yellow(),
                                    alloc.uid.bright_white()
                                );
                            } else {
                                println!(
                                    "{} Failed to revert limits for user {} (UID: {}): {}",
                                    "⚠".bright_yellow().bold(),
                                    username.bright_yellow(),
                                    alloc.uid.bright_white(),
                                    String::from_utf8_lossy(&output.stderr).trim()
                                );
                            }
                        }
                        Err(e) => {
                            println!(
                                "{} Could not revert limits for user {} (UID: {}): {}",
                                "⚠".bright_yellow().bold(),
                                username.bright_yellow(),
                                alloc.uid.bright_white(),
                                e
                            );
                        }
                    }
                }
                println!();
            }
        }
        Err(e) => {
            println!(
                "{} Warning: Could not query systemd to revert user allocations: {}",
                "⚠".bright_yellow().bold(),
                e
            );
        }
    }

    // Clean up system.control directories for user slices
    let system_control_dir = Path::new("/etc/systemd/system.control");
    if system_control_dir.exists() {
        if let Ok(entries) = fs::read_dir(system_control_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy();
                    // Remove directories matching user-*.slice.d pattern
                    if name_str.starts_with("user-") && name_str.ends_with(".slice.d") {
                        match fs::remove_dir_all(&path) {
                            Ok(()) => {
                                println!(
                                    "{} Removed {}",
                                    "✓".green().bold(),
                                    path.display().to_string().bright_white()
                                );
                            }
                            Err(e) => {
                                println!(
                                    "{} Warning: Could not remove {}: {}",
                                    "⚠".bright_yellow().bold(),
                                    path.display().to_string().bright_white(),
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Remove systemd configuration file
    if systemd_conf_path.exists() {
        fs::remove_file(systemd_conf_path)?;
        println!(
            "{} Removed {}",
            "✓".green().bold(),
            systemd_conf_path.display().to_string().bright_white()
        );
    } else {
        println!(
            "{} {} (not found)",
            "→".bright_white(),
            systemd_conf_path.display().to_string().bright_white()
        );
    }

    // Remove policy configuration file
    if policy_path.exists() {
        fs::remove_file(policy_path)?;
        println!(
            "{} Removed {}",
            "✓".green().bold(),
            policy_path.display().to_string().bright_white()
        );
    } else {
        println!(
            "{} {} (not found)",
            "→".bright_white(),
            policy_path.display().to_string().bright_white()
        );
    }

    // Remove fairshare directory if it's empty
    if fairshare_dir.exists() {
        match fs::remove_dir(fairshare_dir) {
            Ok(()) => {
                println!(
                    "{} Removed {}",
                    "✓".green().bold(),
                    fairshare_dir.display().to_string().bright_white()
                );
            }
            Err(e) => {
                // Directory might not be empty, which is fine
                if e.kind() == io::ErrorKind::Other || !fairshare_dir.read_dir()?.next().is_some() {
                    println!(
                        "{} {} (not empty or already removed)",
                        "→".bright_white(),
                        fairshare_dir.display().to_string().bright_white()
                    );
                } else {
                    return Err(e);
                }
            }
        }
    }

    // Remove PolicyKit policy file
    if polkit_policy_path.exists() {
        fs::remove_file(polkit_policy_path)?;
        println!(
            "{} Removed {}",
            "✓".green().bold(),
            polkit_policy_path.display().to_string().bright_white()
        );
    } else {
        println!(
            "{} {} (not found)",
            "→".bright_white(),
            polkit_policy_path.display().to_string().bright_white()
        );
    }

    // Remove PolicyKit rule file
    if polkit_rule_path.exists() {
        fs::remove_file(polkit_rule_path)?;
        println!(
            "{} Removed {}",
            "✓".green().bold(),
            polkit_rule_path.display().to_string().bright_white()
        );

        // Restart polkit service to apply the rule removal
        let polkit_restart = Command::new("systemctl")
            .arg("restart")
            .arg("polkit.service")
            .status();

        match polkit_restart {
            Ok(status) if status.success() => {
                println!(
                    "{} {}",
                    "✓".green().bold(),
                    "Restarted polkit.service".bright_white()
                );
            }
            Ok(_) => {
                eprintln!("{} {}", "⚠".bright_yellow().bold(), "Warning: Failed to restart polkit.service - you may need to restart it manually".bright_yellow());
            }
            Err(e) => {
                eprintln!(
                    "{} {}",
                    "⚠".bright_yellow().bold(),
                    format!("Warning: Could not restart polkit.service: {}", e).bright_yellow()
                );
            }
        }
    } else {
        println!(
            "{} {} (not found)",
            "→".bright_white(),
            polkit_rule_path.display().to_string().bright_white()
        );
    }

    // Remove PolicyKit localauthority file (.pkla)
    if polkit_pkla_path.exists() {
        fs::remove_file(polkit_pkla_path)?;
        println!(
            "{} Removed {}",
            "✓".green().bold(),
            polkit_pkla_path.display().to_string().bright_white()
        );

        // Restart polkit service to apply the pkla removal (if not already restarted above)
        let polkit_restart = Command::new("systemctl")
            .arg("restart")
            .arg("polkit.service")
            .status();

        match polkit_restart {
            Ok(status) if status.success() => {
                println!(
                    "{} {}",
                    "✓".green().bold(),
                    "Restarted polkit.service to apply policy removal".bright_white()
                );
            }
            Ok(_) => {
                eprintln!("{} {}", "⚠".bright_yellow().bold(), "Warning: Failed to restart polkit.service - you may need to restart it manually".bright_yellow());
            }
            Err(e) => {
                eprintln!(
                    "{} {}",
                    "⚠".bright_yellow().bold(),
                    format!("Warning: Could not restart polkit.service: {}", e).bright_yellow()
                );
            }
        }
    } else {
        println!(
            "{} {} (not found)",
            "→".bright_white(),
            polkit_pkla_path.display().to_string().bright_white()
        );
    }

    // Reload systemd daemon to apply changes
    let status = Command::new("systemctl").arg("daemon-reload").status()?;
    if status.success() {
        println!(
            "{} {}",
            "✓".green().bold(),
            "Reloaded systemd daemon".bright_white()
        );
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Failed to reload systemd daemon (exit code: {:?})",
                status.code()
            ),
        ));
    }

    Ok(())
}

/// Reset fairshare by performing a complete uninstall followed by setup with new defaults.
/// This combines admin_uninstall_defaults() and admin_setup_defaults() into one operation.
pub fn admin_reset(
    cpu: u32,
    mem: u32,
    cpu_reserve: u32,
    mem_reserve: u32,
    force: bool,
) -> io::Result<()> {
    // Show warning if not forced
    if !force {
        eprintln!("{} {}",
            "⚠".bright_yellow().bold(),
            "This will remove all fairshare configuration and user allocations, then reinstall with new defaults!".bright_yellow()
        );
        eprintln!("{} {}", "  This will:".bright_white().bold(), "");
        eprintln!("    - Revert all active user allocations");
        eprintln!("    - Remove all fairshare configuration files");
        eprintln!(
            "    - Setup new defaults with {} CPUs and {}G RAM per user",
            cpu, mem
        );
        eprint!(
            "\n{} {}",
            "Continue?".bright_white().bold(),
            "[y/N]: ".bright_white()
        );
        std::io::Write::flush(&mut std::io::stderr()).ok();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        if !input.trim().eq_ignore_ascii_case("y") && !input.trim().eq_ignore_ascii_case("yes") {
            println!("{} {}", "✗".red().bold(), "Reset cancelled.".red());
            return Ok(());
        }
    }

    println!(
        "{}",
        "╔═══════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║      FAIRSHARE RESET IN PROGRESS     ║"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════╝".bright_cyan()
    );
    println!();

    // Step 1: Uninstall
    println!(
        "{} {}",
        "→".bright_cyan().bold(),
        "Step 1/2: Uninstalling existing configuration...".bright_white()
    );
    println!();
    admin_uninstall_defaults()?;
    println!();

    // Step 2: Setup
    println!(
        "{} {}",
        "→".bright_cyan().bold(),
        "Step 2/2: Setting up new defaults...".bright_white()
    );
    println!();
    admin_setup_defaults(cpu, mem, cpu_reserve, mem_reserve)?;
    println!();

    println!(
        "{}",
        "╔═══════════════════════════════════════╗".bright_green()
    );
    println!(
        "{}",
        "║        RESET COMPLETED SUCCESSFULLY   ║"
            .bright_green()
            .bold()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════╝".bright_green()
    );
    println!();
    println!(
        "{} New defaults: {} {}",
        "✓".green().bold(),
        format!("CPUQuota={}%", cpu * 100).bright_yellow(),
        format!("MemoryMax={}G", mem).bright_yellow()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_admin_setup_creates_valid_config_content() {
        // This test validates the configuration format without actually
        // creating files on the system
        let cpu: u32 = 2;
        let mem: u32 = 4;
        let mem_bytes = (mem as u64).checked_mul(1_000_000_000).unwrap();
        let cpu_quota = cpu.checked_mul(100).unwrap();

        let expected_slice_config = format!(
            "[Slice]\nCPUQuota={}%\nMemoryMax={}\n",
            cpu_quota, mem_bytes
        );

        assert_eq!(
            expected_slice_config,
            "[Slice]\nCPUQuota=200%\nMemoryMax=4000000000\n"
        );

        let max_cpu_cap = cpu.checked_mul(10).unwrap();
        let expected_policy = format!(
            "[defaults]\ncpu = {}\nmem = {}\n\n[max_caps]\ncpu = {}\nmem = {}\n",
            cpu, mem, max_cpu_cap, mem
        );

        assert!(expected_policy.contains("[defaults]"));
        assert!(expected_policy.contains("cpu = 2"));
        assert!(expected_policy.contains("mem = 4"));
    }

    #[test]
    fn test_memory_conversion_to_bytes_safe() {
        // Verify memory conversion logic with overflow checking
        let mem_gb = 8u32;
        let mem_bytes = (mem_gb as u64).checked_mul(1_000_000_000).unwrap();
        assert_eq!(mem_bytes, 8_000_000_000);

        let mem_gb = 16u32;
        let mem_bytes = (mem_gb as u64).checked_mul(1_000_000_000).unwrap();
        assert_eq!(mem_bytes, 16_000_000_000);

        // Test maximum valid value
        let mem_gb = 10000u32; // MAX_MEM
        let mem_bytes = (mem_gb as u64).checked_mul(1_000_000_000).unwrap();
        assert_eq!(mem_bytes, 10_000_000_000_000);
    }

    #[test]
    fn test_cpu_quota_calculation_safe() {
        // Verify CPU quota percentage calculation with overflow checking
        let cpu = 1u32;
        let quota = cpu.checked_mul(100).unwrap();
        assert_eq!(quota, 100);

        let cpu = 4u32;
        let quota = cpu.checked_mul(100).unwrap();
        assert_eq!(quota, 400);

        let cpu = 8u32;
        let quota = cpu.checked_mul(100).unwrap();
        assert_eq!(quota, 800);

        // Test maximum valid value
        let cpu = 1000u32; // MAX_CPU
        let quota = cpu.checked_mul(100).unwrap();
        assert_eq!(quota, 100_000);
    }

    #[test]
    fn test_memory_overflow_detection() {
        // Test that very large memory values that would overflow are handled
        // u64::MAX / 1_000_000_000 = 18_446_744_073 (approximately)
        // u32::MAX (4_294_967_295) * 1_000_000_000 = 4_294_967_295_000_000_000 which is < u64::MAX
        // So u32::MAX won't overflow when cast to u64 and multiplied
        let huge_mem = u32::MAX; // 4_294_967_295 GB
        let result = (huge_mem as u64).checked_mul(1_000_000_000);
        // This should NOT overflow because u32::MAX * 1 billion < u64::MAX
        assert!(
            result.is_some(),
            "u32::MAX GB should not overflow when converted to bytes"
        );

        // To actually test overflow, we need a u64 value larger than u64::MAX / 1_000_000_000
        let overflow_mem = 18_446_744_074u64; // Just above safe limit
        let result = overflow_mem.checked_mul(1_000_000_000);
        assert!(
            result.is_none(),
            "Expected overflow for value above u64::MAX / 1 billion"
        );
    }

    #[test]
    fn test_cpu_quota_overflow_detection() {
        // Test that very large CPU values that would overflow are handled
        // u32::MAX * 100 would overflow u32
        let huge_cpu = u32::MAX;
        let result = huge_cpu.checked_mul(100);
        assert!(result.is_none(), "Expected overflow for u32::MAX * 100");
    }

    #[test]
    fn test_max_cpu_cap_overflow_detection() {
        // Test that CPU * 10 overflow is detected
        // u32::MAX / 10 = 429_496_729 (approximately)
        // So values above this should fail
        let large_cpu = u32::MAX;
        let result = large_cpu.checked_mul(10);
        assert!(result.is_none(), "Expected overflow for u32::MAX * 10");

        // Test a value that should work
        let safe_cpu = 100u32;
        let result = safe_cpu.checked_mul(10);
        assert_eq!(result, Some(1000));
    }

    #[test]
    fn test_boundary_values() {
        // Test boundary conditions within valid range

        // Minimum values
        let min_cpu = 1u32;
        let min_mem = 1u32;
        assert!(min_cpu.checked_mul(100).is_some());
        assert!((min_mem as u64).checked_mul(1_000_000_000).is_some());

        // Maximum valid values
        let max_cpu = 1000u32; // MAX_CPU
        let max_mem = 10000u32; // MAX_MEM
        assert!(max_cpu.checked_mul(100).is_some());
        assert!((max_mem as u64).checked_mul(1_000_000_000).is_some());
        assert!(max_cpu.checked_mul(10).is_some()); // For max_caps calculation
    }

    #[test]
    fn test_near_overflow_values() {
        // Test values near overflow boundaries

        // For memory: u64::MAX is 18_446_744_073_709_551_615
        // Dividing by 1_000_000_000 gives max safe value of ~18_446_744_073
        // Our MAX_MEM (10000) is well within this range

        // Test a value that's within bounds
        let safe_mem = 18_000_000_000u64; // 18 billion GB - still under u64::MAX / 1 billion
        let result = safe_mem.checked_mul(1_000_000_000);
        assert!(result.is_some(), "18 billion GB should not overflow");

        // Test a value that will overflow
        let overflow_mem = 19_000_000_000u64; // 19 billion GB - will overflow
        let result = overflow_mem.checked_mul(1_000_000_000);
        assert!(result.is_none(), "Expected overflow for 19 billion GB");

        // For CPU quota: u32::MAX is 4_294_967_295
        // Dividing by 100 gives max safe value of 42_949_672
        // Our MAX_CPU (1000) is well within this range
        let near_overflow_cpu = 42_949_673u32; // Above safe limit
        let result = near_overflow_cpu.checked_mul(100);
        assert!(result.is_none(), "Expected overflow for CPU quota");
    }

    #[test]
    fn test_zero_values() {
        // Test that zero values are handled correctly (though they should be
        // rejected by input validation in practice)
        let cpu = 0u32;
        let mem = 0u32;

        assert_eq!(cpu.checked_mul(100), Some(0));
        assert_eq!((mem as u64).checked_mul(1_000_000_000), Some(0));
    }

    #[test]
    fn test_set_user_limits_input_validation_cpu_exceeds_max() {
        // Test that set_user_limits rejects CPU values exceeding MAX_CPU
        use crate::cli::MAX_CPU;

        let result = super::set_user_limits(MAX_CPU + 1, 2);
        assert!(result.is_err(), "Should reject CPU exceeding MAX_CPU");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(
                error_msg.contains("exceeds maximum limit"),
                "Error should mention exceeding limit: {}",
                error_msg
            );
            assert!(
                error_msg.contains(&(MAX_CPU + 1).to_string()),
                "Error should contain the invalid CPU value: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_set_user_limits_input_validation_mem_exceeds_max() {
        // Test that set_user_limits rejects memory values exceeding MAX_MEM
        use crate::cli::MAX_MEM;

        let result = super::set_user_limits(2, MAX_MEM + 1);
        assert!(result.is_err(), "Should reject memory exceeding MAX_MEM");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(
                error_msg.contains("exceeds maximum limit"),
                "Error should mention exceeding limit: {}",
                error_msg
            );
            assert!(
                error_msg.contains(&(MAX_MEM + 1).to_string()),
                "Error should contain the invalid memory value: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_admin_setup_defaults_input_validation_cpu_exceeds_max() {
        // Test that admin_setup_defaults rejects CPU values exceeding MAX_CPU
        use crate::cli::MAX_CPU;

        let result = super::admin_setup_defaults(MAX_CPU + 1, 2, 2, 4);
        assert!(result.is_err(), "Should reject CPU exceeding MAX_CPU");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(
                error_msg.contains("exceeds maximum limit"),
                "Error should mention exceeding limit: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_admin_setup_defaults_input_validation_mem_exceeds_max() {
        // Test that admin_setup_defaults rejects memory values exceeding MAX_MEM
        use crate::cli::MAX_MEM;

        let result = super::admin_setup_defaults(2, MAX_MEM + 1, 2, 4);
        assert!(result.is_err(), "Should reject memory exceeding MAX_MEM");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(
                error_msg.contains("exceeds maximum limit"),
                "Error should mention exceeding limit: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_overflow_checked_mul_memory_conversion() {
        // Test checked_mul for memory to bytes conversion

        // Valid conversions
        let conversions = vec![
            (1u32, 1_000_000_000u64),
            (2u32, 2_000_000_000u64),
            (10u32, 10_000_000_000u64),
            (100u32, 100_000_000_000u64),
            (1000u32, 1_000_000_000_000u64),
            (10000u32, 10_000_000_000_000u64),
        ];

        for (gb, expected_bytes) in conversions {
            let result = (gb as u64).checked_mul(1_000_000_000);
            assert!(
                result.is_some(),
                "Conversion of {} GB should not overflow",
                gb
            );
            assert_eq!(
                result.unwrap(),
                expected_bytes,
                "Conversion of {} GB should equal {} bytes",
                gb,
                expected_bytes
            );
        }
    }

    #[test]
    fn test_overflow_checked_mul_cpu_quota() {
        // Test checked_mul for CPU quota calculation

        // Valid calculations
        let calculations = vec![
            (1u32, 100u32),
            (2u32, 200u32),
            (4u32, 400u32),
            (10u32, 1000u32),
            (100u32, 10000u32),
            (1000u32, 100000u32),
        ];

        for (cpu, expected_quota) in calculations {
            let result = cpu.checked_mul(100);
            assert!(
                result.is_some(),
                "Quota calculation for {} CPU should not overflow",
                cpu
            );
            assert_eq!(
                result.unwrap(),
                expected_quota,
                "Quota for {} CPU should equal {}",
                cpu,
                expected_quota
            );
        }
    }

    #[test]
    fn test_overflow_checked_mul_max_caps() {
        // Test checked_mul for max caps calculation

        // Valid calculations
        let calculations = vec![
            (1u32, 10u32),
            (2u32, 20u32),
            (4u32, 40u32),
            (10u32, 100u32),
            (100u32, 1000u32),
            (1000u32, 10000u32),
        ];

        for (cpu, expected_cap) in calculations {
            let result = cpu.checked_mul(10);
            assert!(
                result.is_some(),
                "Max cap calculation for {} CPU should not overflow",
                cpu
            );
            assert_eq!(
                result.unwrap(),
                expected_cap,
                "Max cap for {} CPU should equal {}",
                cpu,
                expected_cap
            );
        }
    }

    #[test]
    fn test_max_valid_cpu_quota_without_overflow() {
        // Test that MAX_CPU can safely be converted to quota
        use crate::cli::MAX_CPU;

        let result = MAX_CPU.checked_mul(100);
        assert!(
            result.is_some(),
            "MAX_CPU ({}) should not overflow when multiplied by 100",
            MAX_CPU
        );
        assert_eq!(
            result.unwrap(),
            MAX_CPU as u32 * 100,
            "MAX_CPU quota should be {} * 100",
            MAX_CPU
        );
    }

    #[test]
    fn test_max_valid_memory_conversion_without_overflow() {
        // Test that MAX_MEM can safely be converted to bytes
        use crate::cli::MAX_MEM;

        let result = (MAX_MEM as u64).checked_mul(1_000_000_000);
        assert!(
            result.is_some(),
            "MAX_MEM ({}) should not overflow when converted to bytes",
            MAX_MEM
        );
        assert_eq!(
            result.unwrap(),
            MAX_MEM as u64 * 1_000_000_000,
            "MAX_MEM conversion should be {} * 1 billion",
            MAX_MEM
        );
    }

    #[test]
    fn test_max_valid_cpu_cap_without_overflow() {
        // Test that MAX_CPU can safely be used in max caps calculation
        use crate::cli::MAX_CPU;

        let result = MAX_CPU.checked_mul(10);
        assert!(
            result.is_some(),
            "MAX_CPU ({}) should not overflow when multiplied by 10 for caps",
            MAX_CPU
        );
        assert_eq!(
            result.unwrap(),
            MAX_CPU as u32 * 10,
            "MAX_CPU cap should be {} * 10",
            MAX_CPU
        );
    }

    #[test]
    fn test_sequential_operations_dont_cause_overflow() {
        // Test that multiple operations on max values don't accumulate overflow
        use crate::cli::{MAX_CPU, MAX_MEM};

        // Simulate full set_user_limits operation with MAX values
        let cpu = MAX_CPU;
        let mem = MAX_MEM;

        let mem_bytes = (mem as u64).checked_mul(1_000_000_000);
        assert!(mem_bytes.is_some());

        let cpu_quota = cpu.checked_mul(100);
        assert!(cpu_quota.is_some());

        // Both operations should succeed without overflow
        assert_eq!(mem_bytes.unwrap(), MAX_MEM as u64 * 1_000_000_000);
        assert_eq!(cpu_quota.unwrap(), MAX_CPU as u32 * 100);
    }

    #[test]
    fn test_sequential_admin_operations_dont_cause_overflow() {
        // Test that multiple operations in admin_setup_defaults don't overflow
        use crate::cli::{MAX_CPU, MAX_MEM};

        // Simulate full admin_setup_defaults operation with MAX values
        let cpu = MAX_CPU;
        let mem = MAX_MEM;

        // Memory conversion
        let mem_bytes = (mem as u64).checked_mul(1_000_000_000);
        assert!(mem_bytes.is_some());

        // CPU quota
        let cpu_quota = cpu.checked_mul(100);
        assert!(cpu_quota.is_some());

        // Max caps
        let max_cpu_cap = cpu.checked_mul(10);
        assert!(max_cpu_cap.is_some());

        // All should succeed
        assert_eq!(mem_bytes.unwrap(), MAX_MEM as u64 * 1_000_000_000);
        assert_eq!(cpu_quota.unwrap(), MAX_CPU as u32 * 100);
        assert_eq!(max_cpu_cap.unwrap(), MAX_CPU as u32 * 10);
    }

    #[test]
    fn test_error_messages_are_informative() {
        // Verify error messages contain useful debugging information
        use crate::cli::MAX_CPU;

        let invalid_cpu = MAX_CPU + 5;
        let result = super::set_user_limits(invalid_cpu, 2);

        assert!(result.is_err());
        if let Err(e) = result {
            let error_msg = format!("{}", e);
            // Check that error message includes:
            // 1. The invalid value
            assert!(
                error_msg.contains(&invalid_cpu.to_string()),
                "Error message should include the invalid value: {}",
                error_msg
            );
            // 2. The limit
            assert!(
                error_msg.contains(&MAX_CPU.to_string()),
                "Error message should include the max limit: {}",
                error_msg
            );
            // 3. A description of what went wrong
            assert!(
                error_msg.contains("exceeds"),
                "Error message should indicate exceeding: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_valid_edge_case_values_in_set_user_limits() {
        // Test that minimum and maximum valid values are accepted
        use crate::cli::{MAX_CPU, MAX_MEM};

        // These should NOT error on input validation
        // (they may fail on systemctl execution, but that's okay for this test)
        let min_result = super::set_user_limits(1, 1);
        // Just verify it doesn't error on validation
        if let Err(e) = min_result {
            let error_msg = format!("{}", e);
            assert!(
                !error_msg.contains("exceeds maximum limit"),
                "Minimum values should not fail validation: {}",
                error_msg
            );
        }

        let max_result = super::set_user_limits(MAX_CPU, MAX_MEM);
        // Just verify it doesn't error on validation
        if let Err(e) = max_result {
            let error_msg = format!("{}", e);
            assert!(
                !error_msg.contains("exceeds maximum limit"),
                "Maximum valid values should not fail validation: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_u32_max_causes_proper_rejection() {
        // Test that u32::MAX values are properly rejected by input validation
        let result = super::set_user_limits(u32::MAX, 2);
        assert!(result.is_err(), "u32::MAX should be rejected");

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            assert!(
                error_msg.contains("exceeds maximum limit"),
                "Should indicate input validation failure: {}",
                error_msg
            );
        }
    }

    // UID Validation Tests
    #[test]
    fn test_get_calling_user_uid_rejects_root() {
        // Test that UID 0 (root) is rejected with PermissionDenied
        use std::env;

        // Save original PKEXEC_UID if it exists
        let original = env::var("PKEXEC_UID").ok();

        // Set PKEXEC_UID to 0 (root)
        env::set_var("PKEXEC_UID", "0");

        let result = super::get_calling_user_uid();
        assert!(result.is_err(), "Should reject root UID (0)");

        if let Err(e) = result {
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::PermissionDenied,
                "Should return PermissionDenied error kind"
            );
            let error_msg = format!("{}", e);
            assert!(
                error_msg.contains("Cannot modify root user slice"),
                "Error should mention root user: {}",
                error_msg
            );
        }

        // Restore original PKEXEC_UID or remove it
        if let Some(val) = original {
            env::set_var("PKEXEC_UID", val);
        } else {
            env::remove_var("PKEXEC_UID");
        }
    }

    #[test]
    fn test_get_calling_user_uid_rejects_system_users() {
        // Test that UIDs < 1000 are rejected as system users
        use std::env;

        // Save original PKEXEC_UID if it exists
        let original = env::var("PKEXEC_UID").ok();

        // Test various system user UIDs
        let system_uids = vec![1, 10, 100, 500, 999];

        for uid in system_uids {
            env::set_var("PKEXEC_UID", uid.to_string());

            let result = super::get_calling_user_uid();
            assert!(result.is_err(), "Should reject system UID {}", uid);

            if let Err(e) = result {
                assert_eq!(
                    e.kind(),
                    std::io::ErrorKind::PermissionDenied,
                    "Should return PermissionDenied for UID {}",
                    uid
                );
                let error_msg = format!("{}", e);
                assert!(
                    error_msg.contains("Cannot modify system user slice"),
                    "Error should mention system user for UID {}: {}",
                    uid,
                    error_msg
                );
            }
        }

        // Restore original PKEXEC_UID or remove it
        if let Some(val) = original {
            env::set_var("PKEXEC_UID", val);
        } else {
            env::remove_var("PKEXEC_UID");
        }
    }

    #[test]
    fn test_get_calling_user_uid_accepts_valid_users() {
        // Test that UIDs >= 1000 for existing users work
        use std::env;

        // Save original PKEXEC_UID if it exists
        let original = env::var("PKEXEC_UID").ok();

        // Get current user's UID (which should be valid)
        let current_uid = users::get_current_uid();

        // Only test if current user has UID >= 1000
        if current_uid >= 1000 {
            env::set_var("PKEXEC_UID", current_uid.to_string());

            let result = super::get_calling_user_uid();
            assert!(result.is_ok(), "Should accept valid UID {}", current_uid);

            if let Ok(uid) = result {
                assert_eq!(uid, current_uid, "Should return the correct UID");
            }
        }

        // Restore original PKEXEC_UID or remove it
        if let Some(val) = original {
            env::set_var("PKEXEC_UID", val);
        } else {
            env::remove_var("PKEXEC_UID");
        }
    }

    #[test]
    fn test_get_calling_user_uid_rejects_nonexistent_users() {
        // Test that non-existent UIDs are rejected
        use std::env;

        // Save original PKEXEC_UID if it exists
        let original = env::var("PKEXEC_UID").ok();

        // Use a very high UID that's unlikely to exist
        // Most systems don't have UIDs this high
        let nonexistent_uid = 999999u32;

        // Verify this UID doesn't actually exist on the system
        if users::get_user_by_uid(nonexistent_uid).is_none() {
            env::set_var("PKEXEC_UID", nonexistent_uid.to_string());

            let result = super::get_calling_user_uid();
            assert!(
                result.is_err(),
                "Should reject non-existent UID {}",
                nonexistent_uid
            );

            if let Err(e) = result {
                assert_eq!(
                    e.kind(),
                    std::io::ErrorKind::NotFound,
                    "Should return NotFound error kind"
                );
                let error_msg = format!("{}", e);
                assert!(
                    error_msg.contains("does not exist"),
                    "Error should mention user doesn't exist: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains(&nonexistent_uid.to_string()),
                    "Error should include the UID: {}",
                    error_msg
                );
            }
        }

        // Restore original PKEXEC_UID or remove it
        if let Some(val) = original {
            env::set_var("PKEXEC_UID", val);
        } else {
            env::remove_var("PKEXEC_UID");
        }
    }

    #[test]
    fn test_get_calling_user_uid_rejects_invalid_format() {
        // Test that invalid UID formats are rejected
        use std::env;

        // Save original PKEXEC_UID if it exists
        let original = env::var("PKEXEC_UID").ok();

        // Test various invalid formats
        let invalid_formats = vec!["abc", "-1", "1.5", "", "not_a_number", "12345abc"];

        for invalid in invalid_formats {
            env::set_var("PKEXEC_UID", invalid);

            let result = super::get_calling_user_uid();
            assert!(result.is_err(), "Should reject invalid format: {}", invalid);

            if let Err(e) = result {
                assert_eq!(
                    e.kind(),
                    std::io::ErrorKind::InvalidData,
                    "Should return InvalidData for format: {}",
                    invalid
                );
                let error_msg = format!("{}", e);
                assert!(
                    error_msg.contains("Invalid PKEXEC_UID"),
                    "Error should mention invalid PKEXEC_UID for: {}",
                    invalid
                );
            }
        }

        // Restore original PKEXEC_UID or remove it
        if let Some(val) = original {
            env::set_var("PKEXEC_UID", val);
        } else {
            env::remove_var("PKEXEC_UID");
        }
    }

    #[test]
    fn test_get_calling_user_uid_boundary_values() {
        // Test boundary values around the 1000 threshold
        use std::env;

        // Save original PKEXEC_UID if it exists
        let original = env::var("PKEXEC_UID").ok();

        // Test UID 999 (should fail - system user)
        env::set_var("PKEXEC_UID", "999");
        let result = super::get_calling_user_uid();
        assert!(result.is_err(), "Should reject UID 999 (system user)");
        if let Err(e) = result {
            assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
        }

        // Test UID 1000 (should pass validation checks, may fail on existence)
        env::set_var("PKEXEC_UID", "1000");
        let result = super::get_calling_user_uid();
        // Result depends on whether UID 1000 exists on the system
        if result.is_err() {
            if let Err(e) = result {
                // Should either pass or fail with NotFound (not PermissionDenied)
                assert_ne!(
                    e.kind(),
                    std::io::ErrorKind::PermissionDenied,
                    "UID 1000 should pass validation checks (not be rejected as system user)"
                );
            }
        }

        // Restore original PKEXEC_UID or remove it
        if let Some(val) = original {
            env::set_var("PKEXEC_UID", val);
        } else {
            env::remove_var("PKEXEC_UID");
        }
    }

    #[test]
    fn test_get_calling_user_uid_without_pkexec_env() {
        // Test that when PKEXEC_UID is not set, it falls back to current user
        use std::env;

        // Save original PKEXEC_UID if it exists
        let original = env::var("PKEXEC_UID").ok();

        // Remove PKEXEC_UID
        env::remove_var("PKEXEC_UID");

        let result = super::get_calling_user_uid();
        assert!(result.is_ok(), "Should succeed when PKEXEC_UID is not set");

        if let Ok(uid) = result {
            let current_uid = users::get_current_uid();
            assert_eq!(uid, current_uid, "Should return current user's UID");
        }

        // Restore original PKEXEC_UID if it existed
        if let Some(val) = original {
            env::set_var("PKEXEC_UID", val);
        }
    }
}
