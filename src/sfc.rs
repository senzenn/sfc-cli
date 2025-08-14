use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::os::unix::fs as unix_fs;

use anyhow::{anyhow, Context, Result};
use rand::{distributions::Alphanumeric, Rng};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use owo_colors::OwoColorize;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceMeta {
    #[serde(default)]
    pub notes: Vec<String>,
}

pub fn ensure_workspace_layout(root: &Path) -> Result<()> {
    for sub in ["store", "containers", "links", ".sfc"] {
        let p = root.join(sub);
        if !p.exists() {
            fs::create_dir_all(&p).with_context(|| format!("create {}", p.display()))?;
        }
    }
    let gi = root.join(".gitignore");
    if !gi.exists() {
        let content = [
            "store/",
            ".sfc/toolchains/",
            "**/target/",
            "**/.sfc-cache/",
            "**/.DS_Store",
        ]
        .join("\n");
        fs::write(&gi, content)?;
    }
    let meta_path = root.join(".sfc").join("workspace.toml");
    if !meta_path.exists() {
        let meta = WorkspaceMeta::default();
        let s = toml::to_string_pretty(&meta)?;
        fs::write(meta_path, s)?;
    }
    Ok(())
}

/// Get the default workspace at ~/.sfc
pub fn default_workspace() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(Path::new(&home).join(".sfc"))
}

/// Ensure default workspace exists and is initialized
pub fn ensure_default_workspace() -> Result<PathBuf> {
    let ws = default_workspace()?;
    if !ws.exists() || !ws.join(".sfc").exists() {
        ensure_workspace_layout(&ws)?;
    }
    Ok(ws)
}

/// Get current container from .sfc/current file
pub fn current_container() -> Result<Option<String>> {
    let ws = default_workspace()?;
    let current_file = ws.join(".sfc").join("current");
    if current_file.exists() {
        let name = fs::read_to_string(&current_file)?;
        Ok(Some(name.trim().to_string()))
    } else {
        Ok(None)
    }
}

/// Set current container
pub fn set_current_container(name: &str) -> Result<()> {
    let ws = ensure_default_workspace()?;
    let current_file = ws.join(".sfc").join("current");
    fs::write(&current_file, name)?;
    Ok(())
}

