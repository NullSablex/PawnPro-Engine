//! Cálculo de indentação estrutural para o formatador.
//!
//! Deriva o nível de cada statement da [`StmtTree`] em vez de contar chaves no
//! texto. Além da profundidade de blocos `{}`, soma os "corpos implícitos" —
//! um `if (x)` sem chaves indenta o próximo statement +1 — usando o `kind` de
//! cada statement, de forma determinística.

use crate::parser::stmt_parser::{Stmt, StmtKind, StmtTree};
use crate::util::to_u32;

/// Nível de indentação (em "passos", não colunas) atribuído a cada statement,
/// na mesma ordem de `tree.stmts`.
#[must_use]
pub fn indent_levels(tree: &StmtTree) -> Vec<u32> {
    let mut levels = Vec::with_capacity(tree.stmts.len());

    // Corpos de controle sem chaves pendentes, por profundidade de bloco.
    // `Single` (if/for/while/else/do) dura um statement; `Case` (case/default)
    // dura até a próxima etiqueta ou o fim do switch. Ver [`BodyKind`].
    let mut implicit: Vec<Body> = Vec::new();
    // Nível visual de cada bloco `{}` aberto: conteúdo em `nível + 1`, `}` em
    // `nível`. Guarda o nível resolvido na abertura (Allman alinha com o controle).
    let mut block_levels: Vec<u32> = Vec::new();

    for st in &tree.stmts {
        // Descarta corpos implícitos órfãos: se o statement atual está numa
        // profundidade de chaves MENOR do que um corpo pendente, o bloco que o
        // conteria já fechou (ex.: um `if` sem corpo claro antes de um `}`).
        // Sem isto, o resíduo contaminaria a indentação dos blocos seguintes.
        implicit.retain(|b| b.depth <= st.depth);

        let enclosing = block_levels.last().copied();

        // Corpos implícitos pendentes que ENVOLVEM este statement. Uma etiqueta
        // (case/default) substitui a etiqueta anterior em vez de indentar sob ela
        // — então não conta o `Case` da mesma profundidade que vai substituir.
        let is_label = matches!(st.kind, StmtKind::Case | StmtKind::Default);
        let extra = to_u32(
            implicit
                .iter()
                .filter(|b| b.depth == st.depth && !(is_label && b.kind == BodyKind::Case))
                .count(),
        );

        let level = match st.kind {
            StmtKind::BlockClose => block_levels.pop().unwrap_or(0),
            StmtKind::BlockOpen => {
                // A chave que serve de corpo a um controle satisfaz esse corpo e
                // alinha com o controle (desconta o +1 que ela mesma representa).
                let satisfies = implicit.iter().any(|b| b.depth == st.depth);
                let base = enclosing.map_or(st.depth, |b| b + 1);
                let open_level = base + extra - u32::from(satisfies);
                block_levels.push(open_level);
                open_level
            }
            _ => enclosing.map_or(st.depth, |b| b + 1) + extra,
        };
        levels.push(level);

        update_implicit(&mut implicit, st);
    }

    levels
}

/// Tipo de corpo implícito pendente, com a profundidade de bloco em que vive.
struct Body {
    depth: u32,
    kind: BodyKind,
}

#[derive(PartialEq)]
enum BodyKind {
    /// Corpo de um statement (if/for/while/else/do): um único statement.
    Single,
    /// Corpo de uma etiqueta de switch (case/default): até a próxima etiqueta.
    Case,
}

