use std::env;
use std::process::{Command, Stdio};

use crate::core::WorkspaceManager;
use crate::error::{Result, SfcError};
use crate::cli::ui::{print_success, print_error};
use owo_colors::OwoColorize;
use crossterm::style::Stylize;

/// Handle container creation
pub fn handle_create(workspace: &WorkspaceManager, names: &[String], from: Option<&str>) -> Result<()> {
    // TODO: Implement container creation logic
    print_success(&format!("Container creation not yet implemented. Would create: {:?}", names));
    Ok(())
}

/// Handle container listing
pub fn handle_list(workspace: &WorkspaceManager) -> Result<()> {
    // TODO: Implement container listing logic
    print_success("Container listing not yet implemented");
    Ok(())
}

/// Handle container switching
pub fn handle_switch(workspace: &WorkspaceManager, name: Option<&str>, enter: bool) -> Result<()> {
    // TODO: Implement container switching logic
    print_success(&format!("Container switching not yet implemented. Name: {:?}, Enter: {}", name, enter));
    Ok(())
}

/// Handle container deletion
pub fn handle_delete(workspace: &WorkspaceManager, names: &[String], force: bool) -> Result<()> {
    // TODO: Implement container deletion logic
    print_success(&format!("Container deletion not yet implemented. Names: {:?}, Force: {}", names, force));
    Ok(())
}

/// Handle container status display
pub fn handle_status(workspace: &WorkspaceManager, name: Option<&str>) -> Result<()> {
    // TODO: Implement container status logic
    print_success(&format!("Container status not yet implemented. Name: {:?}", name));
    Ok(())
}

/// Handle container rollback
pub fn handle_rollback(workspace: &WorkspaceManager, name: &str, target: &str) -> Result<()> {
    // TODO: Implement container rollback logic
    print_success(&format!("Container rollback not yet implemented. Name: {}, Target: {}", name, target));
    Ok(())
}

/// Handle workspace cleanup
pub fn handle_clean(workspace: &WorkspaceManager, age: Option<&str>) -> Result<()> {
    // TODO: Implement cleanup logic
    print_success(&format!("Cleanup not yet implemented. Age filter: {:?}", age));
    Ok(())
}

/// Handle banner display
pub fn handle_banner() -> Result<()> {
    // TODO: Implement banner logic
    print_success("Banner display not yet implemented");
    Ok(())
}

/// Handle temporary shell environment (like nix shell)
pub fn handle_shell(workspace: &WorkspaceManager, container: Option<&str>, command: Option<&str>, keep: bool) -> Result<()> {
    use crate::container::ContainerConfig;

    // Determine which container to use
    let container_name = match container {
        Some(name) => name.to_string(),
        None => match workspace.current_container()? {
            Some(current) => current,
            None => return Err(crate::error::SfcError::NotFound {
                resource: "container".to_string(),
                identifier: "current".to_string(),
            }),
        }
    };

    // Load the container configuration
    let container_config = ContainerConfig::load(&workspace.root, &container_name)?;

    // Get current working directory
    let current_dir = env::current_dir()?;

    println!("{} temporary shell for container '{}' in {}", "Starting".green(), container_name.cyan(), current_dir.display());

    // Build environment like enter_shell but for current directory
    let mut env = container_config.environment.clone();
    env.insert("SFC_CONTAINER".to_string(), container_name.clone());
    env.insert("SFC_WORKSPACE".to_string(), workspace.root.to_string_lossy().to_string());
    env.insert("SFC_TEMP_SHELL".to_string(), "1".to_string());

    // Set PS1 to show it's a temp shell
    let ps1 = format!("\\[\\033[33m\\]sfc-temp[{}]\\[\\033[0m\\] \\w $ ", container_name);
    env.insert("PS1".to_string(), ps1);

    // Show active packages
    let package_names: Vec<String> = container_config.packages.iter().map(|p| p.name.clone()).collect();
    if !package_names.is_empty() {
        println!("{} packages: {}", "Active".dimmed(), package_names.join(", "));
    }

    // Execute command or start interactive shell
    let shell_result = if let Some(cmd_str) = command {
        // Run the specified command
        println!("{}: {}", "Running command".blue(), cmd_str);

        let mut cmd = Command::new(&container_config.shell);
        cmd.current_dir(&current_dir);
        cmd.env("SHELL_COMMAND", cmd_str);

        // Set up environment
        for (k, v) in env {
            cmd.env(k, v);
        }

        // Use shell to execute the command
        cmd.arg("-c").arg(cmd_str);

        let status = cmd.status()?;
        Ok(status.success())
    } else {
        // Start interactive shell
        let mut cmd = Command::new(&container_config.shell);
        cmd.current_dir(&current_dir);

        // Set up environment
        for (k, v) in env {
            cmd.env(k, v);
        }

        // Make it interactive
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let status = cmd.status()?;
        Ok(status.success())
    };

    match shell_result {
        Ok(true) => {
            if !keep {
                println!("{}", "Temporary shell session ended".dimmed());
            } else {
                println!("{}", "Environment preserved (--keep specified)".yellow());
            }
            Ok(())
        }
        Ok(false) => {
            Err(crate::error::SfcError::System {
                operation: "shell execution".to_string(),
                reason: "command exited with non-zero status".to_string(),
            })
        }
        Err(e) => Err(e),
    }
}
