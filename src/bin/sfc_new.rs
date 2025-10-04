use std::process;

use clap::Parser;
use my_lib::cli::{Cli, Commands};
use my_lib::cli::ui::{print_banner, print_error};
use my_lib::error::{Result, SfcError};
use my_lib::config::SfcConfig;
use my_lib::core::WorkspaceManager;

fn main() {
    // Initialize logging
    if let Err(e) = init_logging() {
        eprintln!("Failed to initialize logging: {}", e);
    }
    
    // Parse CLI arguments
    let cli = Cli::parse();
    
    // Set up global configuration
    if cli.no_color {
        std::env::set_var("NO_COLOR", "1");
    }
    
    // Run the application
    if let Err(error) = run_app(cli) {
        print_error(&error);
        process::exit(1);
    }
}

fn run_app(cli: Cli) -> Result<()> {
    // Load configuration
    let config = load_config(&cli)?;
    
    // Initialize workspace
    let workspace = initialize_workspace(&cli, &config)?;
    
    // Print banner for most commands (except some that should be quiet)
    if should_print_banner(&cli.command) {
        print_banner();
    }
    
    // Dispatch commands
    match cli.command {
        // Container management
        Commands::Create { names, from } => {
            my_lib::cli::handlers::handle_create(&workspace, &names, from.as_deref())
        }
        Commands::List => {
            my_lib::cli::handlers::handle_list(&workspace)
        }
        Commands::Switch { name, enter } => {
            my_lib::cli::handlers::handle_switch(&workspace, name.as_deref(), enter)
        }
        Commands::Delete { names, force } => {
            my_lib::cli::handlers::handle_delete(&workspace, &names, force)
        }
        Commands::Status { name } => {
            my_lib::cli::handlers::handle_status(&workspace, name.as_deref())
        }
        
        // Package management
        Commands::Add { package, version } => {
            my_lib::cli::handlers::handle_add(&workspace, &package, version.as_deref())
        }
        Commands::Remove { package } => {
            my_lib::cli::handlers::handle_remove(&workspace, &package)
        }
        Commands::Search { query } => {
            my_lib::cli::handlers::handle_search(&workspace, &query)
        }
        Commands::Packages => {
            my_lib::cli::handlers::handle_packages(&workspace)
        }
        
        // Environment management
        Commands::Temp { name, node, npm, rust } => {
            my_lib::cli::handlers::handle_temp(&workspace, name.as_deref(), node.as_deref(), npm.as_deref(), rust.as_deref())
        }
        Commands::Promote { name, temp_alias } => {
            my_lib::cli::handlers::handle_promote(&workspace, name.as_deref(), temp_alias.as_deref())
        }
        Commands::Discard { name, temp_alias } => {
            my_lib::cli::handlers::handle_discard(&workspace, name.as_deref(), temp_alias.as_deref())
        }
        Commands::Rollback { name, target } => {
            my_lib::cli::handlers::handle_rollback(&workspace, &name, &target)
        }
        
        // Snapshot management
        Commands::Snapshots { name } => {
            my_lib::cli::handlers::handle_snapshots(&workspace, &name)
        }
        Commands::Share { name, hash } => {
            my_lib::cli::handlers::handle_share(&workspace, &name, hash.as_deref())
        }
        Commands::DeleteSnapshot { name, hash, force } => {
            my_lib::cli::handlers::handle_delete_snapshot(&workspace, &name, &hash, force)
        }
        
        // System integration
        Commands::SwitchBin { name, force } => {
            my_lib::cli::handlers::handle_switch_bin(&workspace, &name, force)
        }
        Commands::RestoreBin => {
            my_lib::cli::handlers::handle_restore_bin()
        }
        
        // Toolchain management
        Commands::Toolchain { lang } => {
            my_lib::cli::handlers::handle_toolchain(&workspace, lang)
        }
        
        // History and visualization
        Commands::History { cmd } => {
            my_lib::cli::handlers::handle_history(&workspace, cmd)
        }
        
        // Flake management
        Commands::Flake { cmd } => {
            my_lib::cli::handlers::handle_flake(&workspace, cmd)
        }
        
        // Maintenance
        Commands::Clean { age } => {
            my_lib::cli::handlers::handle_clean(&workspace, age.as_deref())
        }
        
        // Configuration
        Commands::Config { cmd } => {
            my_lib::cli::handlers::handle_config(&workspace, cmd)
        }
        
        // UI
        Commands::Banner => {
            my_lib::cli::handlers::handle_banner()
        }
        Commands::Shell { container, command, keep } => {
            my_lib::cli::handlers::handle_shell(&workspace, container.as_deref(), command.as_deref(), keep)
        }
    }
}

fn load_config(cli: &Cli) -> Result<SfcConfig> {
    if let Some(workspace_path) = &cli.workspace {
        SfcConfig::merged_config(workspace_path)
    } else {
        SfcConfig::load_global().or_else(|_| Ok(SfcConfig::default()))
    }
}

fn initialize_workspace(cli: &Cli, config: &SfcConfig) -> Result<WorkspaceManager> {
    let workspace_path = if let Some(path) = &cli.workspace {
        path.clone()
    } else {
        config.workspace_path()?
    };
    
    let workspace = WorkspaceManager::new(workspace_path)?;
    
    // Auto-initialize if configured to do so
    if config.workspace.auto_init {
        workspace.ensure_initialized()?;
    }
    
    Ok(workspace)
}

fn should_print_banner(command: &Commands) -> bool {
    match command {
        Commands::Banner => false, // Banner command handles its own output
        Commands::Config { .. } => false, // Config should be minimal
        _ => true,
    }
}

fn init_logging() -> Result<()> {
    // Initialize simple logging
    // In a production app, you might want to use env_logger, tracing, or similar
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cli_parsing() {
        // Test that CLI parsing works correctly
        let cli = Cli::try_parse_from(&["sfc", "list"]).unwrap();
        matches!(cli.command, Commands::List);
    }
    
    #[test]
    fn test_config_loading() {
        // Test configuration loading
        let cli = Cli::try_parse_from(&["sfc", "list"]).unwrap();
        let config = load_config(&cli);
        assert!(config.is_ok());
    }
}
