use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

use crate::error::{Result, SfcError, ErrorContext};

/// Manages system-wide binary switching
pub struct BinaryManager {
    backup_dir: PathBuf,
    system_bin_dir: PathBuf,
}

impl BinaryManager {
    pub fn new() -> Self {
        Self {
            backup_dir: PathBuf::from("/usr/local/.sfc-backup/bin"),
            system_bin_dir: PathBuf::from("/usr/local/bin"),
        }
    }
    
    /// Check if running with sufficient privileges
    pub fn check_privileges(&self) -> Result<()> {
        #[cfg(unix)]
        {
            if !nix::unistd::Uid::effective().is_root() {
                return Err(SfcError::Permission {
                    operation: "system binary switching".to_string(),
                    required: "sudo privileges".to_string(),
                });
            }
        }
        
        #[cfg(not(unix))]
        {
            // On Windows, we could check for administrator privileges
            // For now, we'll assume it's okay
        }
        
        Ok(())
    }
    
    /// Switch system binaries to use container binaries
    pub fn switch_to_container(&self, container_bin: &Path, force: bool) -> Result<()> {
        self.check_privileges()?;
        
        if !container_bin.exists() {
            return Err(SfcError::NotFound {
                resource: "container binaries".to_string(),
                identifier: container_bin.display().to_string(),
            });
        }
        
        // Check if already switched
        if self.backup_dir.exists() && !force {
            return Err(SfcError::AlreadyExists {
                resource: "binary backup".to_string(),
                identifier: "system binaries already switched".to_string(),
            });
        }
        
        // Create backup directory
        fs::create_dir_all(&self.backup_dir)
            .with_io_context(|| format!("creating backup directory {}", self.backup_dir.display()))?;
        
        // Clear existing backups if force is enabled
        if force && self.backup_dir.exists() {
            fs::remove_dir_all(&self.backup_dir)
                .with_io_context(|| format!("clearing existing backup {}", self.backup_dir.display()))?;
            fs::create_dir_all(&self.backup_dir)
                .with_io_context(|| format!("recreating backup directory {}", self.backup_dir.display()))?;
        }
        
        // Process each binary in the container
        let entries = fs::read_dir(container_bin)
            .with_io_context(|| format!("reading container binaries {}", container_bin.display()))?;
        
        for entry in entries {
            let entry = entry
                .with_io_context(|| "reading container binary entry".to_string())?;
            
            if entry.file_type()
                .with_io_context(|| "getting file type for binary".to_string())?
                .is_file() 
            {
                let exe_name = entry.file_name();
                let system_exe = self.system_bin_dir.join(&exe_name);
                let backup_exe = self.backup_dir.join(&exe_name);
                let container_exe = entry.path();
                
                // Backup original if it exists and we haven't already backed it up
                if system_exe.exists() && !backup_exe.exists() {
                    fs::rename(&system_exe, &backup_exe)
                        .with_io_context(|| format!(
                            "backing up {} to {}", 
                            system_exe.display(), 
                            backup_exe.display()
                        ))?;
                }
                
                // Remove existing file/symlink if present
                if system_exe.exists() || system_exe.is_symlink() {
                    fs::remove_file(&system_exe)
                        .with_io_context(|| format!("removing existing {}", system_exe.display()))?;
                }
                
                // Create symlink to container binary
                self.create_symlink(&container_exe, &system_exe)?;
            }
        }
        
        Ok(())
    }
    
