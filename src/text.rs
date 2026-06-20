//! Utilitários de texto e posição compartilhados pelos provedores LSP.
//!
//! Centraliza a definição de "identificador" (`[A-Za-z0-9_]`) e a localização da
//! palavra sob um cursor, evitando reimplementações divergentes entre provedores.

use tower_lsp::lsp_types::{Position, Range};

use crate::util::to_u32;

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Limites `[start, end)` em bytes do identificador que cobre `col` na linha
/// `line` de `text`. `None` se a posição não toca um identificador.
#[must_use]
pub fn word_bounds(text: &str, line: u32, col: u32) -> Option<(usize, usize)> {
    let line_str = text.lines().nth(line as usize)?;
    let bytes = line_str.as_bytes();
    let col = col as usize;
    if col > bytes.len() {
        return None;
    }
    let mut start = col;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    (start != end).then_some((start, end))
}

/// O identificador sob `pos`, ou `None`.
#[must_use]
pub fn word_at(text: &str, pos: Position) -> Option<String> {
    let line_str = text.lines().nth(pos.line as usize)?;
    let (start, end) = word_bounds(text, pos.line, pos.character)?;
    Some(line_str[start..end].to_string())
}

/// O intervalo LSP do identificador sob `pos`, ou `None`.
#[must_use]
pub fn word_range_at(text: &str, pos: Position) -> Option<Range> {
    let (start, end) = word_bounds(text, pos.line, pos.character)?;
    Some(Range {
        start: Position {
            line: pos.line,
            character: to_u32(start),
        },
        end: Position {
            line: pos.line,
            character: to_u32(end),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, ch: u32) -> Position {
        Position {
            line,
            character: ch,
        }
    }

    #[test]
    fn finds_word_under_cursor() {
        assert_eq!(
            word_at("new playerId;", pos(0, 6)).as_deref(),
            Some("playerId")
        );
    }

    #[test]
    fn cursor_at_word_start_and_end() {
        assert_eq!(word_at("foo bar", pos(0, 4)).as_deref(), Some("bar"));
        assert_eq!(word_at("foo bar", pos(0, 7)).as_deref(), Some("bar"));
    }

    #[test]
    fn none_in_middle_of_whitespace() {
        // Posição cercada por espaços dos dois lados não toca identificador.
        assert!(word_at("a  b", pos(0, 2)).is_none());
    }

    #[test]
    fn cursor_touching_word_end_selects_it() {
        // Encostado no fim de "a" (à esquerda é ident) — seleciona "a".
        assert_eq!(word_at("a  b", pos(0, 1)).as_deref(), Some("a"));
    }

    #[test]
    fn range_covers_identifier() {
        let r = word_range_at("  count = 0;", pos(0, 4)).unwrap();
        assert_eq!((r.start.character, r.end.character), (2, 7));
    }
}
