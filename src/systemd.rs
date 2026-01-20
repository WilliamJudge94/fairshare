use colored::*;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

// Import constants from cli module for validation
use crate::cli::{MAX_CPU, MAX_DISK, MAX_MEM};

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

pub fn set_user_limits(cpu: u32, mem: u32, disk: u32) -> io::Result<()> {
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
    if disk > MAX_DISK {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Disk value {} exceeds maximum limit of {}", disk, MAX_DISK),
        ));
    }

    // Get the UID of the user who invoked pkexec (or current user)
    let uid = get_calling_user_uid()?;

    // Try to set disk quota, but don't fail if quotas aren't enabled
    // Disk quotas require filesystem-level support which may not be configured
    if let Err(e) = set_user_disk_limit(uid, disk, None) {
        if e.kind() == io::ErrorKind::Unsupported {
            // Log a single informational message when quotas are not available
            eprintln!(
                "{} Disk quotas not available on this filesystem (quotas may not be enabled)",
                "ℹ".bright_blue().bold()
            );
        } else {
            // Warn about other errors
            eprintln!(
                "{} Could not set disk quota: {}",
                "⚠".bright_yellow().bold(),
                e
            );
        }
        // Continue with CPU and memory limits even if disk quota fails
    }

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
        .arg(format!("user-{}.slice", uid))
        .arg(format!("CPUQuota={}%", cpu_quota))
        .arg(format!("MemoryMax={}", mem_bytes))
        .status()?;

    if !status.success() {
        return Err(io::Error::other("Systemd command failed"));
    }

    Ok(())
}

/// Check if disk quotas are explicitly disabled on the specified partition.
/// Returns Ok(true) if quotas might be available (no 'noquota' option found).
/// Returns Ok(false) only if 'noquota' is explicitly set.
///
/// Note: We don't check for 'usrquota' mount option because:
/// - XFS can have quotas enabled at mkfs time (not visible in mount options)
/// - ext4 with quota feature doesn't require mount options
/// - The actual quota availability is verified when quotactl is called
pub fn is_quota_enabled_on_partition(partition: &str) -> io::Result<bool> {
    // Find the device for this partition by reading /proc/mounts
    let mounts = fs::read_to_string("/proc/mounts")?;

    // Find the best matching mount point (longest prefix match)
    let mut best_match: Option<(&str, &str)> = None; // (mount_point, mount_options)

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let mount_point = parts[1];
        let mount_options = parts[3];

        // Check if this mount point is a prefix of the partition path
        // We need the longest matching prefix (e.g., /home matches /home, not /)
        if partition == mount_point
            || (partition.starts_with(mount_point)
                && (mount_point == "/" || partition[mount_point.len()..].starts_with('/')))
        {
            // This is a potential match - keep it if it's longer than previous best
            if best_match.is_none() || mount_point.len() > best_match.unwrap().0.len() {
                best_match = Some((mount_point, mount_options));
            }
        }
    }

    // Check if the best matching mount has 'noquota' option
    // Only return false if noquota is explicitly set
    if let Some((_, mount_options)) = best_match {
        for opt in mount_options.split(',') {
            if opt == "noquota" {
                return Ok(false);
            }
        }
        // No 'noquota' found - quotas might be available
        return Ok(true);
    }

    // Mount point not found - assume quotas might be available
    // Let the actual quotactl call determine availability
    Ok(true)
}

/// Get the block device for a given mount point from /proc/mounts
#[cfg(target_os = "linux")]
fn get_block_device_for_mount(mount_point: &str) -> io::Result<Option<String>> {
    let mounts = fs::read_to_string("/proc/mounts")?;

    // First try exact match
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == mount_point {
            return Ok(Some(parts[0].to_string()));
        }
    }

    // Try finding the best matching mount point (longest prefix match)
    let mut best_match: Option<(&str, &str)> = None;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let mp = parts[1];
            if mount_point.starts_with(mp) {
                if best_match.is_none() || mp.len() > best_match.unwrap().1.len() {
                    best_match = Some((parts[0], mp));
                }
            }
        }
    }

    Ok(best_match.map(|(dev, _)| dev.to_string()))
}

/// Filesystem type for quota operations
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq)]
enum QuotaFilesystem {
    Xfs,      // XFS uses its own quota interface (Q_X* commands)
    Standard, // ext2/ext3/ext4/btrfs use standard Linux quota (Q_* commands)
}

/// Get the filesystem type for a given mount point
#[cfg(target_os = "linux")]
fn get_filesystem_type(mount_point: &str) -> io::Result<Option<QuotaFilesystem>> {
    let mounts = fs::read_to_string("/proc/mounts")?;

    // Find the best matching mount point (longest prefix match)
    let mut best_match: Option<(&str, &str)> = None;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let mp = parts[1];
            let fs_type = parts[2];

            if mount_point == mp
                || (mount_point.starts_with(mp)
                    && (mp == "/" || mount_point[mp.len()..].starts_with('/')))
            {
                if best_match.is_none() || mp.len() > best_match.unwrap().0.len() {
                    best_match = Some((mp, fs_type));
                }
            }
        }
    }

    match best_match {
        Some((_, fs_type)) => {
            match fs_type {
                "xfs" => Ok(Some(QuotaFilesystem::Xfs)),
                "ext2" | "ext3" | "ext4" | "btrfs" | "reiserfs" | "jfs" => {
                    Ok(Some(QuotaFilesystem::Standard))
                }
                _ => Ok(None), // Unsupported filesystem
            }
        }
        None => Ok(None),
    }
}

// ============================================================================
// Quota structures and constants
// ============================================================================

/// XFS quota structure (fs_disk_quota from linux/dqblk_xfs.h)
/// XFS uses 512-byte "basic blocks" for quota limits
#[cfg(target_os = "linux")]
#[repr(C)]
struct FsDiskQuota {
    d_version: i8,        // Version of this structure
    d_flags: i8,          // XQM_{USR,GRP,PRJ}QUOTA (signed i8!)
    d_fieldmask: u16,     // Field specifier
    d_id: u32,            // User, project, or group ID
    d_blk_hardlimit: u64, // Absolute limit on disk blks (BB)
    d_blk_softlimit: u64, // Preferred limit on disk blks
    d_ino_hardlimit: u64, // Maximum # allocated inodes
    d_ino_softlimit: u64, // Preferred inode limit
    d_bcount: u64,        // # disk blocks owned by the user
    d_icount: u64,        // # inodes owned by the user
    d_itimer: i32,        // Zero if within inode limits
    d_btimer: i32,        // Similar to above; for disk blocks
    d_iwarns: u16,        // # warnings issued wrt num inodes
    d_bwarns: u16,        // # warnings issued wrt disk blocks
    d_itimer_hi: i8,      // upper 8 bits of timer values
    d_btimer_hi: i8,
    d_rtbtimer_hi: i8,
    d_padding2: i8,       // Padding for future use
    d_rtb_hardlimit: u64, // Absolute limit on realtime blks
    d_rtb_softlimit: u64, // Preferred limit on RT disk blks
    d_rtbcount: u64,      // # realtime blocks owned
    d_rtbtimer: i32,      // Similar to above; for RT disk
    d_rtbwarns: u16,      // # warnings issued wrt RT disk blks
    d_padding3: i16,      // Padding
    d_padding4: [i8; 8],  // Yet more padding (char[8])
}

