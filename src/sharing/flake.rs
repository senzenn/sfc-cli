use std::fs;
use std::path::Path;

use crate::core::WorkspaceManager;
use crate::error::{Result, ErrorContext};
use crate::sharing::ShareInfo;

/// Manages Nix flake generation and sharing
pub struct FlakeManager {
    workspace: WorkspaceManager,
}

impl FlakeManager {
    pub fn new(workspace: WorkspaceManager) -> Self {
        Self { workspace }
    }
    
    pub fn generate_flake(&self, container_name: &str) -> Result<String> {
        let container_config_path = self.workspace.root
            .join(".sfc")
            .join("containers")
            .join(format!("{}.toml", container_name));
        
        if !container_config_path.exists() {
            return Ok(self.generate_minimal_flake(container_name));
        }
        
        let config_content = fs::read_to_string(&container_config_path)
            .with_io_context(|| format!("reading container config {}", container_config_path.display()))?;
        
        self.generate_flake_from_config(container_name, &config_content)
    }
    
    pub fn save_flake(&self, container_name: &str, flake_content: &str) -> Result<()> {
        let container_dir = self.workspace.root.join("containers").join(container_name);
        let flake_path = container_dir.join("flake.nix");
        
        fs::write(&flake_path, flake_content)
            .with_io_context(|| format!("writing flake to {}", flake_path.display()))?;
        
        Ok(())
    }
    
    pub fn generate_flake_from_share(&self, share_info: &ShareInfo) -> Result<String> {
        let mut packages = Vec::new();
        
        for package in &share_info.packages {
            let package_name = match package.source.as_str() {
                "nixpkgs" => package.name.clone(),
                _ => continue, // Skip non-Nix packages
            };
            packages.push(package_name);
        }
        
        Ok(self.build_flake_content(&share_info.container_name, &packages))
    }
    
    fn generate_minimal_flake(&self, container_name: &str) -> String {
        let packages = vec!["git".to_string(), "curl".to_string()];
        self.build_flake_content(container_name, &packages)
    }
    
    fn generate_flake_from_config(&self, container_name: &str, config_content: &str) -> Result<String> {
        let mut packages = Vec::new();
        let mut in_packages_section = false;
        let mut current_package_name = None;
        
        for line in config_content.lines() {
            let line = line.trim();
            
            if line == "[[packages]]" {
                in_packages_section = true;
                current_package_name = None;
            } else if line.starts_with('[') && line != "[[packages]]" {
                in_packages_section = false;
            } else if in_packages_section && line.starts_with("name = ") {
                if let Some(name) = self.extract_toml_string_value(line) {
                    current_package_name = Some(name);
                }
            } else if in_packages_section && line.starts_with("source = ") {
                if let Some(source) = self.extract_toml_string_value(line) {
                    if source == "nixpkgs" || source == "Nixpkgs" {
                        if let Some(name) = current_package_name.take() {
                            packages.push(name);
                        }
                    }
                }
            }
        }
        
        Ok(self.build_flake_content(container_name, &packages))
    }
    
    fn build_flake_content(&self, container_name: &str, packages: &[String]) -> String {
        let mut flake = String::new();
        
        flake.push_str("{\n");
        flake.push_str("  description = \"SFC development environment\";\n\n");
        
        flake.push_str("  inputs = {\n");
        flake.push_str("    nixpkgs.url = \"github:NixOS/nixpkgs/nixos-unstable\";\n");
        flake.push_str("    flake-utils.url = \"github:numtide/flake-utils\";\n");
        flake.push_str("  };\n\n");
        
        flake.push_str("  outputs = { self, nixpkgs, flake-utils }:\n");
        flake.push_str("    flake-utils.lib.eachDefaultSystem (system:\n");
        flake.push_str("      let\n");
        flake.push_str("        pkgs = nixpkgs.legacyPackages.${system};\n");
        flake.push_str("      in\n");
        flake.push_str("      {\n");
        flake.push_str(&format!("        devShells.{} = pkgs.mkShell {{\n", container_name));
        flake.push_str("          packages = with pkgs; [\n");
        
        for package in packages {
            flake.push_str(&format!("            {}\n", package));
        }
        
        flake.push_str("          ];\n\n");
        flake.push_str("          shellHook = ''\n");
        flake.push_str(&format!("            echo \"Welcome to {} development environment!\"\n", container_name));
        flake.push_str("          '';\n");
        flake.push_str("        };\n");
        flake.push_str("      });\n");
        flake.push_str("}\n");
        
        flake
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
}

/// Convenience function
pub fn generate_nix_flake(workspace: &WorkspaceManager, container_name: &str) -> Result<String> {
    let flake_manager = FlakeManager::new(workspace.clone());
    flake_manager.generate_flake(container_name)
}
