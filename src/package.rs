use std::process::Command;
use std::thread;
use std::time::Duration;
use anyhow::{anyhow, Result};
use owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle};
use crossterm::{
    execute,
    style::{Color as CtColor, SetForegroundColor, ResetColor, Print, SetBackgroundColor},
};
use std::io::stdout;

use crate::container::{ContainerConfig, PackageSpec, PackageSource};
use crate::history::{History, Operation};

pub struct PackageManager {
    workspace: std::path::PathBuf,
}

struct SystemPMInfo {
    name: String,
    emoji: &'static str,
}

impl PackageManager {
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self { workspace }
    }

    pub fn add_package(&self, container: &mut ContainerConfig, package_spec: &str) -> Result<String> {
        let spec = self.parse_package_spec(package_spec)?;
        
        // Dramatic installation header
        self.print_installation_header(&spec);
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.magenta} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▰▱▱▱▱▱▱▱▱▱", 
                    "▰▰▱▱▱▱▱▱▱▱", 
                    "▰▰▰▱▱▱▱▱▱▱",
                    "▰▰▰▰▱▱▱▱▱▱", 
                    "▰▰▰▰▰▱▱▱▱▱",
                    "▰▰▰▰▰▰▱▱▱▱",
                    "▰▰▰▰▰▰▰▱▱▱", 
                    "▰▰▰▰▰▰▰▰▱▱", 
                    "▰▰▰▰▰▰▰▰▰▱", 
                    "▰▰▰▰▰▰▰▰▰▰"
                ])
        );
        pb.set_message(format!("Initializing {}...", spec.name));

        // Create container's package directory
        let container_dir = self.workspace.join("containers").join(&container.name);
        let pkg_dir = container_dir.join("packages");
        std::fs::create_dir_all(&pkg_dir)?;

        // Install package using available package manager
        pb.set_message("Downloading and installing...");
        let installed = self.install_package_real(&spec, &pkg_dir)?;
        
        if !installed {
            pb.finish_and_clear();
            return Err(anyhow!("❌ Failed to install package '{}'", spec.name));
        }

        let old_version = container.packages
            .iter()
            .find(|p| p.name == spec.name)
            .and_then(|p| p.version.clone());

        pb.set_message("Updating container configuration...");
        container.add_package(spec.clone())?;
        container.save(&self.workspace)?;

        // Update container's environment using Stow or direct PATH management
        pb.set_message("Setting up symlinks and environment...");
        self.update_container_paths(container, &spec, &pkg_dir)?;

        // Update flake
        pb.set_message("Generating Nix flake...");
        let flake = container.to_flake();
        flake.save(&self.workspace, &container.name)?;

        pb.finish_and_clear();

        // Record in history
        let mut history = History::load(&self.workspace)?;
        let operation = if old_version.is_some() {
            Operation::ModifyPackage {
                name: spec.name.clone(),
                old_version,
                new_version: spec.version.clone(),
            }
        } else {
            Operation::AddPackage {
                name: spec.name.clone(),
                version: spec.version.clone(),
            }
        };

        let message = if let Some(version) = &spec.version {
            format!("Install {}@{}", spec.name, version)
        } else {
            format!("Install {}", spec.name)
        };

        let hash = history.add_entry(container, operation, message)?;
        
        let version_display = if let Some(v) = &spec.version { 
            format!("@{}", v).dimmed().to_string() 
        } else { 
            String::new()
        };
        
        self.print_success_celebration(&spec, &hash);

        Ok(hash)
    }

    pub fn remove_package(&self, container: &mut ContainerConfig, package_name: &str) -> Result<String> {
        if !container.remove_package(package_name)? {
            return Err(anyhow!("❌ Package '{}' not found in container", package_name));
        }

        println!("🗑️  Removing package {}", package_name.red().bold());
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.red} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▰▱▱▱▱▱▱▱▱▱ REMOVING", 
                    "▰▰▱▱▱▱▱▱▱▱ REMOVING", 
                    "▰▰▰▱▱▱▱▱▱▱ REMOVING",
                    "▰▰▰▰▱▱▱▱▱▱ REMOVING", 
                    "▰▰▰▰▰▱▱▱▱▱ REMOVING",
                    "▰▰▰▰▰▰▱▱▱▱ REMOVING",
                    "▰▰▰▰▰▰▰▱▱▱ REMOVING", 
                    "▰▰▰▰▰▰▰▰▱▱ REMOVING", 
                    "▰▰▰▰▰▰▰▰▰▱ REMOVING", 
                    "▰▰▰▰▰▰▰▰▰▰ COMPLETE"
                ])
        );
        pb.set_message(format!("Cleaning up {}...", package_name));

        container.save(&self.workspace)?;

        pb.set_message("Updating configuration...");
        // Update flake
        let flake = container.to_flake();
        flake.save(&self.workspace, &container.name)?;

        pb.set_message("Recording changes...");
        // Record in history
        let mut history = History::load(&self.workspace)?;
        let hash = history.add_entry(
            container,
            Operation::RemovePackage { name: package_name.to_string() },
            format!("Remove {}", package_name),
        )?;

        pb.finish_and_clear();
        println!("{} {} {} {}",
                 "✅".green(),
                 "Successfully removed".green().bold(), 
                 package_name.cyan().bold(),
                 format!("({})", hash.bright_yellow()).dimmed());
        Ok(hash)
    }

    pub fn list_packages(&self, container: &ContainerConfig) -> Result<()> {
        if container.packages.is_empty() {
            println!("📦 {} {}", "Container is empty".yellow().bold(), "- no packages installed yet".dimmed());
            println!("   Try: {} to add packages", "sfc add <package>".cyan());
            return Ok(());
        }

        println!("📦 {} ({} packages)", 
                "Installed packages".bold().green(),
                container.packages.len().to_string().cyan().bold());
        println!("");
        
        for (i, pkg) in container.packages.iter().enumerate() {
            let version_str = if let Some(version) = &pkg.version {
                format!("@{}", version).bright_blue().to_string()
            } else {
                "@latest".dimmed().to_string()
            };

            let (source_emoji, source_str) = match &pkg.source {
                PackageSource::Nixpkgs => ("❄️", "nixpkgs".green().to_string()),
                PackageSource::GitHub { repo, .. } => ("📂", format!("github:{}", repo).blue().to_string()),
                PackageSource::Url(_) => ("🌐", "url".yellow().to_string()),
            };

            println!("   {} {} {} {} [{}{}]", 
                    format!("{:2}.", i + 1).dimmed(),
                    source_emoji,
                    pkg.name.cyan().bold(), 
                    version_str,
                    source_emoji, 
                    source_str);
        }

        println!("");
        println!("💡 {} {} | {} {}", 
                "Use".dimmed(),
                "sfc remove <package>".cyan(),
                "sfc search <query>".cyan(),
                "to modify packages".dimmed());

        Ok(())
    }

    pub fn search_packages(&self, query: &str) -> Result<()> {
        println!("🔍 Searching for packages matching '{}'...", query.cyan().bold());
        
        let pm_info = self.get_system_pm_info();
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.cyan} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "🔍", "🔎", "👀", "🕵️", "🔍", "🔎", "👀", "🕵️"
                ])
        );
        pb.set_message(format!("Searching {} for '{}'...", pm_info.name, query));

        if cfg!(target_os = "macos") && self.which("brew") {
            self.search_with_homebrew(query, &pb)
        } else if cfg!(target_os = "linux") {
            self.search_with_linux_pm(query, &pb)
        } else {
            pb.finish_and_clear();
            println!("💡 {} Install packages directly with:", "Search not available.".yellow());
            println!("   {} {}", "sfc add".cyan(), "<package-name>".yellow());
            Ok(())
        }
    }

    fn search_with_homebrew(&self, query: &str, pb: &ProgressBar) -> Result<()> {
        let output = Command::new("brew")
            .args(&["search", query])
            .output();

        pb.finish_and_clear();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    println!("📭 {} '{}'", "No packages found for".yellow(), query.red());
                    return Ok(());
                }

                let packages: Vec<&str> = stdout.lines().take(10).collect();
                println!("🎯 {} ({} results)", 
                        "Homebrew search results".bold().green(),
                        packages.len().to_string().cyan().bold());
                println!("");
                
                for (i, package) in packages.iter().enumerate() {
                    println!("   {} 🍺 {}", 
                             format!("{:2}.", i + 1).dimmed(),
                             package.cyan().bold());
                }
                
                println!("\n🚀 {} {}", 
                        "Install with:".dimmed(),
                        "sfc add <package-name>".cyan().bold());
            }
            Ok(_) => {
                println!("❌ {} Make sure Homebrew is installed.", "Search failed.".red());
            }
            Err(_) => {
                println!("❌ {}", "Failed to execute brew search.".red());
            }
        }
        Ok(())
    }

    fn search_with_linux_pm(&self, query: &str, pb: &ProgressBar) -> Result<()> {
        if self.which("apt") {
            self.search_with_apt(query, pb)
        } else {
            pb.finish_and_clear();
            println!("💡 {} Your system's package manager doesn't support search through SFC.", "Search limited.".yellow());
            println!("   Try searching manually: {} {} {}", 
                    "sudo".dimmed(), 
                    "apt search".cyan(), 
                    query.yellow());
            Ok(())
        }
    }

    fn search_with_apt(&self, query: &str, pb: &ProgressBar) -> Result<()> {
        let output = Command::new("apt")
            .args(&["search", query])
            .output();

        pb.finish_and_clear();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let packages: Vec<&str> = stdout.lines()
                    .filter(|line| !line.starts_with("WARNING") && !line.starts_with("NOTE"))
                    .take(10)
                    .collect();

                if packages.is_empty() {
                    println!("📭 {} '{}'", "No packages found for".yellow(), query.red());
                    return Ok(());
                }
                
                println!("🎯 {} ({} results)", 
                        "APT search results".bold().green(),
                        packages.len().to_string().cyan().bold());
                println!("");
                
                for (i, package_line) in packages.iter().enumerate() {
                    if let Some(package_name) = package_line.split('/').next() {
                        println!("   {} 📦 {}", 
                                 format!("{:2}.", i + 1).dimmed(),
                                 package_name.cyan().bold());
                    }
                }
                
                println!("\n🚀 {} {}", 
                        "Install with:".dimmed(),
                        "sfc add <package-name>".cyan().bold());
            }
            Ok(_) => {
                println!("❌ {} Make sure APT is available.", "Search failed.".red());
            }
            Err(_) => {
                println!("❌ {}", "Failed to execute apt search.".red());
            }
        }
        Ok(())
    }

    fn parse_package_spec(&self, spec: &str) -> Result<PackageSpec> {
        if spec.contains("github:") {
            // GitHub source: github:owner/repo@rev
            let parts: Vec<&str> = spec.split('@').collect();
            let repo = parts[0].strip_prefix("github:").unwrap_or(parts[0]);
            let rev = parts.get(1).unwrap_or(&"main").to_string();
            
            let name = repo.split('/').last().unwrap_or(repo).to_string();
            
            return Ok(PackageSpec {
                name,
                version: None,
                channel: None,
                source: PackageSource::GitHub { 
                    repo: repo.to_string(), 
                    rev 
                },
            });
        }

        if spec.starts_with("http") {
            // URL source
            let name = spec.split('/').last().unwrap_or(spec).to_string();
            return Ok(PackageSpec {
                name,
                version: None,
                channel: None,
                source: PackageSource::Url(spec.to_string()),
            });
        }

        // Default nixpkgs source: package@version
        let parts: Vec<&str> = spec.split('@').collect();
        let name = parts[0].to_string();
        let version = parts.get(1).map(|v| v.to_string());

        Ok(PackageSpec {
            name,
            version,
            channel: Some("stable".to_string()),
            source: PackageSource::Nixpkgs,
        })
    }

    fn install_package_real(&self, spec: &PackageSpec, pkg_dir: &std::path::Path) -> Result<bool> {
        match &spec.source {
            PackageSource::Nixpkgs => {
                // Prioritize system package managers over Nix
                // Try in order: System PM -> Homebrew -> Portable -> Nix (last resort)
                
                if cfg!(target_os = "macos") {
                    // macOS: Homebrew first, then portable
                    Ok(self.install_with_homebrew(spec, pkg_dir)
                        .or_else(|_| self.install_portable_version(spec, pkg_dir))
                        .unwrap_or(false))
                } else if cfg!(target_os = "linux") {
                    // Linux: System package managers first
                    Ok(self.install_with_linux_pm(spec, pkg_dir)
                        .or_else(|_| self.install_portable_version(spec, pkg_dir))
                        .unwrap_or(false))
                } else {
                    // Other systems: Try portable first
                    Ok(self.install_portable_version(spec, pkg_dir)
                        .unwrap_or(false))
                }
            }
            PackageSource::GitHub { repo, rev } => {
                self.install_from_github(repo, rev, pkg_dir)
            }
            PackageSource::Url(url) => {
                self.install_from_url(url, pkg_dir)
            }
        }
    }

    fn install_with_nix(&self, spec: &PackageSpec, pkg_dir: &std::path::Path) -> Result<bool> {
        if !self.which("nix") {
            return Err(anyhow!("nix not available"));
        }

        // Fix Nix package naming - versions don't work with @ syntax in Nix
        let package_name = match &spec.version {
            Some(version) => {
                // For Node.js, map versions to Nix packages
                match spec.name.as_str() {
                    "nodejs" | "node" => {
                        match version.as_str() {
                            "18" | "18.17.0" | "18.x" => "nodejs_18".to_string(),
                            "20" | "20.5.0" | "20.x" => "nodejs_20".to_string(),
                            "16" | "16.x" => "nodejs_16".to_string(),
                            "14" | "14.x" => "nodejs_14".to_string(),
                            _ => "nodejs".to_string(), // Default to latest
                        }
                    }
                    "python3" | "python" => {
                        match version.as_str() {
                            "3.11" | "3.11.0" => "python311".to_string(),
                            "3.10" | "3.10.0" => "python310".to_string(),
                            "3.9" | "3.9.0" => "python39".to_string(),
                            "3.8" | "3.8.0" => "python38".to_string(),
                            _ => "python3".to_string(), // Default to latest
                        }
                    }
                    _ => spec.name.clone(), // For other packages, ignore version
                }
            }
            None => spec.name.clone(),
        };

        let display_name = if let Some(version) = &spec.version {
            format!("{}@{}", spec.name, version)
        } else {
            spec.name.clone()
        };

        println!("❄️  Installing {} with Nix...", display_name.cyan());
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "❄ ", "❅ ", "❆ ", "❇ ", "❈ ", "❉ ", "❊ ", "❋ "
                ])
        );
        pb.set_message(format!("Installing nixpkgs#{}", package_name));

        let output = Command::new("nix")
            .args(&["profile", "install", &format!("nixpkgs#{}", package_name), "--profile", &pkg_dir.join("nix-profile").to_string_lossy()])
            .output()?;

        pb.finish_and_clear();
        
        if output.status.success() {
            println!("{} Nix installation complete", "✓".green());
        } else {
            println!("{} Nix installation failed", "✗".red());
            // Print error details for debugging
            if !output.stderr.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("{} Error: {}", "Debug:".yellow(), stderr.trim());
            }
        }

        Ok(output.status.success())
    }

    fn install_with_homebrew(&self, spec: &PackageSpec, _pkg_dir: &std::path::Path) -> Result<bool> {
        if !self.which("brew") {
            return Err(anyhow!("homebrew not available"));
        }

        // Map common package names to homebrew formulas with version support
        let brew_name = self.map_to_brew_name_with_version(&spec.name, spec.version.as_deref());
        
        let display_name = if let Some(version) = &spec.version {
            format!("{}@{}", spec.name, version)
        } else {
            spec.name.clone()
        };
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Yellow),
            Print("🍺 Installing "),
            SetForegroundColor(CtColor::Cyan),
            Print(&display_name),
            SetForegroundColor(CtColor::Yellow),
            Print(" with Homebrew...\n"),
            ResetColor
        );
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.yellow} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▰▱▱▱▱▱▱▱▱▱ BREW", 
                    "▰▰▱▱▱▱▱▱▱▱ BREW", 
                    "▰▰▰▱▱▱▱▱▱▱ BREW",
                    "▰▰▰▰▱▱▱▱▱▱ BREW", 
                    "▰▰▰▰▰▱▱▱▱▱ BREW",
                    "▰▰▰▰▰▰▱▱▱▱ BREW",
                    "▰▰▰▰▰▰▰▱▱▱ BREW", 
                    "▰▰▰▰▰▰▰▰▱▱ BREW", 
                    "▰▰▰▰▰▰▰▰▰▱ BREW", 
                    "▰▰▰▰▰▰▰▰▰▰ DONE"
                ])
        );
        pb.set_message(format!("brew install {}", brew_name));
        
        let output = Command::new("brew")
            .args(&["install", &brew_name])
            .output()?;

        pb.finish_and_clear();
        
        if output.status.success() {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Green),
                Print("✅ Homebrew installation complete\n"),
                ResetColor
            );
        } else {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Red),
                Print("❌ Homebrew installation failed\n"),
                ResetColor
            );
            // Print error details for debugging
            if !output.stderr.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let _ = execute!(
                    stdout(),
                    SetForegroundColor(CtColor::Yellow),
                    Print(&format!("Debug: {}\n", stderr.trim())),
                    ResetColor
                );
            }
        }

        Ok(output.status.success())
    }

    fn install_with_apt(&self, spec: &PackageSpec, _pkg_dir: &std::path::Path) -> Result<bool> {
        let package_name = self.map_to_apt_name(&spec.name);
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Blue),
            Print("📦 Installing "),
            SetForegroundColor(CtColor::Cyan),
            Print(&spec.name),
            SetForegroundColor(CtColor::Blue),
            Print(" with APT...\n"),
            ResetColor
        );
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▰▱▱▱▱▱▱▱▱▱ APT", 
                    "▰▰▱▱▱▱▱▱▱▱ APT", 
                    "▰▰▰▱▱▱▱▱▱▱ APT",
                    "▰▰▰▰▱▱▱▱▱▱ APT", 
                    "▰▰▰▰▰▱▱▱▱▱ APT",
                    "▰▰▰▰▰▰▱▱▱▱ APT",
                    "▰▰▰▰▰▰▰▱▱▱ APT", 
                    "▰▰▰▰▰▰▰▰▱▱ APT", 
                    "▰▰▰▰▰▰▰▰▰▱ APT", 
                    "▰▰▰▰▰▰▰▰▰▰ DONE"
                ])
        );
        pb.set_message(format!("sudo apt install {}", package_name));
        
        // First update package list
        let update_output = Command::new("sudo")
            .args(&["apt", "update"])
            .output()?;
            
        if !update_output.status.success() {
            pb.finish_and_clear();
            return Ok(false);
        }
        
        let output = Command::new("sudo")
            .args(&["apt", "install", "-y", &package_name])
            .output()?;

        pb.finish_and_clear();
        
        if output.status.success() {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Green),
                Print("✅ APT installation complete\n"),
                ResetColor
            );
        } else {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Red),
                Print("❌ APT installation failed\n"),
                ResetColor
            );
        }

        Ok(output.status.success())
    }

    fn install_with_yum(&self, spec: &PackageSpec, _pkg_dir: &std::path::Path) -> Result<bool> {
        let package_name = self.map_to_yum_name(&spec.name);
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Red),
            Print("🔴 Installing "),
            SetForegroundColor(CtColor::Cyan),
            Print(&spec.name),
            SetForegroundColor(CtColor::Red),
            Print(" with YUM...\n"),
            ResetColor
        );
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.red} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▰▱▱▱▱▱▱▱▱▱ YUM", 
                    "▰▰▱▱▱▱▱▱▱▱ YUM", 
                    "▰▰▰▱▱▱▱▱▱▱ YUM",
                    "▰▰▰▰▱▱▱▱▱▱ YUM", 
                    "▰▰▰▰▰▱▱▱▱▱ YUM",
                    "▰▰▰▰▰▰▱▱▱▱ YUM",
                    "▰▰▰▰▰▰▰▱▱▱ YUM", 
                    "▰▰▰▰▰▰▰▰▱▱ YUM", 
                    "▰▰▰▰▰▰▰▰▰▱ YUM", 
                    "▰▰▰▰▰▰▰▰▰▰ DONE"
                ])
        );
        pb.set_message(format!("sudo yum install {}", package_name));
        
        let output = Command::new("sudo")
            .args(&["yum", "install", "-y", &package_name])
            .output()?;

        pb.finish_and_clear();
        
        if output.status.success() {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Green),
                Print("✅ YUM installation complete\n"),
                ResetColor
            );
        } else {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Red),
                Print("❌ YUM installation failed\n"),
                ResetColor
            );
        }

        Ok(output.status.success())
    }

    fn install_with_dnf(&self, spec: &PackageSpec, _pkg_dir: &std::path::Path) -> Result<bool> {
        let package_name = self.map_to_dnf_name(&spec.name);
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Blue),
            Print("🔵 Installing "),
            SetForegroundColor(CtColor::Cyan),
            Print(&spec.name),
            SetForegroundColor(CtColor::Blue),
            Print(" with DNF...\n"),
            ResetColor
        );
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▰▱▱▱▱▱▱▱▱▱ DNF", 
                    "▰▰▱▱▱▱▱▱▱▱ DNF", 
                    "▰▰▰▱▱▱▱▱▱▱ DNF",
                    "▰▰▰▰▱▱▱▱▱▱ DNF", 
                    "▰▰▰▰▰▱▱▱▱▱ DNF",
                    "▰▰▰▰▰▰▱▱▱▱ DNF",
                    "▰▰▰▰▰▰▰▱▱▱ DNF", 
                    "▰▰▰▰▰▰▰▰▱▱ DNF", 
                    "▰▰▰▰▰▰▰▰▰▱ DNF", 
                    "▰▰▰▰▰▰▰▰▰▰ DONE"
                ])
        );
        pb.set_message(format!("sudo dnf install {}", package_name));
        
        let output = Command::new("sudo")
            .args(&["dnf", "install", "-y", &package_name])
            .output()?;

        pb.finish_and_clear();
        
        if output.status.success() {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Green),
                Print("✅ DNF installation complete\n"),
                ResetColor
            );
        } else {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Red),
                Print("❌ DNF installation failed\n"),
                ResetColor
            );
        }

        Ok(output.status.success())
    }

    fn install_with_pacman(&self, spec: &PackageSpec, _pkg_dir: &std::path::Path) -> Result<bool> {
        let package_name = self.map_to_pacman_name(&spec.name);
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Yellow),
            Print("⚡ Installing "),
            SetForegroundColor(CtColor::Cyan),
            Print(&spec.name),
            SetForegroundColor(CtColor::Yellow),
            Print(" with Pacman...\n"),
            ResetColor
        );
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.yellow} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▰▱▱▱▱▱▱▱▱▱ PAC", 
                    "▰▰▱▱▱▱▱▱▱▱ PAC", 
                    "▰▰▰▱▱▱▱▱▱▱ PAC",
                    "▰▰▰▰▱▱▱▱▱▱ PAC", 
                    "▰▰▰▰▰▱▱▱▱▱ PAC",
                    "▰▰▰▰▰▰▱▱▱▱ PAC",
                    "▰▰▰▰▰▰▰▱▱▱ PAC", 
                    "▰▰▰▰▰▰▰▰▱▱ PAC", 
                    "▰▰▰▰▰▰▰▰▰▱ PAC", 
                    "▰▰▰▰▰▰▰▰▰▰ DONE"
                ])
        );
        pb.set_message(format!("sudo pacman -S {}", package_name));
        
        let output = Command::new("sudo")
            .args(&["pacman", "-S", "--noconfirm", &package_name])
            .output()?;

        pb.finish_and_clear();
        
        if output.status.success() {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Green),
                Print("✅ Pacman installation complete\n"),
                ResetColor
            );
        } else {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Red),
                Print("❌ Pacman installation failed\n"),
                ResetColor
            );
        }

        Ok(output.status.success())
    }

    fn install_with_zypper(&self, spec: &PackageSpec, _pkg_dir: &std::path::Path) -> Result<bool> {
        let package_name = self.map_to_zypper_name(&spec.name);
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Green),
            Print("🦎 Installing "),
            SetForegroundColor(CtColor::Cyan),
            Print(&spec.name),
            SetForegroundColor(CtColor::Green),
            Print(" with Zypper...\n"),
            ResetColor
        );
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▰▱▱▱▱▱▱▱▱▱ ZYP", 
                    "▰▰▱▱▱▱▱▱▱▱ ZYP", 
                    "▰▰▰▱▱▱▱▱▱▱ ZYP",
                    "▰▰▰▰▱▱▱▱▱▱ ZYP", 
                    "▰▰▰▰▰▱▱▱▱▱ ZYP",
                    "▰▰▰▰▰▰▱▱▱▱ ZYP",
                    "▰▰▰▰▰▰▰▱▱▱ ZYP", 
                    "▰▰▰▰▰▰▰▰▱▱ ZYP", 
                    "▰▰▰▰▰▰▰▰▰▱ ZYP", 
                    "▰▰▰▰▰▰▰▰▰▰ DONE"
                ])
        );
        pb.set_message(format!("sudo zypper install {}", package_name));
        
        let output = Command::new("sudo")
            .args(&["zypper", "install", "-y", &package_name])
            .output()?;

        pb.finish_and_clear();
        
        if output.status.success() {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Green),
                Print("✅ Zypper installation complete\n"),
                ResetColor
            );
        } else {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Red),
                Print("❌ Zypper installation failed\n"),
                ResetColor
            );
        }

        Ok(output.status.success())
    }

    fn install_with_linux_pm(&self, spec: &PackageSpec, pkg_dir: &std::path::Path) -> Result<bool> {
        // Try different Linux package managers
        if self.which("apt") || self.which("apt-get") {
            self.install_with_apt(spec, pkg_dir)
        } else if self.which("yum") {
            self.install_with_yum(spec, pkg_dir)
        } else if self.which("dnf") {
            self.install_with_dnf(spec, pkg_dir)
        } else if self.which("pacman") {
            self.install_with_pacman(spec, pkg_dir)
        } else if self.which("zypper") {
            self.install_with_zypper(spec, pkg_dir)
        } else {
            Err(anyhow!("No supported Linux package manager found"))
        }
    }

    fn install_portable_version(&self, spec: &PackageSpec, pkg_dir: &std::path::Path) -> Result<bool> {
        // Try to install portable/local versions without sudo
        match spec.name.as_str() {
            "nodejs" | "node" => self.install_nodejs_portable(spec, pkg_dir),
            "python3" | "python" => self.install_python_portable(spec, pkg_dir),
            "rust" => self.install_rust_portable(spec, pkg_dir),
            "git" => {
                // Git is usually already available, just create a symlink if found
                if self.which("git") {
                    let git_path = self.which_path("git")?;
                    let bin_dir = pkg_dir.join("bin");
                    std::fs::create_dir_all(&bin_dir)?;
                    std::fs::copy(&git_path, bin_dir.join("git"))?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => {
                println!("{} Package '{}' not available for local install. Try using Nix or Homebrew.", 
                         "Warning:".yellow(), spec.name);
                Ok(false)
            }
        }
    }

    fn install_nodejs_portable(&self, spec: &PackageSpec, pkg_dir: &std::path::Path) -> Result<bool> {
        // Download Node.js portable version
        let version = spec.version.as_deref().unwrap_or("18.17.0");
        let os = if cfg!(target_os = "macos") { "darwin" } else { "linux" };
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
        
        let url = format!("https://nodejs.org/dist/v{}/node-v{}-{}-{}.tar.xz", 
                         version, version, os, arch);
        
        println!("Downloading Node.js {} from nodejs.org...", version);
        self.download_and_extract(&url, pkg_dir, &format!("node-v{}-{}-{}", version, os, arch))
    }

    fn install_python_portable(&self, spec: &PackageSpec, _pkg_dir: &std::path::Path) -> Result<bool> {
        // For Python, we can use pyenv-like approach or download from python.org
        let version = spec.version.as_deref().unwrap_or("3.11.0");
        
        if cfg!(target_os = "macos") {
            let url = format!("https://www.python.org/ftp/python/{}/python-{}-macos11.pkg", version, version);
            println!("Python installation requires manual download from: {}", url);
            println!("Or install via: brew install python@{}", version);
            Ok(false)
        } else {
            println!("Python portable installation not implemented for this platform");
            println!("Please use: nix profile install nixpkgs#python3");
            Ok(false)
        }
    }

    fn install_rust_portable(&self, _spec: &PackageSpec, pkg_dir: &std::path::Path) -> Result<bool> {
        // Use rustup installer but install locally
        let rustup_home = pkg_dir.join("rustup");
        let cargo_home = pkg_dir.join("cargo");
        
        std::fs::create_dir_all(&rustup_home)?;
        std::fs::create_dir_all(&cargo_home)?;

        let output = Command::new("curl")
            .args(&["--proto", "=https", "--tlsv1.2", "-sSf", "https://sh.rustup.rs"])
            .env("RUSTUP_HOME", &rustup_home)
            .env("CARGO_HOME", &cargo_home)
            .arg("-s")
            .arg("--")
            .arg("-y")
            .arg("--no-modify-path")
            .output()?;

        Ok(output.status.success())
    }

    fn download_and_extract(&self, url: &str, pkg_dir: &std::path::Path, extract_dir: &str) -> Result<bool> {
        if !self.which("curl") || !self.which("tar") {
            return Err(anyhow!("curl and tar are required for portable installations"));
        }

        let filename = url.split('/').last().unwrap_or("download.tar.xz");
        let download_path = pkg_dir.join(filename);
        
        println!("📦 Downloading {} ...", filename.cyan());
        
        // Download with progress bar
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂", "▁"
                ])
        );
        pb.set_message("Downloading...");

        let output = Command::new("curl")
            .args(&[
                "-L", 
                "--progress-bar", 
                "-o", download_path.to_string_lossy().as_ref(), 
                url
            ])
            .output()?;
            
        pb.finish_and_clear();
        
        if !output.status.success() {
            println!("{} Download failed", "✗".red());
            return Ok(false);
        }

        println!("{} Download complete", "✓".green());
        
        // Extract with progress
        println!("📂 Extracting {} ...", filename.cyan());
        let extract_pb = ProgressBar::new_spinner();
        extract_pb.enable_steady_tick(std::time::Duration::from_millis(100));
        extract_pb.set_style(
            ProgressStyle::with_template("{spinner:.yellow} {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"
                ])
        );
        extract_pb.set_message("Extracting archive...");

        let output = Command::new("tar")
            .args(&["-xf", download_path.to_string_lossy().as_ref(), "-C", pkg_dir.to_string_lossy().as_ref()])
            .output()?;

        extract_pb.finish_and_clear();

        if output.status.success() {
            println!("{} Extraction complete", "✓".green());
            
            // Move extracted files to standard locations
            let extracted_dir = pkg_dir.join(extract_dir);
            if extracted_dir.exists() {
                let bin_dir = pkg_dir.join("bin");
                std::fs::create_dir_all(&bin_dir)?;
                
                println!("🔧 Setting up binaries...");
                let setup_pb = ProgressBar::new_spinner();
                setup_pb.enable_steady_tick(std::time::Duration::from_millis(80));
                setup_pb.set_style(
                    ProgressStyle::with_template("{spinner:.blue} {wide_msg}")
                        .unwrap()
                        .tick_strings(&["◐", "◓", "◑", "◒"])
                );
                setup_pb.set_message("Installing binaries...");
                
                // Copy binaries
                if let Ok(entries) = std::fs::read_dir(extracted_dir.join("bin")) {
                    for entry in entries.flatten() {
                        let target = bin_dir.join(entry.file_name());
                        std::fs::copy(entry.path(), &target)?;
                        
                        // Make executable on Unix systems
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let mut perms = std::fs::metadata(&target)?.permissions();
                            perms.set_mode(0o755);
                            std::fs::set_permissions(&target, perms)?;
                        }
                    }
                }
                
                setup_pb.finish_and_clear();
                println!("{} Setup complete", "✓".green());
            }
        } else {
            println!("{} Extraction failed", "✗".red());
        }

        // Cleanup
        std::fs::remove_file(download_path).ok();
        
        Ok(output.status.success())
    }

    fn which_path(&self, command: &str) -> Result<std::path::PathBuf> {
        let output = Command::new("which")
            .arg(command)
            .output()?;
            
        if output.status.success() {
            let path_str = String::from_utf8(output.stdout)?;
            Ok(std::path::PathBuf::from(path_str.trim()))
        } else {
            Err(anyhow!("Command '{}' not found", command))
        }
    }

    fn install_from_github(&self, repo: &str, rev: &str, pkg_dir: &std::path::Path) -> Result<bool> {
        if !self.which("git") {
            return Err(anyhow!("git not available"));
        }

        let repo_dir = pkg_dir.join("github").join(repo.replace('/', "_"));
        std::fs::create_dir_all(&repo_dir)?;

        println!("📦 Cloning {} from GitHub...", repo.cyan());
        
        let clone_pb = ProgressBar::new_spinner();
        clone_pb.enable_steady_tick(std::time::Duration::from_millis(100));
        clone_pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "⬇ ", "⬇ ", "⬇ ", "⬇ ", "⬇ ", "⬇ "
                ])
        );
        clone_pb.set_message(format!("git clone {}", repo));

        let output = Command::new("git")
            .args(&["clone", &format!("https://github.com/{}", repo), repo_dir.to_string_lossy().as_ref()])
            .output()?;

        clone_pb.finish_and_clear();

        if !output.status.success() {
            println!("{} Git clone failed", "✗".red());
            return Ok(false);
        }

        println!("{} Clone complete", "✓".green());

        // Checkout specific revision
        println!("🔀 Checking out revision {}...", rev.cyan());
        
        let checkout_pb = ProgressBar::new_spinner();
        checkout_pb.enable_steady_tick(std::time::Duration::from_millis(80));
        checkout_pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {wide_msg}")
                .unwrap()
                .tick_strings(&["⚡", "⚡", "⚡", "⚡"])
        );
        checkout_pb.set_message(format!("git checkout {}", rev));
        
        let output = Command::new("git")
            .current_dir(&repo_dir)
            .args(&["checkout", rev])
            .output()?;

        checkout_pb.finish_and_clear();
        
        if output.status.success() {
            println!("{} Checkout complete", "✓".green());
        } else {
            println!("{} Checkout failed", "✗".red());
        }

        Ok(output.status.success())
    }

    fn install_from_url(&self, url: &str, pkg_dir: &std::path::Path) -> Result<bool> {
        if !self.which("curl") {
            return Err(anyhow!("curl not available"));
        }

        let filename = url.split('/').last().unwrap_or("download");
        let target_path = pkg_dir.join("downloads").join(filename);
        std::fs::create_dir_all(target_path.parent().unwrap())?;

        let output = Command::new("curl")
            .args(&["-L", "-o", target_path.to_string_lossy().as_ref(), url])
            .output()?;

        Ok(output.status.success())
    }

    fn update_container_paths(&self, container: &mut ContainerConfig, spec: &PackageSpec, pkg_dir: &std::path::Path) -> Result<()> {
        println!("🔧 Automatically updating container environment for {}...", spec.name.cyan());
        
        // Auto-detect the best binary paths for this package installation
        let detected_paths = self.detect_package_binary_paths(pkg_dir, &spec.name)?;
        
        if detected_paths.is_empty() {
            println!("{} No binary paths detected for {}", "⚠️".yellow(), spec.name);
            return Ok(());
        }
        
        // Clean up old package installations and stow configurations
        self.cleanup_old_package_setup(container, &spec.name)?;
        
        // Set up the package with the best detected method
        let setup_success = if self.which("stow") {
            self.setup_with_auto_stow(container, spec, pkg_dir, &detected_paths)?
        } else {
            false
        };
        
        // Fallback to direct PATH management if stow fails
        if !setup_success {
            self.setup_with_auto_direct_paths(container, &detected_paths)?;
        }
        
        println!("{} Container environment updated automatically", "✅".green());
        Ok(())
    }

    fn ensure_stow_available(&self) -> Result<bool> {
        // Check if stow is already available
        if self.which("stow") {
            return Ok(true);
        }

        println!("🔧 GNU Stow not found, installing it for better package management...");
        
        // Try to install stow based on the system
        let installed = if self.which("brew") {
            self.install_stow_with_homebrew()?
        } else if self.which("nix") {
            self.install_stow_with_nix()?
        } else if self.which("apt") {
            println!("{} Please install GNU Stow manually: sudo apt install stow", "Note:".yellow());
            false
        } else if self.which("yum") || self.which("dnf") {
            println!("{} Please install GNU Stow manually: sudo yum install stow", "Note:".yellow());
            false
        } else if self.which("pacman") {
            println!("{} Please install GNU Stow manually: sudo pacman -S stow", "Note:".yellow());
            false
        } else {
            // Try to install from source as last resort
            self.install_stow_from_source()?
        };

        if installed {
            println!("{} GNU Stow installed successfully", "✓".green());
        } else {
            println!("{} Falling back to direct PATH management", "Warning:".yellow());
        }

        Ok(installed)
    }

    fn install_stow_with_homebrew(&self) -> Result<bool> {
        println!("🍺 Installing GNU Stow with Homebrew...");
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.yellow} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&["🍺", "🍻", "🍺", "🍻"])
        );
        pb.set_message("brew install stow");
        
        let output = Command::new("brew")
            .args(&["install", "stow"])
            .output()?;

        pb.finish_and_clear();
        Ok(output.status.success())
    }

    fn install_stow_with_nix(&self) -> Result<bool> {
        println!("❄️  Installing GNU Stow with Nix...");
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&["❄ ", "❅ ", "❆ ", "❇ "])
        );
        pb.set_message("nix profile install nixpkgs#stow");

        let output = Command::new("nix")
            .args(&["profile", "install", "nixpkgs#stow"])
            .output()?;

        pb.finish_and_clear();
        Ok(output.status.success())
    }

    fn install_stow_from_source(&self) -> Result<bool> {
        println!("🔨 Installing GNU Stow from source...");
        
        let stow_version = "2.3.1";
        let url = format!("https://ftp.gnu.org/gnu/stow/stow-{}.tar.gz", stow_version);
        let temp_dir = std::env::temp_dir().join("stow-build");
        std::fs::create_dir_all(&temp_dir)?;

        // Download
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .tick_strings(&["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"])
        );
        pb.set_message("Downloading GNU Stow source...");

        let download_path = temp_dir.join(format!("stow-{}.tar.gz", stow_version));
        let output = Command::new("curl")
            .args(&["-L", "-o", download_path.to_string_lossy().as_ref(), &url])
            .output()?;

        pb.finish_and_clear();

        if !output.status.success() {
            return Ok(false);
        }

        // Extract and build
        println!("🔧 Building GNU Stow...");
        let build_pb = ProgressBar::new_spinner();
        build_pb.enable_steady_tick(std::time::Duration::from_millis(100));
        build_pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {wide_msg}")
                .unwrap()
                .tick_strings(&["⚙ ", "⚙ ", "⚙ ", "⚙ "])
        );
        build_pb.set_message("Configuring and building...");

        // Extract
        Command::new("tar")
            .args(&["-xzf", download_path.to_string_lossy().as_ref(), "-C", temp_dir.to_string_lossy().as_ref()])
            .output()?;

        let source_dir = temp_dir.join(format!("stow-{}", stow_version));
        let home = std::env::var("HOME").unwrap_or_default();
        let install_prefix = format!("{}/.local", home);

        // Configure
        let configure_output = Command::new("./configure")
            .current_dir(&source_dir)
            .arg(&format!("--prefix={}", install_prefix))
            .output()?;

        if !configure_output.status.success() {
            build_pb.finish_and_clear();
            return Ok(false);
        }

        // Make and install
        let make_output = Command::new("make")
            .current_dir(&source_dir)
            .output()?;

        if !make_output.status.success() {
            build_pb.finish_and_clear();
            return Ok(false);
        }

        let install_output = Command::new("make")
            .current_dir(&source_dir)
            .arg("install")
            .output()?;

        build_pb.finish_and_clear();

        // Add ~/.local/bin to PATH for this session
        if install_output.status.success() {
            let local_bin = format!("{}/.local/bin", home);
            if let Ok(current_path) = std::env::var("PATH") {
                std::env::set_var("PATH", format!("{}:{}", local_bin, current_path));
            }
        }

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();

        Ok(install_output.status.success())
    }

    fn setup_with_stow(&self, container: &mut ContainerConfig, spec: &PackageSpec, pkg_dir: &std::path::Path) -> Result<()> {
        println!("🔗 Setting up {} with GNU Stow...", spec.name.cyan());
        
        // Create proper stow directory structure - stow and target should be siblings
        let container_dir = self.workspace.join("containers").join(&container.name);
        let stow_dir = container_dir.join("stow");
        let target_dir = container_dir.join("local"); // Changed from "usr" to avoid conflicts
        std::fs::create_dir_all(&stow_dir)?;
        std::fs::create_dir_all(&target_dir)?;

        // Create package directory in stow format
        let package_stow_dir = stow_dir.join(&spec.name);
        std::fs::create_dir_all(&package_stow_dir)?;

        // Move installed files to stow package directory
        self.organize_for_stow(pkg_dir, &package_stow_dir)?;

        // Validate stow directory structure before attempting to stow
        if !self.validate_stow_structure(&package_stow_dir, &spec.name)? {
            println!("{} Package directory structure invalid for stow, using direct paths", "Warning:".yellow());
            return self.setup_with_direct_paths(container, pkg_dir);
        }

        // Use stow to create symlinks
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} {wide_msg}")
                .unwrap()
                .tick_strings(&["🔗", "🔐", "🔑", "🔒"])
        );
        pb.set_message("Creating symlinks with stow...");

        // Fix stow command - run from container directory, use relative paths
        println!("{} Running: cd {} && stow -d stow -t local -v {}", 
                "Debug:".yellow(),
                container_dir.display(),
                spec.name);

        let output = Command::new("stow")
            .current_dir(&container_dir)
            .args(&["-d", "stow", "-t", "local", "-v", &spec.name])
            .output()?;

        pb.finish_and_clear();

        if output.status.success() {
            println!("{} Stow setup complete", "✅".green());
            
            // Update PATH to include the stowed binaries
            let stow_bin_path = target_dir.join("bin");
            if stow_bin_path.exists() {
                self.update_container_path_with_system_dirs(container, &stow_bin_path)?;
                
                // Save container configuration after updating PATH
                container.save(&self.workspace)?;
            }
            
            // Show what was linked
            if !output.stdout.is_empty() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                println!("{} Stow created: {}", "Info:".blue(), 
                        stdout.lines().filter(|l| l.contains("=>")).collect::<Vec<_>>().join(", "));
            }
        } else {
            println!("{} Stow setup failed, falling back to direct symlinks", "⚠️".yellow());
            
            // Debug output to help diagnose the issue
            if !output.stderr.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("{} Stow error: {}", "Debug:".yellow(), stderr.trim());
            }
            if !output.stdout.is_empty() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                println!("{} Stow output: {}", "Debug:".yellow(), stdout.trim());
            }
            
            // Try direct symlink fallback before falling back to direct paths
            let package_stow_dir = stow_dir.join(&spec.name);
            if self.setup_with_direct_symlinks(container, &package_stow_dir, &target_dir)? {
                println!("{} Direct symlinks setup complete", "✅".green());
            } else {
                println!("{} Direct symlinks failed, using PATH management", "⚠️".yellow());
                self.setup_with_direct_paths(container, pkg_dir)?;
            }
        }

        Ok(())
    }

    fn organize_for_stow(&self, pkg_dir: &std::path::Path, stow_pkg_dir: &std::path::Path) -> Result<()> {
        // Move binaries to bin/ - prioritize package-specific directories first
        let src_bin_paths = vec![
            pkg_dir.join("bin"),                    // Direct package binaries (highest priority)
            pkg_dir.join("usr/bin"),               // System-style binaries  
            pkg_dir.join("nix-profile/bin"),       // Nix binaries (lowest priority - may contain other packages)
        ];

        let dest_bin_dir = stow_pkg_dir.join("bin");
        std::fs::create_dir_all(&dest_bin_dir)?;

        let mut copied_files = std::collections::HashSet::new();

        for src_bin in src_bin_paths {
            if src_bin.exists() {
                if let Ok(entries) = std::fs::read_dir(&src_bin) {
                    for entry in entries.flatten() {
                        let filename = entry.file_name();
                        let dest = dest_bin_dir.join(&filename);
                        
                        // Only copy if we haven't already copied this filename
                        // This prevents duplicates from different source directories
                        if !copied_files.contains(&filename) && !dest.exists() {
                            std::fs::copy(entry.path(), &dest)?;
                            copied_files.insert(filename);
                        }
                    }
                }
            }
        }

        // Move libraries to lib/ if they exist
        let src_lib_paths = vec![
            pkg_dir.join("lib"),
            pkg_dir.join("nix-profile/lib"),
            pkg_dir.join("usr/lib"),
        ];

        for src_lib in src_lib_paths {
            if src_lib.exists() {
                let dest_lib_dir = stow_pkg_dir.join("lib");
                std::fs::create_dir_all(&dest_lib_dir)?;
                
                if let Ok(entries) = std::fs::read_dir(&src_lib) {
                    for entry in entries.flatten() {
                        let dest = dest_lib_dir.join(entry.file_name());
                        if !dest.exists() {
                            if entry.path().is_dir() {
                                self.copy_dir_all(&entry.path(), &dest)?;
                            } else {
                                std::fs::copy(entry.path(), dest)?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn copy_dir_all(&self, src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_dir() {
                self.copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
            } else {
                std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
            }
        }
        Ok(())
    }

    fn validate_stow_structure(&self, package_stow_dir: &std::path::Path, package_name: &str) -> Result<bool> {
        if !package_stow_dir.exists() {
            println!("{} Package stow directory doesn't exist: {}", "Debug:".yellow(), package_stow_dir.display());
            return Ok(false);
        }

        // Check for common directories that stow should manage
        let directories_to_check = vec!["bin", "lib", "share", "include", "man"];
        let mut has_content = false;
        let mut structure_info = Vec::new();

        for dir_name in &directories_to_check {
            let dir_path = package_stow_dir.join(dir_name);
            if dir_path.exists() {
                match std::fs::read_dir(&dir_path) {
                    Ok(entries) => {
                        let count = entries.count();
                        if count > 0 {
                            has_content = true;
                            structure_info.push(format!("{}/: {} items", dir_name, count));
                        } else {
                            structure_info.push(format!("{}/: empty", dir_name));
                        }
                    }
                    Err(_) => {
                        structure_info.push(format!("{}/: read error", dir_name));
                    }
                }
            }
        }

        if !structure_info.is_empty() {
            println!("{} Package {} structure: [{}]", 
                    "Debug:".yellow(), 
                    package_name,
                    structure_info.join(", "));
        } else {
            println!("{} Package {} has no standard directories (bin, lib, share, etc.)", 
                    "Debug:".yellow(), 
                    package_name);
        }

        if !has_content {
            println!("{} Package {} has no files to stow", "Debug:".yellow(), package_name);
        }

        Ok(has_content)
    }

    fn update_container_path_with_system_dirs(&self, container: &mut ContainerConfig, bin_path: &std::path::Path) -> Result<()> {
        // Always include system directories to prevent basic commands from breaking
        let system_paths = vec![
            "/usr/local/bin",
            "/usr/bin", 
            "/bin",
            "/usr/sbin",
            "/sbin"
        ];
        
        let mut path_components = vec![bin_path.to_string_lossy().to_string()];
        path_components.extend(system_paths.iter().map(|s| s.to_string()));
        
        // Add user's original PATH if it exists (but avoid system duplication)
        if let Ok(original_path) = std::env::var("PATH") {
            for component in original_path.split(':') {
                if !path_components.contains(&component.to_string()) && 
                   !component.starts_with("/usr/") && !component.starts_with("/bin") {
                    path_components.push(component.to_string());
                }
            }
        }
        
        let new_path = path_components.join(":");
        container.environment.insert("PATH".to_string(), new_path);
        Ok(())
    }

    fn setup_with_direct_symlinks(&self, container: &mut ContainerConfig, package_stow_dir: &std::path::Path, target_dir: &std::path::Path) -> Result<bool> {
        if !package_stow_dir.exists() {
            return Ok(false);
        }

        println!("🔗 Creating direct symlinks for package...");
        let mut success = false;

        // Create symlinks for directories that exist in the package
        let subdirs_to_link = vec!["bin", "lib", "share", "include", "man"];
        
        for subdir in subdirs_to_link {
            let src_dir = package_stow_dir.join(subdir);
            let target_subdir = target_dir.join(subdir);
            
            if src_dir.exists() {
                std::fs::create_dir_all(&target_subdir).ok();
                
                if let Ok(entries) = std::fs::read_dir(&src_dir) {
                    for entry in entries.flatten() {
                        let target_file = target_subdir.join(entry.file_name());
                        
                        // Remove existing file/link if it exists
                        if target_file.exists() || target_file.is_symlink() {
                            std::fs::remove_file(&target_file).ok();
                        }
                        
                        // Create relative symlink
                        let relative_src = std::path::Path::new("../stow")
                            .join(package_stow_dir.file_name().unwrap())
                            .join(subdir)
                            .join(entry.file_name());
                            
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::symlink;
                            if symlink(&relative_src, &target_file).is_ok() {
                                success = true;
                            }
                        }
                        
                        #[cfg(not(unix))]
                        {
                            // For non-Unix systems, copy the file instead
                            if std::fs::copy(entry.path(), &target_file).is_ok() {
                                success = true;
                            }
                        }
                    }
                }
            }
        }

        if success {
            let target_bin = target_dir.join("bin");
            if target_bin.exists() {
                self.update_container_path_with_system_dirs(container, &target_bin)?;
                container.save(&self.workspace)?;
            }
        }

        Ok(success)
    }

    fn setup_with_direct_paths(&self, container: &mut ContainerConfig, pkg_dir: &std::path::Path) -> Result<()> {
        println!("📁 Setting up direct PATH management...");
        
        // Update PATH to include package binaries
        let bin_paths = vec![
            pkg_dir.join("nix-profile/bin"),
            pkg_dir.join("bin"),
            pkg_dir.join("usr/bin"),
        ];

        let mut path_additions = Vec::new();
        for bin_path in bin_paths {
            if bin_path.exists() {
                path_additions.push(bin_path.to_string_lossy().to_string());
            }
        }

        if !path_additions.is_empty() {
            // Create a single bin directory for all package binaries
            let container_bin_dir = self.workspace.join("containers").join(&container.name).join("bin");
            std::fs::create_dir_all(&container_bin_dir)?;
            
            // Copy all binaries to the single directory for easier PATH management
            for path in &path_additions {
                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries.flatten() {
                        let dest = container_bin_dir.join(entry.file_name());
                        if !dest.exists() {
                            std::fs::copy(entry.path(), &dest).ok();
                        }
                    }
                }
            }
            
            self.update_container_path_with_system_dirs(container, &container_bin_dir)?;
            container.save(&self.workspace)?;
            println!("{} PATH updated with package binaries and saved to container config", "✓".green());
        }

        Ok(())
    }

    fn _map_to_brew_name(&self, package: &str) -> String {
        // Map common package names to homebrew formulas
        match package {
            "nodejs" => "node".to_string(),
            "python3" => "python@3.11".to_string(),
            "gcc" => "gcc".to_string(),
            _ => package.to_string(),
        }
    }

    fn map_to_brew_name_with_version(&self, package: &str, version: Option<&str>) -> String {
        // Map package names with version support for Homebrew
        match package {
            "nodejs" | "node" => {
                match version {
                    Some("18") | Some("18.17.0") | Some("18.x") => "node@18".to_string(),
                    Some("20") | Some("20.5.0") | Some("20.x") => "node@20".to_string(),
                    Some("16") | Some("16.x") => "node@16".to_string(),
                    Some("14") | Some("14.x") => "node@14".to_string(),
                    _ => "node".to_string(), // Latest
                }
            }
            "python3" | "python" => {
                match version {
                    Some("3.11") | Some("3.11.0") => "python@3.11".to_string(),
                    Some("3.10") | Some("3.10.0") => "python@3.10".to_string(),
                    Some("3.9") | Some("3.9.0") => "python@3.9".to_string(),
                    Some("3.8") | Some("3.8.0") => "python@3.8".to_string(),
                    _ => "python3".to_string(), // Latest
                }
            }
            _ => package.to_string(),
        }
    }

    fn which(&self, command: &str) -> bool {
        Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn map_to_apt_name(&self, package: &str) -> String {
        match package {
            "nodejs" | "node" => "nodejs".to_string(),
            "python3" | "python" => "python3".to_string(),
            "gcc" => "gcc".to_string(),
            "git" => "git".to_string(),
            "curl" => "curl".to_string(),
            "wget" => "wget".to_string(),
            "vim" => "vim".to_string(),
            "nano" => "nano".to_string(),
            "htop" => "htop".to_string(),
            "tree" => "tree".to_string(),
            "zip" => "zip".to_string(),
            "unzip" => "unzip".to_string(),
            "jq" => "jq".to_string(),
            "docker" => "docker.io".to_string(),
            "rustc" => "rustc".to_string(),
            _ => package.to_string(),
        }
    }

    fn map_to_yum_name(&self, package: &str) -> String {
        match package {
            "nodejs" | "node" => "nodejs".to_string(),
            "python3" | "python" => "python3".to_string(),
            "gcc" => "gcc".to_string(),
            "git" => "git".to_string(),
            "curl" => "curl".to_string(),
            "wget" => "wget".to_string(),
            "vim" => "vim".to_string(),
            "nano" => "nano".to_string(),
            "htop" => "htop".to_string(),
            "tree" => "tree".to_string(),
            "zip" => "zip".to_string(),
            "unzip" => "unzip".to_string(),
            "jq" => "jq".to_string(),
            "docker" => "docker".to_string(),
            "rustc" => "rust".to_string(),
            _ => package.to_string(),
        }
    }

    fn map_to_dnf_name(&self, package: &str) -> String {
        match package {
            "nodejs" | "node" => "nodejs".to_string(),
            "python3" | "python" => "python3".to_string(),
            "gcc" => "gcc".to_string(),
            "git" => "git".to_string(),
            "curl" => "curl".to_string(),
            "wget" => "wget".to_string(),
            "vim" => "vim".to_string(),
            "nano" => "nano".to_string(),
            "htop" => "htop".to_string(),
            "tree" => "tree".to_string(),
            "zip" => "zip".to_string(),
            "unzip" => "unzip".to_string(),
            "jq" => "jq".to_string(),
            "docker" => "docker".to_string(),
            "rustc" => "rust".to_string(),
            _ => package.to_string(),
        }
    }

    fn map_to_pacman_name(&self, package: &str) -> String {
        match package {
            "nodejs" | "node" => "nodejs".to_string(),
            "python3" | "python" => "python".to_string(),
            "gcc" => "gcc".to_string(),
            "git" => "git".to_string(),
            "curl" => "curl".to_string(),
            "wget" => "wget".to_string(),
            "vim" => "vim".to_string(),
            "nano" => "nano".to_string(),
            "htop" => "htop".to_string(),
            "tree" => "tree".to_string(),
            "zip" => "zip".to_string(),
            "unzip" => "unzip".to_string(),
            "jq" => "jq".to_string(),
            "docker" => "docker".to_string(),
            "rustc" => "rust".to_string(),
            _ => package.to_string(),
        }
    }

    fn map_to_zypper_name(&self, package: &str) -> String {
        match package {
            "nodejs" | "node" => "nodejs".to_string(),
            "python3" | "python" => "python3".to_string(),
            "gcc" => "gcc".to_string(),
            "git" => "git".to_string(),
            "curl" => "curl".to_string(),
            "wget" => "wget".to_string(),
            "vim" => "vim".to_string(),
            "nano" => "nano".to_string(),
            "htop" => "htop".to_string(),
            "tree" => "tree".to_string(),
            "zip" => "zip".to_string(),
            "unzip" => "unzip".to_string(),
            "jq" => "jq".to_string(),
            "docker" => "docker".to_string(),
            "rustc" => "rust".to_string(),
            _ => package.to_string(),
        }
    }

    fn get_system_pm_display(&self) -> &'static str {
        if cfg!(target_os = "macos") && self.which("brew") {
            "🍺 homebrew"
        } else if cfg!(target_os = "linux") {
            if self.which("apt") || self.which("apt-get") {
                "📦 apt"
            } else if self.which("yum") {
                "🔴 yum"
            } else if self.which("dnf") {
                "🔵 dnf"
            } else if self.which("pacman") {
                "⚡ pacman"
            } else if self.which("zypper") {
                "🦎 zypper"
            } else {
                "📦 system"
            }
        } else {
            "📦 system"
        }
    }

    fn get_system_pm_info(&self) -> SystemPMInfo {
        if cfg!(target_os = "macos") && self.which("brew") {
            SystemPMInfo { name: "Homebrew".to_string(), emoji: "🍺" }
        } else if cfg!(target_os = "linux") {
            if self.which("apt") || self.which("apt-get") {
                SystemPMInfo { name: "APT".to_string(), emoji: "📦" }
            } else if self.which("yum") {
                SystemPMInfo { name: "YUM".to_string(), emoji: "🔴" }
            } else if self.which("dnf") {
                SystemPMInfo { name: "DNF".to_string(), emoji: "🔵" }
            } else if self.which("pacman") {
                SystemPMInfo { name: "Pacman".to_string(), emoji: "⚡" }
            } else if self.which("zypper") {
                SystemPMInfo { name: "Zypper".to_string(), emoji: "🦎" }
            } else {
                SystemPMInfo { name: "System".to_string(), emoji: "📦" }
            }
        } else {
            SystemPMInfo { name: "System".to_string(), emoji: "📦" }
        }
    }

    fn print_installation_header(&self, spec: &PackageSpec) {
        let _ = execute!(
            stdout(),
            Print("\n"),
            SetForegroundColor(CtColor::Magenta),
            Print("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰\n"),
            ResetColor,
            SetForegroundColor(CtColor::Cyan),
            Print("    ⚡ INSTALLING PACKAGE\n"),
            ResetColor
        );
        
        let version_display = if let Some(v) = &spec.version { 
            format!("@{}", v) 
        } else { 
            "@latest".to_string() 
        };
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::White),
            Print("    📦 Package: "),
            SetForegroundColor(CtColor::Yellow),
            Print(&spec.name),
            SetForegroundColor(CtColor::Blue),
            Print(&version_display),
            Print("\n"),
            ResetColor
        );
        
        let source_display = match &spec.source {
            PackageSource::Nixpkgs => self.get_system_pm_display(),
            PackageSource::GitHub { repo, .. } => return self.print_github_header(repo),
            PackageSource::Url(_) => "🌐 url",
        };
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Green),
            Print("    📍 Source: "),
            SetForegroundColor(CtColor::Cyan),
            Print(source_display),
            Print("\n"),
            SetForegroundColor(CtColor::Magenta),
            Print("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰\n"),
            ResetColor,
            Print("\n")
        );
        
        thread::sleep(Duration::from_millis(500));
    }
    
    fn print_github_header(&self, repo: &str) {
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Green),
            Print("    📍 Source: "),
            SetForegroundColor(CtColor::Blue),
            Print("📂 github.com/"),
            SetForegroundColor(CtColor::Cyan),
            Print(repo),
            Print("\n"),
            SetForegroundColor(CtColor::Magenta),
            Print("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰\n"),
            ResetColor,
            Print("\n")
        );
    }

    fn print_success_celebration(&self, spec: &PackageSpec, hash: &str) {
        let _ = execute!(
            stdout(),
            Print("\n"),
            SetBackgroundColor(CtColor::DarkGreen),
            SetForegroundColor(CtColor::White),
            Print(" ✅ INSTALLATION COMPLETE "),
            ResetColor,
            Print("\n\n")
        );
        
        // Animated success effect
        let celebration = ["🎉", "✨", "🚀", "⭐", "💫"];
        for (i, emoji) in celebration.iter().enumerate() {
            let color = match i % 3 {
                0 => CtColor::Yellow,
                1 => CtColor::Magenta,
                _ => CtColor::Cyan,
            };
            
            let _ = execute!(
                stdout(),
                SetForegroundColor(color),
                Print(&format!("    {} ", emoji))
            );
            thread::sleep(Duration::from_millis(100));
        }
        
        let _ = execute!(
            stdout(),
            ResetColor,
            SetForegroundColor(CtColor::Green),
            Print(&format!("{} installed successfully!", spec.name)),
            Print("\n"),
            SetForegroundColor(CtColor::DarkYellow),
            Print(&format!("    Hash: {}", hash)),
            Print("\n\n"),
            ResetColor
        );
    }

    // ========== AUTOMATION METHODS ==========

    fn detect_package_binary_paths(&self, pkg_dir: &std::path::Path, package_name: &str) -> Result<Vec<std::path::PathBuf>> {
        let mut detected_paths = Vec::new();
        
        // Priority order: Nix profiles (most complete) -> Direct bins -> System style
        let candidate_paths = vec![
            pkg_dir.join("nix-profile/bin"),           // Nix profile binaries (best option)
            pkg_dir.join("bin"),                       // Direct package binaries
            pkg_dir.join("usr/bin"),                   // System-style binaries
        ];
        
        println!("{} Detecting binary paths for {}...", "🔍".cyan(), package_name);
        
        for path in candidate_paths {
            if path.exists() {
                if let Ok(entries) = std::fs::read_dir(&path) {
                    let file_count = entries.count();
                    if file_count > 0 {
                        detected_paths.push(path.clone());
                        println!("{} Found {} binaries in {}", "✓".green(), file_count, path.display());
                    }
                }
            }
        }
        
        Ok(detected_paths)
    }

    fn cleanup_old_package_setup(&self, container: &mut ContainerConfig, package_name: &str) -> Result<()> {
        let container_dir = self.workspace.join("containers").join(&container.name);
        
        // Clean up old stow package directory
        let old_stow_pkg = container_dir.join("stow").join(package_name);
        if old_stow_pkg.exists() {
            println!("{} Cleaning up old stow setup for {}", "🧹".yellow(), package_name);
            
            // Unstow the old package first if stow is available
            if self.which("stow") {
                let _ = Command::new("stow")
                    .current_dir(&container_dir)
                    .args(&["-d", "stow", "-t", "local", "-D", package_name])
                    .output();
            }
            
            // Remove old stow package directory
            std::fs::remove_dir_all(&old_stow_pkg).ok();
        }
        
        // Clean up old local directory if it exists
        let old_local = container_dir.join("local");
        if old_local.exists() {
            std::fs::remove_dir_all(&old_local).ok();
        }
        
        Ok(())
    }

    fn setup_with_auto_stow(&self, container: &mut ContainerConfig, spec: &PackageSpec, pkg_dir: &std::path::Path, detected_paths: &[std::path::PathBuf]) -> Result<bool> {
        if detected_paths.is_empty() {
            return Ok(false);
        }
        
        let container_dir = self.workspace.join("containers").join(&container.name);
        let stow_dir = container_dir.join("stow");
        let target_dir = container_dir.join("local");
        
        std::fs::create_dir_all(&stow_dir)?;
        std::fs::create_dir_all(&target_dir)?;
        
        let package_stow_dir = stow_dir.join(&spec.name);
        std::fs::create_dir_all(&package_stow_dir)?;
        
        println!("{} Setting up {} with GNU Stow (automatic)...", "🔗".green(), spec.name.cyan());
        
        // For Nix profiles, copy the entire structure including lib directories
        let primary_path = &detected_paths[0];
        let nix_profile_root = if primary_path.ends_with("nix-profile/bin") {
            primary_path.parent().unwrap() // Get nix-profile directory
        } else {
            primary_path.parent().unwrap_or(pkg_dir)
        };
        
        // Copy necessary directories (bin, lib, etc.) to stow package directory
        for dir_name in &["bin", "lib", "share", "include"] {
            let src_dir = nix_profile_root.join(dir_name);
            let dest_dir = package_stow_dir.join(dir_name);
            
            if src_dir.exists() {
                println!("{} Copying {} to stow structure...", "📁".blue(), dir_name);
                self.copy_dir_all(&src_dir, &dest_dir)?;
            }
        }
        
        // Use stow to create symlinks
        let output = Command::new("stow")
            .current_dir(&container_dir)
            .args(&["-d", "stow", "-t", "local", "-v", &spec.name])
            .output()?;
        
        if output.status.success() {
            println!("{} Stow setup complete", "✅".green());
            
            // Update container PATH configuration
            let stow_bin_path = target_dir.join("bin");
            if stow_bin_path.exists() {
                self.update_container_path_with_system_dirs(container, &stow_bin_path)?;
                container.save(&self.workspace)?;
                println!("{} Container PATH updated with stowed binaries", "🎯".green());
            }
            
            return Ok(true);
        } else {
            println!("{} Stow setup failed, will use direct PATH method", "⚠️".yellow());
            if !output.stderr.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("{} Stow error: {}", "Debug:".yellow(), stderr.trim());
            }
        }
        
        Ok(false)
    }

    fn setup_with_auto_direct_paths(&self, container: &mut ContainerConfig, detected_paths: &[std::path::PathBuf]) -> Result<()> {
        if detected_paths.is_empty() {
            return Ok(());
        }
        
        println!("{} Setting up direct PATH management (automatic)...", "📁".blue());
        
        // Use the first (highest priority) detected path directly in container PATH
        let primary_bin_path = &detected_paths[0];
        
        // Update container PATH to include the detected binary path
        self.update_container_path_with_system_dirs(container, primary_bin_path)?;
        container.save(&self.workspace)?;
        
        println!("{} Container PATH updated: {}", "🎯".green(), primary_bin_path.display());
        println!("{} Direct PATH setup complete", "✅".green());
        
        Ok(())
    }
}
