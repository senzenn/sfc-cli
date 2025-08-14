// Core modules
pub mod error;
pub mod config;
pub mod core;

// Legacy modules (for backwards compatibility)
pub mod sfc;
pub mod container;
pub mod history;
pub mod flake;
pub mod package;

// New modular structure
pub mod cli;
pub mod system;
pub mod sharing;

// Re-exports for convenience
pub use error::{SfcError, Result};
pub use config::SfcConfig;
pub use core::{WorkspaceManager, SnapshotManager, SymlinkManager};