/// List all containers
pub fn list_containers() -> Result<Vec<String>> {
    let ws = default_workspace()?;
    let containers_dir = ws.join("containers");
    let mut names = Vec::new();
    if containers_dir.exists() {
        for entry in fs::read_dir(&containers_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                names.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

fn read_lines_trimmed(path: &Path) -> Result<Vec<String>> {
    let data = fs::read_to_string(path).unwrap_or_default();
    let mut lines = Vec::new();
    for line in data.lines() {
        let t = line.trim();
        if t.is_empty() { continue; }
        if t.starts_with('#') { continue; }
        lines.push(t.to_string());
    }
    Ok(lines)
}

/// Compute a stable hash for a snapshot directory from known lockfiles.
pub fn compute_snapshot_hash(snapshot_dir: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let lockfiles = [
        "requirements.txt",
        "rockspec.lock",
        "Cargo.lock",
    ];
    for lf in lockfiles.iter() {
        let p = snapshot_dir.join(lf);
        if p.exists() {
            hasher.update(lf.as_bytes());
            let bytes = fs::read(&p).unwrap_or_default();
            hasher.update(&bytes);
        }
    }
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest))
}

#[derive(Debug, Default)]
pub struct FileChangeSummary {
    pub file: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

fn summarize_lockfile(old: Option<&Path>, new: &Path, file: &str) -> Result<Option<FileChangeSummary>> {
    let new_path = new.join(file);
    if !new_path.exists() { return Ok(None); }
    let old_lines: Vec<String> = match old { Some(o) => read_lines_trimmed(&o.join(file))?, None => Vec::new() };
    let new_lines: Vec<String> = read_lines_trimmed(&new_path)?;
    let old_set: std::collections::BTreeSet<_> = old_lines.iter().cloned().collect();
    let new_set: std::collections::BTreeSet<_> = new_lines.iter().cloned().collect();
    let added: Vec<String> = new_set.difference(&old_set).cloned().collect();
    let removed: Vec<String> = old_set.difference(&new_set).cloned().collect();
    if added.is_empty() && removed.is_empty() {
        return Ok(None);
    }
    Ok(Some(FileChangeSummary { file: file.to_string(), added, removed }))
}

/// Produce a human-readable change message between two snapshots (lockfile-based).
pub fn build_change_message(old_snapshot: Option<&Path>, new_snapshot: &Path, old_hash: Option<&str>, new_hash: &str) -> Result<String> {
    let mut lines = Vec::new();
    match (old_hash, Some(new_hash)) {
        (Some(oh), Some(nh)) => lines.push(format!("Switching generation {} -> {}", &oh[..12.min(oh.len())], &nh[..12.min(nh.len())])),
        (None, Some(nh)) => lines.push(format!("Switching to generation {}", &nh[..12.min(nh.len())])),
        _ => {}
    }
    let files = ["requirements.txt", "rockspec.lock", "Cargo.lock"];
    let mut any = false;
    for f in files.iter() {
        if let Some(sum) = summarize_lockfile(old_snapshot, new_snapshot, f)? {
            any = true;
            lines.push(format!("{}:", sum.file));
            if !sum.added.is_empty() { lines.push(format!("  + {} entries", sum.added.len())); }
            if !sum.removed.is_empty() { lines.push(format!("  - {} entries", sum.removed.len())); }
        }
    }
    if !any { lines.push("No lockfile changes detected".to_string()); }
    Ok(lines.join("\n"))
}

fn which(bin: &str) -> bool {
    Command::new("bash").arg("-lc").arg(format!("command -v {} >/dev/null 2>&1", bin)).status().map(|s| s.success()).unwrap_or(false)
}

fn run_in_env(mut cmd: Command, envs: &[(&str, String)]) -> Result<()> {
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().context("failed to spawn command")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow::anyhow!("{} {}", "command failed:".red(), stderr.trim()));
    }
    Ok(())
}

fn run_shell(script: &str, envs: &[(&str, String)]) -> Result<()> {
    let mut cmd = Command::new("bash");
    cmd.arg("-lc").arg(script);
    run_in_env(cmd, envs)
}

