use std::fs;
use std::path::Path;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

use crate::error::{Result, ErrorContext};

/// Compute a stable hash for a snapshot directory from known lockfiles and metadata
pub fn compute_snapshot_hash(snapshot_dir: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    
    // Include snapshot directory name for uniqueness
    if let Some(dir_name) = snapshot_dir.file_name() {
        hasher.update(dir_name.to_string_lossy().as_bytes());
    }
    
    // Hash known lockfiles in a deterministic order
    let lockfiles = [
        "requirements.txt",
        "package-lock.json", 
        "Cargo.lock",
        "rockspec.lock",
        "Gemfile.lock",
        "composer.lock",
        "pubspec.lock",
        "mix.lock",
    ];
    
    for lockfile in lockfiles.iter() {
        let lockfile_path = snapshot_dir.join(lockfile);
        if lockfile_path.exists() {
            // Hash the filename first
            hasher.update(lockfile.as_bytes());
            
            // Then hash the file contents
            let bytes = fs::read(&lockfile_path)
                .with_io_context(|| format!("reading lockfile {}", lockfile_path.display()))?;
            hasher.update(&bytes);
        }
    }
    
    // Hash metadata files if they exist
    let metadata_files = [
        "sfc-metadata.toml",
        "container.toml", 
        "toolchain.toml",
    ];
    
    for metadata_file in metadata_files.iter() {
        let metadata_path = snapshot_dir.join(metadata_file);
        if metadata_path.exists() {
            hasher.update(metadata_file.as_bytes());
            let bytes = fs::read(&metadata_path)
                .with_io_context(|| format!("reading metadata file {}", metadata_path.display()))?;
            hasher.update(&bytes);
        }
    }
    
    // Include modification time of the directory itself
    if let Ok(metadata) = fs::metadata(snapshot_dir) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(&duration.as_secs().to_le_bytes());
            }
        }
    }
    
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest))
}

/// Compute hash for arbitrary content (used for configuration, etc.)
pub fn compute_content_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let digest = hasher.finalize();
    format!("{:x}", digest)
}

/// Compute hash for string content
pub fn compute_string_hash(content: &str) -> String {
    compute_content_hash(content.as_bytes())
}

/// Container metadata for hashing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetadata {
    pub name: String,
    pub packages: Vec<PackageMetadata>,
    pub toolchains: std::collections::HashMap<String, String>,
    pub environment: std::collections::HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageMetadata {
    pub name: String,
    pub version: Option<String>,
    pub source: String,
    pub channel: Option<String>,
}

impl ContainerMetadata {
    /// Compute deterministic hash for container metadata
    pub fn compute_hash(&self) -> Result<String> {
        // Ensure packages are sorted for deterministic hashing
        let mut sorted_packages = self.packages.clone();
        sorted_packages.sort();
        
        // Create a sorted version of environment variables
        let mut sorted_env: Vec<_> = self.environment.iter().collect();
        sorted_env.sort_by_key(|(k, _)| *k);
        
        // Create a sorted version of toolchains
        let mut sorted_toolchains: Vec<_> = self.toolchains.iter().collect();
        sorted_toolchains.sort_by_key(|(k, _)| *k);
        
        // Build the content to hash
        let mut hasher = Sha256::new();
        
        // Hash container name
        hasher.update(self.name.as_bytes());
        
        // Hash packages
        for package in &sorted_packages {
            hasher.update(package.name.as_bytes());
            if let Some(version) = &package.version {
                hasher.update(version.as_bytes());
            }
            hasher.update(package.source.as_bytes());
            if let Some(channel) = &package.channel {
                hasher.update(channel.as_bytes());
            }
        }
        
        // Hash environment variables
        for (key, value) in sorted_env {
            hasher.update(key.as_bytes());
            hasher.update(value.as_bytes());
        }
        
        // Hash toolchains
        for (key, value) in sorted_toolchains {
            hasher.update(key.as_bytes());
            hasher.update(value.as_bytes());
        }
        
        // Hash version
        hasher.update(self.version.as_bytes());
        
        // Hash creation timestamp (truncated to minute for stability)
        let timestamp_minutes = self.created_at.timestamp() / 60;
        hasher.update(&timestamp_minutes.to_le_bytes());
        
        let digest = hasher.finalize();
        Ok(format!("{:x}", digest))
    }
}

/// Generate a short hash (first 12 characters) for display purposes
pub fn short_hash(full_hash: &str) -> String {
    full_hash.chars().take(12).collect()
}

