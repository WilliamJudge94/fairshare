use anyhow::{Result, Context};
use clap::Parser;
use colored::*;
use fairshare::cli::{Cli, Command};
use fairshare::ipc::{IpcClient, Request, Response};
use std::process;

#[tokio::main]
async fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Execute the command and handle errors
    match run_command(&cli).await {
        Ok(_) => {
            process::exit(0);
        }
        Err(e) => {
            eprintln!("{} {}", "Error:".red().bold(), e);
            process::exit(1);
        }
    }
}

async fn run_command(cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::Request { cpu, mem } => {
            handle_request(&cli.socket, *cpu, mem).await
        }
        Command::Release => {
            handle_release(&cli.socket).await
        }
        Command::Status => {
            handle_status(&cli.socket).await
        }
        Command::Exec { command } => {
            handle_exec(&cli.socket, command).await
        }
    }
}

async fn handle_request(socket_path: &str, cpu: u32, mem: &str) -> Result<()> {
    let client = IpcClient::new(socket_path);

    let request = Request::RequestResources {
        cpu,
        mem: mem.to_string(),
    };

    let response = client.send_request(request).await
        .context(format_daemon_error("Failed to communicate with daemon"))?;

    match response {
        Response::Success { message } => {
            println!("{} {}", "✓".green().bold(), "Request approved".bold());
            println!("{}", message);

            // Display formatted allocation details
            let uid = get_current_uid();
            println!("\n{}", "Slice Details:".bold());
            println!("  Slice name: {}", format!("fairshare-{}.slice", uid).cyan());
            println!("  CPU quota:  {}% ({} cores)", cpu * 100, cpu);
            println!("  Memory max: {}", mem);

            Ok(())
        }
        Response::Error { error } => {
            Err(anyhow::anyhow!("{}", error))
        }
        _ => {
            Err(anyhow::anyhow!("Unexpected response from daemon"))
        }
    }
}

async fn handle_release(socket_path: &str) -> Result<()> {
    let client = IpcClient::new(socket_path);

    let request = Request::Release;

    let response = client.send_request(request).await
        .context(format_daemon_error("Failed to communicate with daemon"))?;

    match response {
        Response::Success { message } => {
            let uid = get_current_uid();
            println!("{} {}", "✓".green().bold(), "Resources released".bold());
            println!("{}", message);
            println!("Slice {} removed.", format!("fairshare-{}.slice", uid).cyan());
            Ok(())
        }
        Response::Error { error } => {
            Err(anyhow::anyhow!("{}", error))
        }
        _ => {
            Err(anyhow::anyhow!("Unexpected response from daemon"))
        }
    }
}

async fn handle_status(socket_path: &str) -> Result<()> {
    let client = IpcClient::new(socket_path);

    let request = Request::Status;

    let response = client.send_request(request).await
        .context(format_daemon_error("Failed to communicate with daemon"))?;

    match response {
        Response::StatusInfo { allocated_cpu, allocated_mem } => {
            let uid = get_current_uid();
            println!("{}", "Current Allocation:".bold());
            println!("  Your slice:  {}", format!("fairshare-{}.slice", uid).cyan());
            println!("  CPU cores:   {}", allocated_cpu.to_string().green());
            println!("  Memory:      {}", allocated_mem.green());
            println!("  Status:      {}", "Active".green().bold());
            Ok(())
        }
        Response::Error { error } => {
            if error.contains("No active resource allocation") {
                println!("{}", "No active allocation".yellow());
                println!("Use {} to request resources.", "fairshare request --cpu <N> --mem <SIZE>".cyan());
                Ok(())
            } else {
                Err(anyhow::anyhow!("{}", error))
            }
        }
        _ => {
            Err(anyhow::anyhow!("Unexpected response from daemon"))
        }
    }
}

async fn handle_exec(socket_path: &str, command: &[String]) -> Result<()> {
    if command.is_empty() {
        return Err(anyhow::anyhow!("No command specified"));
    }

    // First, verify that the user has an active allocation
    let client = IpcClient::new(socket_path);
    let status_request = Request::Status;

    let response = client.send_request(status_request).await
        .context(format_daemon_error("Failed to communicate with daemon"))?;

    // Check if user has an allocation
    match response {
        Response::StatusInfo { .. } => {
            // User has an allocation, proceed with exec
        }
        Response::Error { error } => {
            if error.contains("No active resource allocation") {
                return Err(anyhow::anyhow!(
                    "No active resource allocation found.\nPlease request resources first using: {}",
                    "fairshare request --cpu <N> --mem <SIZE>".cyan()
                ));
            } else {
                return Err(anyhow::anyhow!("{}", error));
            }
        }
        _ => {
            return Err(anyhow::anyhow!("Unexpected response from daemon"));
        }
    }

    // Get current user's UID
    let uid = get_current_uid();
    let slice_name = format!("fairshare-{}.slice", uid);

    // Build systemd-run command
    let mut systemd_cmd = process::Command::new("systemd-run");
    systemd_cmd
        .arg("--user")
        .arg("--scope")
        .arg(format!("--slice={}", slice_name))
        .arg("--")
        .args(command);

    // Execute the command
    let status = systemd_cmd.status()
        .context("Failed to execute systemd-run")?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        process::exit(code);
    }

    Ok(())
}

fn get_current_uid() -> u32 {
    unsafe { libc::getuid() }
}

fn format_daemon_error(base_msg: &str) -> String {
    format!(
        "{}\n\nPossible causes:\n  • Daemon not running (is fairshared started?)\n  • Socket permissions (check {})\n  • Socket path incorrect",
        base_msg,
        "/run/fairshare.sock".cyan()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_uid() {
        let uid = get_current_uid();
        assert!(uid >= 0);
    }

    #[test]
    fn test_format_daemon_error() {
        let msg = format_daemon_error("Test error");
        assert!(msg.contains("Daemon not running"));
        assert!(msg.contains("Socket permissions"));
    }
}