/// Atualiza a pilha de corpos implícitos após processar um statement.
fn update_implicit(implicit: &mut Vec<Body>, st: &Stmt) {
    let d = st.depth;
    match st.kind {
        // Uma nova etiqueta encerra o corpo `Case` anterior na profundidade e
        // abre o seu. Etiquetas não são "corpo" de if/for, então também encerram
        // qualquer `Single` pendente na profundidade.
        StmtKind::Case | StmtKind::Default => {
            implicit.retain(|b| b.depth != d);
            implicit.push(Body {
                depth: d,
                kind: BodyKind::Case,
            });
        }
        // Controles de corpo único: ELES SÃO o corpo do controle anterior, mas
        // como também abrem corpo, ESTENDEM a cadeia (não a encerram). Assim
        // `for`→`if`→stmt acumula +1 a cada nível. O `else` reabre o ramo do if.
        StmtKind::If | StmtKind::Else | StmtKind::For | StmtKind::While | StmtKind::Do => {
            implicit.push(Body {
                depth: d,
                kind: BodyKind::Single,
            });
        }
        // `BlockClose` encerra TODOS os corpos da profundidade (incl. o `Case` de
        // um switch que se fecha).
        StmtKind::BlockClose => implicit.retain(|b| b.depth != d),
        // Demais (BlockOpen e statements comuns) encerram só os corpos `Single`:
        // o bloco É o corpo do controle; corpos `Case` persistem até a próxima
        // etiqueta.
        _ => remove_single(implicit, d),
    }
}