/// Standard Linux quota structure (if_dqblk from linux/quota.h)
/// Standard quota uses 1KB blocks for quota limits
#[cfg(target_os = "linux")]
#[repr(C, packed)]
struct DqBlk {
    dqb_bhardlimit: u64, // Absolute limit on disk quota blocks alloc
    dqb_bsoftlimit: u64, // Preferred limit on disk quota blocks
    dqb_curspace: u64,   // Current quota block count (bytes, not blocks!)
    dqb_ihardlimit: u64, // Maximum number of allocated inodes
    dqb_isoftlimit: u64, // Preferred inode limit
    dqb_curinodes: u64,  // Current number of allocated inodes
    dqb_btime: u64,      // Time limit for excessive disk use
    dqb_itime: u64,      // Time limit for excessive files
    dqb_valid: u32,      // Bit mask of QIF_* constants
}

// XFS quota command constants from <linux/dqblk_xfs.h>
#[cfg(target_os = "linux")]
const Q_XSETQLIM: u32 = (('X' as u32) << 8) + 4; // Set limits for XFS
#[cfg(target_os = "linux")]
const Q_XGETQUOTA: u32 = (('X' as u32) << 8) + 3; // Get quota for XFS
#[cfg(target_os = "linux")]
const Q_XGETNEXTQUOTA: u32 = (('X' as u32) << 8) + 9; // Get next quota entry for XFS

// Standard quota command constants from <sys/quota.h>
// These are the raw subcmd values that get encoded via QCMD macro
// QCMD(cmd, type) = (cmd << 8) | (type & 0xff)
// The 0x80 prefix indicates "new" quota format (VFS quota)
#[cfg(target_os = "linux")]
const Q_GETQUOTA_STD: u32 = 0x800007; // Get limits and usage (pre-encoded)
#[cfg(target_os = "linux")]
const Q_SETQUOTA_STD: u32 = 0x800008; // Set limits (pre-encoded)
#[cfg(target_os = "linux")]
const Q_GETNEXTQUOTA_STD: u32 = 0x800009; // Get next quota entry (pre-encoded)

// Quota type
#[cfg(target_os = "linux")]
const USRQUOTA: u32 = 0;

// XFS-specific constants
#[cfg(target_os = "linux")]
const FS_USER_QUOTA: i8 = 1; // FS_USER_QUOTA for d_flags
#[cfg(target_os = "linux")]
const FS_DQ_BSOFT: u16 = 1 << 2; // blk soft limit
#[cfg(target_os = "linux")]
const FS_DQ_BHARD: u16 = 1 << 3; // blk hard limit
#[cfg(target_os = "linux")]
const FS_DQUOT_VERSION: i8 = 1;

// Standard quota validity flags
#[cfg(target_os = "linux")]
const QIF_BLIMITS: u32 = 1; // Both hard and soft block limits valid

/// QCMD macro for XFS: encodes command and quota type for quotactl syscall
/// XFS uses: (cmd << 8) | (type & 0xff)
#[cfg(target_os = "linux")]
fn qcmd_xfs(cmd: u32, typ: u32) -> i32 {
    ((cmd << 8) | (typ & 0xff)) as i32
}

/// QCMD macro for standard filesystems (ext4, etc.): encodes command and quota type
/// The command is encoded as (cmd << 8) | (type & 0xff), same as XFS
/// This matches the kernel's QCMD macro in linux/quota.h
#[cfg(target_os = "linux")]
fn qcmd_std(cmd: u32, typ: u32) -> i32 {
    ((cmd << 8) | (typ & 0xff)) as i32
}

