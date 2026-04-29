use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineConfig {
    pub include_paths: Vec<String>,
    pub analysis: AnalysisConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalysisConfig {
    pub warn_unused_in_inc: bool,
    pub suppress_diagnostics_in_inc: bool,
    pub sdk: SdkConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SdkConfig {
    pub platform: String,
    pub file_path: String,
}

impl EngineConfig {
    pub fn load(workspace_root: Option<&Path>) -> Self {
        let mut cfg = Self::load_global().unwrap_or_default();

        if let Some(root) = workspace_root
            && let Ok(project_cfg) = Self::load_from(root.join(".pawnpro").join("config.json"))
        {
            cfg.merge(project_cfg);
        }

        if cfg.include_paths.is_empty() {
            cfg.include_paths = vec!["${workspaceFolder}/pawno/include".to_string()];
        }

        cfg
    }

    fn load_global() -> Option<Self> {
        let home = home_dir()?;
        Self::load_from(home.join(".pawnpro").join("config.json")).ok()
    }

    fn load_from(path: PathBuf) -> Result<Self, ()> {
        let text = std::fs::read_to_string(path).map_err(|_| ())?;
        serde_json::from_str(&text).map_err(|_| ())
    }

    // Project config wins over global — only non-default values propagate.
    fn merge(&mut self, other: Self) {
        if !other.include_paths.is_empty() {
            self.include_paths = other.include_paths;
        }
        if other.analysis.warn_unused_in_inc {
            self.analysis.warn_unused_in_inc = true;
        }
        if other.analysis.suppress_diagnostics_in_inc {
            self.analysis.suppress_diagnostics_in_inc = true;
        }
    }

    pub fn resolved_include_paths(&self, workspace_root: Option<&Path>) -> Vec<PathBuf> {
        self.include_paths
            .iter()
            .map(|raw| {
                let expanded = workspace_root
                    .map(|root| raw.replace("${workspaceFolder}", &root.to_string_lossy()))
                    .unwrap_or_else(|| raw.clone());
                PathBuf::from(expanded)
            })
            .collect()
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
