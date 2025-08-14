use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;

use crate::container::ContainerConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub hash: String,
    pub container_name: String,
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub operation: Operation,
    pub parent_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Create,
    AddPackage { name: String, version: Option<String> },
    RemovePackage { name: String },
    ModifyPackage { name: String, old_version: Option<String>, new_version: Option<String> },
    Promote,
    Rollback { target_hash: String },
}

#[derive(Debug)]
pub struct History {
    entries: Vec<HistoryEntry>,
    workspace: PathBuf,
}

impl History {
    pub fn load(workspace: &Path) -> Result<Self> {
        let history_file = workspace.join(".sfc").join("history.json");
        let entries = if history_file.exists() {
            let content = fs::read_to_string(&history_file)?;
            serde_json::from_str(&content)?
        } else {
            Vec::new()
        };
        
        Ok(Self {
            entries,
            workspace: workspace.to_path_buf(),
        })
    }

    pub fn save(&self) -> Result<()> {
        let history_file = self.workspace.join(".sfc").join("history.json");
        fs::create_dir_all(history_file.parent().unwrap())?;
        let content = serde_json::to_string_pretty(&self.entries)?;
        fs::write(&history_file, content)?;
        Ok(())
    }

    pub fn add_entry(&mut self, container: &ContainerConfig, operation: Operation, message: String) -> Result<String> {
        let hash = container.compute_hash()?;
        let parent_hash = self.entries
            .iter()
            .rev()
            .find(|e| e.container_name == container.name)
            .map(|e| e.hash.clone());

        let entry = HistoryEntry {
            hash: hash.clone(),
            container_name: container.name.clone(),
            timestamp: Utc::now(),
            message,
            operation,
            parent_hash,
        };

        self.entries.push(entry);
        self.save()?;
        Ok(hash)
    }

    pub fn get_container_history(&self, container_name: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.container_name == container_name)
            .collect()
    }

    pub fn find_by_hash(&self, hash: &str) -> Option<&HistoryEntry> {
        self.entries.iter().find(|e| e.hash.starts_with(hash))
    }

    pub fn print_log(&self, container_name: Option<&str>) -> Result<()> {
        let entries: Vec<_> = match container_name {
            Some(name) => self.get_container_history(name),
            None => self.entries.iter().collect(),
        };

        if entries.is_empty() {
            println!("{}", "No history entries found".yellow());
            return Ok(());
        }

        println!("{}", "Container History:".bold());
        for entry in entries.iter().rev().take(20) {
            self.print_entry(entry);
        }

        Ok(())
    }

    fn print_entry(&self, entry: &HistoryEntry) {
        let time_str = entry.timestamp.format("%Y-%m-%d %H:%M:%S");
        let hash_short = &entry.hash[..8];
        
        let op_str = match &entry.operation {
            Operation::Create => "CREATE".green().to_string(),
            Operation::AddPackage { name, .. } => format!("ADD {}", name).cyan().to_string(),
            Operation::RemovePackage { name } => format!("REMOVE {}", name).red().to_string(),
            Operation::ModifyPackage { name, .. } => format!("MODIFY {}", name).yellow().to_string(),
            Operation::Promote => "PROMOTE".blue().to_string(),
            Operation::Rollback { .. } => "ROLLBACK".magenta().to_string(),
        };

        println!("{} {} [{}] {} - {}", 
                 hash_short.bright_yellow(),
                 time_str.dimmed(),
                 entry.container_name.cyan(),
                 op_str,
                 entry.message);
    }

    pub fn visualize_graph(&self, container_name: Option<&str>) -> Result<()> {
        let entries: Vec<_> = match container_name {
            Some(name) => self.get_container_history(name),
            None => self.entries.iter().collect(),
        };

        if entries.is_empty() {
            println!("{}", "No history to visualize".yellow());
            return Ok(());
        }

        println!("{}", "Container History Graph:".bold());
        println!();

        // Build parent-child relationships
        let mut children: std::collections::HashMap<String, Vec<&HistoryEntry>> = std::collections::HashMap::new();
        for entry in &entries {
            if let Some(parent) = &entry.parent_hash {
                children.entry(parent.clone()).or_default().push(entry);
            }
        }

        // Find root entries (no parent)
        let roots: Vec<_> = entries.iter()
            .filter(|e| e.parent_hash.is_none())
            .collect();

        for root in roots {
            self.print_graph_node(root, &children, "", true);
        }

        Ok(())
    }

    fn print_graph_node(&self, entry: &HistoryEntry, children: &std::collections::HashMap<String, Vec<&HistoryEntry>>, prefix: &str, is_last: bool) {
        let connector = if is_last { "└── " } else { "├── " };
        let hash_short = &entry.hash[..8];
        let time_str = entry.timestamp.format("%m-%d %H:%M");
        
        let op_color = match &entry.operation {
            Operation::Create => hash_short.green().to_string(),
            Operation::AddPackage { .. } => hash_short.cyan().to_string(),
            Operation::RemovePackage { .. } => hash_short.red().to_string(),
            Operation::ModifyPackage { .. } => hash_short.yellow().to_string(),
            Operation::Promote => hash_short.blue().to_string(),
            Operation::Rollback { .. } => hash_short.magenta().to_string(),
        };

        println!("{}{}{} {} {}", 
                 prefix, 
                 connector,
                 op_color,
                 time_str.dimmed(),
                 entry.message);

        // Print children
        if let Some(child_entries) = children.get(&entry.hash) {
            let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
            for (i, child) in child_entries.iter().enumerate() {
                let is_last_child = i == child_entries.len() - 1;
                self.print_graph_node(child, children, &new_prefix, is_last_child);
            }
        }
    }
}