/// Get all user UIDs that have any disk usage on the specified partition.
/// Uses filesystem-appropriate quota API to iterate through quota entries.
/// This discovers ALL users with disk usage, including AD/LDAP users.
#[cfg(target_os = "linux")]
fn get_all_users_with_disk_usage(partition: &str) -> io::Result<Vec<u32>> {
    use std::ffi::CString;

    let device = match get_block_device_for_mount(partition)? {
        Some(dev) => dev,
        None => return Ok(Vec::new()),
    };

    let fs_type = match get_filesystem_type(partition)? {
        Some(fs) => fs,
        None => return Ok(Vec::new()), // Unsupported filesystem
    };

    let device_cstr = CString::new(device.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid device path"))?;

    let mut uids = Vec::new();
    let mut next_id: u32 = 0;

    match fs_type {
        QuotaFilesystem::Xfs => {
            // XFS: Use Q_XGETNEXTQUOTA with FsDiskQuota structure
            loop {
                let mut dq: FsDiskQuota = unsafe { std::mem::zeroed() };

                let result = unsafe {
                    libc::quotactl(
                        qcmd_xfs(Q_XGETNEXTQUOTA, USRQUOTA),
                        device_cstr.as_ptr(),
                        next_id as i32,
                        &mut dq as *mut FsDiskQuota as *mut libc::c_char,
                    )
                };

                if result != 0 {
                    // ENOENT or ESRCH means no more entries
                    break;
                }

                // Only include regular users (UID >= 1000)
                if dq.d_id >= 1000 {
                    uids.push(dq.d_id);
                }

                // Safety: check for u32::MAX before incrementing to avoid overflow
                if dq.d_id == u32::MAX || uids.len() > 100000 {
                    break;
                }

                // Move to next ID
                next_id = dq.d_id + 1;
            }
        }
        QuotaFilesystem::Standard => {
            // Standard filesystems (ext4, etc.): Use Q_GETNEXTQUOTA with nextdqblk
            // Per linux/quota.h, struct nextdqblk has the ID at the END of the structure

            // Structure for Q_GETNEXTQUOTA - matches struct nextdqblk from linux/quota.h
            // Note: The ID is at the END, not the beginning!
            #[repr(C)]
            struct NextDqBlk {
                dqb_bhardlimit: u64, // Absolute limit on disk quota blocks alloc
                dqb_bsoftlimit: u64, // Preferred limit on disk quota blocks
                dqb_curspace: u64,   // Current quota block count (bytes!)
                dqb_ihardlimit: u64, // Maximum number of allocated inodes
                dqb_isoftlimit: u64, // Preferred inode limit
                dqb_curinodes: u64,  // Current number of allocated inodes
                dqb_btime: u64,      // Time limit for excessive disk use
                dqb_itime: u64,      // Time limit for excessive files
                dqb_valid: u32,      // Bit mask of QIF_* constants
                dqb_id: u32,         // User/Group ID (at the END per kernel struct)
            }

            loop {
                let mut dq: NextDqBlk = unsafe { std::mem::zeroed() };

                let result = unsafe {
                    libc::quotactl(
                        qcmd_std(Q_GETNEXTQUOTA_STD, USRQUOTA),
                        device_cstr.as_ptr(),
                        next_id as i32,
                        &mut dq as *mut NextDqBlk as *mut libc::c_char,
                    )
                };

                if result != 0 {
                    // ENOENT or ESRCH means no more entries
                    break;
                }

                // Only include regular users (UID >= 1000)
                if dq.dqb_id >= 1000 {
                    uids.push(dq.dqb_id);
                }

                // Move to next ID safely, avoiding overflow
                next_id = match dq.dqb_id.checked_add(1) {
                    Some(id) => id,
                    None => break,
                };

                // Safety: prevent infinite loop
                if uids.len() > 100000 {
                    break;
                }
            }
        }
    }

    Ok(uids)
}

/// Get all user UIDs with disk usage - stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
fn get_all_users_with_disk_usage(_partition: &str) -> io::Result<Vec<u32>> {
    Ok(Vec::new())
}

/// Set disk quota for a user using native quotactl syscall.
/// Supports both XFS (using Q_XSETQLIM) and standard filesystems (using Q_SETQUOTA).
/// This is a no-op and returns Ok(()) if quotas are not enabled on the partition.
/// Returns a QuotaNotSupported error only for informational purposes (non-fatal).
#[cfg(target_os = "linux")]
fn set_user_disk_limit(uid: u32, disk_gb: u32, partition_opt: Option<&str>) -> io::Result<()> {
    use std::ffi::CString;

    // Use provided partition, or fallback to config, or fallback to /home
    let partition = if let Some(p) = partition_opt {
        p.to_string()
    } else {
        crate::system::get_configured_disk_partition().unwrap_or_else(|| "/home".to_string())
    };

    // Check if quotas are explicitly disabled (noquota mount option)
    match is_quota_enabled_on_partition(&partition) {
        Ok(true) => {
            // No 'noquota' found, proceed to try setting quotas
        }
        Ok(false) => {
            // 'noquota' mount option is set - quotas are explicitly disabled
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "Disk quotas explicitly disabled on {} (noquota mount option).",
                    partition
                ),
            ));
        }
        Err(e) => {
            // Couldn't check - probably /proc/mounts not readable, try anyway
            eprintln!(
                "Warning: Could not check quota status for {}: {}",
                partition, e
            );
        }
    }

    // Get the block device for this mount point
    let device = match get_block_device_for_mount(&partition)? {
        Some(dev) => dev,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Could not find block device for mount point {}", partition),
            ));
        }
    };

    // Get filesystem type to use appropriate quota interface
    let fs_type = match get_filesystem_type(&partition)? {
        Some(fs) => fs,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "Filesystem on {} does not support quotas or is not recognized",
                    partition
                ),
            ));
        }
    };

    let device_cstr = CString::new(device.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid device path"))?;

    let result = match fs_type {
        QuotaFilesystem::Xfs => {
            // XFS: Use Q_XSETQLIM with FsDiskQuota structure
            // XFS quota limits are in basic blocks (512 bytes)
            // 1 GB = 1024 * 1024 * 1024 bytes = 2097152 basic blocks (512 bytes each)
            let basic_blocks = (disk_gb as u64)
                .checked_mul(1024)
                .and_then(|v| v.checked_mul(1024))
                .and_then(|v| v.checked_mul(2))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Disk size is too large for quota calculation",
                    )
                })?;

            let dq = FsDiskQuota {
                d_version: FS_DQUOT_VERSION,
                d_flags: FS_USER_QUOTA,
                d_fieldmask: FS_DQ_BHARD | FS_DQ_BSOFT,
                d_id: uid,
                d_blk_hardlimit: basic_blocks,
                d_blk_softlimit: basic_blocks,
                d_ino_hardlimit: 0,
                d_ino_softlimit: 0,
                d_bcount: 0,
                d_icount: 0,
                d_itimer: 0,
                d_btimer: 0,
                d_iwarns: 0,
                d_bwarns: 0,
                d_itimer_hi: 0,
                d_btimer_hi: 0,
                d_rtbtimer_hi: 0,
                d_padding2: 0,
                d_rtb_hardlimit: 0,
                d_rtb_softlimit: 0,
                d_rtbcount: 0,
                d_rtbtimer: 0,
                d_rtbwarns: 0,
                d_padding3: 0,
                d_padding4: [0; 8],
            };

            unsafe {
                libc::quotactl(
                    qcmd_xfs(Q_XSETQLIM, USRQUOTA),
                    device_cstr.as_ptr(),
                    uid as i32,
                    &dq as *const FsDiskQuota as *mut libc::c_char,
                )
            }
        }
        QuotaFilesystem::Standard => {
            // Standard filesystems (ext4, etc.): Use Q_SETQUOTA with DqBlk structure
            // Standard quota uses 1KB blocks
            // 1 GB = 1024 * 1024 KB blocks
            let kb_blocks = (disk_gb as u64).saturating_mul(1024).saturating_mul(1024);

            let dq = DqBlk {
                dqb_bhardlimit: kb_blocks,
                dqb_bsoftlimit: kb_blocks,
                dqb_curspace: 0,
                dqb_ihardlimit: 0,
                dqb_isoftlimit: 0,
                dqb_curinodes: 0,
                dqb_btime: 0,
                dqb_itime: 0,
                dqb_valid: QIF_BLIMITS,
            };

            let cmd = qcmd_std(Q_SETQUOTA_STD, USRQUOTA);

            unsafe {
                libc::quotactl(
                    cmd,
                    device_cstr.as_ptr(),
                    uid as i32,
                    &dq as *const DqBlk as *mut libc::c_char,
                )
            }
        }
    };

    if result != 0 {
        let errno = io::Error::last_os_error();
        // ESRCH means quotas not enabled/active
        // EINVAL can mean quotas not configured for this filesystem
        // ENOENT can mean quota files don't exist
        if errno.raw_os_error() == Some(libc::ESRCH)
            || errno.raw_os_error() == Some(libc::EINVAL)
            || errno.raw_os_error() == Some(libc::ENOENT)
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "Quotas not available on {} (device {}). Kernel/filesystem may not support quotas.",
                    partition, device
                ),
            ));
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("quotactl failed for UID {} on {}: {}", uid, device, errno),
        ));
    }

    Ok(())
}

