mod cli;
mod system;
mod systemd;

use clap::Parser;
use cli::{Cli, Commands, AdminSubcommands};
use system::*;
use systemd::*;

fn main() {
    let cli = Cli::parse();

    // Check for polkit requirements on commands that need elevated privileges
    match &cli.command {
        Commands::Request { .. } | Commands::Release | Commands::Admin { .. } => {
            if !systemd::check_pkexec_installed() {
                eprintln!("❌ Error: pkexec is not installed.");
                eprintln!("\nPlease install polkit:");
                eprintln!("  Debian/Ubuntu: sudo apt install policykit-1");
                eprintln!("  Fedora/RHEL:   sudo dnf install polkit");
                eprintln!("  Arch:          sudo pacman -S polkit");
                std::process::exit(1);
            }

            if !systemd::check_policy_installed() {
                eprintln!("❌ Error: fairshare polkit policy is not installed.");
                eprintln!("\nPlease install the policy file:");
                eprintln!("  sudo cp com.fairshare.policy /usr/share/polkit-1/actions/");
                eprintln!("  sudo chmod 644 /usr/share/polkit-1/actions/com.fairshare.policy");
                std::process::exit(1);
            }
        }
        _ => {}
    }

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
