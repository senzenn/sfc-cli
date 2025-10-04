use std::io::{stdout, Write};
use std::thread;
use std::time::Duration;

use owo_colors::OwoColorize;
use crossterm::{
    queue, execute,
    style::{Color as CtColor, SetForegroundColor, ResetColor, Print, SetBackgroundColor},
    terminal::{Clear, ClearType},
    cursor::MoveTo,
};
use indicatif::{ProgressBar, ProgressStyle};
use figlet_rs::FIGfont;

use crate::error::SfcError;

/// Print a banner with current container info
pub fn print_banner() {
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
    if let Ok(workspace) = crate::core::WorkspaceManager::default() {
        if let Ok(Some(current)) = workspace.current_container() {
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
    }

    let _ = queue!(out, Print(" "));
    let _ = out.flush();
}

/// Print ASCII art banner
pub fn print_ascii_banner() {
    let font = FIGfont::standard().unwrap();
    let figure = font.convert("SFC");

    if let Some(fig) = figure {
        let fig_string = fig.to_string();
        let lines: Vec<&str> = fig_string.lines().collect();
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

    print_animated_border();
}

/// Print animated border
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

/// Print success message
pub fn print_success(message: &str) {
    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::Green),
        Print("✅ "),
        Print(message),
        Print("\n"),
        ResetColor
    );
}

/// Print warning message
pub fn print_warning(message: &str) {
    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::Yellow),
        Print("⚠️  "),
        Print(message),
        Print("\n"),
        ResetColor
    );
}

/// Print error message
pub fn print_error(error: &SfcError) {
    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::Red),
        Print("❌ "),
        Print(&format!("{}", error)),
        Print("\n"),
        ResetColor
    );
}

/// Print info message
pub fn print_info(message: &str) {
    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::Blue),
        Print("ℹ️  "),
        Print(message),
        Print("\n"),
        ResetColor
    );
}

/// Print containers banner when listing containers
pub fn print_containers_banner(containers: &[String], current: &Option<String>) {
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

/// Print empty workspace banner
pub fn print_empty_workspace_banner() {
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

/// Create a progress bar with a specific style
pub fn create_progress_bar(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {wide_msg}")
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
    pb.set_message(message.to_string());
    pb
}

/// Create a deletion progress bar
pub fn create_deletion_progress_bar(item: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.red} [{elapsed_precise}] {wide_msg}")
            .unwrap()
            .tick_strings(&["🗑️", "🔥", "💥", "⚡"])
    );
    pb.set_message(format!("Deleting {}...", item));
    pb
}

/// Animate startup sequence
pub fn animate_startup_sequence() {
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

/// Print installation header for packages
pub fn print_installation_header(package_name: &str, version: Option<&str>, source: &str) {
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

    let version_display = version.map(|v| format!("@{}", v)).unwrap_or_else(|| "@latest".to_string());

    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::White),
        Print("    📦 Package: "),
        SetForegroundColor(CtColor::Yellow),
        Print(package_name),
        SetForegroundColor(CtColor::Blue),
        Print(&version_display),
        Print("\n"),
        ResetColor
    );

    let _ = execute!(
        stdout(),
        SetForegroundColor(CtColor::Green),
        Print("    📍 Source: "),
        SetForegroundColor(CtColor::Cyan),
        Print(source),
        Print("\n"),
        SetForegroundColor(CtColor::Magenta),
        Print("▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰\n"),
        ResetColor,
        Print("\n")
    );

    thread::sleep(Duration::from_millis(500));
}

/// Print success celebration for package installation
pub fn print_success_celebration(package_name: &str, hash: &str) {
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
        Print(&format!("{} installed successfully!", package_name)),
        Print("\n"),
        SetForegroundColor(CtColor::DarkYellow),
        Print(&format!("    Hash: {}", hash)),
        Print("\n\n"),
        ResetColor
    );
}

/// Get user confirmation for destructive operations
pub fn confirm_destructive_operation(operation: &str, target: &str) -> Result<bool, std::io::Error> {
    print!("⚠️  {} '{}'? [y/N]: ", operation, target.red());
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    Ok(input == "y" || input == "yes")
}

/// Clear screen for dramatic effect
pub fn clear_screen() {
    let _ = execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0));
}