fn run_shell_capture(script: &str, envs: &[(&str, String)]) -> Result<String> {
    let mut cmd = Command::new("bash");
    cmd.arg("-lc").arg(script);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().context("failed to spawn command")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow::anyhow!("{} {}", "command failed:".red(), stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn setup_toolchains(snapshot_dir: &Path, node_ver: Option<&str>, npm_ver: Option<&str>, rust_ver: Option<&str>) -> Result<()> {
    let (envs, tc_root) = build_toolchain_env_from_snapshot(snapshot_dir)?;
    let volta_home = tc_root.join("volta");
    let cargo_home = tc_root.join("cargo");

    // Install Volta if needed (prefer package manager when available)
    if node_ver.is_some() || npm_ver.is_some() {
        let volta_bin = volta_home.join("bin/volta");
        if !volta_bin.exists() {
            if !ensure_volta_with_pkg_manager()? {
                if !which("curl") { return Err(anyhow::anyhow!("curl is required to install Volta")); }
                run_shell("curl -fsSL https://get.volta.sh | bash -s -- --skip-setup", &envs)
                    .context("Volta installer failed")?;
            }
        }
        if let Some(v) = node_ver {
            run_shell(&format!("volta install node@{}", v), &envs)
                .with_context(|| format!("failed to install node@{} with Volta", v))?;
        }
        if let Some(v) = npm_ver {
            run_shell(&format!("volta install npm@{}", v), &envs)
                .with_context(|| format!("failed to install npm@{} with Volta", v))?;
        }
    }

    // Install rustup/rust toolchain if requested
    if let Some(rv) = rust_ver {
        let rustup_bin = cargo_home.join("bin/rustup");
        if !rustup_bin.exists() {
            if !ensure_rustup_with_pkg_manager()? {
                if !which("curl") { return Err(anyhow::anyhow!("curl is required to install rustup")); }
                run_shell(&format!(
                    "curl -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain {}",
                    rv
                ), &envs).context("rustup installer failed")?;
            } else {
                // rustup-init installed system-wide; run it targeting our snapshot
                run_shell(&format!("rustup-init -y --default-toolchain {}", rv), &envs)
                    .with_context(|| format!("rustup-init failed for toolchain {}", rv))?;
            }
        } else {
            run_shell(&format!("rustup toolchain install {} -y", rv), &envs)
                .with_context(|| format!("failed to install rust toolchain {}", rv))?;
            run_shell(&format!("rustup default {}", rv), &envs)
                .with_context(|| format!("failed to set default toolchain {}", rv))?;
        }
    }

    Ok(())
}

fn workspace_root_from_snapshot(snapshot: &Path) -> Result<PathBuf> {
    for ancestor in snapshot.ancestors() {
        if ancestor.join("links").is_dir() && ancestor.join("containers").is_dir() {
            return Ok(ancestor.to_path_buf());
        }
        if ancestor.join(".sfc").is_dir() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(anyhow::anyhow!("cannot locate workspace root from snapshot {}", snapshot.display()))
}

fn build_toolchain_env_from_snapshot(snapshot_dir: &Path) -> Result<(Vec<(&'static str, String)>, PathBuf)> {
    let ws_root = workspace_root_from_snapshot(snapshot_dir)?;
    Ok(build_toolchain_env_for_workspace(&ws_root))
}

fn build_toolchain_env_for_workspace(ws_root: &Path) -> (Vec<(&'static str, String)>, PathBuf) {
    let tc_root = ws_root.join(".sfc").join("toolchains");
    let volta_home = tc_root.join("volta");
    let rustup_home = tc_root.join("rustup");
    let cargo_home = tc_root.join("cargo");
    fs::create_dir_all(&volta_home).ok();
    fs::create_dir_all(&rustup_home).ok();
    fs::create_dir_all(&cargo_home).ok();

    let mut path_val = std::env::var("PATH").unwrap_or_default();
    let mut prepend = Vec::new();
    prepend.push(volta_home.join("bin").to_string_lossy().to_string());
    prepend.push(cargo_home.join("bin").to_string_lossy().to_string());
    path_val = format!("{}:{}", prepend.join(":"), path_val);

    let envs: Vec<(&'static str, String)> = vec![
        ("VOLTA_HOME", volta_home.to_string_lossy().to_string()),
        ("RUSTUP_HOME", rustup_home.to_string_lossy().to_string()),
        ("CARGO_HOME", cargo_home.to_string_lossy().to_string()),
        ("PATH", path_val),
    ];
    (envs, tc_root)
}

pub fn toolchain_node_install(version: &str) -> Result<String> {
    let ws_root = workspace_root()?;
    let (envs, _root) = build_toolchain_env_for_workspace(&ws_root);
    if !ensure_volta_with_pkg_manager()? {
        if !which("curl") { return Err(anyhow::anyhow!("curl is required to install Volta")); }
        run_shell("curl -fsSL https://get.volta.sh | bash -s -- --skip-setup", &envs)?;
    }
    run_shell_capture(&format!("volta install node@{}", version), &envs)
}

pub fn toolchain_node_ls() -> Result<String> {
    let ws_root = workspace_root()?;
    let (envs, _root) = build_toolchain_env_for_workspace(&ws_root);
    run_shell_capture("volta list node", &envs)
}

pub fn toolchain_node_use(version: &str) -> Result<String> {
    toolchain_node_install(version)
}

pub fn toolchain_node_remove(version: &str) -> Result<String> {
    let ws_root = workspace_root()?;
    let (envs, _root) = build_toolchain_env_for_workspace(&ws_root);
    run_shell_capture(&format!("volta uninstall node@{}", version), &envs)
}

pub fn toolchain_rust_install(version: &str) -> Result<String> {
    let ws_root = workspace_root()?;
    let (envs, _root) = build_toolchain_env_for_workspace(&ws_root);
    if !ensure_rustup_with_pkg_manager()? {
        if !which("curl") { return Err(anyhow::anyhow!("curl is required to install rustup")); }
        run_shell(&format!(
            "curl -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain {}",
            version
        ), &envs)?;
    }
    run_shell_capture(&format!("rustup toolchain install {} -y && rustup default {}", version, version), &envs)
}

pub fn toolchain_rust_ls() -> Result<String> {
    let ws_root = workspace_root()?;
    let (envs, _root) = build_toolchain_env_for_workspace(&ws_root);
    run_shell_capture("rustup toolchain list", &envs)
}

pub fn toolchain_rust_use(version: &str) -> Result<String> {
    let ws_root = workspace_root()?;
    let (envs, _root) = build_toolchain_env_for_workspace(&ws_root);
    run_shell_capture(&format!("rustup default {}", version), &envs)
}

pub fn toolchain_rust_remove(version: &str) -> Result<String> {
    let ws_root = workspace_root()?;
    let (envs, _root) = build_toolchain_env_for_workspace(&ws_root);
    run_shell_capture(&format!("rustup toolchain uninstall {} -y", version), &envs)
}

fn detect_pkg_manager() -> Option<&'static str> {
    if which("brew") { return Some("brew"); }
    if which("apt-get") { return Some("apt"); }
    if which("dnf") { return Some("dnf"); }
    if which("yum") { return Some("yum"); }
    if which("pacman") { return Some("pacman"); }
    None
}

fn install_package(pm: &str, pkg: &str) -> bool {
    match pm {
        "brew" => Command::new("bash").arg("-lc").arg(format!("brew install {}", pkg)).status().map(|s| s.success()).unwrap_or(false),
        "apt" => Command::new("bash").arg("-lc").arg(format!("sudo apt-get update && sudo apt-get install -y {}", pkg)).status().map(|s| s.success()).unwrap_or(false),
        "dnf" => Command::new("bash").arg("-lc").arg(format!("sudo dnf install -y {}", pkg)).status().map(|s| s.success()).unwrap_or(false),
        "yum" => Command::new("bash").arg("-lc").arg(format!("sudo yum install -y {}", pkg)).status().map(|s| s.success()).unwrap_or(false),
        "pacman" => Command::new("bash").arg("-lc").arg(format!("sudo pacman -Sy --noconfirm {}", pkg)).status().map(|s| s.success()).unwrap_or(false),
        _ => false,
    }
}

fn ensure_volta_with_pkg_manager() -> Result<bool> {
    if which("volta") { return Ok(true); }
    if let Some(pm) = detect_pkg_manager() {
        let ok = install_package(pm, "volta");
        return Ok(ok && which("volta"));
    }
    Ok(false)
}

fn ensure_rustup_with_pkg_manager() -> Result<bool> {
    if which("rustup") || which("rustup-init") { return Ok(true); }
    if let Some(pm) = detect_pkg_manager() {
        // Homebrew provides rustup-init; many distros provide rustup
        let pkg = if pm == "brew" { "rustup-init" } else { "rustup" };
        let ok = install_package(pm, pkg);
        return Ok(ok && (which("rustup") || which("rustup-init")));
    }
    Ok(false)
}

pub fn workspace_root() -> Result<PathBuf> {
    // Always use ~/.sfc as the workspace
    ensure_default_workspace()
}

pub fn validate_name(name: &str) -> Result<()> {
    let re = Regex::new(r"^[A-Za-z0-9_-]+$").unwrap();
    if !re.is_match(name) {
        return Err(anyhow!("invalid name '{}': must match [A-Za-z0-9_-]+", name));
    }
    if name.is_empty() {
        return Err(anyhow!("container name cannot be empty"));
    }
    Ok(())
}

pub fn create_or_update_symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) -> Result<()> {
    let link = link.as_ref();
    if link.exists() || link.is_symlink() {
        fs::remove_file(link).ok();
    }
    let target = target.as_ref();
    unix_fs::symlink(&target, link).with_context(|| format!("symlink {} -> {}", link.display(), target.display()))?;
    Ok(())
}

fn stow_available() -> bool {
    Command::new("stow")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn stow_pkgs_dir(root: &Path) -> PathBuf {
    root.join(".sfc").join("stow-pkgs")
}

/// Link `links/<alias>` to `../store/<snapshot_dir>` using GNU Stow when available,
/// otherwise fall back to a direct symlink.
pub fn link_alias_to_store(root: &Path, alias: &str, rel_target_from_links: &Path) -> Result<()> {
    let links_dir = root.join("links");
    if stow_available() {
        let pkgs = stow_pkgs_dir(root);
        let pkg_dir = pkgs.join(alias);
        fs::create_dir_all(&pkg_dir)?;
        // Package contains a single entry named `<alias>` which is a symlink to the desired target
        let pkg_symlink = pkg_dir.join(alias);
        if pkg_symlink.exists() || pkg_symlink.is_symlink() {
            fs::remove_file(&pkg_symlink).ok();
        }
        unix_fs::symlink(rel_target_from_links, &pkg_symlink)?;
        fs::create_dir_all(&links_dir)?;
        // Restow the package to (re)create link under links/
        let status = Command::new("stow")
            .arg("-d").arg(&pkgs)
            .arg("-t").arg(&links_dir)
            .arg("-R")
            .arg(alias)
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
        // If stow failed, fall back
    }
    create_or_update_symlink(rel_target_from_links, links_dir.join(alias))
}

/// Unlink `links/<alias>` managed by stow if present; fallback to removing the symlink file.
pub fn unlink_alias_from_links(root: &Path, alias: &str) -> Result<()> {
    let links_dir = root.join("links");
    if stow_available() {
        let pkgs = stow_pkgs_dir(root);
        let status = Command::new("stow")
            .arg("-d").arg(&pkgs)
            .arg("-t").arg(&links_dir)
            .arg("-D")
            .arg(alias)
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
    }
    let link_path = links_dir.join(alias);
    if link_path.exists() || link_path.is_symlink() {
        fs::remove_file(link_path).ok();
    }
    Ok(())
}

pub fn create_snapshot_dir(root: &Path, kind: &str) -> Result<PathBuf> {
    let store = root.join("store");
    let rand: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect();
    let name = format!("{}-{}", rand, kind);
    let dir = store.join(&name);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn seed_lockfiles(snapshot_dir: &Path) -> Result<()> {
    let req = snapshot_dir.join("requirements.txt");
    if !req.exists() {
        fs::write(&req, b"# pinned python deps\n")?;
    }
    let rock = snapshot_dir.join("rockspec.lock");
    if !rock.exists() {
        fs::write(&rock, b"# pinned luarocks deps\n")?;
    }
    let cargo_lock = snapshot_dir.join("Cargo.lock");
    if !cargo_lock.exists() {
        fs::write(&cargo_lock, b"# pinned cargo lock placeholder\n")?;
    }
    Ok(())
}

pub fn resolve_stable_snapshot(root: &Path, name: &str) -> Result<PathBuf> {
    let stable_alias = root.join("links").join(format!("{}-stable", name));
    if !stable_alias.exists() {
        return Err(anyhow!("stable alias missing for {}", name));
    }
    let target = fs::read_link(&stable_alias)?;
    let abs = stable_alias
        .parent()
        .unwrap()
        .join(target)
        .canonicalize()?;
    Ok(abs)
}

pub fn copy_lockfiles(from: &Path, to: &Path) -> Result<()> {
    for fname in ["requirements.txt", "rockspec.lock", "Cargo.lock"] {
        let src = from.join(fname);
        let dst = to.join(fname);
        if src.exists() {
            fs::copy(&src, &dst).with_context(|| format!("copy {}", fname))?;
        }
    }
    Ok(())
}

pub fn find_latest_temp_alias(root: &Path, name: &str) -> Result<Option<String>> {
    let prefix = format!("{}-temp-", name);
    let mut temps: Vec<(String, PathBuf)> = vec![];
    for entry in fs::read_dir(root.join("links"))? {
        let entry = entry?;
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.starts_with(&prefix) && entry.path().is_symlink() {
            temps.push((fname, entry.path()));
        }
    }
    temps.sort_by(|a, b| {
        let am = a.1.metadata().and_then(|m| m.modified()).ok();
        let bm = b.1.metadata().and_then(|m| m.modified()).ok();
        bm.cmp(&am)
    });
    Ok(temps.first().map(|(s, _)| s.clone()))
}

pub fn try_remove_store_if_orphan(root: &Path, rel: &Path) -> Result<()> {
    let abs = root.join("links").join(rel).canonicalize()?;
    let store_dir = root.join("store");
    if !abs.starts_with(&store_dir) {
        return Ok(());
    }
    let file_name = abs.file_name().and_then(|s| s.to_str()).unwrap_or("");
    for entry in fs::read_dir(root.join("links"))? {
        let entry = entry?;
        if entry.path().is_symlink() {
            if let Ok(target) = fs::read_link(entry.path()) {
                let resolved = entry.path().parent().unwrap().join(target).canonicalize().ok();
                if let Some(resolved) = resolved {
                    if resolved.file_name().and_then(|s| s.to_str()) == Some(file_name) {
                        return Ok(());
                    }
                }
            }
        }
    }
    fs::remove_dir_all(&abs).ok();
    Ok(())
}

// ==== NEW FEATURES: System Binary Switching, Snapshots, and Sharing ====

use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub hash: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub is_active: bool,
    pub packages: Vec<PackageInfo>,
    pub toolchains: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareInfo {
    pub packages: Vec<PackageInfo>,
    pub toolchains: HashMap<String, String>,
    pub hash: String,
    pub timestamp: DateTime<Utc>,
}

/// Switch system binaries to use container binaries
pub fn switch_system_binaries(container_bin: &Path, force: bool) -> Result<()> {
    let backup_dir = Path::new("/usr/local/.sfc-backup/bin");
    let system_bin = Path::new("/usr/local/bin");
    
    // Create backup directory
    fs::create_dir_all(backup_dir)?;
    
    // If force, clear existing backups
    if force && backup_dir.exists() {
        fs::remove_dir_all(backup_dir)?;
        fs::create_dir_all(backup_dir)?;
    }
    
    // Backup and symlink each binary
    if let Ok(entries) = fs::read_dir(container_bin) {
        for entry in entries.flatten() {
            if entry.file_type()?.is_file() {
                let exe_name = entry.file_name();
                let system_exe = system_bin.join(&exe_name);
                let backup_exe = backup_dir.join(&exe_name);
                let container_exe = entry.path();
                
                // Backup original if it exists
                if system_exe.exists() && !backup_exe.exists() {
                    fs::rename(&system_exe, &backup_exe)?;
                }
                
                // Remove existing symlink if present
                if system_exe.exists() || system_exe.is_symlink() {
                    fs::remove_file(&system_exe).ok();
                }
                
                // Create symlink to container binary
                unix_fs::symlink(&container_exe, &system_exe)
                    .with_context(|| format!("Failed to create symlink: {} -> {}", 
                                            system_exe.display(), container_exe.display()))?;
            }
        }
    }
    
    Ok(())
}

/// Restore original system binaries
pub fn restore_system_binaries() -> Result<()> {
    let backup_dir = Path::new("/usr/local/.sfc-backup/bin");
    let system_bin = Path::new("/usr/local/bin");
    
    if !backup_dir.exists() {
        return Err(anyhow!("No backup directory found"));
    }
    
    // Remove container symlinks and restore originals
    if let Ok(entries) = fs::read_dir(backup_dir) {
        for entry in entries.flatten() {
            let exe_name = entry.file_name();
            let system_exe = system_bin.join(&exe_name);
            let backup_exe = entry.path();
            
            // Remove symlink if it exists
            if system_exe.exists() || system_exe.is_symlink() {
                fs::remove_file(&system_exe).ok();
            }
            
            // Restore original
            fs::rename(&backup_exe, &system_exe)?;
        }
    }
    
    // Remove backup directory
    fs::remove_dir_all(backup_dir.parent().unwrap())?;
    
    Ok(())
}

/// List all snapshots for a container
pub fn list_container_snapshots(workspace: &Path, container_name: &str) -> Result<Vec<SnapshotInfo>> {
    let mut snapshots = Vec::new();
    let store_dir = workspace.join("store");
    let links_dir = workspace.join("links");
    
    // Get current active snapshot
    let stable_link = links_dir.join(format!("{}-stable", container_name));
    let current_hash = if stable_link.exists() {
        get_snapshot_hash_from_link(&stable_link)?
    } else {
        String::new()
    };
    
    // Scan store directory for snapshots
    if let Ok(entries) = fs::read_dir(&store_dir) {
        for entry in entries.flatten() {
            if entry.file_type()?.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_name.contains("-snapshot-") {
                    let snapshot_path = entry.path();
                    let hash = compute_snapshot_hash(&snapshot_path)?;
                    
                    // Check if this snapshot belongs to this container by looking for links
                    if let Some(snapshot_info) = create_snapshot_info(workspace, container_name, &hash, &snapshot_path, &current_hash)? {
                        snapshots.push(snapshot_info);
                    }
                }
            }
        }
    }
    
    // Sort by timestamp (newest first)
    snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    
    Ok(snapshots)
}

/// Get the current snapshot hash for a container
pub fn get_current_snapshot_hash(workspace: &Path, container_name: &str) -> Result<String> {
    let stable_link = workspace.join("links").join(format!("{}-stable", container_name));
    if !stable_link.exists() {
        return Err(anyhow!("No stable snapshot found for container '{}'", container_name));
    }
    
    get_snapshot_hash_from_link(&stable_link)
}

/// Generate sharing information for a snapshot
pub fn generate_share_info(workspace: &Path, container_name: &str, hash: &str) -> Result<ShareInfo> {
    // Find the snapshot directory
    let snapshot_path = find_snapshot_by_hash(workspace, hash)?;
    
    // Load container configuration
    let container_config_path = workspace.join(".sfc").join("containers").join(format!("{}.toml", container_name));
    let packages = if container_config_path.exists() {
        let config_content = fs::read_to_string(&container_config_path)?;
        parse_packages_from_config(&config_content)?
    } else {
        Vec::new()
    };
    
    // Get toolchain information (simplified)
    let toolchains = get_snapshot_toolchains(&snapshot_path)?;
    
    Ok(ShareInfo {
        packages,
        toolchains,
        hash: hash.to_string(),
        timestamp: Utc::now(),
    })
}

/// Delete a specific snapshot
pub fn delete_snapshot(workspace: &Path, container_name: &str, hash: &str) -> Result<()> {
    let store_dir = workspace.join("store");
    let links_dir = workspace.join("links");
    
    // Find and remove the snapshot directory
    if let Ok(entries) = fs::read_dir(&store_dir) {
        for entry in entries.flatten() {
            if entry.file_type()?.is_dir() {
                let snapshot_path = entry.path();
                let snapshot_hash = compute_snapshot_hash(&snapshot_path)?;
                
                if snapshot_hash.starts_with(hash) {
                    // Remove any links pointing to this snapshot
                    if let Ok(link_entries) = fs::read_dir(&links_dir) {
                        for link_entry in link_entries.flatten() {
                            if link_entry.path().is_symlink() {
                                if let Ok(target) = fs::read_link(link_entry.path()) {
                                    if target.to_string_lossy().contains(&snapshot_path.file_name().unwrap().to_string_lossy().to_string()) {
                                        fs::remove_file(link_entry.path())?;
                                    }
                                }
                            }
                        }
                    }
                    
                    // Remove the snapshot directory
                    fs::remove_dir_all(&snapshot_path)?;
                    return Ok(());
                }
            }
        }
    }
    
    Err(anyhow!("Snapshot with hash '{}' not found", hash))
}

// ==== HELPER FUNCTIONS ====

fn get_snapshot_hash_from_link(link_path: &Path) -> Result<String> {
    let target = fs::read_link(link_path)?;
    let abs_target = link_path.parent().unwrap().join(target).canonicalize()?;
    compute_snapshot_hash(&abs_target)
}

fn create_snapshot_info(
    workspace: &Path, 
    container_name: &str, 
    hash: &str, 
    snapshot_path: &Path,
    current_hash: &str
) -> Result<Option<SnapshotInfo>> {
    let links_dir = workspace.join("links");
    let container_prefix = format!("{}-", container_name);
    
    // Check if any link points to this snapshot
    let mut found_link = false;
    if let Ok(entries) = fs::read_dir(&links_dir) {
        for entry in entries.flatten() {
            let link_name = entry.file_name().to_string_lossy().to_string();
            if link_name.starts_with(&container_prefix) && entry.path().is_symlink() {
                if let Ok(target) = fs::read_link(entry.path()) {
                    let abs_target = entry.path().parent().unwrap().join(target);
                    if let Ok(canonical) = abs_target.canonicalize() {
                        if canonical == snapshot_path {
                            found_link = true;
                            break;
                        }
                    }
                }
            }
        }
    }
    
    if !found_link {
        return Ok(None);
    }
    
    // Get timestamp from directory metadata
    let metadata = fs::metadata(snapshot_path)?;
    let timestamp = metadata.modified()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        .into();
    
    // Generate description
    let description = if hash == current_hash {
        "current stable".to_string()
    } else {
        "snapshot".to_string()
    };
    
    Ok(Some(SnapshotInfo {
        hash: hash.to_string(),
        timestamp,
        description,
        is_active: hash == current_hash,
        packages: Vec::new(), // Would be populated from container config
        toolchains: HashMap::new(), // Would be populated from snapshot
    }))
}

pub fn find_snapshot_by_hash(workspace: &Path, hash: &str) -> Result<PathBuf> {
    let store_dir = workspace.join("store");
    
    if let Ok(entries) = fs::read_dir(&store_dir) {
        for entry in entries.flatten() {
            if entry.file_type()?.is_dir() {
                let snapshot_path = entry.path();
                let snapshot_hash = compute_snapshot_hash(&snapshot_path)?;
                
                if snapshot_hash.starts_with(hash) {
                    return Ok(snapshot_path);
                }
            }
        }
    }
    
    Err(anyhow!("Snapshot with hash '{}' not found", hash))
}

fn parse_packages_from_config(config_content: &str) -> Result<Vec<PackageInfo>> {
    // Simple TOML parsing for packages
    // In a real implementation, this would use the proper container config structures
    let mut packages = Vec::new();
    
    for line in config_content.lines() {
        if line.trim().starts_with("name = ") {
            if let Some(name) = extract_toml_string_value(line) {
                packages.push(PackageInfo {
                    name,
                    version: None,
                    source: "nixpkgs".to_string(),
                });
            }
        }
    }
    
    Ok(packages)
}

fn extract_toml_string_value(line: &str) -> Option<String> {
    if let Some(start) = line.find('"') {
        if let Some(end) = line.rfind('"') {
            if start < end {
                return Some(line[start + 1..end].to_string());
            }
        }
    }
    None
}

fn get_snapshot_toolchains(snapshot_path: &Path) -> Result<HashMap<String, String>> {
    let mut toolchains = HashMap::new();
    
    // Check for common toolchain indicators
    if snapshot_path.join("node_version").exists() {
        if let Ok(version) = fs::read_to_string(snapshot_path.join("node_version")) {
            toolchains.insert("node".to_string(), version.trim().to_string());
        }
    }
    
    if snapshot_path.join("rust_version").exists() {
        if let Ok(version) = fs::read_to_string(snapshot_path.join("rust_version")) {
            toolchains.insert("rust".to_string(), version.trim().to_string());
        }
    }
    
    Ok(toolchains)
}

