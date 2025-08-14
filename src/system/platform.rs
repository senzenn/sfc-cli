use std::process::Command;
use serde::{Deserialize, Serialize};

use crate::error::{Result, SfcError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: OperatingSystem,
    pub architecture: Architecture,
    pub package_managers: Vec<PackageManager>,
    pub preferred_package_manager: Option<PackageManager>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperatingSystem {
    MacOS,
    Linux,
    Windows,
    FreeBSD,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Architecture {
    X86_64,
    Aarch64,
    X86,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PackageManager {
    // macOS
    Homebrew,
    MacPorts,
    
    // Linux - Debian/Ubuntu
    Apt,
    AptGet,
    
    // Linux - Red Hat/Fedora
    Dnf,
    Yum,
    
    // Linux - Arch
    Pacman,
    
    // Linux - openSUSE
    Zypper,
    
    // Linux - Alpine
    Apk,
    
    // Cross-platform
    Nix,
    Snap,
    Flatpak,
    
    // Language-specific
    Volta,
    Rustup,
    Pyenv,
}

impl PlatformInfo {
    pub fn detect() -> Self {
        let os = detect_os();
        let architecture = detect_architecture();
        let package_managers = detect_available_package_managers();
        let preferred_package_manager = determine_preferred_package_manager(&os, &package_managers);
        
        Self {
            os,
            architecture,
            package_managers,
            preferred_package_manager,
        }
    }
    
    pub fn get_install_command(&self, package: &str) -> Option<Vec<String>> {
        if let Some(pm) = &self.preferred_package_manager {
            Some(pm.get_install_command(package))
        } else {
            None
        }
    }
    
    pub fn get_search_command(&self, query: &str) -> Option<Vec<String>> {
        if let Some(pm) = &self.preferred_package_manager {
            Some(pm.get_search_command(query))
        } else {
            None
        }
    }
    pub fn has_package_manager(&self, pm: &PackageManager) -> bool {
        self.package_managers.contains(pm)
    }
}

impl PackageManager {
    pub fn get_install_command(&self, package: &str) -> Vec<String> {
        match self {
            PackageManager::Homebrew => vec!["brew".to_string(), "install".to_string(), package.to_string()],
            PackageManager::MacPorts => vec!["sudo".to_string(), "port".to_string(), "install".to_string(), package.to_string()],
            
            PackageManager::Apt => vec!["sudo".to_string(), "apt".to_string(), "install".to_string(), "-y".to_string(), package.to_string()],
            PackageManager::AptGet => vec!["sudo".to_string(), "apt-get".to_string(), "install".to_string(), "-y".to_string(), package.to_string()],
            
            PackageManager::Dnf => vec!["sudo".to_string(), "dnf".to_string(), "install".to_string(), "-y".to_string(), package.to_string()],
            PackageManager::Yum => vec!["sudo".to_string(), "yum".to_string(), "install".to_string(), "-y".to_string(), package.to_string()],
            
            PackageManager::Pacman => vec!["sudo".to_string(), "pacman".to_string(), "-S".to_string(), "--noconfirm".to_string(), package.to_string()],
            PackageManager::Zypper => vec!["sudo".to_string(), "zypper".to_string(), "install".to_string(), "-y".to_string(), package.to_string()],
            PackageManager::Apk => vec!["sudo".to_string(), "apk".to_string(), "add".to_string(), package.to_string()],
            
            PackageManager::Nix => vec!["nix".to_string(), "profile".to_string(), "install".to_string(), format!("nixpkgs#{}", package)],
            PackageManager::Snap => vec!["sudo".to_string(), "snap".to_string(), "install".to_string(), package.to_string()],
            PackageManager::Flatpak => vec!["flatpak".to_string(), "install".to_string(), "-y".to_string(), package.to_string()],
            
            PackageManager::Volta => vec!["volta".to_string(), "install".to_string(), package.to_string()],
            PackageManager::Rustup => vec!["rustup".to_string(), "toolchain".to_string(), "install".to_string(), package.to_string()],
            PackageManager::Pyenv => vec!["pyenv".to_string(), "install".to_string(), package.to_string()],
        }
    }
    
    /// Get the command to search for packages
    pub fn get_search_command(&self, query: &str) -> Vec<String> {
        match self {
            PackageManager::Homebrew => vec!["brew".to_string(), "search".to_string(), query.to_string()],
            PackageManager::MacPorts => vec!["port".to_string(), "search".to_string(), query.to_string()],
            
            PackageManager::Apt => vec!["apt".to_string(), "search".to_string(), query.to_string()],
            PackageManager::AptGet => vec!["apt-cache".to_string(), "search".to_string(), query.to_string()],
            
            PackageManager::Dnf => vec!["dnf".to_string(), "search".to_string(), query.to_string()],
            PackageManager::Yum => vec!["yum".to_string(), "search".to_string(), query.to_string()],
            
            PackageManager::Pacman => vec!["pacman".to_string(), "-Ss".to_string(), query.to_string()],
            PackageManager::Zypper => vec!["zypper".to_string(), "search".to_string(), query.to_string()],
            PackageManager::Apk => vec!["apk".to_string(), "search".to_string(), query.to_string()],
            
            PackageManager::Nix => vec!["nix".to_string(), "search".to_string(), "nixpkgs".to_string(), query.to_string()],
            PackageManager::Snap => vec!["snap".to_string(), "find".to_string(), query.to_string()],
            PackageManager::Flatpak => vec!["flatpak".to_string(), "search".to_string(), query.to_string()],
            
            _ => vec!["echo".to_string(), "Search not supported for this package manager".to_string()],
        }
    }
    
    /// Get the binary name for this package manager
    pub fn binary_name(&self) -> &'static str {
        match self {
            PackageManager::Homebrew => "brew",
            PackageManager::MacPorts => "port",
            PackageManager::Apt => "apt",
            PackageManager::AptGet => "apt-get",
            PackageManager::Dnf => "dnf", 
            PackageManager::Yum => "yum",
            PackageManager::Pacman => "pacman",
            PackageManager::Zypper => "zypper",
            PackageManager::Apk => "apk",
            PackageManager::Nix => "nix",
            PackageManager::Snap => "snap",
            PackageManager::Flatpak => "flatpak",
            PackageManager::Volta => "volta",
            PackageManager::Rustup => "rustup",
            PackageManager::Pyenv => "pyenv",
        }
    }
}

/// Detect the current operating system
pub fn detect_os() -> OperatingSystem {
    match std::env::consts::OS {
        "macos" => OperatingSystem::MacOS,
        "linux" => OperatingSystem::Linux,
        "windows" => OperatingSystem::Windows,
        "freebsd" => OperatingSystem::FreeBSD,
        other => OperatingSystem::Unknown(other.to_string()),
    }
}

/// Detect the current architecture
pub fn detect_architecture() -> Architecture {
    match std::env::consts::ARCH {
        "x86_64" => Architecture::X86_64,
        "aarch64" => Architecture::Aarch64,
        "x86" => Architecture::X86,
        other => Architecture::Unknown(other.to_string()),
    }
}

/// Detect current platform
pub fn detect_platform() -> PlatformInfo {
    PlatformInfo::detect()
}

/// Detect available package managers on the system
pub fn detect_available_package_managers() -> Vec<PackageManager> {
    let candidates = vec![
        PackageManager::Homebrew,
        PackageManager::MacPorts,
        PackageManager::Apt,
        PackageManager::AptGet,
        PackageManager::Dnf,
        PackageManager::Yum,
        PackageManager::Pacman,
        PackageManager::Zypper,
        PackageManager::Apk,
        PackageManager::Nix,
        PackageManager::Snap,
        PackageManager::Flatpak,
        PackageManager::Volta,
        PackageManager::Rustup,
        PackageManager::Pyenv,
    ];
    
    candidates.into_iter()
        .filter(|pm| is_command_available(pm.binary_name()))
        .collect()
}

/// Detect the best package manager for the current platform
pub fn detect_package_manager() -> Result<PackageManager> {
    let platform = detect_platform();
    
    if let Some(pm) = platform.preferred_package_manager {
        Ok(pm)
    } else {
        Err(SfcError::NotFound {
            resource: "package manager".to_string(),
            identifier: "no suitable package manager found".to_string(),
        })
    }
}

/// Determine the preferred package manager based on OS and available managers
fn determine_preferred_package_manager(
    os: &OperatingSystem, 
    available: &[PackageManager]
) -> Option<PackageManager> {
    match os {
        OperatingSystem::MacOS => {
            // Prefer Homebrew on macOS
            if available.contains(&PackageManager::Homebrew) {
                Some(PackageManager::Homebrew)
            } else if available.contains(&PackageManager::MacPorts) {
                Some(PackageManager::MacPorts)
            } else {
                available.iter().find(|pm| matches!(pm, PackageManager::Nix)).cloned()
            }
        }
        OperatingSystem::Linux => {
            // Prefer system package manager on Linux
            if available.contains(&PackageManager::Apt) {
                Some(PackageManager::Apt)
            } else if available.contains(&PackageManager::AptGet) {
                Some(PackageManager::AptGet)
            } else if available.contains(&PackageManager::Dnf) {
                Some(PackageManager::Dnf)
            } else if available.contains(&PackageManager::Yum) {
                Some(PackageManager::Yum)
            } else if available.contains(&PackageManager::Pacman) {
                Some(PackageManager::Pacman)
            } else if available.contains(&PackageManager::Zypper) {
                Some(PackageManager::Zypper)
            } else if available.contains(&PackageManager::Apk) {
                Some(PackageManager::Apk)
            } else {
                available.iter().find(|pm| matches!(pm, PackageManager::Nix)).cloned()
            }
        }
        OperatingSystem::Windows => {
            // On Windows, prefer Nix or other cross-platform managers
            available.iter()
                .find(|pm| matches!(pm, PackageManager::Nix))
                .cloned()
        }
        _ => {
            // For other/unknown OS, try to find any available manager
            available.first().cloned()
        }
    }
}

/// Check if a command is available in PATH
fn is_command_available(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or_else(|_| {
            // Fallback: try to run the command with --version or --help
            Command::new(command)
                .arg("--version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_os() {
        let os = detect_os();
        assert!(!matches!(os, OperatingSystem::Unknown(_)));
    }
    
    #[test]
    fn test_detect_architecture() {
        let arch = detect_architecture();
        assert!(!matches!(arch, Architecture::Unknown(_)));
    }
    
    #[test]
    fn test_platform_detection() {
        let platform = detect_platform();
        assert!(!matches!(platform.os, OperatingSystem::Unknown(_)));
        assert!(!matches!(platform.architecture, Architecture::Unknown(_)));
    }
    
    #[test]
    fn test_package_manager_commands() {
        let pm = PackageManager::Homebrew;
        let install_cmd = pm.get_install_command("git");
        assert_eq!(install_cmd, vec!["brew", "install", "git"]);
        
        let search_cmd = pm.get_search_command("git");
        assert_eq!(search_cmd, vec!["brew", "search", "git"]);
    }
    
    #[test]
    fn test_available_package_managers() {
        let managers = detect_available_package_managers();
        // We can't assert specific managers since it depends on the test environment,
        // but we can ensure the detection doesn't crash
        assert!(managers.len() >= 0);
    }
}
