pub mod snapshot;
pub mod flake;

pub use snapshot::{ShareManager, ShareInfo, share_snapshot, recreate_from_share};
pub use flake::{FlakeManager, generate_nix_flake};