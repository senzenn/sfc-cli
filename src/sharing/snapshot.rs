use std::collections::HashMap;
use std::fs;
use std::path::Path;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::core::{SnapshotManager, WorkspaceManager};
use crate::error::{Result, SfcError, ErrorContext};

/// Manages sharing and recreation of snapshots
pub struct ShareManager {
    workspace: WorkspaceManager,
    snapshot_manager: SnapshotManager,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareInfo {
    pub hash: String,
    pub container_name: String,
    pub description: String,
    pub timestamp: DateTime<Utc>,
    pub packages: Vec<PackageInfo>,
    pub toolchains: HashMap<String, String>,
    pub environment: HashMap<String, String>,
    pub metadata: ShareMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub source: String,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareMetadata {
    pub sfc_version: String,
    pub platform_os: String,
    pub platform_arch: String,
    pub created_by: String,
    pub shared_at: DateTime<Utc>,
}

impl ShareManager {
    pub fn new(workspace: &WorkspaceManager) -> Self {
        let snapshot_manager = SnapshotManager::new(&workspace.root);
        Self {
            workspace: workspace.clone(),
            snapshot_manager,
        }
    }
    
    /// Generate sharing information for a snapshot
    pub fn generate_share_info(&self, container_name: &str, hash: Option<&str>) -> Result<ShareInfo> {
        let snapshot_hash = if let Some(h) = hash {
            h.to_string()
        } else {
            self.snapshot_manager.get_current_snapshot_hash(container_name)?
        };
        
        // Find the snapshot
        let snapshot_path = self.snapshot_manager.find_snapshot_by_hash(&snapshot_hash)?;
        
        // Load container configuration
        let container_config_path = self.workspace.root
            .join(".sfc")
            .join("containers")
            .join(format!("{}.toml", container_name));
        
        let (packages, environment) = if container_config_path.exists() {
            let config_content = fs::read_to_string(&container_config_path)
                .with_io_context(|| format!("reading container config {}", container_config_path.display()))?;
            
            let packages = self.parse_packages_from_config(&config_content)?;
            let environment = self.parse_environment_from_config(&config_content)?;
            (packages, environment)
        } else {
            (Vec::new(), HashMap::new())
        };
        
        // Get toolchain information
        let toolchains = self.get_snapshot_toolchains(&snapshot_path)?;
        
        // Generate description
        let description = if Some(&snapshot_hash) == self.snapshot_manager.get_current_snapshot_hash(container_name).ok().as_ref() {
            "current stable".to_string()
        } else {
            "snapshot".to_string()
        };
        
        // Generate metadata
        let metadata = ShareMetadata {
            sfc_version: env!("CARGO_PKG_VERSION").to_string(),
            platform_os: std::env::consts::OS.to_string(),
            platform_arch: std::env::consts::ARCH.to_string(),
            created_by: whoami::username(),
            shared_at: Utc::now(),
        };
        
        Ok(ShareInfo {
            hash: snapshot_hash,
            container_name: container_name.to_string(),
            description,
            timestamp: Utc::now(),
            packages,
            toolchains,
            environment,
            metadata,
        })
    }
    
    /// Create a shareable export of a snapshot
    pub fn export_snapshot(&self, container_name: &str, hash: Option<&str>) -> Result<String> {
        let share_info = self.generate_share_info(container_name, hash)?;
        
        // Serialize to JSON for sharing
        serde_json::to_string_pretty(&share_info)
            .map_err(|e| SfcError::Generic {
                message: format!("Failed to serialize share info: {}", e),
                source: Some(Box::new(e)),
            })
    }
    
    /// Import and recreate a snapshot from shared data
    pub fn import_snapshot(&self, share_data: &str, new_container_name: &str) -> Result<String> {
        // Deserialize the share info
        let share_info: ShareInfo = serde_json::from_str(share_data)
            .map_err(|e| SfcError::Generic {
                message: format!("Failed to parse share data: {}", e),
                source: Some(Box::new(e)),
            })?;
        
        // Validate compatibility
        self.validate_share_compatibility(&share_info)?;
        
        // Create new snapshot
        let new_snapshot = self.snapshot_manager.create_snapshot("recreated-from-share")?;
        
        // Recreate lockfiles and metadata
        self.recreate_snapshot_content(&new_snapshot, &share_info)?;
        
        // Create container configuration
        self.create_container_config(new_container_name, &share_info)?;
        
        // Create container directory structure
        self.create_container_structure(new_container_name, &new_snapshot)?;
        
        // Return the new snapshot hash
        crate::core::compute_snapshot_hash(&new_snapshot)
    }
    
    /// Print share information in a user-friendly format
    pub fn format_share_info(&self, share_info: &ShareInfo) -> String {
        let mut output = String::new();
        
        output.push_str(&format!("🔗 Sharing snapshot {} for container '{}'\n\n", 
            &share_info.hash[..12], share_info.container_name));
        
        output.push_str("📋 Share this command to recreate the environment:\n\n");
        output.push_str(&format!("   sfc create {} --from {}\n\n", 
            share_info.container_name, share_info.hash));
        
        if !share_info.packages.is_empty() {
            output.push_str(&format!("📦 Included packages ({}):\n", share_info.packages.len()));
            for package in &share_info.packages {
                let version = package.version.as_deref().unwrap_or("latest");
                output.push_str(&format!("   • {} {} [{}]\n", 
                    package.name, version, package.source));
            }
            output.push('\n');
        }
        
        if !share_info.toolchains.is_empty() {
            output.push_str(&format!("🛠️  Included toolchains ({}):\n", share_info.toolchains.len()));
            for (toolchain, version) in &share_info.toolchains {
                output.push_str(&format!("   • {} {}\n", toolchain, version));
            }
            output.push('\n');
        }
        
        if !share_info.environment.is_empty() {
            output.push_str(&format!("🌍 Environment variables ({}):\n", share_info.environment.len()));
            for (key, value) in &share_info.environment {
                output.push_str(&format!("   • {}={}\n", key, value));
            }
            output.push('\n');
        }
        
        output.push_str(&format!("ℹ️  Created with SFC {} on {} ({})\n", 
            share_info.metadata.sfc_version,
            share_info.metadata.platform_os,
            share_info.metadata.platform_arch));
        
        output
    }
    
    // Helper methods
    
    fn parse_packages_from_config(&self, config_content: &str) -> Result<Vec<PackageInfo>> {
        // Simple TOML parsing - in production, use proper TOML parsing
        let mut packages = Vec::new();
        
        // This is a simplified parser - in production, use proper TOML deserialization
        let mut in_packages_section = false;
        let mut current_package = PackageInfo {
            name: String::new(),
            version: None,
            source: "nixpkgs".to_string(),
            channel: None,
        };
        
        for line in config_content.lines() {
            let line = line.trim();
            
            if line == "[[packages]]" {
                if in_packages_section && !current_package.name.is_empty() {
                    packages.push(current_package.clone());
                }
                in_packages_section = true;
                current_package = PackageInfo {
                    name: String::new(),
                    version: None,
                    source: "nixpkgs".to_string(),
                    channel: None,
                };
            } else if in_packages_section {
                if line.starts_with("name = ") {
                    if let Some(name) = self.extract_toml_string_value(line) {
                        current_package.name = name;
                    }
                } else if line.starts_with("version = ") {
                    if let Some(version) = self.extract_toml_string_value(line) {
                        current_package.version = Some(version);
                    }
                } else if line.starts_with("source = ") {
                    if let Some(source) = self.extract_toml_string_value(line) {
                        current_package.source = source;
                    }
                } else if line.starts_with("channel = ") {
                    if let Some(channel) = self.extract_toml_string_value(line) {
                        current_package.channel = Some(channel);
                    }
                } else if line.starts_with('[') && line != "[[packages]]" {
                    // New section started
                    if !current_package.name.is_empty() {
                        packages.push(current_package.clone());
                    }
                    in_packages_section = false;
                }
            }
        }
        
        // Add the last package if we were in a packages section
        if in_packages_section && !current_package.name.is_empty() {
            packages.push(current_package);
        }
        
        Ok(packages)
    }
    
    fn parse_environment_from_config(&self, config_content: &str) -> Result<HashMap<String, String>> {
        let mut environment = HashMap::new();
        
        // Simple parser for [environment] section
        let mut in_env_section = false;
        
        for line in config_content.lines() {
            let line = line.trim();
            
            if line == "[environment]" {
                in_env_section = true;
            } else if line.starts_with('[') && line != "[environment]" {
                in_env_section = false;
            } else if in_env_section && line.contains('=') {
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim().trim_matches('"');
                    let value = value.trim().trim_matches('"');
                    environment.insert(key.to_string(), value.to_string());
                }
            }
        }
        
        Ok(environment)
    }
    
    fn extract_toml_string_value(&self, line: &str) -> Option<String> {
        if let Some(start) = line.find('"') {
            if let Some(end) = line.rfind('"') {
                if start < end {
                    return Some(line[start + 1..end].to_string());
                }
            }
        }
        None
    }
    
    fn get_snapshot_toolchains(&self, snapshot_path: &Path) -> Result<HashMap<String, String>> {
        let mut toolchains = HashMap::new();
        
        // Check for toolchain indicator files
        let toolchain_files = [
            ("node", "node_version"),
            ("rust", "rust_version"),
            ("python", "python_version"),
        ];
        
        for (toolchain, filename) in &toolchain_files {
            let version_file = snapshot_path.join(filename);
            if version_file.exists() {
                if let Ok(version) = fs::read_to_string(&version_file) {
                    toolchains.insert(toolchain.to_string(), version.trim().to_string());
                }
            }
        }
        
        Ok(toolchains)
    }
    
    fn validate_share_compatibility(&self, share_info: &ShareInfo) -> Result<()> {
        // Check SFC version compatibility
        let current_version = env!("CARGO_PKG_VERSION");
        
        // For now, just warn about version differences
        if share_info.metadata.sfc_version != current_version {
            eprintln!("⚠️  Version mismatch: share created with SFC {}, current version is {}", 
                share_info.metadata.sfc_version, current_version);
        }
        
        // Check platform compatibility
        if share_info.metadata.platform_os != std::env::consts::OS {
            eprintln!("⚠️  Platform mismatch: share created on {}, current platform is {}", 
                share_info.metadata.platform_os, std::env::consts::OS);
        }
        
        Ok(())
    }
    
    fn recreate_snapshot_content(&self, snapshot_path: &Path, share_info: &ShareInfo) -> Result<()> {
        // Create lockfiles based on packages
        self.snapshot_manager.seed_lockfiles(snapshot_path)?;
        
        // Write toolchain version files
        for (toolchain, version) in &share_info.toolchains {
            let version_file = snapshot_path.join(format!("{}_version", toolchain));
            fs::write(&version_file, version)
                .with_io_context(|| format!("writing toolchain version file {}", version_file.display()))?;
        }
        
        // Write share metadata
        let metadata_file = snapshot_path.join("sfc-share-metadata.json");
        let metadata_content = serde_json::to_string_pretty(share_info)
            .map_err(|e| SfcError::Generic {
                message: format!("Failed to serialize share metadata: {}", e),
                source: Some(Box::new(e)),
            })?;
        
        fs::write(&metadata_file, metadata_content)
            .with_io_context(|| format!("writing share metadata to {}", metadata_file.display()))?;
        
        Ok(())
    }
    
    fn create_container_config(&self, container_name: &str, share_info: &ShareInfo) -> Result<()> {
        // Create container configuration based on share info
        let config_dir = self.workspace.root.join(".sfc").join("containers");
        fs::create_dir_all(&config_dir)
            .with_io_context(|| format!("creating container config directory {}", config_dir.display()))?;
        
        let config_file = config_dir.join(format!("{}.toml", container_name));
        
        // Generate TOML configuration
        let mut config_content = String::new();
        config_content.push_str(&format!("name = \"{}\"\n", container_name));
        config_content.push_str(&format!("created_at = \"{}\"\n", Utc::now().to_rfc3339()));
        config_content.push_str("shell = \"/bin/bash\"\n\n");
        
        // Add packages
        for package in &share_info.packages {
            config_content.push_str("[[packages]]\n");
            config_content.push_str(&format!("name = \"{}\"\n", package.name));
            if let Some(version) = &package.version {
                config_content.push_str(&format!("version = \"{}\"\n", version));
            }
            config_content.push_str(&format!("source = \"{}\"\n", package.source));
            if let Some(channel) = &package.channel {
                config_content.push_str(&format!("channel = \"{}\"\n", channel));
            }
            config_content.push('\n');
        }
        
        // Add environment variables
        if !share_info.environment.is_empty() {
            config_content.push_str("[environment]\n");
            for (key, value) in &share_info.environment {
                config_content.push_str(&format!("{} = \"{}\"\n", key, value));
            }
        }
        
        fs::write(&config_file, config_content)
            .with_io_context(|| format!("writing container config to {}", config_file.display()))?;
        
        Ok(())
    }
    
    fn create_container_structure(&self, container_name: &str, snapshot_path: &Path) -> Result<()> {
        // Create container directory structure
        let container_dir = self.workspace.root.join("containers").join(container_name);
        fs::create_dir_all(container_dir.join("src"))
            .with_io_context(|| format!("creating container src directory"))?;
        fs::create_dir_all(container_dir.join("temp"))
            .with_io_context(|| format!("creating container temp directory"))?;
        
        // Create stable link
        let stable_alias = format!("{}-stable", container_name);
        let rel_target = Path::new("../store").join(snapshot_path.file_name().unwrap());
        
        let symlink_manager = crate::core::SymlinkManager::new(&self.workspace.root);
        symlink_manager.link_alias_to_store(&stable_alias, &rel_target)?;
        
        let container_stable = container_dir.join("stable");
        symlink_manager.create_or_update(&Path::new("../../links").join(&stable_alias), &container_stable)?;
        
        Ok(())
    }
}

/// Share a snapshot (convenience function)
pub fn share_snapshot(workspace: &WorkspaceManager, container_name: &str, hash: Option<&str>) -> Result<ShareInfo> {
    let share_manager = ShareManager::new(&workspace);
    share_manager.generate_share_info(container_name, hash)
}

/// Recreate a container from shared data (convenience function)
pub fn recreate_from_share(workspace: &WorkspaceManager, share_data: &str, new_container_name: &str) -> Result<String> {
    let share_manager = ShareManager::new(&workspace);
    share_manager.import_snapshot(share_data, new_container_name)
}
