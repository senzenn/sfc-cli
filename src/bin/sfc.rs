use std::fs;
use std::io::{stdout, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use crossterm::{
    queue,
    style::{Color as CtColor, SetForegroundColor, ResetColor, Print, SetBackgroundColor, Attribute},
    terminal::{Clear, ClearType},
    cursor::MoveTo,
    execute
};
use colored::control as colored_control;
use my_lib::sfc as core;
use my_lib::container::ContainerConfig;
use my_lib::history::History;
use my_lib::package::PackageManager;
use indicatif::{ProgressBar, ProgressStyle};
use figlet_rs::FIGfont;

#[derive(Parser, Debug)]
#[command(name = "sfc", version, about = "Suffix-container CLI (symlink-based)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create one or more containers
    Create { 
        names: Vec<String>,
        #[arg(long)] from: Option<String>, // Recreate from snapshot hash
    },

    /// Open a temp environment (uses current container if name not provided)
    Temp {
        name: Option<String>,
        #[arg(long)] node: Option<String>,
        #[arg(long)] npm: Option<String>,
        #[arg(long)] rust: Option<String>,
    },

    /// Promote a temp snapshot to stable (uses current container if name not provided)
    Promote { name: Option<String>, temp_alias: Option<String> },

    /// Discard a temp snapshot (uses current container if name not provided)
    Discard { name: Option<String>, temp_alias: Option<String> },

    /// List containers and temps
    List,

    /// Switch to a container (or show selection if no name provided)
    Switch { 
        name: Option<String>,
        #[arg(short = 'c', long = "cd")] enter: bool,
    },

    /// Delete a container and all its data
    Delete { 
        names: Vec<String>,
        #[arg(short = 'f', long = "force")] force: bool,
    },

    /// Show status for NAME
    Status { name: Option<String> },

    /// Clean dangling links and orphaned store snapshots
    Clean { #[arg(long)] age: Option<String> },

    /// Rollback NAME to a previous stable link target
    Rollback { name: String, target: String },

    /// Manage shared toolchains stored under workspace .sfc/toolchains (Language-specific)
    Toolchain {
        #[command(subcommand)]
        lang: ToolchainLang,
    },

    /// Add a package to current container
    Add { 
        package: String,
        #[arg(short, long)] version: Option<String>,
    },

    /// Remove a package from current container
    Remove { package: String },

    /// Search for packages
    Search { query: String },

    /// List installed packages
    Packages,

    /// History and visualization
    History {
        #[command(subcommand)]
        cmd: HistoryCmd,
    },

    /// Flake management for sharing
    Flake {
        #[command(subcommand)]
        cmd: FlakeCmd,
    },

    /// Show animated SFC banner
    Banner,

    /// Switch system binaries to use container binaries (requires sudo)
    SwitchBin { 
        name: String,
        #[arg(long)] force: bool,
    },

    /// Restore system binaries to original state (requires sudo)  
    RestoreBin,

    /// List all snapshots for a container
    Snapshots { name: String },

    /// Share a container snapshot for others to recreate
    Share { 
        name: String, 
        hash: Option<String>,
    },

    /// Delete a specific snapshot
    DeleteSnapshot { 
        name: String, 
        hash: String,
        #[arg(short = 'f', long = "force")] force: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ToolchainLang {
    /// Node via Volta
    Node { #[command(subcommand)] cmd: ToolchainCmd },
    /// Rust via rustup
    Rust { #[command(subcommand)] cmd: ToolchainCmd },
}

#[derive(Subcommand, Debug)]
enum ToolchainCmd {
    /// Install a version
    Install { version: String },
    /// List installed versions
    Ls,
    /// Select active version (also installs if missing)
    Use { version: String },
    /// Remove a version
    Remove { version: String },
}



#[derive(Subcommand, Debug)]
enum HistoryCmd {
    /// Show history log (like git reflog)
    Log { container: Option<String> },
    /// Show visual graph of container history
    Graph { container: Option<String> },
    /// Rollback to a specific hash
    Rollback { hash: String },
}

#[derive(Subcommand, Debug)]
enum FlakeCmd {
    /// Generate flake.nix for current container
    Generate,
    /// Push container config to GitHub
    Push { repo: String },
    /// Pull container config from GitHub
    Pull { repo: String },
}

// No local metadata types; use library implementation

fn main() -> Result<()> {
    // Ensure colored output even in some CI shells unless NO_COLOR is set
    if std::env::var_os("NO_COLOR").is_none() {
        let _ = colored_control::set_override(true);
    }
    print_banner();
    let cli = Cli::parse();
    match cli.command {
        Commands::Create { names, from } => cmd_create(&names, from.as_deref()),
        Commands::Temp { name, node, npm, rust } => cmd_temp(name.as_deref(), node.as_deref(), npm.as_deref(), rust.as_deref()),
        Commands::Promote { name, temp_alias } => cmd_promote(name.as_deref(), temp_alias.as_deref()),
        Commands::Discard { name, temp_alias } => cmd_discard(name.as_deref(), temp_alias.as_deref()),
        Commands::List => cmd_list(),
        Commands::Switch { name, enter } => cmd_switch(name.as_deref(), &enter),
        Commands::Delete { names, force } => cmd_delete(&names, force),
        Commands::Status { name } => cmd_status(name.as_deref()),
        Commands::Clean { age } => cmd_clean(age.as_deref()),
        Commands::Rollback { name, target } => cmd_rollback(&name, &target),
        Commands::Toolchain { lang } => cmd_toolchain(lang),
        Commands::Add { package, version } => cmd_add(&package, version.as_deref()),
        Commands::Remove { package } => cmd_remove(&package),
        Commands::Search { query } => cmd_search(&query),
        Commands::Packages => cmd_packages(),
        Commands::History { cmd } => cmd_history(&cmd),
        Commands::Flake { cmd } => cmd_flake(&cmd),
        Commands::Banner => cmd_banner(),
        Commands::SwitchBin { name, force } => cmd_switch_bin(&name, force),
        Commands::RestoreBin => cmd_restore_bin(),
        Commands::Snapshots { name } => cmd_snapshots(&name),
        Commands::Share { name, hash } => cmd_share(&name, hash.as_deref()),
        Commands::DeleteSnapshot { name, hash, force } => cmd_delete_snapshot(&name, &hash, force),
    }
}

fn print_ascii_banner() {
    let font = FIGfont::standard().unwrap();
    let figure = font.convert("SFC");
    
    if let Some(fig) = figure {
        let lines: Vec<&str> = fig.to_string().lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let color = match i % 5 {
                0 => CtColor::Magenta,
                1 => CtColor::Blue, 
                2 => CtColor::Cyan,
                3 => CtColor::Green,
                _ => CtColor::Yellow,
            };
            let _ = execute!(
                stdout(),
                SetForegroundColor(color),
                Print(format!("    {}\n", line)),
                ResetColor
            );
        }
    }
    
    // Add animated border
    print_animated_border();
}

fn print_animated_border() {
    let border_chars = "▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱▰▱";
    let width = 80;
    
    for i in 0..3 {
        let color = match i {
            0 => CtColor::Magenta,
            1 => CtColor::Blue,
            _ => CtColor::Cyan,
        };
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(color),
            Print(format!("    {}\n", border_chars.chars().take(width).collect::<String>())),
            ResetColor
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn print_banner() {
    let mut out = stdout();
    
    // Create a dramatic effect with colors
    let _ = queue!(out,
        SetForegroundColor(CtColor::Magenta),
        Print("⚡ "),
        SetForegroundColor(CtColor::Blue),
        Print("SFC"),
        ResetColor,
    );
    
    // Show current container with enhanced styling
    if let Ok(Some(current)) = core::current_container() {
        let _ = queue!(out,
            Print(" "),
            SetBackgroundColor(CtColor::DarkBlue),
            SetForegroundColor(CtColor::White),
            Print("📦"),
            Print(&current),
            ResetColor,
        );
    } else {
        let _ = queue!(out,
            Print(" "),
            SetForegroundColor(CtColor::DarkYellow),
            Print("⚠️ no-container"),
            ResetColor,
        );
    }
    
    let _ = queue!(out, Print(" "));
    let _ = out.flush();
}

fn _ensure_workspace_layout(root: &Path) -> Result<()> { core::ensure_workspace_layout(root) }

fn workspace_root() -> Result<PathBuf> { core::workspace_root() }

fn validate_name(name: &str) -> Result<()> { core::validate_name(name) }



fn cmd_create(names: &[String], from_hash: Option<&str>) -> Result<()> {
    if names.is_empty() {
        return Err(anyhow!("no container names provided"));
    }
    let root = workspace_root()?;
    
    let mut created_names = Vec::new();
    let mut any_error = false;
    
    for name in names {
        let create_one = || -> Result<()> {
            validate_name(name)?;
            let container_dir = root.join("containers").join(name);
            if container_dir.exists() {
                return Err(anyhow!("container '{}' already exists", name));
            }
            fs::create_dir_all(container_dir.join("src"))?;
            fs::create_dir_all(container_dir.join("temp"))?;

            let snapshot_dir = if let Some(hash) = from_hash {
                // Recreate from existing snapshot hash
                println!("🔄 {} container '{}' from snapshot {}", 
                        "Recreating".yellow().bold(),
                        name.cyan(),
                        &hash[..12.min(hash.len())].bright_yellow());
                
                recreate_from_snapshot(&root, name, hash)?
            } else {
                // Create new snapshot
                let snapshot_dir = core::create_snapshot_dir(&root, "snapshot-000")?;
                core::seed_lockfiles(&snapshot_dir)?;
                
                // Auto-generate hash for new snapshot
                let hash = core::compute_snapshot_hash(&snapshot_dir)?;
                println!("{} {} at snapshot {}", 
                        "Created container".green(), 
                        name.bold(),
                        &hash[..12].bright_yellow());
                
                snapshot_dir
            };

            let alias = format!("{}-stable", name);
            let rel = Path::new("../store").join(snapshot_dir.file_name().unwrap());
            core::link_alias_to_store(&root, &alias, &rel)?;

            let container_stable = container_dir.join("stable");
            core::create_or_update_symlink(Path::new("../../links").join(&alias), &container_stable)?;

            Ok(())
        };

        if let Err(e) = create_one() {
            any_error = true;
            eprintln!("{} {}: {}", "Error creating".red(), name, e);
        } else {
            created_names.push(name.clone());
        }
    }
    
    // If only one container was created, switch to it and auto-enter
    if created_names.len() == 1 {
        core::set_current_container(&created_names[0])?;
        println!("{} {}", "Switched to container".cyan(), created_names[0].bold());
        
        // Auto-enter the container
        let workspace = workspace_root()?;
        let container = ContainerConfig::load(&workspace, &created_names[0])?;
        container.enter_shell(&workspace)?;
    }

    if any_error {
        return Err(anyhow!("one or more containers failed to create"));
    }
    Ok(())
}

fn recreate_from_snapshot(root: &Path, container_name: &str, hash: &str) -> Result<PathBuf> {
    // Find the source snapshot
    let source_snapshot = core::find_snapshot_by_hash(root, hash)
        .map_err(|_| anyhow!("Snapshot with hash '{}' not found", &hash[..12.min(hash.len())]))?;
    
    // Create new snapshot by copying from source
    let new_snapshot_dir = core::create_snapshot_dir(root, "snapshot-recreated")?;
    
    // Copy all files from source snapshot
    copy_dir_all(&source_snapshot, &new_snapshot_dir)?;
    
    // Load and recreate the container configuration if it exists
    let share_info = core::generate_share_info(root, "temp", hash)?;
    
    // Create container config
    let container_config_path = root.join(".sfc").join("containers").join(format!("{}.toml", container_name));
    let mut container = ContainerConfig::new(container_name.to_string());
    
    // Add packages from the shared snapshot
    for package in &share_info.packages {
        use crate::container::{PackageSpec, PackageSource};
        let spec = PackageSpec {
            name: package.name.clone(),
            version: package.version.clone(),
            channel: Some("stable".to_string()),
            source: match package.source.as_str() {
                "github" => PackageSource::GitHub { 
                    repo: "unknown/unknown".to_string(), 
                    rev: "main".to_string() 
                },
                "url" => PackageSource::Url("unknown".to_string()),
                _ => PackageSource::Nixpkgs,
            },
        };
        container.add_package(spec)?;
    }
    
    container.save(root)?;
    
    println!("📦 {} {} packages and {} toolchains", 
            "Recreated".green(),
            share_info.packages.len().to_string().cyan(),
            share_info.toolchains.len().to_string().cyan());
    
    Ok(new_snapshot_dir)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// delegations removed in favor of library module

fn cmd_temp(name: Option<&str>, node: Option<&str>, npm: Option<&str>, rust: Option<&str>) -> Result<()> {
    let name = match name {
        Some(n) => n.to_string(),
        None => match core::current_container()? {
            Some(current) => current,
            None => return Err(anyhow!("no current container selected; use 'sfc switch' to select one")),
        }
    };
    validate_name(&name)?;
    let root = workspace_root()?;
    let alias = format!("{}-temp-{}", name, monotonic_suffix());
    let temp_snapshot = core::create_snapshot_dir(&root, "snapshot-temp")?;
    // Copy lockfiles from stable snapshot into temp snapshot
    let stable_snapshot = core::resolve_stable_snapshot(&root, &name)?;
    core::copy_lockfiles(&stable_snapshot, &temp_snapshot)?;

    // Optional toolchain setup inside this snapshot
    if node.is_some() || npm.is_some() || rust.is_some() {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
        pb.set_message("Installing toolchains...");
        let result = core::setup_toolchains(&temp_snapshot, node, npm, rust);
        pb.finish_and_clear();
        match result {
            Ok(()) => println!("{}", "Toolchains installed in temp snapshot".green()),
            Err(e) => {
                eprintln!("{} {}", "Toolchain setup failed:".red().bold(), e);
                eprintln!("{}", "Proceeding without toolchain installs".yellow());
            }
        }
    }

    // link alias in links/
    let rel = Path::new("../store").join(temp_snapshot.file_name().unwrap());
    core::link_alias_to_store(&root, &alias, &rel)?;
    println!("{} {} -> {}", "Temp created".green(), name.bold(), alias.cyan());
    Ok(())
}

fn monotonic_suffix() -> String {
    // timestamp-based suffix for readability
    let now = chrono::Utc::now();
    now.format("%Y%m%d%H%M%S").to_string()
}

fn cmd_promote(name: Option<&str>, temp_alias: Option<&str>) -> Result<()> {
    let name = match name {
        Some(n) => n.to_string(),
        None => match core::current_container()? {
            Some(current) => current,
            None => return Err(anyhow!("no current container selected; use 'sfc switch' to select one")),
        }
    };
    let root = workspace_root()?;
    let chosen = match temp_alias {
        Some(alias) => alias.to_string(),
        None => core::find_latest_temp_alias(&root, &name)?.ok_or_else(|| anyhow!("no temp snapshots for {}", name))?,
    };
    let link_path = root.join("links").join(&chosen);
    if !link_path.exists() {
        return Err(anyhow!("temp alias not found: {}", chosen));
    }

    let new_target_rel = fs::read_link(&link_path)?; // ../store/<dir>
    // Compute change summary and generation hash info
    let old_stable = root.join("links").join(format!("{}-stable", name));
    let old_target_rel = fs::read_link(&old_stable).ok();
    let old_abs = old_target_rel.as_ref().and_then(|rel| old_stable.parent().map(|p| p.join(rel))).and_then(|p| p.canonicalize().ok());
    let new_abs = old_stable.parent().unwrap().join(&new_target_rel).canonicalize()?;
    let old_hash = old_abs.as_ref().and_then(|p| core::compute_snapshot_hash(p).ok());
    let new_hash = core::compute_snapshot_hash(&new_abs)?;
    let msg = core::build_change_message(old_abs.as_deref(), &new_abs, old_hash.as_deref(), &new_hash)?;

    // Update stable alias atomically (stow or symlink)
    let new_rel = new_target_rel;
    core::link_alias_to_store(&root, &format!("{}-stable", name), &new_rel)?;
    println!("{}", msg);
    println!("{} {} -> {}", "Promoted".green(), name.bold(), chosen.cyan());
    Ok(())
}

fn cmd_discard(name: Option<&str>, temp_alias: Option<&str>) -> Result<()> {
    let name = match name {
        Some(n) => n.to_string(),
        None => match core::current_container()? {
            Some(current) => current,
            None => return Err(anyhow!("no current container selected; use 'sfc switch' to select one")),
        }
    };
    let root = workspace_root()?;
    let alias = match temp_alias {
        Some(a) => a.to_string(),
        None => core::find_latest_temp_alias(&root, &name)?.ok_or_else(|| anyhow!("no temp snapshots for {}", name))?,
    };
    let link = root.join("links").join(&alias);
    if link.exists() {
        let target_rel = fs::read_link(&link).ok();
        core::unlink_alias_from_links(&root, &alias)?;
        if let Some(target_rel) = target_rel {
            // if no other links point to this snapshot, we can remove it
            core::try_remove_store_if_orphan(&root, &target_rel)?;
        }
        println!("{} {}", "Discarded temp".yellow(), alias.cyan());
    } else {
        println!("{}", "Nothing to discard".yellow());
    }
    Ok(())
}

// delegations removed in favor of library module

fn cmd_list() -> Result<()> {
    let containers = core::list_containers()?;
    let current = core::current_container()?;
    
    if containers.is_empty() {
        print_empty_workspace_banner();
        return Ok(());
    }
    
    print_containers_banner(&containers, &current);
    
    Ok(())
}

fn print_empty_workspace_banner() {
    let _ = execute!(
        stdout(),
        Print("\n"),
        SetForegroundColor(CtColor::Magenta),
        Print("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰\n"),
        SetForegroundColor(CtColor::Yellow),
        Print("    📦 WORKSPACE IS EMPTY\n"),
        SetForegroundColor(CtColor::Magenta),
        Print("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰\n"),
        ResetColor,
        Print("\n"),
        SetForegroundColor(CtColor::Green),
        Print("    🚀 Get started: "),
        SetForegroundColor(CtColor::Cyan),
        Print("sfc create <name>\n"),
        SetForegroundColor(CtColor::Blue),
        Print("    💡 Example: "),
        SetForegroundColor(CtColor::Green),
        Print("sfc create my-project\n\n"),
        ResetColor
    );
}

fn print_containers_banner(containers: &[String], current: &Option<String>) {
    let _ = execute!(
        stdout(),
        Print("\n"),
        SetForegroundColor(CtColor::Magenta),
        Print("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰\n"),
        SetForegroundColor(CtColor::Cyan),
        Print(&format!("    📦 CONTAINERS ({} total)\n", containers.len())),
        SetForegroundColor(CtColor::Magenta),
        Print("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰\n"),
        ResetColor,
        Print("\n")
    );
    
    for (i, name) in containers.iter().enumerate() {
        let (marker, color) = if current.as_ref() == Some(name) {
            (" ← ACTIVE", CtColor::Green)
        } else {
            ("", CtColor::Cyan)
        };
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::DarkGrey),
            Print(&format!("   {:2}. ", i + 1)),
            SetForegroundColor(color),
            Print(&format!("📦 {}{}\n", name, marker)),
            ResetColor
        );
    }
    
    if let Some(current_name) = current {
        let _ = execute!(
            stdout(),
            Print("\n"),
            SetBackgroundColor(CtColor::DarkBlue),
            SetForegroundColor(CtColor::White),
            Print(&format!(" 🎯 ACTIVE: {} ", current_name)),
            ResetColor,
            Print("\n")
        );
    } else {
        let _ = execute!(
            stdout(),
            Print("\n"),
            SetBackgroundColor(CtColor::DarkYellow),
            SetForegroundColor(CtColor::Black),
            Print(" ⚠️  NO CONTAINER SELECTED "),
            ResetColor,
            Print("\n")
        );
    }
    
    let _ = execute!(
        stdout(),
        Print("\n"),
        SetForegroundColor(CtColor::DarkGrey),
        Print("    Commands: "),
        SetForegroundColor(CtColor::Cyan),
        Print("switch"),
        SetForegroundColor(CtColor::DarkGrey),
        Print(" | "),
        SetForegroundColor(CtColor::Cyan),
        Print("status"),
        SetForegroundColor(CtColor::DarkGrey),
        Print(" | "),
        SetForegroundColor(CtColor::Cyan),
        Print("delete"),
        SetForegroundColor(CtColor::DarkGrey),
        Print(" | "),
        SetForegroundColor(CtColor::Yellow),
        Print("banner"),
        Print("\n\n"),
        ResetColor
    );
}

fn cmd_delete(names: &[String], force: bool) -> Result<()> {
    if names.is_empty() {
        return Err(anyhow!("no container names provided"));
    }
    
    let root = workspace_root()?;
    let current = core::current_container()?;
    let existing_containers = core::list_containers()?;
    
    for name in names {
        // Check if container exists
        if !existing_containers.contains(name) {
            eprintln!("{} Container '{}' does not exist", "⚠️".yellow(), name.red());
            continue;
        }
        
        // Check if trying to delete current container
        if current.as_ref() == Some(name) && !force {
            eprintln!("{} Cannot delete current container '{}'. Use --force to override or switch to another container first.", 
                     "❌".red(), name.red());
            continue;
        }
        
        // Confirmation prompt unless force is used
        if !force {
            print!("{} Delete container '{}' and all its data? [y/N]: ", "🗑️".red(), name.red());
            let _ = std::io::stdout().flush();
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();
            
            if input != "y" && input != "yes" {
                println!("{} Skipping deletion of '{}'", "✋".yellow(), name);
                continue;
            }
        }
        
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.red} {msg}")
                .unwrap()
                .tick_strings(&["🗑️", "🔥", "💥", "⚡"])
        );
        pb.set_message(format!("Deleting container '{}'...", name));
        
        let mut deletion_errors = Vec::new();
        
        // Remove container directory
        let container_dir = root.join("containers").join(name);
        if container_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&container_dir) {
                deletion_errors.push(format!("container directory: {}", e));
            }
        }
        
        // Remove container config
        let config_file = root.join(".sfc").join("containers").join(format!("{}.toml", name));
        if config_file.exists() {
            if let Err(e) = std::fs::remove_file(&config_file) {
                deletion_errors.push(format!("config file: {}", e));
            }
        }
        
        // Remove stable link
        let stable_link = root.join("links").join(format!("{}-stable", name));
        if stable_link.exists() {
            if let Err(e) = std::fs::remove_file(&stable_link) {
                deletion_errors.push(format!("stable link: {}", e));
            }
        }
        
        // Remove all temp links for this container
        if let Ok(entries) = std::fs::read_dir(root.join("links")) {
            let temp_prefix = format!("{}-temp-", name);
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.starts_with(&temp_prefix) {
                        if let Err(e) = std::fs::remove_file(entry.path()) {
                            deletion_errors.push(format!("temp link {}: {}", filename, e));
                        }
                    }
                }
            }
        }
        
        pb.finish_and_clear();
        
        if deletion_errors.is_empty() {
            println!("{} Container '{}' deleted successfully", "✅".green(), name.cyan());
            
            // Clear current container if it was deleted
            if current.as_ref() == Some(name) {
                if let Err(_) = core::set_current_container("") {
                    // Ignore error if clearing fails
                }
                println!("{} Cleared current container selection", "ℹ️".blue());
            }
        } else {
            println!("{} Container '{}' deleted with some errors:", "⚠️".yellow(), name.yellow());
            for error in deletion_errors {
                println!("  - {}", error);
            }
        }
    }
    
    // Run cleanup to remove any orphaned snapshots
    let _ = cmd_clean(None);
    
    Ok(())
}

