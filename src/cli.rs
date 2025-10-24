use clap::{Parser, Subcommand};

/// Fairshare - User-facing resource management CLI
#[derive(Parser, Debug)]
#[command(name = "fairshare")]
#[command(author = "Fairshare Project")]
#[command(version = "0.1.0")]
#[command(about = "Request and manage compute resources via fairshared daemon", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to the Unix socket for daemon communication
    #[arg(long, default_value = "/run/fairshare.sock")]
    pub socket: String,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Request compute resources from the daemon
    #[command(about = "Request CPU and memory resources")]
    Request {
        /// Number of CPU cores to request
        #[arg(long, value_name = "CORES")]
        cpu: u32,

        /// Amount of memory to request (e.g., "8G", "512M")
        #[arg(long, value_name = "SIZE")]
        mem: String,
    },

    /// Release currently allocated resources
    #[command(about = "Release your current resource allocation")]
    Release,

    /// Check current resource allocation status
    #[command(about = "Display your current resource allocation")]
    Status,

    /// Execute a command within your allocated resource slice
    #[command(about = "Run a command in your fairshare slice")]
    Exec {
        /// Command and arguments to execute
        #[arg(required = true, num_args = 1.., value_name = "COMMAND", allow_hyphen_values = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_request() {
        let cli = Cli::parse_from(&["fairshare", "request", "--cpu", "4", "--mem", "8G"]);
        match cli.command {
            Command::Request { cpu, mem } => {
                assert_eq!(cpu, 4);
                assert_eq!(mem, "8G");
            }
            _ => panic!("Expected Request command"),
        }
    }

    #[test]
    fn test_cli_parsing_release() {
        let cli = Cli::parse_from(&["fairshare", "release"]);
        matches!(cli.command, Command::Release);
    }

    #[test]
    fn test_cli_parsing_status() {
        let cli = Cli::parse_from(&["fairshare", "status"]);
        matches!(cli.command, Command::Status);
    }

    #[test]
    fn test_cli_parsing_exec() {
        let cli = Cli::parse_from(&["fairshare", "exec", "bash", "-c", "echo hello"]);
        match cli.command {
            Command::Exec { command } => {
                assert_eq!(command, vec!["bash", "-c", "echo hello"]);
            }
            _ => panic!("Expected Exec command"),
        }
    }

    #[test]
    fn test_custom_socket_path() {
        let cli = Cli::parse_from(&["fairshare", "--socket", "/tmp/test.sock", "status"]);
        assert_eq!(cli.socket, "/tmp/test.sock");
    }
}
