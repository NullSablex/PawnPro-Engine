//! Quick fixes de remoção de código não usado (`PP0005`/`PP0006`/`PP0009`/
//! `PP0016`). Produz o `Range` da declaração inteira a remover a partir da
//! posição do diagnóstico no texto.
//!
//! Conservador: na dúvida sobre os limites da declaração, devolve `None` (sem
//! quick fix) em vez de um range que apagaria código demais.

use tower_lsp::lsp_types::{Position, Range};

use crate::util::to_u32;

/// Tipo de declaração a remover, derivado do código do diagnóstico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalKind {
    /// Variável local/global (`new x;`, `new x = 0;`): do início da linha ao `;`.
    Variable,
    /// Função (`stock`/`static`/plain): do início ao `}` que fecha o corpo.
    Function,
    /// Parâmetro: apenas o identificador dentro da lista (sem mexer em chamadas).
    Parameter,
    /// Diretiva de uma linha (`#define`, `#include`): a linha inteira. Uma
    /// continuação com `\` no fim arrasta as linhas seguintes.
    Directive,
    /// Corpo `{ … }` de um `native`/`forward`, que o compilador não aceita:
    /// remove do `{` ao `}`, deixando a declaração terminada em `;`.
    IllegalBody,
}

/// Mapeia um código de diagnóstico para o tipo de remoção, ou `None` se o código
/// não for removível.
#[must_use]
pub fn removal_kind(code: &str) -> Option<RemovalKind> {
    match code {
        "PP0005" => Some(RemovalKind::Variable),
        "PP0006" | "PP0016" => Some(RemovalKind::Function),
        "PP0009" => Some(RemovalKind::Parameter),
        "PP0011" | "PP0012" => Some(RemovalKind::Directive),
        "PP0002" | "PP0003" => Some(RemovalKind::IllegalBody),
        _ => None,
    }
}

/// Range a remover para a declaração na linha `line`, conforme `kind`. `col` é a
/// coluna do identificador (do diagnóstico). `None` se os limites não puderem ser
/// determinados com segurança.
#[must_use]
pub fn removal_range(text: &str, line: u32, col: u32, kind: RemovalKind) -> Option<Range> {
    let lines: Vec<&str> = text.lines().collect();
    let cur = *lines.get(line as usize)?;
    match kind {
        RemovalKind::Variable => variable_range(cur, line),
        RemovalKind::Function => function_range(&lines, line),
        RemovalKind::Parameter => parameter_range(cur, line, col),
        RemovalKind::Directive => directive_range(&lines, line),
        RemovalKind::IllegalBody => illegal_body_range(&lines, line),
    }
}