fn cmd_switch(name: Option<&str>, enter: &bool) -> Result<()> {
    match name {
        Some(n) => {
            // Validate container exists
            let containers = core::list_containers()?;
            if !containers.contains(&n.to_string()) {
                return Err(anyhow!("container '{}' not found", n));
            }
            core::set_current_container(n)?;
            println!("{} {}", "Switched to container".cyan(), n.bold());
            
            if *enter {
                // Load container and enter shell
                let workspace = core::workspace_root()?;
                let container = ContainerConfig::load(&workspace, n)?;
                container.enter_shell(&workspace)?;
            } else {
                println!("\nTo enter the container shell, run:");
                println!("  {} or {}", "sfc switch -c".cyan(), format!("cd ~/.sfc/containers/{}", n).cyan());
            }
            Ok(())
        }
        None => {
            // Interactive selection
            let containers = core::list_containers()?;
            if containers.is_empty() {
                println!("{}", "No containers found. Create one with: sfc create <name>".yellow());
                return Ok(());
            }
            
            println!("{}", "Available containers:".bold());
            for (i, name) in containers.iter().enumerate() {
                println!("  {} {}", format!("[{}]", i + 1).cyan(), name);
            }
            
            print!("{}", "Select container (number): ".bold());
            let _ = stdout().flush();
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();
            
            if let Ok(num) = input.parse::<usize>() {
                if num > 0 && num <= containers.len() {
                    let selected = &containers[num - 1];
                    core::set_current_container(selected)?;
                    println!("{} {}", "Switched to container".cyan(), selected.bold());
                    
                    if *enter {
                        let workspace = core::workspace_root()?;
                        let container = ContainerConfig::load(&workspace, selected)?;
                        container.enter_shell(&workspace)?;
                    } else {
                        println!("\nTo enter the container shell, run:");
                        println!("  {} or {}", "sfc switch -c".cyan(), format!("cd ~/.sfc/containers/{}", selected).cyan());
                    }
                } else {
                    println!("{}", "Invalid selection".red());
                }
            } else {
                println!("{}", "Invalid input".red());
            }
            Ok(())
        }
    }
}

