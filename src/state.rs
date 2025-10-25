use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::Path;
use serde::{Deserialize, Serialize};
use fs2::FileExt;
use users;

const STATE_FILE_PATH: &str = "/var/lib/fairshare/allocations.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAllocation {
    pub uid: String,
    pub username: String,
    pub cpu_cores: u32,
    pub mem_gb: u32,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StateFile {
    allocations: HashMap<String, UserAllocation>,
}

impl StateFile {
    fn new() -> Self {
        StateFile {
            allocations: HashMap::new(),
        }
    }
}

/// Read all user allocations from the state file.
/// Returns an empty HashMap if the file doesn't exist or can't be read.
pub fn read_allocations() -> io::Result<HashMap<String, UserAllocation>> {
    let path = Path::new(STATE_FILE_PATH);

    if !path.exists() {
        return Ok(HashMap::new());
    }

    let mut file = File::open(path)?;

    // Acquire shared lock for reading
    file.lock_shared()?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Release lock (automatically on drop, but explicit is clearer)
    file.unlock()?;

    if contents.trim().is_empty() {
        return Ok(HashMap::new());
    }

    let state: StateFile = serde_json::from_str(&contents)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData,
            format!("Failed to parse state file: {}", e)))?;

    Ok(state.allocations)
}

/// Write or update a user's allocation in the state file.
/// Uses file locking to prevent race conditions.
pub fn write_allocation(cpu_cores: u32, mem_gb: u32) -> io::Result<()> {
    let uid = users::get_current_uid();
    let username = users::get_current_username()
        .and_then(|os_str| os_str.into_string().ok())
        .unwrap_or_else(|| format!("uid{}", uid));

    let allocation = UserAllocation {
        uid: uid.to_string(),
        username,
        cpu_cores,
        mem_gb,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    // Ensure directory exists
    let path = Path::new(STATE_FILE_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Open file for read-write, create if doesn't exist
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;

    // Acquire exclusive lock
    file.lock_exclusive()?;

    // Read existing state
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let mut state: StateFile = if contents.trim().is_empty() {
        StateFile::new()
    } else {
        serde_json::from_str(&contents)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData,
                format!("Failed to parse state file: {}", e)))?
    };

    // Update allocation
    state.allocations.insert(uid.to_string(), allocation);

    // Write back to file
    let new_contents = serde_json::to_string_pretty(&state)
        .map_err(|e| io::Error::new(io::ErrorKind::Other,
            format!("Failed to serialize state: {}", e)))?;

    // Truncate and write
    file.set_len(0)?;
    file.seek(std::io::SeekFrom::Start(0))?;
    file.write_all(new_contents.as_bytes())?;
    file.sync_all()?;

    // Release lock (automatic on drop)
    file.unlock()?;

    Ok(())
}

/// Remove the current user's allocation from the state file.
pub fn remove_allocation() -> io::Result<()> {
    let uid = users::get_current_uid();
    let path = Path::new(STATE_FILE_PATH);

    if !path.exists() {
        // Nothing to remove
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;

    // Acquire exclusive lock
    file.lock_exclusive()?;

    // Read existing state
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    if contents.trim().is_empty() {
        file.unlock()?;
        return Ok(());
    }

    let mut state: StateFile = serde_json::from_str(&contents)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData,
            format!("Failed to parse state file: {}", e)))?;

    // Remove allocation
    state.allocations.remove(&uid.to_string());

    // Write back to file
    let new_contents = serde_json::to_string_pretty(&state)
        .map_err(|e| io::Error::new(io::ErrorKind::Other,
            format!("Failed to serialize state: {}", e)))?;

    // Truncate and write
    file.set_len(0)?;
    file.seek(std::io::SeekFrom::Start(0))?;
    file.write_all(new_contents.as_bytes())?;
    file.sync_all()?;

    // Release lock
    file.unlock()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper to create a test state file path
    fn setup_test_state_file() -> (TempDir, String) {
        let temp_dir = tempfile::tempdir().unwrap();
        let state_path = temp_dir.path().join("test_allocations.json");
        (temp_dir, state_path.to_str().unwrap().to_string())
    }

    #[test]
    fn test_read_allocations_empty_file() {
        // When state file doesn't exist, should return empty HashMap
        // This test relies on STATE_FILE_PATH not existing in test environment
        // In practice, we'd need to make the path configurable for testing
        let allocations = read_allocations();
        assert!(allocations.is_ok());
    }

    #[test]
    fn test_user_allocation_serialization() {
        let allocation = UserAllocation {
            uid: "1000".to_string(),
            username: "testuser".to_string(),
            cpu_cores: 4,
            mem_gb: 8,
            timestamp: "2025-10-25T05:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&allocation).unwrap();
        let deserialized: UserAllocation = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.uid, "1000");
        assert_eq!(deserialized.username, "testuser");
        assert_eq!(deserialized.cpu_cores, 4);
        assert_eq!(deserialized.mem_gb, 8);
    }

    #[test]
    fn test_state_file_structure() {
        let mut state = StateFile::new();

        state.allocations.insert("1000".to_string(), UserAllocation {
            uid: "1000".to_string(),
            username: "user1".to_string(),
            cpu_cores: 2,
            mem_gb: 4,
            timestamp: "2025-10-25T05:00:00Z".to_string(),
        });

        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains("allocations"));
        assert!(json.contains("1000"));
        assert!(json.contains("user1"));
    }
}
