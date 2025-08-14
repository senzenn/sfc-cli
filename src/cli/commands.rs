use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "sfc", version, about = "Suffix-container CLI (symlink-based environment management)")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
    
    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,
    
    /// Workspace path (defaults to ~/.sfc)
    #[arg(short, long)]
    pub workspace: Option<std::path::PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create one or more containers
    Create {
        names: Vec<String>,
        #[arg(long, help = "Recreate from snapshot hash")]
        from: Option<String>,
    },

    /// Open a temp environment (uses current container if name not provided)
    Temp {
        name: Option<String>,
        #[arg(long)] node: Option<String>,
        #[arg(long)] npm: Option<String>,
        #[arg(long)] rust: Option<String>,
    },

    /// Promote a temp snapshot to stable (uses current container if name not provided)
    Promote { 
        name: Option<String>, 
        temp_alias: Option<String> 
    },

    /// Discard a temp snapshot (uses current container if name not provided)
    Discard { 
        name: Option<String>, 
        temp_alias: Option<String> 
    },

    /// List containers and temps
    List,

    /// Switch to a container (or show selection if no name provided)
    Switch {
        name: Option<String>,
        #[arg(short = 'c', long = "cd", help = "Enter container shell")]
        enter: bool,
    },

    /// Delete a container and all its data
    Delete {
        names: Vec<String>,
        #[arg(short = 'f', long = "force")]
        force: bool,
    },

    /// Show status for NAME
    Status { 
        name: Option<String> 
    },

    /// Clean dangling links and orphaned store snapshots
    Clean { 
        #[arg(long, help = "Remove snapshots older than specified age (e.g., '30d', '1w')")]
        age: Option<String> 
    },

    /// Rollback NAME to a previous stable link target
    Rollback { 
        name: String, 
        target: String 
    },

    /// Manage shared toolchains stored under workspace .sfc/toolchains
    Toolchain {
        #[command(subcommand)]
        lang: ToolchainLang,
    },

    /// Add a package to current container
    Add {
        package: String,
        #[arg(short, long)]
        version: Option<String>,
    },

    /// Remove a package from current container
    Remove { 
        package: String 
    },

    /// Search for packages
    Search { 
        query: String 
    },

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

    /// Switch system binaries to use container binaries (requires sudo)
    SwitchBin {
        name: String,
        #[arg(long)]
        force: bool,
    },

    /// Restore system binaries to original state (requires sudo)
    RestoreBin,

    /// List all snapshots for a container
    Snapshots { 
        name: String 
    },

    /// Share a container snapshot for others to recreate
    Share {
        name: String,
        hash: Option<String>,
    },

    /// Delete a specific snapshot
    DeleteSnapshot {
        name: String,
        hash: String,
        #[arg(short = 'f', long = "force")]
        force: bool,
    },

    /// Show animated SFC banner
    Banner,

    /// Show configuration information
    Config {
        #[command(subcommand)]
        cmd: Option<ConfigCmd>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ToolchainLang {
    /// Node via Volta
    Node { 
        #[command(subcommand)] 
        cmd: ToolchainCmd 
    },
    /// Rust via rustup
    Rust { 
        #[command(subcommand)] 
        cmd: ToolchainCmd 
    },
}

#[derive(Subcommand, Debug)]
pub enum ToolchainCmd {
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
pub enum HistoryCmd {
    /// Show history log (like git reflog)
    Log { container: Option<String> },
    /// Show visual graph of container history
    Graph { container: Option<String> },
    /// Rollback to a specific hash
    Rollback { hash: String },
}

#[derive(Subcommand, Debug)]
pub enum FlakeCmd {
    /// Generate flake.nix for current container
    Generate,
    /// Push container config to GitHub
    Push { repo: String },
    /// Pull container config from GitHub
    Pull { repo: String },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Show current configuration
    Show,
    /// Edit configuration
    Edit,
    /// Reset configuration to defaults
    Reset,
    /// Set a configuration value
    Set { key: String, value: String },
    /// Get a configuration value
    Get { key: String },
}
