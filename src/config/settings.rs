use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use serde::{Deserialize, Serialize};
use crate::error::{Result, SfcError, ErrorContext};

/// Main SFC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SfcConfig {
    pub workspace: WorkspaceConfig,
    pub defaults: ContainerDefaults,
    pub package_sources: PackageSourceConfig,
    pub ui: UiConfig,
    pub advanced: AdvancedConfig,
}

/// Workspace-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Default workspace path (defaults to ~/.sfc)
    pub path: Option<PathBuf>,
    /// Whether to auto-initialize workspace
    pub auto_init: bool,
    /// Default shell for containers
    pub default_shell: String,
    /// Notes and metadata
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Default settings for new containers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDefaults {
    /// Default packages to install in new containers
    pub packages: Vec<String>,
    /// Default environment variables
    pub environment: HashMap<String, String>,
    /// Default toolchain versions
    pub toolchains: HashMap<String, String>,
    /// Whether to auto-enter shell after creation
    pub auto_enter: bool,
}

/// Package source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSourceConfig {
    /// Preferred package manager order
    pub preferred_managers: Vec<String>,
    /// Whether to fallback to Nix if system packages fail
    pub nix_fallback: bool,
    /// Whether to enable portable binary downloads
    pub portable_enabled: bool,
    /// Custom package source URLs
    pub custom_sources: HashMap<String, String>,
}

/// UI and display configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Whether to show colored output
    pub colored: bool,
    /// Whether to show progress bars
    pub progress_bars: bool,
    /// Whether to show banners and animations
    pub animations: bool,
    /// Log level (error, warn, info, debug, trace)
    pub log_level: String,
}

/// Advanced configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    /// Whether to use GNU Stow for package management
    pub stow_enabled: bool,
    /// Maximum number of snapshots to keep per container
    pub max_snapshots: usize,
    /// Whether to auto-cleanup orphaned snapshots
    pub auto_cleanup: bool,
    /// Parallel package installation limit
    pub parallel_installs: usize,
    /// Custom snapshot storage path
    pub snapshot_storage: Option<PathBuf>,
}

impl Default for SfcConfig {
    fn default() -> Self {
        Self {
            workspace: WorkspaceConfig::default(),
            defaults: ContainerDefaults::default(),
            package_sources: PackageSourceConfig::default(),
            ui: UiConfig::default(),
            advanced: AdvancedConfig::default(),
        }
    }
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            path: None, // Will use ~/.sfc
            auto_init: true,
            default_shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
            notes: Vec::new(),
        }
    }
}

impl Default for ContainerDefaults {
    fn default() -> Self {
        Self {
            packages: vec![
                "git".to_string(),
                "curl".to_string(),
                "jq".to_string(),
            ],
            environment: HashMap::new(),
            toolchains: HashMap::new(),
            auto_enter: true,
        }
    }
}

impl Default for PackageSourceConfig {
    fn default() -> Self {
        let mut preferred_managers = vec!["system".to_string()];
        
        if cfg!(target_os = "macos") {
            preferred_managers.push("homebrew".to_string());
        }
        
        preferred_managers.extend([
            "portable".to_string(),
            "nix".to_string(),
            "github".to_string(),
        ]);
        
        Self {
            preferred_managers,
            nix_fallback: true,
            portable_enabled: true,
            custom_sources: HashMap::new(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            colored: std::env::var("NO_COLOR").is_err(),
            progress_bars: true,
            animations: true,
            log_level: "info".to_string(),
        }
    }
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            stow_enabled: true,
            max_snapshots: 50,
            auto_cleanup: true,
            parallel_installs: 4,
            snapshot_storage: None,
        }
    }
}

impl SfcConfig {
    /// Load configuration from file or create default
    pub fn load<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let config_path = config_path.as_ref();
        
        if config_path.exists() {
            let content = fs::read_to_string(config_path)
                .with_io_context(|| format!("reading config file {}", config_path.display()))?;
            
            toml::from_str(&content)
                .map_err(|e| SfcError::Config {
                    message: format!("Invalid TOML: {}", e),
                    path: Some(config_path.to_path_buf()),
                })
        } else {
            Ok(Self::default())
        }
    }
    
    /// Save configuration to file
    pub fn save<P: AsRef<Path>>(&self, config_path: P) -> Result<()> {
        let config_path = config_path.as_ref();
        
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_io_context(|| format!("creating config directory {}", parent.display()))?;
        }
        
        let content = toml::to_string_pretty(self)
            .map_err(|e| SfcError::Config {
                message: format!("Failed to serialize config: {}", e),
                path: Some(config_path.to_path_buf()),
            })?;
        
        fs::write(config_path, content)
            .with_io_context(|| format!("writing config file {}", config_path.display()))?;
        
        Ok(())
    }
    
    /// Get workspace path (either configured or default ~/.sfc)
    pub fn workspace_path(&self) -> Result<PathBuf> {
        if let Some(path) = &self.workspace.path {
            Ok(path.clone())
        } else {
            let home = std::env::var("HOME")
                .map_err(|_| SfcError::Config {
                    message: "HOME environment variable not set".to_string(),
                    path: None,
                })?;
            Ok(Path::new(&home).join(".sfc"))
        }
    }
    
    /// Load global configuration
    pub fn load_global() -> Result<Self> {
        let config_path = Self::global_config_path()?;
        Self::load(config_path)
    }
    
    /// Save global configuration
    pub fn save_global(&self) -> Result<()> {
        let config_path = Self::global_config_path()?;
        self.save(config_path)
    }
    
    /// Get global configuration file path
    pub fn global_config_path() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| SfcError::Config {
                message: "HOME environment variable not set".to_string(),
                path: None,
            })?;
        Ok(Path::new(&home).join(".sfc").join("config.toml"))
    }
    
    /// Load workspace-specific configuration
    pub fn load_workspace<P: AsRef<Path>>(workspace_path: P) -> Result<Self> {
        let config_path = workspace_path.as_ref().join(".sfc").join("workspace.toml");
        Self::load(config_path)
    }
    
    /// Save workspace-specific configuration
    pub fn save_workspace<P: AsRef<Path>>(&self, workspace_path: P) -> Result<()> {
        let config_path = workspace_path.as_ref().join(".sfc").join("workspace.toml");
        self.save(config_path)
    }
    
    /// Merge workspace config with global config
    pub fn merged_config<P: AsRef<Path>>(workspace_path: P) -> Result<Self> {
        let mut global_config = Self::load_global().unwrap_or_default();
        
        if let Ok(workspace_config) = Self::load_workspace(workspace_path) {
            // Workspace config takes precedence
            global_config.workspace = workspace_config.workspace;
            global_config.defaults = workspace_config.defaults;
        }
        
        Ok(global_config)
    }
}
