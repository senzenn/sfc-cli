## sfc: Symlink-driven suffix-container CLI

This crate provides a Git-friendly, symlink-based container workflow. It exposes a colorized CLI `sfc` and a reusable Rust library (`my_lib::sfc`).

### Requirements

- Rust (stable) with Cargo
- macOS or Linux (uses Unix symlinks). Windows: use WSL or treat as experimental.

### Build

```bash
cargo build
```

### Run the CLI without installing

```bash
# Show help
cargo run --bin sfc -- --help

# Initialize a workspace
cargo run --bin sfc -- init ~/Projects/sfc-containers

# Create a container
cd ~/Projects/sfc-containers
cargo run --bin sfc -- create myapp42

# Inspect status
cargo run --bin sfc -- status myapp42

# Create a temp snapshot and then promote it
cargo run --bin sfc -- temp myapp42
cargo run --bin sfc -- promote myapp42

# Optional clean-up
cargo run --bin sfc -- clean
```

Tips:
- Symlink pointers live under `links/`. Inspect them with `ls -la links/` and `readlink links/<name>`.
- The content-addressed `store/` is ignored by Git; only symlinks and metadata are tracked.

### Install the CLI locally (optional)

```bash
cargo install --path .
# then use `sfc` directly
sfc --help
```

### Automated tests

This repository includes an integration test that runs the CLI end-to-end in a temporary directory.

Run all tests:
```bash
cargo test
```

Run a specific test:
```bash
cargo test init_and_create_and_status_flow -- --nocapture
```

What the tests cover:
- `init` creates `store/`, `containers/`, `links/`, and `.sfc/` in a temp workspace
- `create` scaffolds a container and stable symlink
- `temp` creates a temp snapshot and link
- `promote` flips the stable symlink to the temp snapshot
- `discard` and `clean` tidy temps and orphaned snapshots
- `rollback` repoints the stable symlink to a specific snapshot

No global state is modified; tests operate in isolated temp dirs.

### Manual verification checklist

```bash
WS=~/Projects/sfc-containers
cargo run --bin sfc -- init "$WS"
cd "$WS"
cargo run --bin sfc -- create demo1
cargo run --bin sfc -- status demo1
cargo run --bin sfc -- temp demo1
cargo run --bin sfc -- promote demo1
ls -la links/
readlink links/demo1-stable
cargo run --bin sfc -- clean

# Rollback to the current stable snapshot name (from readlink)
SNAP=$(readlink links/demo1-stable | xargs basename)
cargo run --bin sfc -- rollback demo1 "$SNAP"
```

### Notes

- The CLI uses ANSI colors. If your environment strips colors, set `NO_COLOR=1`.
- Windows support is experimental; prefer WSL or junctions.


# sfc-cli
