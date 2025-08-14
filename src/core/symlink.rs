use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

use crate::error::{Result, SfcError, ErrorContext};

pub struct SymlinkManager {
    workspace_root: std::path::PathBuf,
}

impl SymlinkManager {
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }
    
    /// Create or update a symlink
    pub fn create_or_update(&self, target: &Path, link: &Path) -> Result<()> {
        create_or_update_symlink(target, link)
    }
    
    /// Link alias to store using GNU Stow when available, otherwise direct symlink
    pub fn link_alias_to_store(&self, alias: &str, rel_target_from_links: &Path) -> Result<()> {
        let links_dir = self.workspace_root.join("links");
        
        if self.is_stow_available() {
            self.link_with_stow(alias, rel_target_from_links)
        } else {
            self.create_or_update(rel_target_from_links, &links_dir.join(alias))
        }
    }
    
    /// Unlink alias from links directory
    pub fn unlink_alias_from_links(&self, alias: &str) -> Result<()> {
        let links_dir = self.workspace_root.join("links");
        
        if self.is_stow_available() {
            self.unlink_with_stow(alias)
        } else {
            let link_path = links_dir.join(alias);
            if link_path.exists() || link_path.is_symlink() {
                fs::remove_file(&link_path)
                    .with_io_context(|| format!("removing symlink {}", link_path.display()))?;
            }
            Ok(())
        }
    }
    
    /// Check if GNU Stow is available
    fn is_stow_available(&self) -> bool {
        Command::new("stow")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    
    /// Link using GNU Stow
    fn link_with_stow(&self, alias: &str, rel_target_from_links: &Path) -> Result<()> {
        let links_dir = self.workspace_root.join("links");
        let pkgs_dir = self.stow_pkgs_dir();
        let pkg_dir = pkgs_dir.join(alias);
        
        // Create package directory structure
        fs::create_dir_all(&pkg_dir)
            .with_io_context(|| format!("creating stow package directory {}", pkg_dir.display()))?;
        
        // Package contains a single entry named `<alias>` which is a symlink to the desired target
        let pkg_symlink = pkg_dir.join(alias);
        if pkg_symlink.exists() || pkg_symlink.is_symlink() {
            fs::remove_file(&pkg_symlink).ok();
        }
        
        #[cfg(unix)]
        unix_fs::symlink(rel_target_from_links, &pkg_symlink)
            .with_io_context(|| format!(
                "creating package symlink {} -> {}", 
                pkg_symlink.display(), 
                rel_target_from_links.display()
            ))?;
        
        #[cfg(not(unix))]
        {
            // For non-Unix systems, create a copy instead
            if rel_target_from_links.is_file() {
                fs::copy(rel_target_from_links, &pkg_symlink)
                    .with_io_context(|| format!(
                        "copying file {} to {}", 
                        rel_target_from_links.display(), 
                        pkg_symlink.display()
                    ))?;
            }
        }
        
        fs::create_dir_all(&links_dir)
            .with_io_context(|| format!("creating links directory {}", links_dir.display()))?;
        
        // Restow the package to (re)create link under links/
        let status = Command::new("stow")
            .arg("-d").arg(&pkgs_dir)
            .arg("-t").arg(&links_dir)
            .arg("-R")
            .arg(alias)
            .status()
            .with_io_context(|| "executing stow command".to_string())?;
        
        if status.success() {
            Ok(())
        } else {
            // If stow failed, fall back to direct symlink
            self.create_or_update(rel_target_from_links, &links_dir.join(alias))
        }
    }
    
    /// Unlink using GNU Stow
    fn unlink_with_stow(&self, alias: &str) -> Result<()> {
        let links_dir = self.workspace_root.join("links");
        let pkgs_dir = self.stow_pkgs_dir();
        
        let status = Command::new("stow")
            .arg("-d").arg(&pkgs_dir)
            .arg("-t").arg(&links_dir)
            .arg("-D")
            .arg(alias)
            .status()
            .with_io_context(|| "executing stow delete command".to_string())?;
        
        if !status.success() {
            // If stow failed, try direct removal
            let link_path = links_dir.join(alias);
            if link_path.exists() || link_path.is_symlink() {
                fs::remove_file(&link_path)
                    .with_io_context(|| format!("removing symlink {}", link_path.display()))?;
            }
        }
        
        Ok(())
    }
    
    /// Get stow packages directory
    fn stow_pkgs_dir(&self) -> std::path::PathBuf {
        self.workspace_root.join(".sfc").join("stow-pkgs")
    }
}

/// Create or update a symlink atomically
pub fn create_or_update_symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) -> Result<()> {
    let link = link.as_ref();
    let target = target.as_ref();
    
    // Remove existing file/symlink if it exists
    if link.exists() || link.is_symlink() {
        fs::remove_file(link)
            .with_io_context(|| format!("removing existing link {}", link.display()))?;
    }
    
    // Create parent directory if it doesn't exist
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)
            .with_io_context(|| format!("creating parent directory {}", parent.display()))?;
    }
    
    // Create the symlink
    #[cfg(unix)]
    unix_fs::symlink(target, link)
        .with_io_context(|| format!("creating symlink {} -> {}", link.display(), target.display()))?;
    
    #[cfg(not(unix))]
    {
        // For Windows, try to create a symlink if possible, otherwise copy
        if let Err(_) = std::os::windows::fs::symlink_file(target, link) {
            // If symlinking fails, copy the file instead
            if target.is_file() {
                fs::copy(target, link)
                    .with_io_context(|| format!("copying file {} to {}", target.display(), link.display()))?;
            } else if target.is_dir() {
                return Err(SfcError::System {
                    operation: "symlink creation".to_string(),
                    reason: "Directory symlinks not supported on this platform".to_string(),
                });
            }
        }
    }
    
    Ok(())
}

/// Validate that a symlink target is safe (within workspace bounds)
pub fn validate_symlink_target(workspace_root: &Path, target: &Path) -> Result<()> {
    // Resolve the target to an absolute path
    let resolved_target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        workspace_root.join(target)
    };
    
    // Canonicalize to resolve any .. or . components
    let canonical_target = resolved_target.canonicalize()
        .with_io_context(|| format!("resolving symlink target {}", target.display()))?;
    
    let canonical_workspace = workspace_root.canonicalize()
        .with_io_context(|| format!("resolving workspace root {}", workspace_root.display()))?;
    
    // Ensure the target is within the workspace
    if !canonical_target.starts_with(canonical_workspace) {
        return Err(SfcError::Validation {
            field: "symlink target".to_string(),
            value: target.display().to_string(),
            reason: "Target is outside workspace bounds".to_string(),
        });
    }
    
    Ok(())
}

/// Check if a path is a symlink and return its target
pub fn read_symlink_target(link: &Path) -> Result<std::path::PathBuf> {
    if !link.is_symlink() {
        return Err(SfcError::Validation {
            field: "path".to_string(),
            value: link.display().to_string(),
            reason: "Path is not a symlink".to_string(),
        });
    }
    
    fs::read_link(link)
        .with_io_context(|| format!("reading symlink target for {}", link.display()))
}

/// Resolve a symlink to its final target
pub fn resolve_symlink(link: &Path) -> Result<std::path::PathBuf> {
    let target = read_symlink_target(link)?;
    
    let resolved = if target.is_absolute() {
        target
    } else {
        link.parent().unwrap_or_else(|| Path::new(".")).join(target)
    };
    
    resolved.canonicalize()
        .with_io_context(|| format!("resolving symlink {}", link.display()))
}