/// Corpo ilegal de `native`/`forward`: do `{` até o `}` que o fecha. O `{` pode
/// estar na própria linha da declaração ou na seguinte.
///
/// Devolve `None` se as chaves não fecharem no arquivo — melhor não oferecer o
/// fix do que apagar até o fim.
fn illegal_body_range(lines: &[&str], line: u32) -> Option<Range> {
    let decl = line as usize;
    // Acha a linha e a coluna do `{` de abertura (declaração ou a seguinte).
    let (open_line, open_col) =
        (decl..=decl + 1).find_map(|i| lines.get(i).and_then(|l| l.find('{').map(|c| (i, c))))?;

    let mut depth = 0i32;
    for (i, l) in lines.iter().enumerate().skip(open_line) {
        let from = if i == open_line { open_col } else { 0 };
        for (c, ch) in l.char_indices().skip_while(|(c, _)| *c < from) {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Range {
                            start: Position {
                                line: to_u32(open_line),
                                character: to_u32(open_col),
                            },
                            end: Position {
                                line: to_u32(i),
                                character: to_u32(c + 1),
                            },
                        });
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Diretiva: a linha inteira, mais as seguintes enquanto a anterior terminar em
/// `\` (continuação de macro). Só age quando a linha começa mesmo com `#`.
fn directive_range(lines: &[&str], line: u32) -> Option<Range> {
    let start = line as usize;
    if !lines.get(start)?.trim_start().starts_with('#') {
        return None;
    }
    let mut end = start;
    while lines.get(end).is_some_and(|l| l.trim_end().ends_with('\\')) {
        // Uma continuação sem linha seguinte não existe; parar evita estourar.
        if end + 1 >= lines.len() {
            break;
        }
        end += 1;
    }
    Some(Range {
        start: Position {
            line: to_u32(start),
            character: 0,
        },
        end: Position {
            line: to_u32(end + 1),
            character: 0,
        },
    })
}

/// Variável: remove a linha inteira da declaração se ela contém só essa
/// declaração (`new …;`). Conservador — só age quando a linha termina em `;` e
/// começa com um qualificador de declaração.
fn variable_range(cur: &str, line: u32) -> Option<Range> {
    let t = cur.trim_start();
    let is_decl = t.starts_with("new ")
        || t.starts_with("static ")
        || t.starts_with("decl ")
        || t.starts_with("const ");
    if !is_decl || !cur.trim_end().ends_with(';') {
        return None;
    }
    Some(full_line_range(line))
}

/// Função: do início da linha de declaração até a linha do `}` que fecha o corpo,
/// contando o balanço de chaves. `None` se não houver `{` ou se não fechar.
fn function_range(lines: &[&str], line: u32) -> Option<Range> {
    let start = line as usize;
    let mut depth: i32 = 0;
    let mut seen_open = false;
    for (offset, raw) in lines.iter().enumerate().skip(start) {
        for ch in strip_for_braces(raw).chars() {
            match ch {
                '{' => {
                    depth += 1;
                    seen_open = true;
                }
                '}' => {
                    depth -= 1;
                    if seen_open && depth == 0 {
                        let end_line = to_u32(offset);
                        let last = lines[offset];
                        return Some(Range {
                            start: Position { line, character: 0 },
                            end: Position {
                                line: end_line,
                                character: to_u32(last.chars().count()),
                            },
                        });
                    }
                }
                _ => {}
            }
        }
        // Forward sem corpo (`stock foo();`) — termina em `;` antes de qualquer `{`.
        if !seen_open && strip_for_braces(raw).trim_end().ends_with(';') {
            return Some(full_line_range(line));
        }
    }
    None
}

/// Parâmetro: remove apenas o identificador na lista (sem tocar nas chamadas),
/// junto da vírgula adjacente quando houver, para não deixar `,,` ou `(,`.
fn parameter_range(cur: &str, line: u32, col: u32) -> Option<Range> {
    let bytes = cur.as_bytes();
    let start = col as usize;
    if start >= bytes.len() {
        return None;
    }
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut end = start;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }
    // Inclui uma vírgula imediatamente após (e espaços) ou antes, para limpar o
    // separador remanescente.
    let mut from = start;
    let mut to = end;
    let mut after = to;
    while after < bytes.len() && (bytes[after] == b' ' || bytes[after] == b'\t') {
        after += 1;
    }
    if after < bytes.len() && bytes[after] == b',' {
        to = after + 1;
    } else {
        // Sem vírgula à frente: tenta remover a vírgula anterior (último parâmetro).
        let mut before = from;
        while before > 0 && (bytes[before - 1] == b' ' || bytes[before - 1] == b'\t') {
            before -= 1;
        }
        if before > 0 && bytes[before - 1] == b',' {
            from = before - 1;
        }
    }
    Some(Range {
        start: Position {
            line,
            character: to_u32(from),
        },
        end: Position {
            line,
            character: to_u32(to),
        },
    })
}

/// Range que cobre a linha inteira incluindo a quebra (para removê-la por completo).
fn full_line_range(line: u32) -> Range {
    Range {
        start: Position { line, character: 0 },
        end: Position {
            line: line + 1,
            character: 0,
        },
    }
}

/// Remove conteúdo de strings, chars e comentário de linha para a contagem de
/// chaves não ser enganada por `{`/`}` dentro deles.
fn strip_for_braces(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_str = false;
    let mut in_char = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' if !in_char => in_str = !in_str,
            '\'' if !in_str => in_char = !in_char,
            '\\' if in_str || in_char => {
                i += 2;
                continue;
            }
            '/' if !in_str && !in_char && chars.get(i + 1) == Some(&'/') => break,
            _ if in_str || in_char => {
                i += 1;
                continue;
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {

    #[test]
    fn directive_removes_the_whole_line() {
        let src = "#define MAX 10\nmain() {}\n";
        let r = removal_range(src, 0, 8, RemovalKind::Directive).unwrap();
        assert_eq!(r.start.line, 0);
        assert_eq!(r.end.line, 1);
        assert_eq!(r.end.character, 0);
    }

    #[test]
    fn directive_follows_backslash_continuations() {
        let src = "#define LONGA(%0) \\\n    foo(%0) \\\n    bar(%0)\nmain() {}\n";
        let r = removal_range(src, 0, 8, RemovalKind::Directive).unwrap();
        assert_eq!(r.end.line, 3, "deve arrastar as continuações");
    }

    #[test]
    fn directive_ignores_a_line_that_is_not_a_directive() {
        assert!(removal_range("new x;\n", 0, 4, RemovalKind::Directive).is_none());
    }

    #[test]
    fn illegal_body_on_the_same_line() {
        let src = "native Foo() { return 1; }\n";
        let r = removal_range(src, 0, 7, RemovalKind::IllegalBody).unwrap();
        assert_eq!((r.start.line, r.start.character), (0, 13));
        assert_eq!((r.end.line, r.end.character), (0, 26));
    }

    #[test]
    fn illegal_body_with_the_brace_on_the_next_line() {
        let src = "forward Foo()\n{\n    return 1;\n}\n";
        let r = removal_range(src, 0, 8, RemovalKind::IllegalBody).unwrap();
        assert_eq!(r.start.line, 1);
        assert_eq!((r.end.line, r.end.character), (3, 1));
    }

    #[test]
    fn illegal_body_handles_nested_braces() {
        let src = "native Foo() { if (a) { b(); } }\n";
        let r = removal_range(src, 0, 7, RemovalKind::IllegalBody).unwrap();
        assert_eq!((r.end.line, r.end.character), (0, 32));
    }

    #[test]
    fn illegal_body_unbalanced_gives_no_fix() {
        // Sem `}` que feche: melhor não oferecer o fix do que apagar até o fim.
        let src = "native Foo() {\n    return 1;\n";
        assert!(removal_range(src, 0, 7, RemovalKind::IllegalBody).is_none());
    }
    use super::*;

    #[test]
    fn maps_codes() {
        assert_eq!(removal_kind("PP0005"), Some(RemovalKind::Variable));
        assert_eq!(removal_kind("PP0006"), Some(RemovalKind::Function));
        assert_eq!(removal_kind("PP0016"), Some(RemovalKind::Function));
        assert_eq!(removal_kind("PP0009"), Some(RemovalKind::Parameter));
        assert_eq!(removal_kind("PP0001"), None);
    }

    #[test]
    fn removes_whole_variable_line() {
        let text = "main()\n{\n    new unused = 0;\n}\n";
        let r = removal_range(text, 2, 8, RemovalKind::Variable).unwrap();
        assert_eq!(r.start.line, 2);
        assert_eq!(r.end.line, 3); // remove a linha inteira (até o início da próxima)
    }

    #[test]
    fn variable_range_none_when_not_a_decl() {
        // Linha que não é declaração simples não é removida.
        let text = "    foo();\n";
        assert!(removal_range(text, 0, 4, RemovalKind::Variable).is_none());
    }

    #[test]
    fn removes_function_block() {
        let text = "stock unused()\n{\n    return 1;\n}\nmain() {}\n";
        let r = removal_range(text, 0, 6, RemovalKind::Function).unwrap();
        assert_eq!(r.start.line, 0);
        assert_eq!(r.end.line, 3); // fecha no `}` da função
    }

    #[test]
    fn removes_forward_without_body() {
        let text = "forward unused();\nmain() {}\n";
        let r = removal_range(text, 0, 8, RemovalKind::Function).unwrap();
        assert_eq!(r.start.line, 0);
        assert_eq!(r.end.line, 1);
    }

    #[test]
    fn removes_parameter_with_trailing_comma() {
        // `f(a, b)` — remover `a` tira também a vírgula seguinte.
        let text = "stock f(a, b) {}\n";
        let r = removal_range(text, 0, 8, RemovalKind::Parameter).unwrap();
        let removed = &"stock f(a, b) {}"[r.start.character as usize..r.end.character as usize];
        assert_eq!(removed, "a,");
    }

    #[test]
    fn removes_last_parameter_with_leading_comma() {
        // `f(a, b)` — remover `b` (último) tira a vírgula anterior.
        let text = "stock f(a, b) {}\n";
        let r = removal_range(text, 0, 11, RemovalKind::Parameter).unwrap();
        let removed = &"stock f(a, b) {}"[r.start.character as usize..r.end.character as usize];
        assert_eq!(removed, ", b");
    }

    #[test]
    fn brace_inside_string_does_not_close_function() {
        let text = "stock f()\n{\n    new s[] = \"}\";\n    return s;\n}\nmain(){}\n";
        let r = removal_range(text, 0, 6, RemovalKind::Function).unwrap();
        assert_eq!(r.end.line, 4); // o `}` real, não o da string
    }
}
