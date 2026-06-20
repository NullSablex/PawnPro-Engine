//! Heurísticas de detecção de nomes pobres. Conservadoras por desenho: na
//! dúvida, não sinaliza.

use super::{NameCategory, NameIssue, NameIssueKind, NameSite, style};
use crate::config::{NamingConfig, StyleConfig};
use crate::util::to_u32;

/// Listas já resolvidas (de arquivo ou inline) para uma execução da análise,
/// evitando reler arquivo por identificador.
pub struct ResolvedLists {
    pub blocklist: Vec<String>,
    pub loop_indices: Vec<String>,
}

/// Avalia um identificador. `Some` = sinalizar; `None` = aceitável.
///
/// Ordem das regras (da mais específica para a mais geral): placeholder →
/// comprimento → estilo. Um `tmp` curto é reportado como placeholder, não como
/// "muito curto"; um nome OK em conteúdo mas fora da caixa cai na regra de estilo.
pub fn evaluate(site: &NameSite, cfg: &NamingConfig, lists: &ResolvedLists) -> Option<NameIssue> {
    let lower = site.name.to_ascii_lowercase();

    if lists
        .blocklist
        .iter()
        .any(|b| b.eq_ignore_ascii_case(&lower))
    {
        return Some(issue(site, NameIssueKind::Placeholder));
    }

    // Comprimento por contagem de caracteres (não bytes): nomes podem ter
    // acentos. `_` sozinho (descarte) e índices de loop liberados são tolerados.
    let len = to_u32(site.name.chars().count());
    let tolerated_short =
        site.name == "_" || (site.in_loop_header && is_loop_index(&site.name, lists));
    if len < cfg.min_length && !tolerated_short {
        return Some(issue(site, NameIssueKind::TooShort));
    }

    // Estilo de caixa: a categoria pode aceitar VÁRIOS estilos. Só sinaliza se o
    // nome não casar com NENHUM deles. Índices de loop curtos ficam isentos.
    if !tolerated_short {
        let accepted = accepted_styles(&cfg.style, site.category);
        if !accepted.is_empty() && !accepted.iter().any(|c| style::matches(&site.name, *c)) {
            let labels = accepted.iter().map(|c| style::label(*c)).collect();
            return Some(issue(site, NameIssueKind::WrongStyle(labels)));
        }
    }

    None
}

/// Estilos aceitos para a categoria (vazio = sem checagem). Valores inválidos
/// na configuração são ignorados.
fn accepted_styles(cfg: &StyleConfig, category: NameCategory) -> Vec<style::Case> {
    let raw = match category {
        NameCategory::Function => &cfg.functions,
        NameCategory::Global => &cfg.globals,
        NameCategory::Local => &cfg.locals,
        NameCategory::Constant => &cfg.constants,
        NameCategory::Macro => &cfg.macros,
        NameCategory::Parameter => &cfg.parameters,
    };
    raw.iter()
        .filter_map(|s| style::Case::from_config(s))
        .collect()
}

fn is_loop_index(name: &str, lists: &ResolvedLists) -> bool {
    lists
        .loop_indices
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(name))
}