fn cmd_status(name: Option<&str>) -> Result<()> {
    let name = match name {
        Some(n) => n.to_string(),
        None => match core::current_container()? {
            Some(current) => current,
            None => return Err(anyhow!("❌ No current container selected; use 'sfc switch' to select one")),
        }
    };
    
    println!("📊 {} {}", "Container status for".bold().green(), name.cyan().bold());
    println!("");
    
    let root = workspace_root()?;
    let stable = root.join("links").join(format!("{}-stable", name));
    if !stable.exists() {
        println!("⚠️  {} {}", "No stable environment found for".yellow(), name.red());
        println!("💡 Try creating the container: {}", format!("sfc create {}", name).cyan());
        return Ok(());
    }
    
    let target = fs::read_link(&stable)?;
    println!("✅ {} {} → {}", 
            "Stable".green().bold(),
            name.cyan().bold(), 
            target.display().to_string().dimmed());
    
    // Load container config to show packages
    if let Ok(container) = ContainerConfig::load(&root, &name) {
        println!("");
        println!("📦 {} ({})", 
                "Installed packages".bold(),
                container.packages.len().to_string().cyan().bold());
        
        if container.packages.is_empty() {
            println!("   {} - try {} to add packages", 
                    "No packages installed".dimmed(),
                    "sfc add <package>".cyan());
        } else {
            for (i, pkg) in container.packages.iter().take(5).enumerate() {
                let version = pkg.version.as_ref()
                    .map(|v| format!("@{}", v))
                    .unwrap_or_else(|| "@latest".to_string());
                println!("   {} {} {} {}", 
                        format!("{}.", i + 1).dimmed(),
                        "❄️",
                        pkg.name.cyan(),
                        version.bright_blue());
            }
            if container.packages.len() > 5 {
                println!("   {} and {} more packages", 
                        "...".dimmed(),
                        (container.packages.len() - 5).to_string().yellow());
                println!("   Use {} to see all", "sfc packages".cyan());
            }
        }
    }
    
    // List temporary environments
    let prefix = format!("{}-temp-", name);
    let mut temp_count = 0;
    if let Ok(entries) = fs::read_dir(root.join("links")) {
        for entry in entries.flatten() {
            if entry.path().is_symlink() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.starts_with(&prefix) {
                    if temp_count == 0 {
                        println!("");
                        println!("🧪 {} environments:", "Temporary".yellow().bold());
                    }
                    temp_count += 1;
                    let t = fs::read_link(entry.path())?;
                    let timestamp = fname.trim_start_matches(&prefix);
                    println!("   {} ⚡ {} → {}", 
                            format!("{}.", temp_count).dimmed(),
                            timestamp.bright_yellow(),
                            t.display().to_string().dimmed());
                }
            }
        }
    }
    
    if temp_count == 0 {
        println!("");
        println!("🧪 {} - try {} to create one", 
                "No temporary environments".dimmed(),
                "sfc temp".cyan());
    } else {
        println!("   Use {} to manage temps", "sfc promote/discard".cyan());
    }
    
    println!("");
    println!("🚀 {} {} | {} {}", 
            "Quick actions:".dimmed(),
            "sfc add <package>".cyan(),
            "sfc temp".cyan(),
            "sfc switch -c".cyan());
    
    Ok(())
}

