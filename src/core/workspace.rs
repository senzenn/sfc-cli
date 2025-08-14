use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use crate::config::SfcConfig;
use crate::error::{Result, SfcError, ErrorContext};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceMeta {
    #[serde(default)]
    pub notes: Vec<String>,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct WorkspaceManager {
    pub root: PathBuf,
    pub config: SfcConfig,
}

impl WorkspaceManager {
    /// Create a new workspace manager
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let config = SfcConfig::merged_config(&root)?;
        
        Ok(Self { root, config })
    }
    
    /// Get the default workspace manager (~/.sfc)
    pub fn default() -> Result<Self> {
        let home = std::env::var("HOME")
            .map_err(|_| SfcError::Config {
                message: "HOME environment variable not set".to_string(),
                path: None,
            })?;
        let root = Path::new(&home).join(".sfc");
        Self::new(root)
    }
    
    /// Initialize workspace if it doesn't exist
    pub fn ensure_initialized(&self) -> Result<()> {
        ensure_workspace_layout(&self.root)?;
        
        // Create default configuration if it doesn't exist
        let workspace_config_path = self.root.join(".sfc").join("workspace.toml");
        if !workspace_config_path.exists() {
            self.config.save_workspace(&self.root)?;
        }
        
        Ok(())
    }
    
    /// Check if workspace is properly initialized
    pub fn is_initialized(&self) -> bool {
        self.root.join(".sfc").exists() &&
        self.root.join("store").exists() &&
        self.root.join("containers").exists() &&
        self.root.join("links").exists()
    }
    
    /// Get workspace metadata
    pub fn metadata(&self) -> Result<WorkspaceMeta> {
        let meta_path = self.root.join(".sfc").join("workspace.toml");
        
        if meta_path.exists() {
            let content = fs::read_to_string(&meta_path)
                .with_io_context(|| format!("reading workspace metadata from {}", meta_path.display()))?;
            
            toml::from_str(&content)
                .map_err(|e| SfcError::Config {
                    message: format!("Invalid workspace metadata: {}", e),
                    path: Some(meta_path),
                })
        } else {
            Ok(WorkspaceMeta {
                version: env!("CARGO_PKG_VERSION").to_string(),
                created_at: chrono::Utc::now(),
                ..Default::default()
            })
        }
    }
    
    /// Save workspace metadata
    pub fn save_metadata(&self, meta: &WorkspaceMeta) -> Result<()> {
        let meta_path = self.root.join(".sfc").join("workspace.toml");
        let content = toml::to_string_pretty(meta)
            .map_err(|e| SfcError::Config {
                message: format!("Failed to serialize workspace metadata: {}", e),
                path: Some(meta_path.clone()),
            })?;
        
        fs::write(&meta_path, content)
            .with_io_context(|| format!("writing workspace metadata to {}", meta_path.display()))?;
        
        Ok(())
    }
    
    /// List all containers in the workspace
    pub fn list_containers(&self) -> Result<Vec<String>> {
        let containers_dir = self.root.join("containers");
        let mut names = Vec::new();
        
        if containers_dir.exists() {
            let entries = fs::read_dir(&containers_dir)
                .with_io_context(|| format!("reading containers directory {}", containers_dir.display()))?;
            
            for entry in entries {
                let entry = entry
                    .with_io_context(|| "reading container directory entry".to_string())?;
                
                if entry.file_type()
                    .with_io_context(|| "getting file type for container entry".to_string())?
                    .is_dir() 
                {
                    names.push(entry.file_name().to_string_lossy().to_string());
                }
            }
        }
        
        names.sort();
        Ok(names)
    }
    
    /// Get current container from .sfc/current file
    pub fn current_container(&self) -> Result<Option<String>> {
        let current_file = self.root.join(".sfc").join("current");
        
        if current_file.exists() {
            let name = fs::read_to_string(&current_file)
                .with_io_context(|| format!("reading current container from {}", current_file.display()))?;
            Ok(Some(name.trim().to_string()))
        } else {
            Ok(None)
        }
    }
    
    /// Set current container
    pub fn set_current_container(&self, name: &str) -> Result<()> {
        let current_file = self.root.join(".sfc").join("current");
        fs::write(&current_file, name)
            .with_io_context(|| format!("writing current container to {}", current_file.display()))?;
        Ok(())
    }
    
    /// Clear current container
    pub fn clear_current_container(&self) -> Result<()> {
        let current_file = self.root.join(".sfc").join("current");
        if current_file.exists() {
            fs::remove_file(&current_file)
                .with_io_context(|| format!("removing current container file {}", current_file.display()))?;
        }
        Ok(())
    }
    
    /// Clean up workspace (remove orphaned snapshots, etc.)
    pub fn cleanup(&self) -> Result<()> {
        self.cleanup_orphaned_snapshots()?;
        self.cleanup_dangling_links()?;
        Ok(())
    }
    
    /// Remove orphaned snapshots that aren't referenced by any links
    fn cleanup_orphaned_snapshots(&self) -> Result<()> {
        let store_dir = self.root.join("store");
        let links_dir = self.root.join("links");
        
        if !store_dir.exists() {
            return Ok(());
        }
        
        // Get all referenced snapshots
        let mut referenced_snapshots = std::collections::HashSet::new();
        
        if links_dir.exists() {
            let entries = fs::read_dir(&links_dir)
                .with_io_context(|| format!("reading links directory {}", links_dir.display()))?;
            
            for entry in entries {
                let entry = entry
                    .with_io_context(|| "reading link entry".to_string())?;
                
                if entry.path().is_symlink() {
                    if let Ok(target) = fs::read_link(entry.path()) {
                        if let Some(target_name) = target.file_name() {
                            referenced_snapshots.insert(target_name.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        
        // Remove unreferenced snapshots
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
                if !referenced_snapshots.contains(&dir_name) {
                    fs::remove_dir_all(entry.path())
                        .with_io_context(|| format!("removing orphaned snapshot {}", dir_name))?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Remove dangling symlinks in links directory
    fn cleanup_dangling_links(&self) -> Result<()> {
        let links_dir = self.root.join("links");
        
        if !links_dir.exists() {
            return Ok(());
        }
        
        let entries = fs::read_dir(&links_dir)
            .with_io_context(|| format!("reading links directory {}", links_dir.display()))?;
        
        for entry in entries {
            let entry = entry
                .with_io_context(|| "reading link entry".to_string())?;
            
            if entry.path().is_symlink() {
                if let Ok(target) = fs::read_link(entry.path()) {
                    let resolved = entry.path().parent().unwrap().join(target);
                    if !resolved.exists() {
                        fs::remove_file(entry.path())
                            .with_io_context(|| format!("removing dangling link {}", entry.path().display()))?;
                    }
                } else {
                    // Invalid symlink
                    fs::remove_file(entry.path())
                        .with_io_context(|| format!("removing invalid symlink {}", entry.path().display()))?;
                }
            }
        }
        
        Ok(())
    }
}

/// Ensure workspace directory structure exists
pub fn ensure_workspace_layout(root: &Path) -> Result<()> {
    for sub in ["store", "containers", "links", ".sfc"] {
        let p = root.join(sub);
        if !p.exists() {
            fs::create_dir_all(&p)
                .with_io_context(|| format!("creating directory {}", p.display()))?;
        }
    }
    
    // Create .gitignore if it doesn't exist
    let gitignore = root.join(".gitignore");
    if !gitignore.exists() {
        let content = [
            "store/",
            ".sfc/toolchains/",
            ".sfc/cache/",
            "**/target/",
            "**/.sfc-cache/",
            "**/.DS_Store",
            "**/.tmp",
        ].join("\n");
        
        fs::write(&gitignore, content)
            .with_io_context(|| format!("creating .gitignore file {}", gitignore.display()))?;
    }
    
    // Ensure .sfc subdirectories exist
    for sub in ["containers", "toolchains", "cache"] {
        let p = root.join(".sfc").join(sub);
        if !p.exists() {
            fs::create_dir_all(&p)
                .with_io_context(|| format!("creating .sfc subdirectory {}", p.display()))?;
        }
    }
    
    Ok(())
}
