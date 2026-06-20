//! Adaptador entre os símbolos parseados e o assistente de nomes (`crate::naming`).
//! Emite `PP0018` para nomes pobres, de forma conservadora.
//!
//! Escopo: símbolos definidos pelo usuário neste arquivo — funções com corpo,
//! seus parâmetros e variáveis locais. Nativas/forwards de includes são API
//! externa cujo nome o autor não controla, então ficam de fora.

use crate::analyzer::codes;
use crate::analyzer::diagnostic::PawnDiagnostic;
use crate::config::NamingConfig;
use crate::messages::{Locale, MsgKey, msg};
use crate::naming::{self, NameCategory, NameIssueKind, NameSite};
use crate::parser::types::{Symbol, SymbolKind};
use crate::util::to_u32;

pub fn analyze_naming(
    text: &str,
    symbols: &[Symbol],
    cfg: &NamingConfig,
    locale: Locale,
) -> Vec<PawnDiagnostic> {
    if !cfg.enabled {
        return Vec::new();
    }

    let mut sites: Vec<NameSite> = naming::collect_local_decls(text);
    for sym in symbols {
        let Some(category) = category_of(&sym.kind) else {
            continue;
        };
        sites.push(NameSite {
            name: sym.name.clone(),
            line: sym.line,
            col: sym.col,
            in_loop_header: false,
            category,
        });
        // Parâmetros (só para funções): a posição precisa não é rastreada por
        // símbolo, então ancoramos na declaração da função. Variádicos ("...")
        // não têm nome a avaliar.
        if category == NameCategory::Function {
            for p in &sym.params {
                if p.is_variadic {
                    continue;
                }
                sites.push(NameSite {
                    name: p.name.clone(),
                    line: sym.line,
                    col: sym.col,
                    in_loop_header: false,
                    category: NameCategory::Parameter,
                });
            }
        }
    }

    naming::analyze(&sites, cfg)
        .into_iter()
        .map(|issue| {
            let text = match &issue.kind {
                NameIssueKind::TooShort => {
                    msg(locale, MsgKey::NameTooShort).replace("{}", &issue.name)
                }
                NameIssueKind::Placeholder => {
                    msg(locale, MsgKey::NamePlaceholder).replace("{}", &issue.name)
                }
                NameIssueKind::WrongStyle(styles) => msg(locale, MsgKey::NameStyle)
                    .replace("{}", &issue.name)
                    .replace("{style}", &styles.join(" | ")),
            };
            let col_end = issue.col + to_u32(issue.name.chars().count());
            PawnDiagnostic::hint(issue.line, issue.col, col_end, codes::PP0018, text)
        })
        .collect()
}

/// Categoria de um símbolo de topo escrito pelo usuário, ou `None` quando não se
/// avalia (nativas/forwards de include — API externa cujo nome o autor não
/// controla). `Enum` é o nome do tipo; tratado como constante por convenção.
fn category_of(kind: &SymbolKind) -> Option<NameCategory> {
    match kind {
        SymbolKind::Stock | SymbolKind::Public | SymbolKind::Static | SymbolKind::Plain => {
            Some(NameCategory::Function)
        }
        SymbolKind::Variable => Some(NameCategory::Global),
        SymbolKind::StaticConst | SymbolKind::Const | SymbolKind::Enum => {
            Some(NameCategory::Constant)
        }
        // `#define` é macro do preprocessador, não constante tipada.
        SymbolKind::Define => Some(NameCategory::Macro),
        SymbolKind::Native | SymbolKind::Forward => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::types::Param;

    fn func(name: &str, params: Vec<Param>, kind: SymbolKind) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            signature: None,
            params,
            deprecated: false,
            doc: None,
            line: 0,
            col: 0,
        }
    }

    fn param(name: &str) -> Param {
        Param {
            name: name.to_string(),
            tag: None,
            is_variadic: false,
        }
    }

    fn on() -> NamingConfig {
        NamingConfig {
            enabled: true,
            ..NamingConfig::default()
        }
    }

    #[test]
    fn disabled_yields_nothing() {
        let syms = vec![func("tmp", vec![], SymbolKind::Stock)];
        assert!(analyze_naming("", &syms, &NamingConfig::default(), Locale::En).is_empty());
    }

    #[test]
    fn flags_placeholder_function_name() {
        let syms = vec![func("tmp", vec![], SymbolKind::Stock)];
        let d = analyze_naming("", &syms, &on(), Locale::En);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, codes::PP0018);
    }

    #[test]
    fn flags_short_parameter() {
        let syms = vec![func("doThing", vec![param("a")], SymbolKind::Public)];
        let d = analyze_naming("", &syms, &on(), Locale::En);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains('a'));
    }

    #[test]
    fn ignores_native_declarations() {
        // Nativas são API externa — nome fora do controle do autor.
        let syms = vec![func("x", vec![], SymbolKind::Native)];
        assert!(analyze_naming("", &syms, &on(), Locale::En).is_empty());
    }

    #[test]
    fn accepts_good_names() {
        let syms = vec![func(
            "givePlayerWeapon",
            vec![param("playerId"), param("weaponId")],
            SymbolKind::Stock,
        )];
        assert!(analyze_naming("", &syms, &on(), Locale::En).is_empty());
    }

    #[test]
    fn flags_poor_local_variable() {
        // `tmp` local é sinalizado; o índice `i` do for é tolerado.
        let text = "f() { new tmp = 0; for (new i = 0; i < 3; i++) {} }";
        let d = analyze_naming(text, &[], &on(), Locale::En);
        assert_eq!(d.len(), 1, "apenas `tmp` deve ser sinalizado, got: {d:?}");
        assert!(d[0].message.contains("tmp"));
    }

    #[test]
    fn macro_style_upper_case() {
        // macros = UPPER_CASE; um #define em minúsculas destoa.
        let mut cfg = on();
        cfg.style.macros = vec!["UPPER_CASE".to_string()];
        let syms = vec![func("maxPlayers", vec![], SymbolKind::Define)];
        let d = analyze_naming("", &syms, &cfg, Locale::En);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("maxPlayers"));
    }

    #[test]
    fn define_uses_macros_not_constants() {
        // #define responde a `macros`, não a `constants` (são categorias distintas).
        let mut cfg = on();
        cfg.style.constants = vec!["UPPER_CASE".to_string()]; // não deve afetar #define
        let syms = vec![func("maxPlayers", vec![], SymbolKind::Define)];
        assert!(analyze_naming("", &syms, &cfg, Locale::En).is_empty());
    }
}