/// Remove os corpos `Single` pendentes na profundidade `d` (corpos `Case` ficam).
fn remove_single(implicit: &mut Vec<Body>, d: u32) {
    implicit.retain(|b| !(b.depth == d && b.kind == BodyKind::Single));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::stmt_parser::parse_stmts;
    use crate::parser::token_lexer::tokenize;

    fn levels(src: &str) -> Vec<(StmtKind, u32, u32)> {
        let tree = parse_stmts(tokenize(src));
        let lv = indent_levels(&tree);
        tree.stmts
            .iter()
            .zip(lv)
            .map(|(s, l)| (s.kind.clone(), s.line, l))
            .collect()
    }

    #[test]
    fn for_if_block_levels_allman() {
        // main() { for (corpo: if) { conteúdo } bar() }
        // Estilo Allman: a chave de abertura do corpo de um controle alinha com o
        // controle; o conteúdo fica +1; o '}' alinha com a chave.
        let src = "main()\n{\nfor (i)\nif (x)\n{\nfoo();\n}\nbar();\n}\n";
        let lv = levels(src);
        let by_line = |ln: u32| lv.iter().find(|(_, l, _)| *l == ln).map(|(_, _, n)| *n);
        assert_eq!(by_line(0), Some(0)); // main
        assert_eq!(by_line(1), Some(0)); // { (corpo de main)
        assert_eq!(by_line(2), Some(1)); // for (dentro de main)
        assert_eq!(by_line(3), Some(2)); // if (corpo do for)
        assert_eq!(by_line(4), Some(2)); // { (corpo do if) — alinha com o if
        assert_eq!(by_line(5), Some(3)); // foo() — conteúdo do bloco
        assert_eq!(by_line(6), Some(2)); // } — alinha com a chave de abertura
        assert_eq!(by_line(7), Some(1)); // bar() — de volta ao corpo de main
        assert_eq!(by_line(8), Some(0)); // } fecha main
    }

    #[test]
    fn implicit_if_body() {
        // if (a)\n foo();  →  foo() indenta +1
        let src = "if (a)\nfoo();\n";
        let lv = levels(src);
        let foo = lv.iter().find(|(k, _, _)| *k == StmtKind::Expr).unwrap();
        assert_eq!(foo.2, 1, "corpo do if sem chaves deve estar em nível 1");
    }

    #[test]
    fn nested_implicit_for_if() {
        // for sem chaves cujo corpo é um if sem chaves cujo corpo é foo()
        let src = "for (i)\nif (x)\nfoo();\n";
        let lv = levels(src);
        let by_kind = |k: StmtKind| lv.iter().find(|(kk, _, _)| *kk == k).map(|(_, _, n)| *n);
        assert_eq!(by_kind(StmtKind::For), Some(0));
        assert_eq!(by_kind(StmtKind::If), Some(1)); // corpo do for
        assert_eq!(by_kind(StmtKind::Expr), Some(2)); // corpo do if
    }

    #[test]
    fn switch_case_bodies() {
        // case/default indentam seu corpo (pode ter vários statements).
        let src = "main()\n{\nswitch (x)\n{\ncase 1:\nfoo();\nbar();\ncase 2:\nbaz();\ndefault:\nqux();\n}\n}\n";
        let lv = levels(src);
        let by_line = |ln: u32| lv.iter().find(|(_, l, _)| *l == ln).map(|(_, _, n)| *n);
        assert_eq!(by_line(2), Some(1)); // switch
        assert_eq!(by_line(3), Some(1)); // { do switch (alinha com switch)
        assert_eq!(by_line(4), Some(2)); // case 1:
        assert_eq!(by_line(5), Some(3)); // foo() — corpo do case
        assert_eq!(by_line(6), Some(3)); // bar() — ainda corpo do case 1
        assert_eq!(by_line(7), Some(2)); // case 2:
        assert_eq!(by_line(8), Some(3)); // baz() — corpo do case 2
        assert_eq!(by_line(9), Some(2)); // default:
        assert_eq!(by_line(10), Some(3)); // qux() — corpo do default
        assert_eq!(by_line(11), Some(1)); // } fecha switch
    }

    #[test]
    fn if_else_blocks() {
        // if/else com blocos em Allman.
        let src = "main()\n{\nif (a)\n{\nfoo();\n}\nelse\n{\nbar();\n}\n}\n";
        let lv = levels(src);
        let by_line = |ln: u32| lv.iter().find(|(_, l, _)| *l == ln).map(|(_, _, n)| *n);
        assert_eq!(by_line(2), Some(1)); // if
        assert_eq!(by_line(3), Some(1)); // { do if (alinha com if)
        assert_eq!(by_line(4), Some(2)); // foo()
        assert_eq!(by_line(5), Some(1)); // }
        assert_eq!(by_line(6), Some(1)); // else
        assert_eq!(by_line(7), Some(1)); // { do else
        assert_eq!(by_line(8), Some(2)); // bar()
        assert_eq!(by_line(9), Some(1)); // }
    }

    #[test]
    fn do_while_block() {
        // do { corpo } while (x); — o while de fechamento alinha com o do.
        let src = "main()\n{\ndo\n{\nfoo();\n}\nwhile (x);\n}\n";
        let lv = levels(src);
        let by_line = |ln: u32| lv.iter().find(|(_, l, _)| *l == ln).map(|(_, _, n)| *n);
        assert_eq!(by_line(2), Some(1)); // do
        assert_eq!(by_line(3), Some(1)); // { do do (alinha com do)
        assert_eq!(by_line(4), Some(2)); // foo()
        assert_eq!(by_line(5), Some(1)); // }
        assert_eq!(by_line(6), Some(1)); // while (x);
    }

    #[test]
    fn else_if_chain() {
        // if/else if/else sem chaves: cada corpo indenta sob seu controle, mas o
        // `else if` reabre no nível do `else` (não acumula um degrau extra).
        let src = "main()\n{\nif (a)\nfoo();\nelse if (b)\nbar();\nelse\nbaz();\n}\n";
        let lv = levels(src);
        // line 4 tem dois statements (Else + If); checamos por kind+linha.
        let at = |ln: u32, k: StmtKind| {
            lv.iter()
                .find(|(kk, l, _)| *l == ln && *kk == k)
                .map(|(_, _, n)| *n)
        };
        assert_eq!(at(2, StmtKind::If), Some(1)); // if (a)
        assert_eq!(at(3, StmtKind::Expr), Some(2)); // foo() — corpo do if
        assert_eq!(at(4, StmtKind::Else), Some(1)); // else
        assert_eq!(at(4, StmtKind::If), Some(2)); // if (b) — corpo do else
        assert_eq!(at(5, StmtKind::Expr), Some(3)); // bar() — corpo do else-if
        assert_eq!(at(6, StmtKind::Else), Some(1)); // else final
        assert_eq!(at(7, StmtKind::Expr), Some(2)); // baz() — corpo do else
    }
}
