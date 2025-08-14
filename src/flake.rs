use std::fs;
use std::path::Path;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::container::{ContainerConfig, PackageSpec, PackageSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeConfig {
    pub description: String,
    pub inputs: FlakeInputs,
    pub outputs: FlakeOutputs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeInputs {
    pub nixpkgs: FlakeInput,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, FlakeInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeInput {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeOutputs {
    pub packages: std::collections::HashMap<String, String>,
    pub shell: ShellConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    pub packages: Vec<String>,
    pub shell_hook: String,
}

impl FlakeConfig {
    pub fn from_container(container: &ContainerConfig) -> Self {
        let mut inputs = FlakeInputs {
            nixpkgs: FlakeInput {
                url: "github:NixOS/nixpkgs/nixos-unstable".to_string(),
                rev: None,
            },
            extra: std::collections::HashMap::new(),
        };

        let mut packages = Vec::new();
        let mut shell_packages = std::collections::HashMap::new();

        for pkg in &container.packages {
            match &pkg.source {
                PackageSource::Nixpkgs => {
                    let pkg_name = if let Some(version) = &pkg.version {
                        format!("{}@{}", pkg.name, version)
                    } else {
                        pkg.name.clone()
                    };
                    packages.push(pkg_name);
                }
                PackageSource::GitHub { repo, rev } => {
                    let input_name = pkg.name.replace("-", "_");
                    inputs.extra.insert(input_name.clone(), FlakeInput {
                        url: format!("github:{}", repo),
                        rev: Some(rev.clone()),
                    });
                    packages.push(format!("inputs.{}.packages.${{system}}.default", input_name));
                }
                PackageSource::Url(url) => {
                    let input_name = pkg.name.replace("-", "_");
                    inputs.extra.insert(input_name.clone(), FlakeInput {
                        url: url.clone(),
                        rev: None,
                    });
                    packages.push(format!("inputs.{}.packages.${{system}}.default", input_name));
                }
            }
        }

        let shell_hook = format!(
            r#"
echo "🚀 Entering {} container"
echo "📦 Packages: {}"
export SFC_CONTAINER="{}"
"#,
            container.name,
            container.packages.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "),
            container.name
        );

        Self {
            description: format!("SFC container: {}", container.name),
            inputs,
            outputs: FlakeOutputs {
                packages: shell_packages,
                shell: ShellConfig {
                    packages,
                    shell_hook,
                },
            },
        }
    }

    pub fn to_nix(&self) -> String {
        let mut inputs_str = String::new();
        inputs_str.push_str(&format!("    nixpkgs.url = \"{}\";\n", self.inputs.nixpkgs.url));
        
        for (name, input) in &self.inputs.extra {
            inputs_str.push_str(&format!("    {}.url = \"{}\";\n", name, input.url));
            if let Some(rev) = &input.rev {
                inputs_str.push_str(&format!("    {}.rev = \"{}\";\n", name, rev));
            }
        }

        let packages_str = self.outputs.shell.packages
            .iter()
            .map(|p| format!("        {}", p))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"{{
  description = "{}";

  inputs = {{
{}  }};

  outputs = {{ self, nixpkgs, ... }}@inputs:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${{system}};
    in
    {{
      devShells.${{system}}.default = pkgs.mkShell {{
        buildInputs = with pkgs; [
{}
        ];

        shellHook = ''
{}        '';
      }};
    }};
}}"#,
            self.description,
            inputs_str,
            packages_str,
            self.outputs.shell.shell_hook
        )
    }

    pub fn save(&self, workspace: &Path, container_name: &str) -> Result<()> {
        let flake_dir = workspace.join("containers").join(container_name);
        fs::create_dir_all(&flake_dir)?;
        
        let flake_nix = flake_dir.join("flake.nix");
        fs::write(&flake_nix, self.to_nix())?;
        
        let flake_lock = flake_dir.join("flake.lock");
        if !flake_lock.exists() {
            fs::write(&flake_lock, "{\"version\": 7, \"nodes\": {}}")?;
        }
        
        Ok(())
    }
}
