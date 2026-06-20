use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Teto de tamanho do `config.json`. Config legítima fica em KB; este limite é
/// uma barreira de segurança contra arquivos absurdos (memória/parse), não uma
/// restrição de uso real. Acima disto, o arquivo é ignorado. Fixo (não exposto).
const MAX_CONFIG_BYTES: u64 = 32 * 1024 * 1024;

/// Limite-padrão de processamento dos arquivos de lista (`.ban`/`.allow`), em
/// bytes. Não impede o dev de escrever no arquivo — impede a engine de
/// processá-lo acima disto, por segurança. Configurável via `maxListFileBytes`.
const DEFAULT_MAX_LIST_BYTES: u64 = 32 * 1024 * 1024;

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
    pub naming: NamingConfig,
}

/// Configuração do assistente de nomes (PP0018). Conservadora por padrão:
/// desligada, e mesmo ligada só sinaliza nomes claramente pobres.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NamingConfig {
    /// Liga o diagnóstico de nomes. Padrão `false` — quem não pediu não é incomodado.
    pub enabled: bool,
    /// Comprimento mínimo de identificador antes de sinalizar (exceto loops).
    pub min_length: u32,
    /// Nomes de 1 letra tolerados em cabeçalho de `for` (índices clássicos).
    /// Usado como fallback quando `loop_indices_file` não resolve.
    pub allow_short_in_loops: Vec<String>,
    /// Identificadores genéricos sempre sinalizados (placeholders). Usado como
    /// fallback quando `blocklist_file` não resolve.
    pub blocklist: Vec<String>,
    /// Caminho de um arquivo `.ban` (um termo por linha, `#` comenta) com os
    /// nomes proibidos. Tem prioridade sobre `blocklist` quando o arquivo existe.
    pub blocklist_file: String,
    /// Caminho de um arquivo `.allow` com os índices de loop tolerados. Tem
    /// prioridade sobre `allow_short_in_loops` quando o arquivo existe.
    pub loop_indices_file: String,
    /// Limite de processamento (bytes) de cada arquivo `.ban`/`.allow`. Acima
    /// disto a engine não processa o arquivo (cai no fallback inline), por
    /// segurança — não impede o dev de escrever no arquivo.
    pub max_list_file_bytes: u64,
    /// Estilo de caixa esperado por categoria. Vazio (`"off"`) = não checa.
    pub style: StyleConfig,
}

impl Default for NamingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_length: 2,
            allow_short_in_loops: ["i", "j", "k"].iter().map(|s| (*s).to_string()).collect(),
            blocklist: ["tmp", "temp", "aux", "foo", "bar", "data", "var"]
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            blocklist_file: String::new(),
            loop_indices_file: String::new(),
            max_list_file_bytes: DEFAULT_MAX_LIST_BYTES,
            style: StyleConfig::default(),
        }
    }
}

impl NamingConfig {
    /// Resolve a lista de nomes proibidos: do arquivo `.ban` se configurado e
    /// legível; senão, da lista inline (`blocklist`).
    #[must_use]
    pub fn resolved_blocklist(&self) -> Vec<String> {
        read_list_file(&self.blocklist_file, self.max_list_file_bytes)
            .unwrap_or_else(|| self.blocklist.clone())
    }

    /// Resolve os índices de loop tolerados: do arquivo `.allow` se configurado e
    /// legível; senão, da lista inline (`allow_short_in_loops`).
    #[must_use]
    pub fn resolved_loop_indices(&self) -> Vec<String> {
        read_list_file(&self.loop_indices_file, self.max_list_file_bytes)
            .unwrap_or_else(|| self.allow_short_in_loops.clone())
    }
}

/// Lê um arquivo de texto recusando-o se exceder `max` bytes — barreira de
/// segurança contra arquivos absurdos antes de carregá-los na memória.
fn read_capped(path: &Path, max: u64) -> Result<String, ()> {
    if std::fs::metadata(path).map_err(|_| ())?.len() > max {
        return Err(());
    }
    std::fs::read_to_string(path).map_err(|_| ())
}

