use anyhow::{Result, Context, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use tracing::{info, debug};
use crate::utils::parse_memory_size;

/// Represents a policy configuration loaded from YAML
/// Structure matches: /etc/fairshare/policy.d/default.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub defaults: ResourceSpec,
    pub max: ResourceSpec,
}

/// Resource specification for CPU and memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSpec {
    pub cpu: u32,
    pub mem: String,
}

/// Manages policy loading, parsing, and validation
pub struct PolicyManager {
    config: Option<PolicyConfig>,
    policy_path: String,
}

impl PolicyManager {
    /// Create a new policy manager
    pub fn new(policy_path: impl Into<String>) -> Self {
        Self {
            config: None,
            policy_path: policy_path.into(),
        }
    }

    /// Load policies from YAML file
    pub fn load_policies(&mut self) -> Result<()> {
        info!("Loading policies from: {}", self.policy_path);

        // Read YAML file
        let yaml_content = fs::read_to_string(&self.policy_path)
            .with_context(|| format!("Failed to read policy file: {}", self.policy_path))?;

        // Parse with serde_yaml
        let config: PolicyConfig = serde_yaml::from_str(&yaml_content)
            .with_context(|| format!("Failed to parse YAML policy file: {}", self.policy_path))?;

        // Validate policy configuration
        Self::validate_config(&config)?;

        // Store parsed config
        self.config = Some(config);

        info!("Successfully loaded and validated policy configuration");
        debug!("Policy: {:?}", self.config);

        Ok(())
    }

    /// Reload policies from disk
    pub fn reload_policies(&mut self) -> Result<()> {
        info!("Reloading policies");

        // Clear existing config
        self.config = None;

        // Load fresh policies
        self.load_policies()?;

        info!("Policies reloaded successfully");

        Ok(())
    }

    /// Get the policy configuration
    pub fn get_config(&self) -> Option<&PolicyConfig> {
        self.config.as_ref()
    }

    /// Get default resource specification
    pub fn get_defaults(&self) -> Result<&ResourceSpec> {
        self.config
            .as_ref()
            .map(|c| &c.defaults)
            .ok_or_else(|| anyhow::anyhow!("Policy not loaded"))
    }

    /// Get maximum resource specification
    pub fn get_max(&self) -> Result<&ResourceSpec> {
        self.config
            .as_ref()
            .map(|c| &c.max)
            .ok_or_else(|| anyhow::anyhow!("Policy not loaded"))
    }

    /// Validate a resource request against policy limits
    pub fn validate_request(&self, cpu: u32, mem: &str) -> Result<()> {
        let config = self.config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Policy not loaded"))?;

        // Parse requested memory
        let mem_bytes = parse_memory_size(mem)?;
        let max_mem_bytes = parse_memory_size(&config.max.mem)?;

        // Validate CPU
        if cpu == 0 {
            bail!("CPU count must be greater than 0");
        }

        if cpu > config.max.cpu {
            bail!(
                "Requested CPU ({}) exceeds maximum allowed ({})",
                cpu,
                config.max.cpu
            );
        }

        // Validate memory
        if mem_bytes > max_mem_bytes {
            bail!(
                "Requested memory ({}) exceeds maximum allowed ({})",
                mem,
                config.max.mem
            );
        }

        Ok(())
    }

