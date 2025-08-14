use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{anyhow, Context, Result};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use chrono::{DateTime, Utc};

use crate::history::HistoryEntry;
use crate::flake::FlakeConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub packages: Vec<PackageSpec>,
    pub environment: std::collections::HashMap<String, String>,
    pub shell: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSpec {
    pub name: String,
    pub version: Option<String>,
    pub channel: Option<String>, // stable, unstable, etc
    pub source: PackageSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackageSource {
    Nixpkgs,
    GitHub { repo: String, rev: String },
    Url(String),
}

impl ContainerConfig {
    pub fn new(name: String) -> Self {
        // Detect user's current shell
        let current_shell = std::env::var("SHELL")
            .unwrap_or_else(|_| "/bin/bash".to_string());
        
        Self {
            name,
            created_at: Utc::now(),
            packages: Vec::new(),
            environment: std::collections::HashMap::new(),
            shell: current_shell,
        }
    }

    pub fn compute_hash(&self) -> Result<String> {
        let mut hasher = Sha256::new();
        let serialized = serde_json::to_string(self)?;
        hasher.update(serialized.as_bytes());
        let digest = hasher.finalize();
        Ok(format!("{:x}", digest)[..16].to_string()) // Short hash
    }

    pub fn add_package(&mut self, spec: PackageSpec) -> Result<()> {
        // Remove existing package with same name
        self.packages.retain(|p| p.name != spec.name);
        self.packages.push(spec);
        self.packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    pub fn remove_package(&mut self, name: &str) -> Result<bool> {
        let len_before = self.packages.len();
        self.packages.retain(|p| p.name != name);
        Ok(self.packages.len() < len_before)
    }

    pub fn save(&self, workspace: &Path) -> Result<()> {
        let config_dir = workspace.join(".sfc").join("containers");
        fs::create_dir_all(&config_dir)?;
        let config_file = config_dir.join(format!("{}.toml", self.name));
        let content = toml::to_string_pretty(self)?;
        fs::write(&config_file, content)?;
        Ok(())
    }

    pub fn load(workspace: &Path, name: &str) -> Result<Self> {
        let config_file = workspace.join(".sfc").join("containers").join(format!("{}.toml", name));
        if !config_file.exists() {
            return Ok(Self::new(name.to_string()));
        }
        let content = fs::read_to_string(&config_file)?;
        let mut config: ContainerConfig = toml::from_str(&content)?;
        config.name = name.to_string(); // Ensure name matches
        Ok(config)
    }

    pub fn enter_shell(&self, workspace: &Path) -> Result<()> {
        let container_dir = workspace.join("containers").join(&self.name);
        fs::create_dir_all(&container_dir)?;

        // Build environment
        let mut env = self.environment.clone();
        env.insert("SFC_CONTAINER".to_string(), self.name.clone());
        env.insert("SFC_WORKSPACE".to_string(), workspace.to_string_lossy().to_string());
        
        // Set PS1 to show container
        let ps1 = format!("\\[\\033[32m\\]sfc[{}]\\[\\033[0m\\] \\w $ ", self.name);
        env.insert("PS1".to_string(), ps1);

        println!("{} {}", "Entering container".green(), self.name.cyan());
        let package_names: Vec<String> = self.packages.iter().map(|p| p.name.clone()).collect();
        println!("{} packages: {}", "Active".dimmed(), package_names.join(", "));

        // Spawn shell in container directory
        let mut cmd = Command::new(&self.shell);
        cmd.current_dir(&container_dir);
        for (k, v) in env {
            cmd.env(k, v);
        }
        
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow!("Shell exited with non-zero status"));
        }
        Ok(())
    }

    pub fn to_flake(&self) -> FlakeConfig {
        FlakeConfig::from_container(self)
    }
}

impl PackageSpec {
    pub fn from_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            version: None,
            channel: Some("stable".to_string()),
            source: PackageSource::Nixpkgs,
        }
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }

    pub fn with_channel(mut self, channel: &str) -> Self {
        self.channel = Some(channel.to_string());
        self
    }
}