/// Lê um arquivo de lista (um termo por linha; linhas vazias e iniciadas por `#`
/// são ignoradas; espaços nas pontas são removidos). `None` se o caminho for
/// vazio, ilegível ou acima de `max_bytes` — deixando o chamador cair no
/// fallback inline.
fn read_list_file(path: &str, max_bytes: u64) -> Option<Vec<String>> {
    if path.is_empty() {
        return None;
    }
    let text = read_capped(Path::new(path), max_bytes).ok()?;
    Some(
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect(),
    )
}

/// Estilos de caixa aceitos por categoria de identificador. Cada campo é uma
/// lista de `"camelCase" | "snake_case" | "PascalCase" | "UPPER_CASE"`; lista
/// vazia = sem checagem. Um nome é aceito se casar com QUALQUER estilo da lista.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct StyleConfig {
    pub functions: Vec<String>,
    pub globals: Vec<String>,
    pub locals: Vec<String>,
    pub constants: Vec<String>,
    pub macros: Vec<String>,
    pub parameters: Vec<String>,
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
            && let Ok(project_cfg) = Self::load_from(&root.join(".pawnpro").join("config.json"))
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
        Self::load_from(&home.join(".pawnpro").join("config.json")).ok()
    }

    fn load_from(path: &Path) -> Result<Self, ()> {
        let text = read_capped(path, MAX_CONFIG_BYTES)?;
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
        // Naming: se o projeto liga o assistente, sua configuração inteira vence
        // (evita misturar blocklist/min_length de escopos diferentes).
        if other.analysis.naming.enabled {
            self.analysis.naming = other.analysis.naming;
        }
    }

    pub fn resolved_include_paths(&self, workspace_root: Option<&Path>) -> Vec<PathBuf> {
        self.include_paths
            .iter()
            .map(|raw| {
                let expanded = workspace_root.map_or_else(
                    || raw.clone(),
                    |root| raw.replace("${workspaceFolder}", &root.to_string_lossy()),
                );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_path_falls_back_to_inline() {
        let cfg = NamingConfig {
            blocklist: vec!["tmp".to_string()],
            allow_short_in_loops: vec!["i".to_string()],
            ..NamingConfig::default()
        };
        // Sem arquivo configurado: usa a lista inline.
        assert_eq!(cfg.resolved_blocklist(), vec!["tmp"]);
        assert_eq!(cfg.resolved_loop_indices(), vec!["i"]);
    }

    #[test]
    fn missing_file_falls_back_to_inline() {
        let cfg = NamingConfig {
            blocklist: vec!["foo".to_string()],
            blocklist_file: "/caminho/inexistente/x.ban".to_string(),
            ..NamingConfig::default()
        };
        // Arquivo ilegível → fallback para inline, sem panic.
        assert_eq!(cfg.resolved_blocklist(), vec!["foo"]);
    }

    #[test]
    fn read_capped_rejects_oversized_file() {
        let path = std::env::temp_dir().join("pawnpro_test_capped.json");
        std::fs::write(&path, "0123456789").unwrap(); // 10 bytes
        assert!(
            read_capped(&path, 5).is_err(),
            "10 bytes acima do teto de 5"
        );
        assert!(
            read_capped(&path, 20).is_ok(),
            "10 bytes dentro do teto de 20"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reads_list_file_ignoring_comments_and_blanks() {
        let dir = std::env::temp_dir();
        let path = dir.join("pawnpro_test_blocklist.ban");
        std::fs::write(&path, "# comentário\ntmp\n\n  foo  \n# outro\nbar\n").unwrap();
        let cfg = NamingConfig {
            blocklist_file: path.to_string_lossy().into_owned(),
            ..NamingConfig::default()
        };
        assert_eq!(cfg.resolved_blocklist(), vec!["tmp", "foo", "bar"]);
        let _ = std::fs::remove_file(&path);
    }
}