    /// Validate policy configuration
    fn validate_config(config: &PolicyConfig) -> Result<()> {
        // Validate defaults
        if config.defaults.cpu == 0 {
            bail!("Default CPU must be greater than 0");
        }

        let defaults_mem_bytes = parse_memory_size(&config.defaults.mem)
            .with_context(|| format!("Invalid default memory size: {}", config.defaults.mem))?;

        // Validate max
        if config.max.cpu == 0 {
            bail!("Maximum CPU must be greater than 0");
        }

        let max_mem_bytes = parse_memory_size(&config.max.mem)
            .with_context(|| format!("Invalid maximum memory size: {}", config.max.mem))?;

        // Validate that max >= defaults
        if config.max.cpu < config.defaults.cpu {
            bail!(
                "Maximum CPU ({}) must be greater than or equal to default CPU ({})",
                config.max.cpu,
                config.defaults.cpu
            );
        }

        if max_mem_bytes < defaults_mem_bytes {
            bail!(
                "Maximum memory ({}) must be greater than or equal to default memory ({})",
                config.max.mem,
                config.defaults.mem
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_policy_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_valid_policy_parsing() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: 8G
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());

        assert!(manager.load_policies().is_ok());

        let config = manager.get_config().unwrap();
        assert_eq!(config.defaults.cpu, 2);
        assert_eq!(config.defaults.mem, "8G");
        assert_eq!(config.max.cpu, 8);
        assert_eq!(config.max.mem, "32G");
    }

    #[test]
    fn test_policy_validation_max_less_than_defaults() {
        let policy_yaml = r#"
defaults:
  cpu: 8
  mem: 32G
max:
  cpu: 2
  mem: 8G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());

        let result = manager.load_policies();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be greater than or equal to default"));
    }

    #[test]
    fn test_policy_validation_zero_cpu() {
        let policy_yaml = r#"
defaults:
  cpu: 0
  mem: 8G
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());

        let result = manager.load_policies();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be greater than 0"));
    }

    #[test]
    fn test_policy_validation_invalid_memory_format() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: invalid
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());

        let result = manager.load_policies();
        assert!(result.is_err());
    }

    #[test]
    fn test_request_validation_within_limits() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: 8G
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());
        manager.load_policies().unwrap();

        // Test valid requests
        assert!(manager.validate_request(4, "16G").is_ok());
        assert!(manager.validate_request(8, "32G").is_ok());
        assert!(manager.validate_request(1, "1G").is_ok());
    }

    #[test]
    fn test_request_validation_exceeds_cpu_limit() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: 8G
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());
        manager.load_policies().unwrap();

        let result = manager.validate_request(16, "8G");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum allowed"));
    }

    #[test]
    fn test_request_validation_exceeds_memory_limit() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: 8G
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());
        manager.load_policies().unwrap();

        let result = manager.validate_request(4, "64G");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum allowed"));
    }

    #[test]
    fn test_request_validation_zero_cpu() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: 8G
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());
        manager.load_policies().unwrap();

        let result = manager.validate_request(0, "8G");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be greater than 0"));
    }

    #[test]
    fn test_memory_unit_parsing_in_policy() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: 8192M
max:
  cpu: 8
  mem: 32GB
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());

        assert!(manager.load_policies().is_ok());

        // Validate that 16G (16GB) is within the 32GB limit
        assert!(manager.validate_request(4, "16G").is_ok());
    }

    #[test]
    fn test_reload_policies() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: 8G
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());

        // Load initial policies
        manager.load_policies().unwrap();
        assert_eq!(manager.get_config().unwrap().defaults.cpu, 2);

        // Reload policies
        assert!(manager.reload_policies().is_ok());
        assert_eq!(manager.get_config().unwrap().defaults.cpu, 2);
    }

    #[test]
    fn test_get_defaults_and_max() {
        let policy_yaml = r#"
defaults:
  cpu: 2
  mem: 8G
max:
  cpu: 8
  mem: 32G
"#;

        let file = create_test_policy_file(policy_yaml);
        let mut manager = PolicyManager::new(file.path().to_str().unwrap());
        manager.load_policies().unwrap();

        let defaults = manager.get_defaults().unwrap();
        assert_eq!(defaults.cpu, 2);
        assert_eq!(defaults.mem, "8G");

        let max = manager.get_max().unwrap();
        assert_eq!(max.cpu, 8);
        assert_eq!(max.mem, "32G");
    }

    #[test]
    fn test_policy_not_loaded() {
        let manager = PolicyManager::new("/nonexistent/path.yaml");

        // Should return error when trying to get config before loading
        assert!(manager.get_defaults().is_err());
        assert!(manager.get_max().is_err());
    }
}
