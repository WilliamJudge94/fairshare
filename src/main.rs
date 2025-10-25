mod cli;
mod system;
mod systemd;

use clap::Parser;
use cli::{Cli, Commands, AdminSubcommands};
use system::*;
use systemd::*;
use colored::*;

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Status => {
            let totals = get_system_totals();
            let allocations = match get_user_allocations() {
                Ok(allocs) => allocs,
                Err(e) => {
                    eprintln!("{} Failed to get user allocations: {}", "✗".red().bold(), e);
                    std::process::exit(1);
                }
            };
            print_status(&totals, &allocations);
        }

        Commands::Request { cpu, mem } => {
            let totals = get_system_totals();
            let allocations = match get_user_allocations() {
                Ok(allocs) => allocs,
                Err(e) => {
                    eprintln!("{} Failed to get user allocations: {}", "✗".red().bold(), e);
                    std::process::exit(1);
                }
            };

            if !check_request(&totals, &allocations, *cpu, &mem.to_string()) {
                eprintln!("{} {}", "✗".red().bold(), "Request exceeds available system resources.".red());
                std::process::exit(1);
            }

            if let Err(e) = set_user_limits(*cpu, *mem) {
                eprintln!("{} {}: {}", "✗".red().bold(), "Failed to set limits".red(), e);
                std::process::exit(1);
            }

            println!("{} Allocated {} and {}.",
                "✓".green().bold(),
                format!("{} CPU(s)", cpu).bright_yellow().bold(),
                format!("{}G RAM", mem).bright_yellow().bold()
            );
        }

        Commands::Release => {
            if let Err(e) = release_user_limits() {
                eprintln!("{} {}: {}", "✗".red().bold(), "Failed to release limits".red(), e);
                std::process::exit(1);
            }
            println!("{} {}",
                "✓".green().bold(),
                "Released user limits back to defaults.".green()
            );
        }

        Commands::Info => {
            if let Err(e) = show_user_info() {
                eprintln!("{} {}", "✗".red().bold(), e.to_string().red());
            }
        }

        Commands::Admin { sub } => match sub {
            AdminSubcommands::Setup { cpu, mem } => {
                if let Err(e) = admin_setup_defaults(*cpu, *mem) {
                    eprintln!("{} {}: {}", "✗".red().bold(), "Setup failed".red(), e);
                    std::process::exit(1);
                }
                println!("{} Global defaults applied: {} {}",
                    "✓".green().bold(),
                    format!("CPUQuota={}%", cpu).bright_yellow(),
                    format!("MemoryMax={}G", mem).bright_yellow()
                );
            }
            AdminSubcommands::Uninstall { force } => {
                if !force {
                    eprintln!("{} {}",
                        "⚠".bright_yellow().bold(),
                        "This will remove all fairshare admin configuration!".bright_yellow()
                    );
                    eprintln!("{} {}", "  Files to be removed:".bright_white().bold(), "");
                    eprintln!("    - /etc/systemd/system/user-.slice.d/00-defaults.conf");
                    eprintln!("    - /etc/fairshare/policy.toml");
                    eprintln!("    - /etc/fairshare/ (if empty)");
                    eprint!("\n{} {}", "Continue?".bright_white().bold(), "[y/N]: ".bright_white());
                    std::io::Write::flush(&mut std::io::stderr()).ok();

                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).ok();
                    if !input.trim().eq_ignore_ascii_case("y") && !input.trim().eq_ignore_ascii_case("yes") {
                        println!("{} {}", "✗".red().bold(), "Uninstall cancelled.".red());
                        return;
                    }
                }

                if let Err(e) = admin_uninstall_defaults() {
                    eprintln!("{} {}: {}", "✗".red().bold(), "Uninstall failed".red(), e);
                    std::process::exit(1);
                }
                println!("{} {}",
                    "✓".green().bold(),
                    "Global defaults uninstalled. System reverted to standard resource limits.".green()
                );
            }
        },
    }
}
