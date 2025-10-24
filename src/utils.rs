use anyhow::{Result, Context};
use std::path::{Path, PathBuf};
use std::fs;
use tracing::{debug, warn};

/// Parse memory size string (e.g., "1G", "512M") to bytes
pub fn parse_memory_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim();

    // Extract the numeric part and the unit part
    let mut num_end = 0;
    for (i, c) in size_str.chars().enumerate() {
        if !c.is_ascii_digit() && c != '.' {
            num_end = i;
            break;
        }
    }

    // If num_end is still 0, the entire string was numeric
    if num_end == 0 {
        num_end = size_str.len();
    }

    if num_end == 0 {
        anyhow::bail!("Invalid memory size format: {}", size_str);
    }

    let number: f64 = size_str[..num_end]
        .parse()
        .with_context(|| format!("Failed to parse number from: {}", size_str))?;

    let unit = size_str[num_end..].trim().to_uppercase();

    let multiplier: u64 = match unit.as_str() {
        "B" | "" => 1,
        "K" | "KB" => 1024,
        "M" | "MB" => 1024 * 1024,
        "G" | "GB" => 1024 * 1024 * 1024,
        "T" | "TB" => 1024 * 1024 * 1024 * 1024,
        _ => anyhow::bail!("Unknown memory unit: {}. Supported units: B, K, KB, M, MB, G, GB, T, TB", unit),
    };

    let bytes = (number * multiplier as f64) as u64;

    if bytes == 0 {
        anyhow::bail!("Memory size must be greater than 0");
    }

    Ok(bytes)
}

/// Format bytes to human-readable size string
pub fn format_memory_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2}T", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2}K", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Validate slice name format
pub fn validate_slice_name(name: &str) -> Result<()> {
    // TODO: Check if slice name follows systemd naming conventions
    // TODO: Ensure it ends with .slice
    // TODO: Check for invalid characters

    todo!("Implement validate_slice_name")
}

/// Get the cgroup path for a process
pub fn get_process_cgroup(pid: u32) -> Result<String> {
    debug!("Getting cgroup for PID: {}", pid);

    // TODO: Read /proc/{pid}/cgroup
    // TODO: Parse cgroup information
    // TODO: Return cgroup path

    todo!("Implement get_process_cgroup")
}

/// Read process information from /proc
pub fn get_process_info(pid: u32) -> Result<ProcessInfo> {
    debug!("Getting process info for PID: {}", pid);

    // TODO: Read /proc/{pid}/status
    // TODO: Read /proc/{pid}/cmdline
    // TODO: Parse and return process information

    todo!("Implement get_process_info")
}

/// Process information structure
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub uid: u32,
    pub gid: u32,
    pub command: String,
    pub cgroup: String,
}

/// Ensure a directory exists, create if it doesn't
pub fn ensure_directory(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    if !path.exists() {
        debug!("Creating directory: {:?}", path);
        fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {:?}", path))?;
    }

    Ok(())
}

/// Check if the current process has root privileges
pub fn is_root() -> bool {
    // TODO: Check effective UID
    // TODO: Return true if running as root

    todo!("Implement is_root")
}

/// Convert systemd property name to DBus variant
pub fn property_to_dbus_variant(key: &str, value: &str) -> Result<String> {
    // TODO: Convert property values to appropriate DBus variant types
    // TODO: Handle different property types (strings, integers, etc.)

    todo!("Implement property_to_dbus_variant")
}

/// Parse a glob pattern for matching cgroup paths
pub fn match_cgroup_pattern(pattern: &str, cgroup_path: &str) -> bool {
    // TODO: Implement glob pattern matching
    // TODO: Support wildcards (* and ?)
    // TODO: Handle cgroup hierarchy

    todo!("Implement match_cgroup_pattern")
}