// delegations removed in favor of library module

fn cmd_clean(_age: Option<&str>) -> Result<()> {
    let root = workspace_root()?;
    // Remove dangling symlinks
    for entry in fs::read_dir(root.join("links"))? {
        let entry = entry?;
        if entry.path().is_symlink() {
            match fs::read_link(entry.path()) {
                Ok(target) => {
                    let resolved = entry.path().parent().unwrap().join(target);
                    if !resolved.exists() {
                        println!("{} {}", "Removing dangling link".yellow(), entry.file_name().to_string_lossy());
                        fs::remove_file(entry.path()).ok();
                    }
                }
                Err(_) => {
                    fs::remove_file(entry.path()).ok();
                }
            }
        }
    }
    // Remove unreferenced store dirs
    let referenced: Vec<String> = fs::read_dir(root.join("links"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_symlink())
        .filter_map(|e| fs::read_link(e.path()).ok())
        .filter_map(|rel| rel.file_name().map(|s| s.to_string_lossy().to_string()))
        .collect();
    for entry in fs::read_dir(root.join("store"))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !referenced.contains(&name) {
            println!("{} {}", "Pruning orphaned snapshot".yellow(), name);
            fs::remove_dir_all(entry.path()).ok();
        }
    }
    println!("{}", "Clean completed".green());
    Ok(())
}