/// Validate hash format (64 character hex string)
pub fn validate_hash_format(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

/// Find hash by prefix (minimum 6 characters for safety)
pub fn find_hash_by_prefix(candidate_hashes: &[String], prefix: &str) -> Option<String> {
    if prefix.len() < 6 {
        return None;
    }
    
    let matches: Vec<_> = candidate_hashes
        .iter()
        .filter(|hash| hash.starts_with(prefix))
        .collect();
    
    match matches.len() {
        1 => Some(matches[0].clone()),
        _ => None, // Either no matches or ambiguous
    }
}

/// Compare two hashes and determine if they match (supports prefix matching)
pub fn hashes_match(hash1: &str, hash2: &str) -> bool {
    if hash1.len() == hash2.len() {
        hash1 == hash2
    } else {
        // Support prefix matching (shorter hash is treated as prefix)
        let (shorter, longer) = if hash1.len() < hash2.len() {
            (hash1, hash2)
        } else {
            (hash2, hash1)
        };
        
        // Require at least 6 characters for prefix matching
        if shorter.len() >= 6 {
            longer.starts_with(shorter)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    
    #[test]
    fn test_compute_string_hash() {
        let hash1 = compute_string_hash("hello world");
        let hash2 = compute_string_hash("hello world");
        let hash3 = compute_string_hash("hello world!");
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA-256 produces 64 hex characters
    }
    
    #[test]
    fn test_short_hash() {
        let full_hash = "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456";
        let short = short_hash(full_hash);
        assert_eq!(short, "a1b2c3d4e5f6");
        assert_eq!(short.len(), 12);
    }
    
    #[test]
    fn test_validate_hash_format() {
        assert!(validate_hash_format("a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"));
        assert!(!validate_hash_format("a1b2c3d4e5f6")); // Too short
        assert!(!validate_hash_format("g1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456")); // Invalid character
    }
    
    #[test]
    fn test_find_hash_by_prefix() {
        let hashes = vec![
            "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456".to_string(),
            "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef654321".to_string(),
            "b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456789".to_string(),
        ];
        
        assert!(find_hash_by_prefix(&hashes, "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456").is_some());
        assert!(find_hash_by_prefix(&hashes, "b2c3d4").is_some());
        assert!(find_hash_by_prefix(&hashes, "a1b2c3").is_none()); // Ambiguous
        assert!(find_hash_by_prefix(&hashes, "xyz").is_none()); // Not found
        assert!(find_hash_by_prefix(&hashes, "a1b").is_none()); // Too short
    }
    
    #[test]
    fn test_hashes_match() {
        let full_hash = "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456";
        
        assert!(hashes_match(full_hash, full_hash));
        assert!(hashes_match("a1b2c3d4e5f6", full_hash));
        assert!(hashes_match(full_hash, "a1b2c3d4e5f6"));
        assert!(!hashes_match("a1b2c3d4e5f7", full_hash));
        assert!(!hashes_match("a1b", full_hash)); // Too short for prefix
    }
    
    #[test]
    fn test_container_metadata_hash_deterministic() {
        let mut packages = vec![
            PackageMetadata {
                name: "nodejs".to_string(),
                version: Some("20.0.0".to_string()),
                source: "homebrew".to_string(),
                channel: Some("stable".to_string()),
            },
            PackageMetadata {
                name: "python".to_string(),
                version: Some("3.11.0".to_string()),
                source: "system".to_string(),
                channel: None,
            },
        ];
        
        let mut env = HashMap::new();
        env.insert("NODE_ENV".to_string(), "development".to_string());
        env.insert("DEBUG".to_string(), "app:*".to_string());
        
        let mut toolchains = HashMap::new();
        toolchains.insert("node".to_string(), "20.0.0".to_string());
        
        let metadata1 = ContainerMetadata {
            name: "test-container".to_string(),
            packages: packages.clone(),
            toolchains: toolchains.clone(),
            environment: env.clone(),
            created_at: chrono::Utc::now(),
            version: "1.0.0".to_string(),
        };
        
        // Reverse package order
        packages.reverse();
        let metadata2 = ContainerMetadata {
            name: "test-container".to_string(),
            packages,
            toolchains,
            environment: env,
            created_at: metadata1.created_at, // Same timestamp
            version: "1.0.0".to_string(),
        };
        
        // Hashes should be the same despite different package order
        assert_eq!(metadata1.compute_hash().unwrap(), metadata2.compute_hash().unwrap());
    }
}
