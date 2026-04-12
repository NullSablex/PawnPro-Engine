use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Subconjunto do .pawnpro/config.json relevante para o motor.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineConfig {
    /// Diretórios de includes; suporta ${workspaceFolder}.
    pub include_paths: Vec<String>,
    /// Configurações de análise.
    pub analysis: AnalysisConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalysisConfig {
    /// Quando true, emite warnings de stock não usada mesmo em .inc (para plugin makers).
    pub warn_unused_in_inc: bool,
}

impl EngineConfig {
    /// Carrega o config do projeto (`.pawnpro/config.json`) e/ou global (`~/.pawnpro/config.json`).
    pub fn load(workspace_root: Option<&Path>) -> Self {
        let mut cfg = Self::load_global().unwrap_or_default();

        if let Some(root) = workspace_root {
            if let Ok(project_cfg) = Self::load_from(root.join(".pawnpro").join("config.json")) {
                cfg.merge(project_cfg);
            }
        }
        cfg
    }

    fn load_global() -> Option<Self> {
        let home = dirs_home()?;
        Self::load_from(home.join(".pawnpro").join("config.json")).ok()
    }

    fn load_from(path: PathBuf) -> Result<Self, ()> {
        let text = std::fs::read_to_string(path).map_err(|_| ())?;
        serde_json::from_str(&text).map_err(|_| ())
    }

    fn merge(&mut self, other: Self) {
        if !other.include_paths.is_empty() {
            self.include_paths = other.include_paths;
        }
        if other.analysis.warn_unused_in_inc {
            self.analysis.warn_unused_in_inc = true;
        }
    }

    /// Resolve include_paths expandindo ${workspaceFolder}.
    pub fn resolved_include_paths(&self, workspace_root: Option<&Path>) -> Vec<PathBuf> {
        self.include_paths
            .iter()
            .map(|p| {
                let expanded = if let Some(root) = workspace_root {
                    p.replace("${workspaceFolder}", &root.to_string_lossy())
                } else {
                    p.clone()
                };
                PathBuf::from(expanded)
            })
            .collect()
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