    /// Restore original system binaries
    pub fn restore_system_binaries(&self) -> Result<()> {
        self.check_privileges()?;
        
        if !self.backup_dir.exists() {
            return Err(SfcError::NotFound {
                resource: "binary backup".to_string(),
                identifier: self.backup_dir.display().to_string(),
            });
        }
        
        // Restore each backed up binary
        let entries = fs::read_dir(&self.backup_dir)
            .with_io_context(|| format!("reading backup directory {}", self.backup_dir.display()))?;
        
        for entry in entries {
            let entry = entry
                .with_io_context(|| "reading backup entry".to_string())?;
            
            let exe_name = entry.file_name();
            let system_exe = self.system_bin_dir.join(&exe_name);
            let backup_exe = entry.path();
            
            // Remove container symlink if it exists
            if system_exe.exists() || system_exe.is_symlink() {
                fs::remove_file(&system_exe)
                    .with_io_context(|| format!("removing container symlink {}", system_exe.display()))?;
            }
            
            // Restore original binary
            fs::rename(&backup_exe, &system_exe)
                .with_io_context(|| format!(
                    "restoring {} from backup", 
                    system_exe.display()
                ))?;
        }
        
        // Remove backup directory
        fs::remove_dir_all(self.backup_dir.parent().unwrap())
            .with_io_context(|| format!("removing backup directory {}", self.backup_dir.parent().unwrap().display()))?;
        
        Ok(())
    }
    
    /// Check if system binaries are currently switched
    pub fn is_switched(&self) -> bool {
        self.backup_dir.exists()
    }
    
    /// Get information about current state
    pub fn get_status(&self) -> Result<BinarySwitchStatus> {
        if self.is_switched() {
            // Count backed up binaries
            let backup_count = if self.backup_dir.exists() {
                fs::read_dir(&self.backup_dir)
                    .map(|entries| entries.count())
                    .unwrap_or(0)
            } else {
                0
            };
            
            Ok(BinarySwitchStatus {
                is_switched: true,
                backup_count,
                backup_location: Some(self.backup_dir.clone()),
            })
        } else {
            Ok(BinarySwitchStatus {
                is_switched: false,
                backup_count: 0,
                backup_location: None,
            })
        }
    }
    
    // Helper methods
    
    fn create_symlink(&self, target: &Path, link: &Path) -> Result<()> {
        #[cfg(unix)]
        unix_fs::symlink(target, link)
            .with_io_context(|| format!(
                "creating symlink {} -> {}", 
                link.display(), 
                target.display()
            ))?;
        
        #[cfg(not(unix))]
        {
            // On Windows, try to create a symlink if possible, otherwise copy
            if let Err(_) = std::os::windows::fs::symlink_file(target, link) {
                // If symlinking fails, copy the file instead
                fs::copy(target, link)
                    .with_io_context(|| format!(
                        "copying binary {} to {}", 
                        target.display(), 
                        link.display()
                    ))?;
            }
        }
        
        Ok(())
    }
}

impl Default for BinaryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct BinarySwitchStatus {
    pub is_switched: bool,
    pub backup_count: usize,
    pub backup_location: Option<PathBuf>,
}

/// Switch system binaries to use container binaries (convenience function)
pub fn switch_system_binaries(container_bin: &Path, force: bool) -> Result<()> {
    let manager = BinaryManager::new();
    manager.switch_to_container(container_bin, force)
}

/// Restore original system binaries (convenience function)
pub fn restore_system_binaries() -> Result<()> {
    let manager = BinaryManager::new();
    manager.restore_system_binaries()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[test]
    fn test_binary_manager_creation() {
        let manager = BinaryManager::new();
        assert_eq!(manager.backup_dir, PathBuf::from("/usr/local/.sfc-backup/bin"));
        assert_eq!(manager.system_bin_dir, PathBuf::from("/usr/local/bin"));
    }
    
    #[test]
    fn test_status_when_not_switched() {
        let manager = BinaryManager::new();
        // This will likely fail in test environment, but tests the structure
        if let Ok(status) = manager.get_status() {
            assert!(!status.is_switched);
            assert_eq!(status.backup_count, 0);
            assert!(status.backup_location.is_none());
        }
    }
    
    // Note: Testing actual binary switching requires root privileges and is not
    // practical in unit tests. Integration tests would be better for this.
}