fn issue(site: &NameSite, kind: NameIssueKind) -> NameIssue {
    NameIssue {
        line: site.line,
        col: site.col,
        name: site.name.clone(),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(name: &str, in_loop: bool) -> NameSite {
        sited(name, in_loop, NameCategory::Local)
    }

    fn sited(name: &str, in_loop: bool, category: NameCategory) -> NameSite {
        NameSite {
            name: name.to_string(),
            line: 0,
            col: 0,
            in_loop_header: in_loop,
            category,
        }
    }

    fn cfg() -> NamingConfig {
        NamingConfig {
            enabled: true,
            ..NamingConfig::default()
        }
    }

    /// Avalia resolvendo as listas a partir do próprio `cfg` (fallback inline).
    fn ev(site: &NameSite, cfg: &NamingConfig) -> Option<NameIssue> {
        let lists = ResolvedLists {
            blocklist: cfg.resolved_blocklist(),
            loop_indices: cfg.resolved_loop_indices(),
        };
        evaluate(site, cfg, &lists)
    }

    #[test]
    fn flags_blocklisted_placeholder() {
        let r = ev(&site("tmp", false), &cfg()).unwrap();
        assert_eq!(r.kind, NameIssueKind::Placeholder);
    }

    #[test]
    fn placeholder_match_is_case_insensitive() {
        assert!(ev(&site("TMP", false), &cfg()).is_some());
        assert!(ev(&site("Foo", false), &cfg()).is_some());
    }

    #[test]
    fn flags_too_short_outside_loop() {
        let r = ev(&site("a", false), &cfg()).unwrap();
        assert_eq!(r.kind, NameIssueKind::TooShort);
    }

    #[test]
    fn allows_loop_index_in_header() {
        assert!(ev(&site("i", true), &cfg()).is_none());
        assert!(ev(&site("j", true), &cfg()).is_none());
    }

    #[test]
    fn short_non_index_still_flagged_in_loop() {
        // "x" não está na lista de índices tolerados: sinalizado mesmo em loop.
        assert!(ev(&site("x", true), &cfg()).is_some());
    }

    #[test]
    fn allows_underscore_discard() {
        assert!(ev(&site("_", false), &cfg()).is_none());
    }

    #[test]
    fn accepts_reasonable_name() {
        assert!(ev(&site("playerHealth", false), &cfg()).is_none());
    }

    #[test]
    fn placeholder_wins_over_length() {
        // "var" é curto E placeholder — deve reportar como placeholder.
        let r = ev(&site("var", false), &cfg()).unwrap();
        assert_eq!(r.kind, NameIssueKind::Placeholder);
    }

    fn cfg_style(functions: &[&str]) -> NamingConfig {
        let mut c = cfg();
        c.style.functions = functions.iter().map(|s| (*s).to_string()).collect();
        c
    }

    #[test]
    fn style_off_by_default_does_not_flag() {
        // Sem estilo configurado, um nome PascalCase de função passa.
        let r = ev(&sited("DoThing", false, NameCategory::Function), &cfg());
        assert!(r.is_none());
    }

    #[test]
    fn style_flags_wrong_case() {
        // functions = [camelCase]; "DoThing" é Pascal → sinaliza estilo.
        let r = ev(
            &sited("DoThing", false, NameCategory::Function),
            &cfg_style(&["camelCase"]),
        )
        .unwrap();
        assert_eq!(r.kind, NameIssueKind::WrongStyle(vec!["camelCase"]));
    }

    #[test]
    fn style_accepts_correct_case() {
        let r = ev(
            &sited("doThing", false, NameCategory::Function),
            &cfg_style(&["camelCase"]),
        );
        assert!(r.is_none());
    }

    #[test]
    fn multi_style_accepts_any_match() {
        // Aceita camelCase OU PascalCase: ambos passam, snake_case é sinalizado.
        let cfg = cfg_style(&["camelCase", "PascalCase"]);
        assert!(ev(&sited("doThing", false, NameCategory::Function), &cfg).is_none());
        assert!(ev(&sited("DoThing", false, NameCategory::Function), &cfg).is_none());
        let r = ev(&sited("do_thing", false, NameCategory::Function), &cfg).unwrap();
        assert_eq!(
            r.kind,
            NameIssueKind::WrongStyle(vec!["camelCase", "PascalCase"])
        );
    }

    #[test]
    fn placeholder_still_wins_over_style() {
        // "tmp" com estilo configurado ainda reporta placeholder (mais específico).
        let r = ev(
            &sited("tmp", false, NameCategory::Function),
            &cfg_style(&["camelCase"]),
        )
        .unwrap();
        assert_eq!(r.kind, NameIssueKind::Placeholder);
    }
}