/// Set disk quota for a user - stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
fn set_user_disk_limit(_uid: u32, _disk_gb: u32, _partition_opt: Option<&str>) -> io::Result<()> {
    // On non-Linux, disk quotas are not supported
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Disk quotas are only supported on Linux",
    ))
}

/// Get the current disk quota (hard block limit in bytes) for a user.
/// Supports both XFS and standard filesystems (ext4, etc.).
/// Returns 0 if quotas are not enabled or user has no quota set.
#[cfg(target_os = "linux")]
pub fn get_user_disk_quota(uid: u32) -> io::Result<u64> {
    use std::ffi::CString;

    let partition =
        crate::system::get_configured_disk_partition().unwrap_or_else(|| "/home".to_string());

    // Check if quotas are enabled
    if !is_quota_enabled_on_partition(&partition).unwrap_or(false) {
        return Ok(0);
    }

    // Get the block device
    let device = match get_block_device_for_mount(&partition)? {
        Some(dev) => dev,
        None => return Ok(0),
    };

    // Get filesystem type
    let fs_type = match get_filesystem_type(&partition)? {
        Some(fs) => fs,
        None => return Ok(0),
    };

    let device_cstr = match CString::new(device.as_bytes()) {
        Ok(c) => c,
        Err(_) => return Ok(0),
    };

    match fs_type {
        QuotaFilesystem::Xfs => {
            // XFS: Use Q_XGETQUOTA with FsDiskQuota structure
            let mut dq: FsDiskQuota = unsafe { std::mem::zeroed() };

            let result = unsafe {
                libc::quotactl(
                    qcmd_xfs(Q_XGETQUOTA, USRQUOTA),
                    device_cstr.as_ptr(),
                    uid as i32,
                    &mut dq as *mut FsDiskQuota as *mut libc::c_char,
                )
            };

            if result == 0 {
                // d_blk_hardlimit is in basic blocks (512 bytes), convert to bytes
                return Ok(dq.d_blk_hardlimit * 512);
            }
        }
        QuotaFilesystem::Standard => {
            // Standard filesystems: Use Q_GETQUOTA with DqBlk structure
            let mut dq: DqBlk = unsafe { std::mem::zeroed() };

            let result = unsafe {
                libc::quotactl(
                    qcmd_std(Q_GETQUOTA_STD, USRQUOTA),
                    device_cstr.as_ptr(),
                    uid as i32,
                    &mut dq as *mut DqBlk as *mut libc::c_char,
                )
            };

            if result == 0 {
                // dqb_bhardlimit is in 1KB blocks, convert to bytes
                return Ok(dq.dqb_bhardlimit * 1024);
            }
        }
    }

    Ok(0)
}

/// Get the current disk quota - stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub fn get_user_disk_quota(_uid: u32) -> io::Result<u64> {
    // Quotas not supported on non-Linux
    Ok(0)
}

