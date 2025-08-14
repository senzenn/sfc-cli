use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};

use crate::error::{Result, SfcError, ErrorContext};
use crate::core::hash::compute_snapshot_hash;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub hash: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub is_active: bool,
    pub packages: Vec<PackageInfo>,
    pub toolchains: HashMap<String, String>,
    pub container_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareInfo {
    pub packages: Vec<PackageInfo>,
    pub toolchains: HashMap<String, String>,
    pub hash: String,
    pub timestamp: DateTime<Utc>,
    pub container_name: String,
    pub description: String,
}

pub struct SnapshotManager {
    workspace_root: PathBuf,
}

impl SnapshotManager {
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }
    
    /// Create a new snapshot directory
    pub fn create_snapshot(&self, kind: &str) -> Result<PathBuf> {
        create_snapshot_dir(&self.workspace_root, kind)
    }
    
    /// List all snapshots for a container
    pub fn list_container_snapshots(&self, container_name: &str) -> Result<Vec<SnapshotInfo>> {
        let mut snapshots = Vec::new();
        let store_dir = self.workspace_root.join("store");
        let links_dir = self.workspace_root.join("links");
        
        if !store_dir.exists() {
            return Ok(snapshots);
        }
        
        // Get current active snapshot
        let stable_link = links_dir.join(format!("{}-stable", container_name));
        let current_hash = if stable_link.exists() {
            self.get_snapshot_hash_from_link(&stable_link).ok()
        } else {
            None
        };
        
        // Scan store directory for snapshots
        let entries = fs::read_dir(&store_dir)
            .with_io_context(|| format!("reading store directory {}", store_dir.display()))?;
        
        for entry in entries {
            let entry = entry
                .with_io_context(|| "reading store entry".to_string())?;
            
            if entry.file_type()
                .with_io_context(|| "getting file type for store entry".to_string())?
                .is_dir() 
            {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_name.contains("-snapshot-") {
                    let snapshot_path = entry.path();
                    let hash = compute_snapshot_hash(&snapshot_path)?;
                    
                    // Check if this snapshot belongs to this container by looking for links
                    if let Some(snapshot_info) = self.create_snapshot_info(
                        container_name, 
                        &hash, 
                        &snapshot_path, 
                        current_hash.as_deref()
                    )? {
                        snapshots.push(snapshot_info);
                    }
                }
            }
        }
        
        // Sort by timestamp (newest first)
        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(snapshots)
    }
    
    /// Get the current snapshot hash for a container
    pub fn get_current_snapshot_hash(&self, container_name: &str) -> Result<String> {
        let stable_link = self.workspace_root.join("links").join(format!("{}-stable", container_name));
        if !stable_link.exists() {
            return Err(SfcError::NotFound {
                resource: "stable snapshot".to_string(),
                identifier: container_name.to_string(),
            });
        }
        
        self.get_snapshot_hash_from_link(&stable_link)
    }
    
    /// Find snapshot by hash prefix
    pub fn find_snapshot_by_hash(&self, hash: &str) -> Result<PathBuf> {
        let store_dir = self.workspace_root.join("store");
        
        if !store_dir.exists() {
            return Err(SfcError::NotFound {
                resource: "snapshot".to_string(),
                identifier: hash.to_string(),
            });
        }
        
        let entries = fs::read_dir(&store_dir)
            .with_io_context(|| format!("reading store directory {}", store_dir.display()))?;
        
        for entry in entries {
            let entry = entry
                .with_io_context(|| "reading store entry".to_string())?;
            
            if entry.file_type()
                .with_io_context(|| "getting file type for store entry".to_string())?
                .is_dir() 
            {
                let snapshot_path = entry.path();
                let snapshot_hash = compute_snapshot_hash(&snapshot_path)?;
                
                if snapshot_hash.starts_with(hash) {
                    return Ok(snapshot_path);
                }
            }
        }
        
        Err(SfcError::NotFound {
            resource: "snapshot".to_string(),
            identifier: hash.to_string(),
        })
    }
    
    /// Generate sharing information for a snapshot
    pub fn generate_share_info(&self, container_name: &str, hash: &str) -> Result<ShareInfo> {
        let snapshot_path = self.find_snapshot_by_hash(hash)?;
        
        // Load container configuration
        let container_config_path = self.workspace_root
            .join(".sfc")
            .join("containers")
            .join(format!("{}.toml", container_name));
        
        let packages = if container_config_path.exists() {
            let config_content = fs::read_to_string(&container_config_path)
                .with_io_context(|| format!("reading container config {}", container_config_path.display()))?;
            self.parse_packages_from_config(&config_content)?
        } else {
            Vec::new()
        };
        
        // Get toolchain information
        let toolchains = self.get_snapshot_toolchains(&snapshot_path)?;
        
        // Generate description
        let description = if Some(hash) == self.get_current_snapshot_hash(container_name).ok().as_deref() {
            "current stable".to_string()
        } else {
            "snapshot".to_string()
        };
        
        Ok(ShareInfo {
            packages,
            toolchains,
            hash: hash.to_string(),
            timestamp: Utc::now(),
            container_name: container_name.to_string(),
            description,
        })
    }
    
    /// Delete a specific snapshot
    pub fn delete_snapshot(&self, container_name: &str, hash: &str) -> Result<()> {
        let store_dir = self.workspace_root.join("store");
        let links_dir = self.workspace_root.join("links");
        
        // Find and remove the snapshot directory
        let entries = fs::read_dir(&store_dir)
            .with_io_context(|| format!("reading store directory {}", store_dir.display()))?;
        
        for entry in entries {
            let entry = entry
                .with_io_context(|| "reading store entry".to_string())?;
            
            if entry.file_type()
                .with_io_context(|| "getting file type for store entry".to_string())?
                .is_dir() 
            {
                let snapshot_path = entry.path();
                let snapshot_hash = compute_snapshot_hash(&snapshot_path)?;
                
                if snapshot_hash.starts_with(hash) {
                    // Remove any links pointing to this snapshot
                    if let Ok(link_entries) = fs::read_dir(&links_dir) {
                        for link_entry in link_entries {
                            if let Ok(link_entry) = link_entry {
                                if link_entry.path().is_symlink() {
                                    if let Ok(target) = fs::read_link(link_entry.path()) {
                                        if target.to_string_lossy().contains(
                                            &snapshot_path.file_name().unwrap().to_string_lossy().to_string()
                                        ) {
                                            fs::remove_file(link_entry.path())
                                                .with_io_context(|| format!(
                                                    "removing link {}", 
                                                    link_entry.path().display()
                                                ))?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Remove the snapshot directory
                    fs::remove_dir_all(&snapshot_path)
                        .with_io_context(|| format!("removing snapshot directory {}", snapshot_path.display()))?;
                    
                    return Ok(());
                }
            }
        }
        
        Err(SfcError::NotFound {
            resource: "snapshot".to_string(),
            identifier: hash.to_string(),
        })
    }
    
    /// Copy a snapshot to create a new one
    pub fn copy_snapshot(&self, source_hash: &str, new_kind: &str) -> Result<PathBuf> {
        let source_path = self.find_snapshot_by_hash(source_hash)?;
        let new_snapshot = self.create_snapshot(new_kind)?;
        
        self.copy_dir_all(&source_path, &new_snapshot)?;
        
        Ok(new_snapshot)
    }
    
    /// Seed default lockfiles in a snapshot
    pub fn seed_lockfiles(&self, snapshot_dir: &Path) -> Result<()> {
        seed_lockfiles(snapshot_dir)
    }
    
    // Helper methods
    
    fn get_snapshot_hash_from_link(&self, link_path: &Path) -> Result<String> {
        let target = fs::read_link(link_path)
            .with_io_context(|| format!("reading symlink {}", link_path.display()))?;
        
        let abs_target = link_path.parent().unwrap().join(target).canonicalize()
            .with_io_context(|| format!("resolving symlink target {}", link_path.display()))?;
        
        compute_snapshot_hash(&abs_target)
    }
    
    fn create_snapshot_info(
        &self,
        container_name: &str, 
        hash: &str, 
        snapshot_path: &Path,
        current_hash: Option<&str>
    ) -> Result<Option<SnapshotInfo>> {
        let links_dir = self.workspace_root.join("links");
        let container_prefix = format!("{}-", container_name);
        
        // Check if any link points to this snapshot
        let mut found_link = false;
        if let Ok(entries) = fs::read_dir(&links_dir) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let link_name = entry.file_name().to_string_lossy().to_string();
                    if link_name.starts_with(&container_prefix) && entry.path().is_symlink() {
                        if let Ok(target) = fs::read_link(entry.path()) {
                            let abs_target = entry.path().parent().unwrap().join(target);
                            if let Ok(canonical) = abs_target.canonicalize() {
                                if canonical == snapshot_path {
                                    found_link = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if !found_link {
            return Ok(None);
        }
        
        // Get timestamp from directory metadata
        let metadata = fs::metadata(snapshot_path)
            .with_io_context(|| format!("getting metadata for snapshot {}", snapshot_path.display()))?;
        
        let timestamp = metadata.modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .into();
        
        // Generate description
        let description = if Some(hash) == current_hash {
            "current stable".to_string()
        } else {
            "snapshot".to_string()
        };
        
        Ok(Some(SnapshotInfo {
            hash: hash.to_string(),
            timestamp,
            description,
            is_active: Some(hash) == current_hash,
            packages: Vec::new(), // Would be populated from container config
            toolchains: HashMap::new(), // Would be populated from snapshot
            container_name: container_name.to_string(),
        }))
    }
    
    fn parse_packages_from_config(&self, config_content: &str) -> Result<Vec<PackageInfo>> {
        // Simple TOML parsing for packages
        // In a real implementation, this would use the proper container config structures
        let mut packages = Vec::new();
        
        for line in config_content.lines() {
            if line.trim().starts_with("name = ") {
                if let Some(name) = self.extract_toml_string_value(line) {
                    packages.push(PackageInfo {
                        name,
                        version: None,
                        source: "nixpkgs".to_string(),
                    });
                }
            }
        }
        
        Ok(packages)
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
        
        // Check for common toolchain indicators
        if snapshot_path.join("node_version").exists() {
            if let Ok(version) = fs::read_to_string(snapshot_path.join("node_version")) {
                toolchains.insert("node".to_string(), version.trim().to_string());
            }
        }
        
        if snapshot_path.join("rust_version").exists() {
            if let Ok(version) = fs::read_to_string(snapshot_path.join("rust_version")) {
                toolchains.insert("rust".to_string(), version.trim().to_string());
            }
        }
        
        Ok(toolchains)
    }
    
    fn copy_dir_all(&self, src: &Path, dst: &Path) -> Result<()> {
        fs::create_dir_all(dst)
            .with_io_context(|| format!("creating directory {}", dst.display()))?;
        
        let entries = fs::read_dir(src)
            .with_io_context(|| format!("reading directory {}", src.display()))?;
        
        for entry in entries {
            let entry = entry
                .with_io_context(|| format!("reading entry in {}", src.display()))?;
            
            let ty = entry.file_type()
                .with_io_context(|| "getting file type".to_string())?;
            
            if ty.is_dir() {
                self.copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
            } else {
                fs::copy(entry.path(), dst.join(entry.file_name()))
                    .with_io_context(|| format!(
                        "copying file {} to {}", 
                        entry.path().display(),
                        dst.join(entry.file_name()).display()
                    ))?;
            }
        }
        
        Ok(())
    }
}

/// Create a new snapshot directory in the workspace store
pub fn create_snapshot_dir(workspace_root: &Path, kind: &str) -> Result<PathBuf> {
    let store = workspace_root.join("store");
    
    fs::create_dir_all(&store)
        .with_io_context(|| format!("creating store directory {}", store.display()))?;
    
    let rand: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect();
    
    let name = format!("{}-{}", rand, kind);
    let dir = store.join(&name);
    
    fs::create_dir_all(&dir)
        .with_io_context(|| format!("creating snapshot directory {}", dir.display()))?;
    
    Ok(dir)
}

/// Seed default lockfiles in a snapshot directory
pub fn seed_lockfiles(snapshot_dir: &Path) -> Result<()> {
    let lockfiles = [
        ("requirements.txt", b"# pinned python deps\n"),
        ("rockspec.lock", b"# pinned luarocks deps\n"),
        ("Cargo.lock", b"# pinned cargo lock placeholder\n"),
        ("package-lock.json", b"{\n  \"name\": \"sfc-container\",\n  \"lockfileVersion\": 2\n}\n"),
    ];
    
    for (filename, content) in &lockfiles {
        let file_path = snapshot_dir.join(filename);
        if !file_path.exists() {
            fs::write(&file_path, content)
                .with_io_context(|| format!("creating lockfile {}", file_path.display()))?;
        }
    }
    
    Ok(())
}
