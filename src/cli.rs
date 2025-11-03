use clap::{builder::RangedU64ValueParser, Parser, Subcommand};

/// Maximum number of CPUs that can be requested
pub const MAX_CPU: u32 = 1000;

/// Maximum amount of memory (in GB) that can be requested
pub const MAX_MEM: u32 = 10000;

/// Minimum number of CPUs that must be requested
pub const MIN_CPU: u32 = 1;

/// Minimum amount of memory (in GB) that must be requested
pub const MIN_MEM: u32 = 1;

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

    /// Request resources (e.g. --cpu 4 --mem 8, or --all for all available)
    Request {
        /// Number of CPUs to request (1-1000)
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64), required_unless_present = "all")]
        cpu: Option<u32>,
        /// Amount of memory in GB to request (1-10000)
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64), required_unless_present = "all")]
        mem: Option<u32>,
        /// Request all remaining available resources
        #[arg(long, conflicts_with_all = ["cpu", "mem"])]
        all: bool,
    },

    /// Release all signed-out resources back to default
    Release,

    /// Show current user's resource allocation
    Info,

    /// Admin operations - setup/uninstall global resource limits (requires root)
    Admin {
        #[command(subcommand)]
        sub: AdminSubcommands,
    },
}

#[derive(Subcommand)]
pub enum AdminSubcommands {
    /// Manage system service resource limits
    Service {
        #[command(subcommand)]
        sub: ServiceSubcommands,
    },

    /// Setup global baseline for all users (default: 1 CPU, 2G RAM, 2 CPU reserve, 4G RAM reserve)
    Setup {
        /// Default number of CPUs per user (1-1000)
        #[arg(long, default_value_t = 1, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu: u32,
        /// Default amount of memory per user in GB (1-10000)
        #[arg(long, default_value_t = 2, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem: u32,
        /// System CPU reserve (1-1000, default: 2)
        #[arg(long, default_value_t = 2, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu_reserve: u32,
        /// System memory reserve in GB (1-10000, default: 4)
        #[arg(long, default_value_t = 4, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem_reserve: u32,
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
        /// System CPU reserve (1-1000, default: 2)
        #[arg(long, default_value_t = 2, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu_reserve: u32,
        /// System memory reserve in GB (1-10000, default: 4)
        #[arg(long, default_value_t = 4, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem_reserve: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ServiceSubcommands {
    /// Request resources for a system service (e.g., docker, containerd)
    Request {
        /// Service name (docker, containerd, podman, lxc, libvirtd, qemu-kvm)
        #[arg(long)]
        name: String,
        /// Number of CPUs to allocate (1-1000)
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_CPU as u64..=MAX_CPU as u64))]
        cpu: u32,
        /// Amount of memory in GB to allocate (1-10000)
        #[arg(long, value_parser = RangedU64ValueParser::<u32>::new().range(MIN_MEM as u64..=MAX_MEM as u64))]
        mem: u32,
    },

    /// Release resources from a system service
    Release {
        /// Service name
        #[arg(long)]
        name: String,
    },

    /// Show resource allocation for a specific service
    Info {
        /// Service name
        #[arg(long)]
        name: String,
    },

    /// List all service allocations
    List,
}