fn cmd_rollback(name: &str, target: &str) -> Result<()> {
    let root = workspace_root()?;
    // target can be a commit-ish or a direct snapshot name. Here we accept direct snapshot dir name under store/
    let candidate = root.join("store").join(target);
    if !candidate.exists() {
        return Err(anyhow!("target snapshot not found in store/: {}", target));
    }
    // Compute change summary and generation hash info
    let stable_alias = root.join("links").join(format!("{}-stable", name));
    let old_target_rel = fs::read_link(&stable_alias).ok();
    let old_abs = old_target_rel.as_ref().and_then(|rel| stable_alias.parent().map(|p| p.join(rel))).and_then(|p| p.canonicalize().ok());
    let new_abs = root.join("store").join(target).canonicalize()?;
    let old_hash = old_abs.as_ref().and_then(|p| core::compute_snapshot_hash(p).ok());
    let new_hash = core::compute_snapshot_hash(&new_abs)?;
    let msg = core::build_change_message(old_abs.as_deref(), &new_abs, old_hash.as_deref(), &new_hash)?;

    let alias = format!("{}-stable", name);
    let rel = Path::new("../store").join(target);
    core::link_alias_to_store(&root, &alias, &rel)?;
    println!("{}", msg);
    println!("{} {} -> {}", "Rolled back".green(), name.bold(), target.cyan());
    Ok(())
}


