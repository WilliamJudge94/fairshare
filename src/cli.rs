use clap::{Parser, Subcommand};
use clap::builder::RangedU64ValueParser;

/// Minimum number of CPUs that must be requested
pub const MIN_CPU: u32 = 1;
/// Maximum number of CPUs that can be requested
pub const MAX_CPU: u32 = 1000;

/// Minimum amount of memory (in GB) that can be requested
pub const MIN_MEM: u32 = 1;
/// Maximum amount of memory (in GB) that can be requested
pub const MAX_MEM: u32 = 10000;

/// Minimum amount of disk (in GB) that can be requested
pub const MIN_DISK: u32 = 1;
/// Maximum amount of disk (in GB) that can be requested
pub const MAX_DISK: u32 = 10000;

#[derive(Parser)]
#[command(
    name = "fairshare",
    version,
    about = "Systemd-based resource manager for multi-user Linux systems"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show system totals and all user allocations
    Status,

    /// Request resources (e.g. --cpu 4 --mem 8 --disk 20, or --all for all available)
    Request {
        /// Number of CPUs to request (1-1000)
        #[arg(long, required_unless_present = "all", value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu: Option<u32>,

        /// Amount of memory in GB to request (1-10000)
        #[arg(long, required_unless_present = "all", value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem: Option<u32>,

        /// Amount of Disk in GB to request (1-10000)
        #[arg(long, required_unless_present = "all", value_parser = RangedU64ValueParser::<u32>::new().range(MIN_DISK as u64..=MAX_DISK as u64))]
        disk: Option<u32>,

        /// Request all remaining available resources
        #[arg(long, conflicts_with_all = ["cpu", "mem", "disk"])]
        all: bool,
    },

    /// Release all signed-out resources back to default
    Release,

    /// Show current user's resource usage info
    Info,

    /// Admin operations - setup/uninstall global resource limits (requires root)
    Admin {
        #[command(subcommand)]
        sub: AdminSubcommands,
    },
}

#[derive(Subcommand)]
pub enum AdminSubcommands {
    /// Setup global baseline
    Setup {
        /// Default number of CPUs per user (1-1000)
        #[arg(long, default_value_t = 1, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu: u32,

        /// Default amount of memory per user in GB (1-10000)
        #[arg(long, default_value_t = 2, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem: u32,

        /// Default amount of disk per user in GB (1-10000). Only applied when --disk-partition is also set.
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_DISK as u64..=MAX_DISK as u64))]
        disk: Option<u32>,

        /// System CPU reserve (1-1000, default: 2)
        #[arg(long, default_value_t = 2, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu_reserve: u32,

        /// System memory reserve in GB (1-10000, default: 4)
        #[arg(long, default_value_t = 4, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem_reserve: u32,

        /// System disk reserve in GB (1-10000, default: 4). Only used when --disk is set.
        #[arg(long, default_value_t = 4, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_DISK as u64..=MAX_DISK as u64))]
        disk_reserve: u32,

        /// System disk partition to monitor (e.g., /home, /data). Required for disk quotas.
        #[arg(long)]
        disk_partition: Option<String>,
    },

    /// Uninstall global defaults and remove all fairshare admin configuration
    Uninstall {
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Reset fairshare (uninstall then setup with new defaults)
    Reset {
        /// Default number of CPUs per user (1-1000)
        #[arg(long, default_value_t = 1, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu: u32,

        /// Default amount of memory per user in GB (1-10000)
        #[arg(long, default_value_t = 2, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem: u32,

        /// Default amount of disk per user in GB (1-10000). Only applied when --disk-partition is also set.
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_DISK as u64..=MAX_DISK as u64))]
        disk: Option<u32>,

        /// System CPU reserve (1-1000, default: 2)
        #[arg(long, default_value_t = 2, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu_reserve: u32,

        /// System memory reserve in GB (1-10000, default: 4)
        #[arg(long, default_value_t = 4, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem_reserve: u32,

        /// System disk reserve in GB (1-10000, default: 4). Only used when --disk is set.
        #[arg(long, default_value_t = 4, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_DISK as u64..=MAX_DISK as u64))]
        disk_reserve: u32,

        /// System disk partition to monitor (e.g., /home, /data). Required for disk quotas.
        #[arg(long)]
        disk_partition: Option<String>,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Force set resources for a specific user (even if signed out)
    SetUser {
        /// Username or UID of the target user
        #[arg(long)]
        user: String,

        /// Number of CPUs to allocate (1-1000)
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu: u32,

        /// Amount of memory in GB to allocate (1-10000)
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem: u32,

        /// Amount of Disk in GB to allocate (1-10000)
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_DISK as u64..=MAX_DISK as u64))]
        disk: u32,

        /// Skip resource availability warning prompt
        #[arg(long)]
        force: bool,
    },
}
