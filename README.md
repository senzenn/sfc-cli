# SFC - Suffix Container Framework (WORK IN PROGRESS) 

>> **Work in Progress** - Production Refactor In Progress
```
Core Architecture     ████████████████████ 100%
Error Handling        ████████████████████ 100%  
Configuration System  ████████████████████ 100%
CLI Structure         ████████████████████ 100%
System Integration    ████████████████████ 100%
Sharing & Flakes      ████████████████████ 100%
Command Handlers      ██████░░░░░░░░░░░░░░░  30%
Package Refactor      ░░░░░░░░░░░░░░░░░░░░   0%
Testing & Docs        ░░░░░░░░░░░░░░░░░░░░   0%
```

Symlink-driven container management with O(1) environment switching, multi-source package management, and shareable configurations.

## Quick Start

### Build
```bash
cargo build --release
```

### Basic Usage
```bash
# Legacy CLI (fully functional)
./target/release/sfc list
./target/release/sfc create myapp
./target/release/sfc switch myapp

# New modular CLI (partial implementation)
./target/release/sfc-new list
./target/release/sfc-new config show
```

## Key Features

### [ENV] **Environment Management**
- **O(1) switching** via symlinks
- **Immutable snapshots** with content-based hashing
- **Temp environments** for safe experimentation

### [PKG] **Package Management**
- **Auto-detection**: macOS (Homebrew), Linux (apt/dnf/pacman)
- **Multi-source**: System PMs, Nix, Portable, GitHub
- **GNU Stow integration** with fallback strategies

### [SYS] **System Integration**
- **Binary switching**: `sudo sfc switch-bin myapp`
- **Safe restore**: `sudo sfc restore-bin`
- **Cross-platform** support (macOS, Linux, WSL)

### [SHARE] **Sharing & Collaboration**
- **Snapshot sharing**: `sfc share myapp abc123`
- **Recreate environments**: `sfc create project --from abc123`
- **Nix flake generation** for reproducibility

## Architecture

```
src/
├── bin/               # CLI entry points
│   ├── sfc.rs        # Legacy CLI [OK]
│   └── sfc_new.rs    # New modular CLI [WIP]
├── core/             # Core functionality [OK]
├── cli/              # Command structure [OK]
├── system/           # Platform integration [OK]
├── sharing/          # Collaboration features [OK]
└── config/           # Configuration management [OK]
```

## Current Status

**[OK] Working**: Legacy CLI with full container lifecycle  
**[WIP] In Progress**: New modular CLI with enhanced features  
**[TODO] Todo**: Command handlers, package refactor, comprehensive testing

## Requirements

- **Rust** (stable)
- **Platform**: macOS, Linux, or Windows WSL
- **Privileges**: `sudo` for system binary switching

---
*This is a production-level refactor. Use legacy CLI for full functionality.*