fn cmd_toolchain(lang: ToolchainLang) -> Result<()> {
    match lang {
        ToolchainLang::Node { cmd } => match cmd {
            ToolchainCmd::Install { version } => {
                let pb = ProgressBar::new_spinner();
                pb.enable_steady_tick(std::time::Duration::from_millis(80));
                pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
                pb.set_message(format!("Installing node@{}...", version));
                let res = core::toolchain_node_install(&version);
                pb.finish_and_clear();
                match res {
                    Ok(out) => { println!("{}\n{}", "Installed".green().bold(), out.trim()); Ok(()) }
                    Err(e) => { eprintln!("{} {}", "Install failed:".red().bold(), e); Err(e) }
                }
            }
            ToolchainCmd::Ls => { print!("{}", core::toolchain_node_ls()?); Ok(()) }
            ToolchainCmd::Use { version } => { print!("{}", core::toolchain_node_use(&version)?); Ok(()) }
            ToolchainCmd::Remove { version } => { print!("{}", core::toolchain_node_remove(&version)?); Ok(()) }
        },
        ToolchainLang::Rust { cmd } => match cmd {
            ToolchainCmd::Install { version } => {
                let pb = ProgressBar::new_spinner();
                pb.enable_steady_tick(std::time::Duration::from_millis(80));
                pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
                pb.set_message(format!("Installing rust {}...", version));
                let res = core::toolchain_rust_install(&version);
                pb.finish_and_clear();
                match res {
                    Ok(out) => { println!("{}\n{}", "Installed".green().bold(), out.trim()); Ok(()) }
                    Err(e) => { eprintln!("{} {}", "Install failed:".red().bold(), e); Err(e) }
                }
            }
            ToolchainCmd::Ls => { print!("{}", core::toolchain_rust_ls()?); Ok(()) }
            ToolchainCmd::Use { version } => { print!("{}", core::toolchain_rust_use(&version)?); Ok(()) }
            ToolchainCmd::Remove { version } => { print!("{}", core::toolchain_rust_remove(&version)?); Ok(()) }
        },
    }
}

fn cmd_add(package: &str, version: Option<&str>) -> Result<()> {
    let workspace = workspace_root()?;
    let current_container = core::current_container()?
        .ok_or_else(|| anyhow!("no current container selected; use 'sfc switch' to select one"))?;
    
    let mut container = ContainerConfig::load(&workspace, &current_container)?;
    let pkg_mgr = PackageManager::new(workspace);
    
    let spec = if let Some(v) = version {
        format!("{}@{}", package, v)
    } else {
        package.to_string()
    };
    
    pkg_mgr.add_package(&mut container, &spec)?;
    Ok(())
}

