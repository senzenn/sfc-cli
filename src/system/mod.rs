pub mod binary;
pub mod platform;

pub use binary::{BinaryManager, switch_system_binaries, restore_system_binaries};
pub use platform::{detect_platform, detect_package_manager, PlatformInfo};
