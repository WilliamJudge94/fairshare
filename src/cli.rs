use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "fairshare", version, about = "Systemd-based resource manager for multi-user Linux systems")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show system totals and all user allocations
    Status,

    /// Request resources (e.g. --cpu 4 --mem 8)
    Request {
        #[arg(long)]
        cpu: u32,
        #[arg(long)]
        mem: u32,
    },

    /// Release all signed-out resources back to default
    Release,

    /// Show current user's resource allocation
    Info,

    /// Admin operations (requires root)
    Admin {
        #[command(subcommand)]
        sub: AdminSubcommands,
    },
}

#[derive(Subcommand)]
pub enum AdminSubcommands {
    /// Setup global baseline for all users (default: 1 CPU, 2G RAM)
    Setup {
        #[arg(long, default_value_t = 1)]
        cpu: u32,
        #[arg(long, default_value_t = 2)]
        mem: u32,
    },
}