fn cmd_remove(package: &str) -> Result<()> {
    let workspace = workspace_root()?;
    let current_container = core::current_container()?
        .ok_or_else(|| anyhow!("no current container selected; use 'sfc switch' to select one"))?;
    
    let mut container = ContainerConfig::load(&workspace, &current_container)?;
    let pkg_mgr = PackageManager::new(workspace);
    
    pkg_mgr.remove_package(&mut container, package)?;
    Ok(())
}

fn cmd_search(query: &str) -> Result<()> {
    let workspace = workspace_root()?;
    let pkg_mgr = PackageManager::new(workspace);
    pkg_mgr.search_packages(query)?;
    Ok(())
}

fn cmd_packages() -> Result<()> {
    let workspace = workspace_root()?;
    let current_container = core::current_container()?
        .ok_or_else(|| anyhow!("no current container selected; use 'sfc switch' to select one"))?;
    
    let container = ContainerConfig::load(&workspace, &current_container)?;
    let pkg_mgr = PackageManager::new(workspace);
    pkg_mgr.list_packages(&container)?;
    Ok(())
}

fn cmd_history(cmd: &HistoryCmd) -> Result<()> {
    let workspace = workspace_root()?;
    let history = History::load(&workspace)?;
    
    match cmd {
        HistoryCmd::Log { container } => {
            history.print_log(container.as_deref())?;
        }
        HistoryCmd::Graph { container } => {
            history.visualize_graph(container.as_deref())?;
        }
        HistoryCmd::Rollback { hash } => {
            if let Some(entry) = history.find_by_hash(hash) {
                println!("{} to hash {}", "Rolling back".yellow(), hash.bright_yellow());
                println!("Operation: {:?}", entry.operation);
                // TODO: Implement actual rollback logic
            } else {
                return Err(anyhow!("Hash '{}' not found in history", hash));
            }
        }
    }
    Ok(())
}

fn cmd_flake(cmd: &FlakeCmd) -> Result<()> {
    let workspace = workspace_root()?;
    let current_container = core::current_container()?
        .ok_or_else(|| anyhow!("no current container selected; use 'sfc switch' to select one"))?;
    
    let container = ContainerConfig::load(&workspace, &current_container)?;
    
    match cmd {
        FlakeCmd::Generate => {
            let flake = container.to_flake();
            flake.save(&workspace, &current_container)?;
            println!("{} flake.nix for container {}", "Generated".green(), current_container.cyan());
            println!("Location: ~/.sfc/containers/{}/flake.nix", current_container);
        }
        FlakeCmd::Push { repo } => {
            println!("{} pushing to {}", "TODO:".yellow(), repo.cyan());
            // TODO: Implement GitHub push
        }
        FlakeCmd::Pull { repo } => {
            println!("{} pulling from {}", "TODO:".yellow(), repo.cyan());
            // TODO: Implement GitHub pull
        }
    }
    Ok(())
}

fn cmd_banner() -> Result<()> {
    // Clear screen for dramatic effect
    let _ = execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0));
    
    print_ascii_banner();
    
    // Add version and system info with dramatic styling
    println!("");
    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::DarkGrey),
        Print("    "),
        SetForegroundColor(CtColor::Magenta),
        Print("▰▰▰ "),
        SetForegroundColor(CtColor::White),
        Print("Suffix Container Framework "),
        SetForegroundColor(CtColor::Magenta), 
        Print("▰▰▰\n"),
        ResetColor
    );
    
    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::Cyan),
        Print("    Version: "),
        SetForegroundColor(CtColor::Yellow),
        Print(env!("CARGO_PKG_VERSION")),
        ResetColor,
        Print("\n\n")
    );
    
    // Show current workspace status with animation
    if let Ok(containers) = core::list_containers() {
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Green),
            Print("    Workspace Status: "),
            SetForegroundColor(CtColor::Cyan),
            Print(&format!("{} containers", containers.len())),
            ResetColor,
            Print("\n")
        );
        
        if let Ok(Some(current)) = core::current_container() {
            let _ = execute!(
                stdout(),
                SetForegroundColor(CtColor::Blue),
                Print("    Active Container: "),
                SetBackgroundColor(CtColor::DarkBlue),
                SetForegroundColor(CtColor::White),
                Print(&format!(" {} ", current)),
                ResetColor,
                Print("\n")
            );
        }
    }
    
    println!("");
    animate_startup_sequence();
    
    Ok(())
}

fn animate_startup_sequence() {
    let steps = [
        ("⚡", "Initializing", CtColor::Yellow),
        ("🔧", "Loading modules", CtColor::Blue),
        ("📦", "Container system", CtColor::Green),
        ("✨", "Ready!", CtColor::Magenta),
    ];
    
    for (emoji, text, color) in &steps {
        let _ = execute!(
            stdout(),
            SetForegroundColor(*color),
            Print(&format!("    {} {}", emoji, text))
        );
        
        // Animated dots
        for _ in 0..3 {
            thread::sleep(Duration::from_millis(200));
            let _ = execute!(stdout(), Print("."));
        }
        
        let _ = execute!(
            stdout(),
            SetForegroundColor(CtColor::Green),
            Print(" ✓\n"),
            ResetColor
        );
        
        thread::sleep(Duration::from_millis(100));
    }
    
    println!("");
    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::Cyan),
        Print("    Ready to manage containers! Try: "),
        SetForegroundColor(CtColor::Yellow),
        Print("sfc list"),
        ResetColor,
        Print("\n\n")
    );
}