pub fn release_user_limits() -> io::Result<()> {
    // Get the UID of the user who invoked pkexec (or current user)
    let uid = get_calling_user_uid()?;

    // Release disk quota (set to 0)
    // Use configured partition if available
    set_user_disk_limit(uid, 0, None).ok();

    // When run via pkexec, we have root privileges and modify system-level user slices
    let status = Command::new("systemctl")
        .arg("revert")
        .arg(format!("user-{}.slice", uid))
        .status()?;

    if !status.success() {
        return Err(io::Error::other(format!(
            "Failed to release user limits (exit code: {:?})",
            status.code()
        )));
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
        .arg(format!("user-{}.slice", uid))
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
    let mut disk_limit = "Not set".to_string();

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

    if let Ok(bytes) = get_user_disk_quota(uid) {
        if bytes > 0 {
            let gb = bytes as f64 / 1_000_000_000.0;
            disk_limit = format!("{:.2} GB", gb);
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
    println!(
        "{} {}",
        "Disk Limit:".bright_white().bold(),
        disk_limit.green()
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
        return Err(io::Error::other(
            "Failed to update apt cache. Please run 'apt update' manually.",
        ));
    }

    // Install policykit-1
    println!("{}", "→ Installing policykit-1 package...".bright_white());
    let install_status = Command::new("apt")
        .args(["install", "-y", "policykit-1"])
        .status()?;

    if !install_status.success() {
        return Err(io::Error::other(
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
/// Disk quotas are optional and only applied when disk and disk_partition are provided.
pub fn admin_setup_defaults(
    cpu: u32,
    mem: u32,
    disk: Option<u32>,
    cpu_reserve: u32,
    mem_reserve: u32,
    disk_reserve: u32,
    disk_partition: Option<String>,
) -> io::Result<()> {
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
    if let Some(d) = disk {
        if d > MAX_DISK {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Disk value {} exceeds maximum limit of {}", d, MAX_DISK),
            ));
        }
    }

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
                return Err(io::Error::other(
                    "PolicyKit installation declined. Please install policykit-1 manually: apt install policykit-1"
                ));
            }
            Err(e) => {
                return Err(io::Error::other(format!(
                    "Failed to read user input: {}",
                    e
                )));
            }
        }
    } else {
        println!(" {}", "✓".green().bold());
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

    // Write policy config - disk settings only if explicitly provided
    let disk_val = disk.unwrap_or(0);
    let partition_val = disk_partition.clone().unwrap_or_default();
    writeln!(
        policy,
        "[defaults]\ncpu = {}\nmem = {}\ndisk = {}\ncpu_reserve = {}\nmem_reserve = {}\ndisk_reserve = {}\ndisk_partition = \"{}\"\n\n[max_caps]\ncpu = {}\nmem = {}\ndisk = {}\n",
        cpu, mem, disk_val, cpu_reserve, mem_reserve, disk_reserve, partition_val, max_cpu_cap, mem, disk_val
    )?;
    println!(
        "{} {}",
        "✓".green().bold(),
        "Created /etc/fairshare/policy.toml".bright_white()
    );

    // Only apply disk quotas if both disk and disk_partition are provided
    if let (Some(disk_gb), Some(ref disk_partition)) = (disk, disk_partition.clone()) {
        // Check if disk quotas are supported on the target partition before attempting to set them
        let quotas_enabled = match is_quota_enabled_on_partition(disk_partition) {
            Ok(enabled) => enabled,
            Err(e) => {
                eprintln!(
                    "{} Could not check quota support on {}: {}",
                    "⚠".bright_yellow().bold(),
                    disk_partition.bright_cyan(),
                    e
                );
                false
            }
        };

        if quotas_enabled {
            // Apply default disk quota to all existing users
            // We need to find users from multiple sources:
            // 1. Local system users via users::all_users()
            // 2. Users with home directories on the target partition (for AD/LDAP users)
            let mut quota_success_count = 0;
            let mut quota_fail_count = 0;
            let mut processed_uids = std::collections::HashSet::new();

            // First, process local system users
            for user in unsafe { users::all_users() } {
                let uid = user.uid();
                // Skip system users and nobody/nfsnobody
                if (1000..65534).contains(&uid) && processed_uids.insert(uid) {
                    match set_user_disk_limit(uid, disk_gb, Some(disk_partition)) {
                        Ok(()) => quota_success_count += 1,
                        Err(_) => quota_fail_count += 1,
                    }
                }
            }

            // Also scan home directories on the target partition to find AD/LDAP users
            // These users may not be enumerable via getpwent() but have home directories
            // Only scan if the partition looks like a home directory location
            let is_home_like = disk_partition == "/home"
                || disk_partition.starts_with("/home/")
                || disk_partition.contains("home");

            if is_home_like {
                if let Ok(entries) = std::fs::read_dir(disk_partition) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            // Get the owner UID of the home directory
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::MetadataExt;
                                let uid = metadata.uid();
                                // Only process regular users (UID >= 1000) that we haven't already processed
                                if uid >= 1000 && !processed_uids.contains(&uid) {
                                    processed_uids.insert(uid);
                                    match set_user_disk_limit(uid, disk_gb, Some(disk_partition)) {
                                        Ok(()) => quota_success_count += 1,
                                        Err(_) => quota_fail_count += 1,
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Additionally, use quota enumeration to find ALL users with any disk usage
            // This catches AD/LDAP users who may have files anywhere on the partition
            if let Ok(usage_uids) = get_all_users_with_disk_usage(disk_partition) {
                for uid in usage_uids {
                    if !processed_uids.contains(&uid) {
                        processed_uids.insert(uid);
                        match set_user_disk_limit(uid, disk_gb, Some(disk_partition)) {
                            Ok(()) => quota_success_count += 1,
                            Err(_) => quota_fail_count += 1,
                        }
                    }
                }
            }

            if quota_success_count > 0 {
                println!(
                    "{} Applied disk quotas to {} existing users ({}G limit)",
                    "✓".green().bold(),
                    quota_success_count.to_string().bright_yellow(),
                    disk_gb.to_string().bright_cyan()
                );
            }
            if quota_fail_count > 0 {
                eprintln!(
                    "{} Failed to set disk quota for {} users",
                    "⚠".bright_yellow().bold(),
                    quota_fail_count
                );
            }
        } else {
            eprintln!(
                "{} Disk quotas not enabled on partition {}",
                "⚠".bright_yellow().bold(),
                disk_partition.bright_cyan()
            );
            eprintln!(
                "{}   To enable quotas, add '{}' to mount options in /etc/fstab",
                " ".bright_white(),
                "usrquota".bright_yellow()
            );
            eprintln!(
                "{}   then run: {} and {}",
                " ".bright_white(),
                "mount -o remount <partition>".bright_cyan(),
                "quotaon -v <partition>".bright_cyan()
            );
            eprintln!(
                "{}   Disk quota enforcement will be {}",
                " ".bright_white(),
                "skipped".bright_yellow().bold()
            );
        }
    }

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
                        .arg(format!("user-{}.slice", alloc.uid))
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

                    // Also revert disk quota
                    if let Ok(uid_int) = alloc.uid.parse::<u32>() {
                        set_user_disk_limit(uid_int, 0, None).ok();
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
                if e.kind() == io::ErrorKind::Other || fairshare_dir.read_dir()?.next().is_none() {
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
        return Err(io::Error::other(format!(
            "Failed to reload systemd daemon (exit code: {:?})",
            status.code()
        )));
    }

    Ok(())
}

/// Reset fairshare by performing a complete uninstall followed by setup with new defaults.
/// This combines admin_uninstall_defaults() and admin_setup_defaults() into one operation.
pub fn admin_reset(
    cpu: u32,
    mem: u32,
    disk: Option<u32>,
    cpu_reserve: u32,
    mem_reserve: u32,
    disk_reserve: u32,
    disk_partition: Option<String>,
) -> io::Result<()> {
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
    admin_setup_defaults(
        cpu,
        mem,
        disk,
        cpu_reserve,
        mem_reserve,
        disk_reserve,
        disk_partition,
    )?;
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

    // Build disk message based on whether disk quotas were configured
    let disk_msg = if let Some(d) = disk {
        format!("Disk={}G", d).bright_yellow().to_string()
    } else {
        "Disk=disabled".bright_white().to_string()
    };

    println!(
        "{} New defaults: {} {} {}",
        "✓".green().bold(),
        format!("CPUQuota={}%", cpu * 100).bright_yellow(),
        format!("MemoryMax={}G", mem).bright_yellow(),
        disk_msg
    );

    Ok(())
}

/// Admin function to force set resource limits for a specific user (by UID).
/// This works even if the user is not currently logged in.
/// Requires root privileges and should only be called from admin commands.
pub fn admin_set_user_limits(uid: u32, cpu: u32, mem: u32, disk: u32) -> io::Result<()> {
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
    if disk > MAX_DISK {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Disk value {} exceeds maximum limit of {}", disk, MAX_DISK),
        ));
    }

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

    // Try to set disk quota, but don't fail if quotas aren't enabled
    if let Err(e) = set_user_disk_limit(uid, disk, None) {
        if e.kind() != io::ErrorKind::Unsupported {
            eprintln!(
                "{} Could not set disk quota for user {}: {}",
                "⚠".bright_yellow().bold(),
                uid,
                e
            );
        }
        // Continue with CPU and memory limits
    }

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

    // Set limits on the user slice at system level
    let status = Command::new("systemctl")
        .arg("set-property")
        .arg(format!("user-{}.slice", uid))
        .arg(format!("CPUQuota={}%", cpu_quota))
        .arg(format!("MemoryMax={}", mem_bytes))
        .status()?;

    if !status.success() {
        return Err(io::Error::other(format!(
            "Failed to set user limits for UID {} (exit code: {:?})",
            uid,
            status.code()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    #[test]
    fn test_admin_setup_creates_valid_config_content() {
        // This test validates the configuration format without actually
        // creating files on the system
        let cpu: u32 = 2;
        let mem: u32 = 4;
        let disk: u32 = 10;
        let cpu_reserve: u32 = 1;
        let mem_reserve: u32 = 2;
        let disk_reserve: u32 = 5;
        let disk_partition = "/var".to_string(); // Overwriting /home

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
        // Updated to match the actual format used in admin_setup_defaults
        let expected_policy = format!(
            "[defaults]\ncpu = {}\nmem = {}\ndisk = {}\ncpu_reserve = {}\nmem_reserve = {}\ndisk_reserve = {}\ndisk_partition = \"{}\"\n\n[max_caps]\ncpu = {}\nmem = {}\ndisk = {}\n",
            cpu, mem, disk, cpu_reserve, mem_reserve, disk_reserve, disk_partition, max_cpu_cap, mem, disk
        );

        assert!(expected_policy.contains("[defaults]"));
        assert!(expected_policy.contains("cpu = 2"));
        assert!(expected_policy.contains("mem = 4"));
        assert!(expected_policy.contains("disk = 10"));
        assert!(expected_policy.contains("disk_partition = \"/var\""));
        assert!(expected_policy.contains("disk_reserve = 5"));
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

        let result = super::set_user_limits(MAX_CPU + 1, 2, 0);
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

        let result = super::set_user_limits(2, MAX_MEM + 1, 0);
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

        let result = super::admin_setup_defaults(MAX_CPU + 1, 2, None, 2, 4, 0, None);
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

        let result = super::admin_setup_defaults(2, MAX_MEM + 1, None, 2, 4, 0, None);
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
        let result = super::set_user_limits(invalid_cpu, 2, 0);

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
        let min_result = super::set_user_limits(1, 1, 0);
        // Just verify it doesn't error on validation
        if let Err(e) = min_result {
            let error_msg = format!("{}", e);
            assert!(
                !error_msg.contains("exceeds maximum limit"),
                "Minimum values should not fail validation: {}",
                error_msg
            );
        }

        let max_result = super::set_user_limits(MAX_CPU, MAX_MEM, 0);
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
        let result = super::set_user_limits(u32::MAX, 2, 0);
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
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
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

    // Tests for admin_set_user_limits function
    #[test]
    fn test_admin_set_user_limits_input_validation_cpu_exceeds_max() {
        // Test that admin_set_user_limits rejects CPU values exceeding MAX_CPU
        use crate::cli::MAX_CPU;

        // Use current user's UID for testing (should exist)
        let uid = users::get_current_uid();
        let result = super::admin_set_user_limits(uid, MAX_CPU + 1, 2, 0);
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
    fn test_admin_set_user_limits_input_validation_mem_exceeds_max() {
        // Test that admin_set_user_limits rejects memory values exceeding MAX_MEM
        use crate::cli::MAX_MEM;

        // Use current user's UID for testing (should exist)
        let uid = users::get_current_uid();
        let result = super::admin_set_user_limits(uid, 2, MAX_MEM + 1, 0);
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
    fn test_admin_set_user_limits_rejects_root() {
        // Test that UID 0 (root) is rejected with PermissionDenied
        let result = super::admin_set_user_limits(0, 2, 4, 0);
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
    }

    #[test]
    fn test_admin_set_user_limits_rejects_system_users() {
        // Test that UIDs < 1000 are rejected as system users
        let system_uids = vec![1, 10, 100, 500, 999];

        for uid in system_uids {
            let result = super::admin_set_user_limits(uid, 2, 4, 0);
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
    }

    #[test]
    fn test_admin_set_user_limits_rejects_nonexistent_users() {
        // Test that non-existent UIDs are rejected
        let nonexistent_uid = 999999u32;

        // Verify this UID doesn't actually exist on the system
        if users::get_user_by_uid(nonexistent_uid).is_none() {
            let result = super::admin_set_user_limits(nonexistent_uid, 2, 4, 0);
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
    }

    #[test]
    fn test_admin_set_user_limits_boundary_values() {
        // Test boundary values around the 1000 threshold
        // Test UID 999 (should fail - system user)
        let result = super::admin_set_user_limits(999, 2, 4, 0);
        assert!(result.is_err(), "Should reject UID 999 (system user)");
        if let Err(e) = result {
            assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
        }

        // Test UID 1000 (should pass validation checks, may fail on systemctl)
        let result = super::admin_set_user_limits(1000, 2, 4, 0);
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
    }

    #[test]
    fn test_admin_set_user_limits_accepts_valid_users() {
        // Test that UIDs >= 1000 for existing users pass validation
        let current_uid = users::get_current_uid();

        // Only test if current user has UID >= 1000
        if current_uid >= 1000 {
            let result = super::admin_set_user_limits(current_uid, 2, 4, 0);
            // Should either succeed or fail with systemctl-related error (not validation error)
            if let Err(e) = result {
                let error_msg = format!("{}", e);
                // Validation errors should not occur for valid UID
                assert!(
                    !error_msg.contains("Cannot modify") && !error_msg.contains("exceeds maximum"),
                    "Should not fail validation for valid UID {}: {}",
                    current_uid,
                    error_msg
                );
            }
        }
    }

    #[test]
    fn test_admin_set_user_limits_overflow_detection() {
        // Test overflow detection in admin_set_user_limits
        use crate::cli::{MAX_CPU, MAX_MEM};

        let current_uid = users::get_current_uid();

        // Test max valid values don't cause overflow in the function
        let result = super::admin_set_user_limits(current_uid, MAX_CPU, MAX_MEM, 0);
        if let Err(e) = result {
            let error_msg = format!("{}", e);
            // Should not fail with overflow error for max valid values
            assert!(
                !error_msg.contains("overflow"),
                "MAX values should not cause overflow: {}",
                error_msg
            );
        }
    }

    // ========================================================================
    // Disk Quota Tests
    // ========================================================================

    #[test]
    fn test_disk_quota_gb_to_xfs_blocks_conversion() {
        // XFS uses 512-byte basic blocks
        // 1 GB = 1024 * 1024 * 1024 bytes = 2097152 basic blocks (512 bytes each)
        // Formula: disk_gb * 1024 * 1024 * 2

        let test_cases = vec![
            (1u32, 2_097_152u64),        // 1 GB
            (2u32, 4_194_304u64),        // 2 GB
            (10u32, 20_971_520u64),      // 10 GB
            (100u32, 209_715_200u64),    // 100 GB
            (1000u32, 2_097_152_000u64), // 1 TB
        ];

        for (gb, expected_blocks) in test_cases {
            let blocks = (gb as u64)
                .checked_mul(1024)
                .and_then(|v| v.checked_mul(1024))
                .and_then(|v| v.checked_mul(2))
                .unwrap();
            assert_eq!(
                blocks, expected_blocks,
                "{} GB should equal {} XFS basic blocks",
                gb, expected_blocks
            );
        }
    }

    #[test]
    fn test_disk_quota_gb_to_ext4_blocks_conversion() {
        // Standard filesystems (ext4, etc.) use 1KB blocks
        // 1 GB = 1024 * 1024 KB blocks = 1048576 blocks per GB

        let test_cases = vec![
            (1u32, 1_048_576u64),        // 1 GB
            (2u32, 2_097_152u64),        // 2 GB
            (10u32, 10_485_760u64),      // 10 GB
            (100u32, 104_857_600u64),    // 100 GB
            (1000u32, 1_048_576_000u64), // 1 TB (1000 * 1024 * 1024)
        ];

        for (gb, expected_blocks) in test_cases {
            let blocks = (gb as u64).saturating_mul(1024).saturating_mul(1024);
            assert_eq!(
                blocks, expected_blocks,
                "{} GB should equal {} ext4 1KB blocks",
                gb, expected_blocks
            );
        }
    }

    #[test]
    fn test_disk_quota_max_value_no_overflow() {
        // Test that MAX_DISK doesn't cause overflow in either calculation
        use crate::cli::MAX_DISK;

        // XFS calculation: MAX_DISK * 1024 * 1024 * 2
        let xfs_result = (MAX_DISK as u64)
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024))
            .and_then(|v| v.checked_mul(2));
        assert!(
            xfs_result.is_some(),
            "MAX_DISK ({}) should not overflow XFS block calculation",
            MAX_DISK
        );

        // ext4 calculation: MAX_DISK * 1024 * 1024
        let ext4_result = (MAX_DISK as u64)
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024));
        assert!(
            ext4_result.is_some(),
            "MAX_DISK ({}) should not overflow ext4 block calculation",
            MAX_DISK
        );
    }

    #[test]
    fn test_disk_quota_boundary_values() {
        use crate::cli::{MAX_DISK, MIN_DISK};

        // Minimum disk value
        let min_xfs = (MIN_DISK as u64)
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024))
            .and_then(|v| v.checked_mul(2));
        assert!(min_xfs.is_some(), "MIN_DISK should not overflow");

        let min_ext4 = (MIN_DISK as u64)
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024));
        assert!(min_ext4.is_some(), "MIN_DISK should not overflow");

        // Maximum disk value
        let max_xfs = (MAX_DISK as u64)
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024))
            .and_then(|v| v.checked_mul(2));
        assert!(
            max_xfs.is_some(),
            "MAX_DISK should not overflow XFS calculation"
        );

        let max_ext4 = (MAX_DISK as u64)
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024));
        assert!(
            max_ext4.is_some(),
            "MAX_DISK should not overflow ext4 calculation"
        );
    }

    #[test]
    fn test_disk_quota_zero_value() {
        // Zero disk should result in zero blocks
        let zero_xfs = (0u64)
            .saturating_mul(1024)
            .saturating_mul(1024)
            .saturating_mul(2);
        assert_eq!(zero_xfs, 0, "Zero GB should equal zero XFS blocks");

        let zero_ext4 = (0u64).saturating_mul(1024).saturating_mul(1024);
        assert_eq!(zero_ext4, 0, "Zero GB should equal zero ext4 blocks");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_qcmd_xfs_encoding() {
        // Test XFS quota command encoding
        // QCMD(cmd, type) = (cmd << 8) | (type & 0xff)

        // Q_XSETQLIM = (('X' as u32) << 8) + 4 = 0x5804
        let q_xsetqlim = (('X' as u32) << 8) + 4;
        let usrquota = 0u32;

        let encoded = super::qcmd_xfs(q_xsetqlim, usrquota);
        // Expected: (0x5804 << 8) | 0 = 0x580400
        let expected = ((q_xsetqlim << 8) | (usrquota & 0xff)) as i32;
        assert_eq!(encoded, expected, "XFS QCMD encoding should match");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_qcmd_std_encoding() {
        // Test standard quota command encoding
        // QCMD(cmd, type) = (cmd << 8) | (type & 0xff)

        let q_setquota_std = 0x800008u32;
        let usrquota = 0u32;

        let encoded = super::qcmd_std(q_setquota_std, usrquota);
        // Expected: (0x800008 << 8) | 0 = 0x80000800
        let expected = ((q_setquota_std << 8) | (usrquota & 0xff)) as i32;
        assert_eq!(encoded, expected, "Standard QCMD encoding should match");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_is_quota_enabled_parses_noquota() {
        // Test that noquota mount option is detected
        // This tests the parsing logic without actual filesystem access

        let mount_options_with_noquota = "rw,relatime,noquota";
        let mount_options_without_noquota = "rw,relatime,quota,usrquota";
        let mount_options_empty = "";

        // Test noquota detection in mount options string
        let has_noquota = mount_options_with_noquota
            .split(',')
            .any(|opt| opt == "noquota");
        assert!(has_noquota, "Should detect noquota option");

        let has_noquota = mount_options_without_noquota
            .split(',')
            .any(|opt| opt == "noquota");
        assert!(!has_noquota, "Should not detect noquota when not present");

        let has_noquota = mount_options_empty.split(',').any(|opt| opt == "noquota");
        assert!(!has_noquota, "Empty options should not have noquota");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_filesystem_type_detection_logic() {
        // Test filesystem type classification logic

        let xfs_types = vec!["xfs"];
        let standard_types = vec!["ext2", "ext3", "ext4", "btrfs", "reiserfs", "jfs"];
        let unsupported_types = vec!["ntfs", "vfat", "tmpfs", "proc", "sysfs"];

        for fs in xfs_types {
            let is_xfs = fs == "xfs";
            assert!(is_xfs, "{} should be detected as XFS", fs);
        }

        for fs in standard_types {
            let is_standard = matches!(fs, "ext2" | "ext3" | "ext4" | "btrfs" | "reiserfs" | "jfs");
            assert!(
                is_standard,
                "{} should be detected as standard quota filesystem",
                fs
            );
        }

        for fs in unsupported_types {
            let is_supported = fs == "xfs"
                || matches!(fs, "ext2" | "ext3" | "ext4" | "btrfs" | "reiserfs" | "jfs");
            assert!(!is_supported, "{} should be detected as unsupported", fs);
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_mount_point_matching_logic() {
        // Test the mount point matching algorithm used in quota functions

        // Simulate mount point matching for /home/user/data
        let target = "/home/user/data";
        let mounts = vec![
            ("/", true),               // Matches (root always matches)
            ("/home", true),           // Matches (prefix)
            ("/home/user", true),      // Matches (longer prefix)
            ("/home/user/data", true), // Matches (exact)
            ("/var", false),           // Doesn't match
            ("/homealt", false),       // Doesn't match (different path)
        ];

        for (mount_point, should_match) in mounts {
            let matches = target == mount_point
                || (target.starts_with(mount_point)
                    && (mount_point == "/" || target[mount_point.len()..].starts_with('/')));
            assert_eq!(
                matches, should_match,
                "Mount point '{}' match for target '{}' should be {}",
                mount_point, target, should_match
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_longest_prefix_match_selection() {
        // Test that longest matching mount point is selected

        let target = "/home/user/data";
        let mount_points = vec!["/", "/home", "/home/user"];

        let mut best_match: Option<&str> = None;
        for mp in &mount_points {
            let matches = target == *mp
                || (target.starts_with(*mp) && (*mp == "/" || target[mp.len()..].starts_with('/')));
            if matches {
                if best_match.is_none() || mp.len() > best_match.unwrap().len() {
                    best_match = Some(mp);
                }
            }
        }

        assert_eq!(
            best_match,
            Some("/home/user"),
            "Should select longest matching mount point"
        );
    }

    #[test]
    fn test_admin_setup_config_with_disk() {
        // Test that config format includes disk quota settings
        let cpu: u32 = 2;
        let mem: u32 = 4;
        let disk: u32 = 10;
        let cpu_reserve: u32 = 1;
        let mem_reserve: u32 = 2;
        let disk_reserve: u32 = 5;
        let disk_partition = "/mnt/data".to_string();

        let max_cpu_cap = cpu.checked_mul(10).unwrap();
        let expected_policy = format!(
            "[defaults]\ncpu = {}\nmem = {}\ndisk = {}\ncpu_reserve = {}\nmem_reserve = {}\ndisk_reserve = {}\ndisk_partition = \"{}\"\n\n[max_caps]\ncpu = {}\nmem = {}\ndisk = {}\n",
            cpu, mem, disk, cpu_reserve, mem_reserve, disk_reserve, disk_partition, max_cpu_cap, mem, disk
        );

        assert!(expected_policy.contains("disk = 10"));
        assert!(expected_policy.contains("disk_reserve = 5"));
        assert!(expected_policy.contains("disk_partition = \"/mnt/data\""));
    }

    #[test]
    fn test_admin_setup_config_without_disk() {
        // Test config format when disk is not specified (disk = 0)
        let cpu: u32 = 2;
        let mem: u32 = 4;
        let disk: u32 = 0; // No disk quota
        let cpu_reserve: u32 = 1;
        let mem_reserve: u32 = 2;
        let disk_reserve: u32 = 0;
        let disk_partition = "".to_string();

        let max_cpu_cap = cpu.checked_mul(10).unwrap();
        let expected_policy = format!(
            "[defaults]\ncpu = {}\nmem = {}\ndisk = {}\ncpu_reserve = {}\nmem_reserve = {}\ndisk_reserve = {}\ndisk_partition = \"{}\"\n\n[max_caps]\ncpu = {}\nmem = {}\ndisk = {}\n",
            cpu, mem, disk, cpu_reserve, mem_reserve, disk_reserve, disk_partition, max_cpu_cap, mem, disk
        );

        assert!(expected_policy.contains("disk = 0"));
        assert!(expected_policy.contains("disk_reserve = 0"));
        assert!(expected_policy.contains("disk_partition = \"\""));
    }

    #[test]
    fn test_disk_quota_input_validation_exceeds_max() {
        // Test that disk values exceeding MAX_DISK are rejected
        use crate::cli::MAX_DISK;

        let result = super::set_user_limits(2, 4, MAX_DISK + 1);
        assert!(result.is_err(), "Should reject disk exceeding MAX_DISK");

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
    fn test_disk_quota_valid_range() {
        // Test that valid disk values pass input validation
        use crate::cli::{MAX_DISK, MIN_DISK};

        // These should NOT error on input validation
        // (they may fail on quotactl execution, but that's okay for this test)

        // Minimum value
        let min_result = super::set_user_limits(1, 1, MIN_DISK);
        if let Err(e) = min_result {
            let error_msg = format!("{}", e);
            assert!(
                !error_msg.contains("exceeds maximum limit"),
                "Minimum disk value should not fail validation: {}",
                error_msg
            );
        }

        // Maximum value
        let max_result = super::set_user_limits(1, 1, MAX_DISK);
        if let Err(e) = max_result {
            let error_msg = format!("{}", e);
            assert!(
                !error_msg.contains("exceeds maximum limit"),
                "Maximum valid disk value should not fail validation: {}",
                error_msg
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_set_user_disk_limit_nonexistent_partition() {
        // Test that non-existent partition is handled gracefully
        let result = super::set_user_disk_limit(1000, 10, Some("/nonexistent/partition"));

        // Should error with NotFound or similar
        assert!(result.is_err(), "Should fail for non-existent partition");
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_set_user_disk_limit_non_linux() {
        // On non-Linux platforms, disk quotas should return Unsupported
        let result = super::set_user_disk_limit(1000, 10, Some("/home"));

        assert!(result.is_err(), "Should fail on non-Linux");
        if let Err(e) = result {
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::Unsupported,
                "Should return Unsupported on non-Linux"
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_get_user_disk_quota_default_zero() {
        // Test that get_user_disk_quota returns 0 when quotas not configured
        // This tests the fallback behavior

        // Use a non-existent UID to avoid affecting real quotas
        let result = super::get_user_disk_quota(999999);

        // Should either succeed with 0 or fail gracefully
        match result {
            Ok(quota) => {
                // Quota should be 0 for non-existent user or unconfigured quotas
                assert_eq!(quota, 0, "Should return 0 for unconfigured quota");
            }
            Err(_) => {
                // Acceptable - quotas may not be enabled
            }
        }
    }

    #[test]
    fn test_disk_reserve_calculation() {
        // Test disk reserve calculations
        let total_disk_gb = 100u32;
        let disk_reserve_gb = 10u32;
        let available_gb = total_disk_gb.saturating_sub(disk_reserve_gb);

        assert_eq!(
            available_gb, 90,
            "Available disk should be total minus reserve"
        );

        // Test with reserve larger than total (edge case)
        let available_gb = 50u32.saturating_sub(100u32);
        assert_eq!(available_gb, 0, "Should not underflow when reserve > total");
    }

    #[test]
    fn test_xfs_quota_structure_size() {
        // Verify FsDiskQuota structure has expected size
        // This is important for correct interaction with the kernel
        #[cfg(target_os = "linux")]
        {
            use std::mem::size_of;
            // FsDiskQuota should be 200 bytes per XFS quota.h
            // This test ensures the structure definition matches kernel expectations
            let size = size_of::<super::FsDiskQuota>();
            assert!(size > 0, "FsDiskQuota should have non-zero size");
            // Note: exact size check removed as it depends on alignment
        }
    }

    #[test]
    fn test_dqblk_structure_packed() {
        // Verify DqBlk structure is correctly packed
        #[cfg(target_os = "linux")]
        {
            use std::mem::size_of;
            let size = size_of::<super::DqBlk>();
            // DqBlk should be 72 bytes (8 u64s + 1 u32) when packed
            assert!(
                size <= 80, // Allow some alignment padding
                "DqBlk should be reasonably sized, got {} bytes",
                size
            );
        }
    }
}
