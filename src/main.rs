mod cli;
mod system;
mod systemd;

use clap::Parser;
use cli::{Cli, Commands, AdminSubcommands};
use system::*;
use systemd::*;

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Status => {
            let totals = get_system_totals();
            let allocations = get_user_allocations();
            print_status(&totals, &allocations);
        }

        Commands::Request { cpu, mem } => {
            let totals = get_system_totals();
            let allocations = get_user_allocations();

            if !check_request(&totals, &allocations, *cpu, mem) {
                eprintln!("❌ Request exceeds available system resources.");
                std::process::exit(1);
            }

            if let Err(e) = set_user_limits(*cpu, mem) {
                eprintln!("❌ Failed to set limits: {}", e);
                std::process::exit(1);
            }

            println!("✅ Allocated {} CPU(s) and {} RAM.", cpu, mem);
        }

        Commands::Release => {
            if let Err(e) = release_user_limits() {
                eprintln!("❌ Failed to release limits: {}", e);
                std::process::exit(1);
            }
            println!("✅ Released user limits back to defaults.");
        }

        Commands::Info => {
            if let Err(e) = show_user_info() {
                eprintln!("❌ {}", e);
            }
        }

        Commands::Admin { sub } => match sub {
            AdminSubcommands::Setup { cpu, mem } => {
                if let Err(e) = admin_setup_defaults(*cpu, mem) {
                    eprintln!("❌ Setup failed: {}", e);
                    std::process::exit(1);
                }
                println!(
                    "✅ Global defaults applied: CPUQuota={}%, MemoryMax={}",
                    cpu, mem
                );
            }
        },
    }
}