/// Get the default configuration directory
pub fn get_config_dir() -> PathBuf {
    // TODO: Return default config directory
    // TODO: Check XDG_CONFIG_HOME or use /etc/fairshared

    PathBuf::from("/etc/fairshared")
}

/// Get the default socket path
pub fn get_socket_path() -> PathBuf {
    // TODO: Return default socket path
    // TODO: Use runtime directory or /var/run

    PathBuf::from("/var/run/fairshared.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_size_bytes() {
        assert_eq!(parse_memory_size("1024").unwrap(), 1024);
        assert_eq!(parse_memory_size("1024B").unwrap(), 1024);
    }

    #[test]
    fn test_parse_memory_size_kilobytes() {
        assert_eq!(parse_memory_size("1K").unwrap(), 1024);
        assert_eq!(parse_memory_size("1KB").unwrap(), 1024);
        assert_eq!(parse_memory_size("2K").unwrap(), 2048);
    }

    #[test]
    fn test_parse_memory_size_megabytes() {
        assert_eq!(parse_memory_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_memory_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_memory_size("512M").unwrap(), 512 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_size_gigabytes() {
        assert_eq!(parse_memory_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("8G").unwrap(), 8 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("32GB").unwrap(), 32 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_size_terabytes() {
        assert_eq!(parse_memory_size("1T").unwrap(), 1024u64 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("1TB").unwrap(), 1024u64 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_size_decimals() {
        assert_eq!(parse_memory_size("1.5G").unwrap(), (1.5 * 1024.0 * 1024.0 * 1024.0) as u64);
        assert_eq!(parse_memory_size("0.5M").unwrap(), (0.5 * 1024.0 * 1024.0) as u64);
    }

    #[test]
    fn test_parse_memory_size_whitespace() {
        assert_eq!(parse_memory_size("  8G  ").unwrap(), 8 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("8 G").unwrap(), 8 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_size_case_insensitive() {
        assert_eq!(parse_memory_size("8g").unwrap(), 8 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("8gb").unwrap(), 8 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("512m").unwrap(), 512 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_size_invalid_format() {
        assert!(parse_memory_size("").is_err());
        assert!(parse_memory_size("invalid").is_err());
        assert!(parse_memory_size("G8").is_err());
    }

    #[test]
    fn test_parse_memory_size_invalid_unit() {
        assert!(parse_memory_size("8X").is_err());
        assert!(parse_memory_size("8PB").is_err());
    }

    #[test]
    fn test_parse_memory_size_zero() {
        assert!(parse_memory_size("0G").is_err());
        assert!(parse_memory_size("0").is_err());
    }

    #[test]
    fn test_format_memory_size_bytes() {
        assert_eq!(format_memory_size(512), "512B");
        assert_eq!(format_memory_size(1023), "1023B");
    }

    #[test]
    fn test_format_memory_size_kilobytes() {
        assert_eq!(format_memory_size(1024), "1.00K");
        assert_eq!(format_memory_size(2048), "2.00K");
    }

    #[test]
    fn test_format_memory_size_megabytes() {
        assert_eq!(format_memory_size(1024 * 1024), "1.00M");
        assert_eq!(format_memory_size(512 * 1024 * 1024), "512.00M");
    }

    #[test]
    fn test_format_memory_size_gigabytes() {
        assert_eq!(format_memory_size(1024 * 1024 * 1024), "1.00G");
        assert_eq!(format_memory_size(8 * 1024 * 1024 * 1024), "8.00G");
    }

    #[test]
    fn test_format_memory_size_terabytes() {
        assert_eq!(format_memory_size(1024u64 * 1024 * 1024 * 1024), "1.00T");
    }

    #[test]
    fn test_slice_name_validation() {
        // TODO: Add tests for slice name validation
        // This will be implemented when validate_slice_name is completed
    }

    #[test]
    fn test_cgroup_pattern_matching() {
        // TODO: Add tests for pattern matching
        // This will be implemented when match_cgroup_pattern is completed
    }
}
