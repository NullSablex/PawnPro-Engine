//! Assistente de nomes, offline e determinístico.
//!
//! Não usa IA: aplica heurísticas sobre os identificadores que a engine já
//! conhece — detecção de nomes pobres (`analyze`), checagem de estilo e
//! sugestões de renomeação (`suggestions_for`). Conservador: só atua quando
//! ligado em `analysis.naming` (padrão desligado).

mod locals;
mod rules;
mod style;
mod suggest;

pub use locals::collect_local_decls;
pub use style::Case;

use crate::config::NamingConfig;

/// Categoria de um identificador — define qual estilo de caixa se aplica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameCategory {
    Function,
    Global,
    Local,
    /// `const`, membro de enum — constante tipada da linguagem.
    Constant,
    /// `#define` — macro do preprocessador (substituição textual).
    Macro,
    Parameter,
}

/// Um identificador a avaliar, com sua posição, categoria e se está no cabeçalho
/// de um loop (onde nomes de uma letra são idiomáticos).
pub struct NameSite {
    pub name: String,
    pub line: u32,
    pub col: u32,
    pub in_loop_header: bool,
    pub category: NameCategory,
}

/// Motivo pelo qual um nome foi sinalizado.
#[derive(Debug, PartialEq, Eq)]
pub enum NameIssueKind {
    /// Mais curto que `min_length` e fora de um cabeçalho de loop.
    TooShort,
    /// Identificador genérico da blocklist (`tmp`, `foo`, …).
    Placeholder,
    /// Caixa fora dos estilos aceitos para a categoria. Carrega os rótulos dos
    /// estilos aceitos (ex.: `camelCase`, `snake_case`) para a mensagem.
    WrongStyle(Vec<&'static str>),
}

/// Resultado da análise de um identificador.
pub struct NameIssue {
    pub line: u32,
    pub col: u32,
    pub name: String,
    pub kind: NameIssueKind,
}

/// Avalia os identificadores e devolve as ocorrências a sinalizar. Vazio quando
/// o assistente está desligado.
#[must_use]
pub fn analyze(sites: &[NameSite], cfg: &NamingConfig) -> Vec<NameIssue> {
    if !cfg.enabled {
        return Vec::new();
    }
    // Resolve as listas (arquivo ou inline) uma vez por execução.
    let lists = rules::ResolvedLists {
        blocklist: cfg.resolved_blocklist(),
        loop_indices: cfg.resolved_loop_indices(),
    };
    sites
        .iter()
        .filter_map(|s| rules::evaluate(s, cfg, &lists))
        .collect()
}

/// Sugestões de renomeação para `name`, conforme os estilos configurados em
/// `naming.style`. Para cada estilo distinto presente na configuração, propõe a
/// conversão de caixa correspondente (sem duplicatas e sem o próprio nome).
/// Vazio quando o assistente está desligado ou não há conversão a oferecer.
#[must_use]
pub fn suggestions_for(name: &str, cfg: &NamingConfig) -> Vec<String> {
    if !cfg.enabled {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let s = &cfg.style;
    let all = [
        &s.functions,
        &s.globals,
        &s.locals,
        &s.constants,
        &s.macros,
        &s.parameters,
    ];
    for raw in all.into_iter().flatten() {
        if let Some(case) = Case::from_config(raw)
            && let Some(suggestion) = suggest::to_style(name, case)
            && !out.contains(&suggestion)
        {
            out.push(suggestion);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StyleConfig;

    fn cfg_with_style(style: StyleConfig) -> NamingConfig {
        NamingConfig {
            enabled: true,
            style,
            ..NamingConfig::default()
        }
    }

    #[test]
    fn no_suggestions_when_disabled() {
        let cfg = NamingConfig {
            style: StyleConfig {
                functions: vec!["camelCase".to_string()],
                ..StyleConfig::default()
            },
            ..NamingConfig::default() // enabled: false
        };
        assert!(suggestions_for("DoThing", &cfg).is_empty());
    }

    #[test]
    fn suggests_configured_style() {
        let cfg = cfg_with_style(StyleConfig {
            functions: vec!["camelCase".to_string()],
            ..StyleConfig::default()
        });
        assert_eq!(suggestions_for("DoThing", &cfg), vec!["doThing"]);
    }

    #[test]
    fn dedups_across_categories() {
        // functions e locals ambos camelCase → uma só sugestão.
        let cfg = cfg_with_style(StyleConfig {
            functions: vec!["camelCase".to_string()],
            locals: vec!["camelCase".to_string()],
            ..StyleConfig::default()
        });
        assert_eq!(suggestions_for("DoThing", &cfg), vec!["doThing"]);
    }

    #[test]
    fn offers_multiple_distinct_styles() {
        let cfg = cfg_with_style(StyleConfig {
            functions: vec!["snake_case".to_string()],
            constants: vec!["UPPER_CASE".to_string()],
            ..StyleConfig::default()
        });
        let s = suggestions_for("playerHealth", &cfg);
        assert!(s.contains(&"player_health".to_string()));
        assert!(s.contains(&"PLAYER_HEALTH".to_string()));
    }
}
