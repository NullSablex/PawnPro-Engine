//! Extração dos nomes de variáveis locais a partir dos tokens.
//!
//! O `StmtTree` marca uma declaração `new`/`decl` como um único `VarDecl` sem
//! guardar os identificadores. Aqui varremos os tokens e reconstruímos a lista
//! de nomes declarados — apenas dentro de corpos de função (profundidade de
//! chaves > 0), já que globais de topo já vêm de `parsed.symbols`.
//!
//! Gramática reconhecida (simplificada do compilador):
//! `new|decl|static [const] [tag:] nome [[dim]] [= init] [, …] ;`

use super::{NameCategory, NameSite};
use crate::parser::token_lexer::{TokenKind, tokenize};

/// Devolve os identificadores locais declarados em `text`, com posição. Loops
/// (`for (new i …)`) marcam `in_loop_header = true` para tolerar índices curtos.
#[must_use]
pub fn collect_local_decls(text: &str) -> Vec<NameSite> {
    let tokens = tokenize(text).tokens;
    let mut sites = Vec::new();
    let mut brace_depth: i32 = 0;
    // Profundidade de parênteses do `for (...)` corrente, ou None fora de um for.
    // Usado para marcar declarações na cláusula de inicialização do loop.
    let mut for_paren: Option<i32> = None;
    let mut paren_depth: i32 = 0;

    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        match t.kind {
            TokenKind::LBrace => brace_depth += 1,
            TokenKind::RBrace => brace_depth -= 1,
            TokenKind::LParen => paren_depth += 1,
            TokenKind::RParen => {
                paren_depth -= 1;
                if for_paren == Some(paren_depth) {
                    for_paren = None;
                }
            }
            TokenKind::Ident if t.value == "for" => {
                // O '(' seguinte abre a cláusula do for; declarações ali são índices.
                for_paren = Some(paren_depth);
            }
            // `new`/`decl` e `static` local (persistente). `static` só conta aqui
            // dentro de um corpo (brace_depth > 0); no topo é global, já coberto
            // por `parsed.symbols`.
            TokenKind::Ident
                if brace_depth > 0 && matches!(t.value.as_str(), "new" | "decl" | "static") =>
            {
                let in_loop = for_paren.is_some();
                i = collect_decl_names(&tokens, i + 1, in_loop, &mut sites);
                continue;
            }
            _ => {}
        }
        i += 1;
    }

    sites
}

/// A partir do token após `new`/`decl`, coleta os nomes até o fim do statement
/// (`;`) ou da cláusula. Devolve o índice do token onde parar.
fn collect_decl_names(
    tokens: &[crate::parser::token_lexer::Token],
    start: usize,
    in_loop: bool,
    sites: &mut Vec<NameSite>,
) -> usize {
    let mut i = start;
    let mut expect_name = true; // início de cada item da lista espera um nome
    while i < tokens.len() {
        match tokens[i].kind {
            TokenKind::Semicolon | TokenKind::LBrace | TokenKind::RBrace => break,
            TokenKind::Comma => {
                expect_name = true;
                i += 1;
            }
            TokenKind::Ident if expect_name => {
                // Qualificador `const` (ex.: `static const x`) não é o nome.
                if tokens[i].value == "const" {
                    i += 1;
                    continue;
                }
                // `tag:` antes do nome — se o próximo é ':' simples, este Ident é a
                // tag e o nome verdadeiro vem depois.
                if matches!(tokens.get(i + 1).map(|t| &t.kind), Some(TokenKind::Colon)) {
                    i += 2; // pula `tag` e `:`, continua esperando o nome
                    continue;
                }
                let tok = &tokens[i];
                sites.push(NameSite {
                    name: tok.value.clone(),
                    line: tok.line,
                    col: tok.col,
                    in_loop_header: in_loop,
                    category: NameCategory::Local,
                });
                expect_name = false; // já temos o nome deste item
                i += 1;
            }
            // Após o nome: dimensões, inicializador, etc. — ignorados até a vírgula.
            _ => {
                expect_name = false;
                i += 1;
            }
        }
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        collect_local_decls(text)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn single_local() {
        assert_eq!(names("f() { new count; }"), vec!["count"]);
    }

    #[test]
    fn multiple_in_one_decl() {
        assert_eq!(names("f() { new a, b, c; }"), vec!["a", "b", "c"]);
    }

    #[test]
    fn skips_tag() {
        // `Float:x` — o nome é `x`, não `Float`.
        assert_eq!(names("f() { new Float:x; }"), vec!["x"]);
    }

    #[test]
    fn skips_initializer_and_dims() {
        assert_eq!(
            names("f() { new n = GetCount(), arr[32]; }"),
            vec!["n", "arr"]
        );
    }

    #[test]
    fn ignores_globals_at_top_level() {
        // Fora de qualquer função (brace_depth == 0) não coletamos.
        assert!(names("new gGlobal;").is_empty());
    }

    #[test]
    fn marks_for_loop_index() {
        let sites = collect_local_decls("f() { for (new i = 0; i < 3; i++) {} }");
        let idx = sites.iter().find(|s| s.name == "i").unwrap();
        assert!(
            idx.in_loop_header,
            "índice de for deve marcar in_loop_header"
        );
    }

    #[test]
    fn local_after_loop_is_not_marked() {
        let sites = collect_local_decls("f() { for (new i = 0; i < 3; i++) {} new x; }");
        let x = sites.iter().find(|s| s.name == "x").unwrap();
        assert!(!x.in_loop_header);
    }

    #[test]
    fn captures_static_local() {
        assert_eq!(names("f() { static counter; }"), vec!["counter"]);
    }

    #[test]
    fn skips_const_qualifier() {
        // `static const MAX = 10` — o nome é `MAX`, não `const`.
        assert_eq!(names("f() { static const MAX = 10; }"), vec!["MAX"]);
    }
}