fn cmd_switch_bin(name: &str, force: bool) -> Result<()> {
    // Check if running as root/sudo
    if !nix::unistd::Uid::effective().is_root() {
        return Err(anyhow!("❌ This command requires sudo privileges: sudo sfc switch-bin {}", name));
    }

    let workspace = workspace_root()?;
    let containers = core::list_containers()?;
    
    if !containers.contains(&name.to_string()) {
        return Err(anyhow!("❌ Container '{}' not found", name));
    }

    println!("🔄 {} system binaries to container '{}'", "Switching".yellow().bold(), name.cyan().bold());
    
    // Check if container has binaries
    let container_bin = workspace.join("containers").join(name).join("local").join("bin");
    if !container_bin.exists() {
        return Err(anyhow!("❌ Container '{}' has no binaries to switch to", name));
    }

    // Check if already switched
    let backup_dir = std::path::Path::new("/usr/local/.sfc-backup");
    if backup_dir.exists() && !force {
        return Err(anyhow!("⚠️ System binaries already switched. Use --force to override or run 'sudo sfc restore-bin' first"));
    }

    core::switch_system_binaries(&container_bin, force)?;
    
    println!("{} System binaries switched to container '{}'", "✅".green(), name.cyan());
    println!("💡 Run {} to restore original binaries", "sudo sfc restore-bin".cyan());
    
    Ok(())
}

fn cmd_restore_bin() -> Result<()> {
    // Check if running as root/sudo
    if !nix::unistd::Uid::effective().is_root() {
        return Err(anyhow!("❌ This command requires sudo privileges: sudo sfc restore-bin"));
    }

    println!("🔄 {} original system binaries", "Restoring".yellow().bold());
    
    let backup_dir = std::path::Path::new("/usr/local/.sfc-backup");
    if !backup_dir.exists() {
        return Err(anyhow!("❌ No backup found. System binaries may not have been switched"));
    }

    core::restore_system_binaries()?;
    
    println!("{} Original system binaries restored", "✅".green());
    
    Ok(())
}

fn cmd_snapshots(name: &str) -> Result<()> {
    let workspace = workspace_root()?;
    let containers = core::list_containers()?;
    
    if !containers.contains(&name.to_string()) {
        return Err(anyhow!("❌ Container '{}' not found", name));
    }

    println!("📸 {} for container '{}'", "Snapshots".bold().green(), name.cyan().bold());
    
    let snapshots = core::list_container_snapshots(&workspace, name)?;
    
    if snapshots.is_empty() {
        println!("   {} No snapshots found", "📭".yellow());
        return Ok(());
    }

    println!("");
    for (i, snapshot) in snapshots.iter().enumerate() {
        let status_icon = if snapshot.is_active { "🎯" } else { "📸" };
        let hash_display = format!("{}", &snapshot.hash[..12]).bright_yellow();
        let time_display = snapshot.timestamp.format("%Y-%m-%d %H:%M:%S").to_string().dimmed();
        
        println!("   {} {} {} {} {}", 
                format!("{:2}.", i + 1).dimmed(),
                status_icon,
                hash_display,
                time_display,
                snapshot.description.cyan());
    }
    
    println!("");
    println!("💡 Use {} to share or {} to recreate", 
            "sfc share".cyan(), 
            "sfc create --from <hash>".cyan());
    
    Ok(())
}

fn cmd_share(name: &str, hash: Option<&str>) -> Result<()> {
    let workspace = workspace_root()?;
    let containers = core::list_containers()?;
    
    if !containers.contains(&name.to_string()) {
        return Err(anyhow!("❌ Container '{}' not found", name));
    }

    let snapshot_hash = match hash {
        Some(h) => h.to_string(),
        None => {
            // Use current stable snapshot
            core::get_current_snapshot_hash(&workspace, name)?
        }
    };

    println!("🔗 {} snapshot {} for container '{}'", 
            "Sharing".yellow().bold(),
            &snapshot_hash[..12].bright_yellow(),
            name.cyan().bold());

    let share_info = core::generate_share_info(&workspace, name, &snapshot_hash)?;
    
    println!("");
    println!("📋 {} this command to recreate the environment:", "Share".green().bold());
    println!("");
    println!("   {}", format!("sfc create {} --from {}", name, snapshot_hash).on_bright_black().white());
    println!("");
    println!("📦 {} packages in this snapshot:", "Included".blue());
    for package in &share_info.packages {
        println!("   • {} {}", package.name.cyan(), package.version.as_deref().unwrap_or("latest").dimmed());
    }
    
    if !share_info.toolchains.is_empty() {
        println!("");
        println!("🛠️  {} toolchains:", "Included".blue());
        for (toolchain, version) in &share_info.toolchains {
            println!("   • {} {}", toolchain.cyan(), version.dimmed());
        }
    }
    
    Ok(())
}

fn cmd_delete_snapshot(name: &str, hash: &str, force: bool) -> Result<()> {
    let workspace = workspace_root()?;
    let containers = core::list_containers()?;
    
    if !containers.contains(&name.to_string()) {
        return Err(anyhow!("❌ Container '{}' not found", name));
    }

    // Check if trying to delete active snapshot
    let current_hash = core::get_current_snapshot_hash(&workspace, name)?;
    if hash.starts_with(&current_hash[..hash.len()]) && !force {
        return Err(anyhow!("❌ Cannot delete active snapshot '{}'. Use --force to override or switch to another snapshot first", &hash[..12.min(hash.len())]));
    }

    if !force {
        print!("🗑️  Delete snapshot {} for container '{}'? [y/N]: ", 
               &hash[..12.min(hash.len())].red(), 
               name.red());
        let _ = std::io::stdout().flush();
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        
        if input != "y" && input != "yes" {
            println!("✋ {} deletion", "Cancelled".yellow());
            return Ok(());
        }
    }

    println!("🗑️  {} snapshot {}", "Deleting".yellow().bold(), &hash[..12.min(hash.len())].red());
    
    core::delete_snapshot(&workspace, name, hash)?;
    
    println!("{} Snapshot deleted successfully", "✅".green());
    
    Ok(())
}
