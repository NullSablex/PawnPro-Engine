use tower_lsp::lsp_types::*;

use crate::parser::lexer::strip_line_comments;
use crate::parser::types::SymbolKind;
use crate::workspace::WorkspaceState;

pub fn get_references(
    state: &WorkspaceState,
    uri: &str,
    pos: Position,
) -> Vec<Location> {
    let (word, is_callable) = {
        let Some(doc) = state.open_docs.get(uri) else {
            return vec![];
        };
        let Some(w) = word_at(&doc.text, pos.line, pos.character) else {
            return vec![];
        };
        let callable = resolve_callable(state, uri, &doc.text, &w, pos);
        (w, callable)
    };

    let mut locations: Vec<Location> = Vec::new();

    for entry in state.open_docs.iter() {
        let doc_uri = entry.key().as_str();
        let text = &entry.text;

        let mut in_block = false;
        for (line_idx, raw_line) in text.lines().enumerate() {
            let stripped = strip_line_comments(raw_line.trim_end_matches('\r'), in_block);
            in_block = stripped.in_block;
            let line = &stripped.text;

            let bytes = line.as_bytes();
            let wb = word.as_bytes();
            let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
            let mut col = 0usize;

            while col + wb.len() <= bytes.len() {
                if &bytes[col..col + wb.len()] == wb {
                    let before_ok = col == 0 || !is_ident(bytes[col - 1]);
                    let after_ok = col + wb.len() >= bytes.len() || !is_ident(bytes[col + wb.len()]);

                    if before_ok && after_ok {
                        let call_ok = !is_callable || {
                            let mut j = col + wb.len();
                            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                                j += 1;
                            }
                            j < bytes.len() && bytes[j] == b'('
                        };

                        if call_ok {
                            let start = Position { line: line_idx as u32, character: col as u32 };
                            let end = Position {
                                line: line_idx as u32,
                                character: (col + wb.len()) as u32,
                            };
                            if let Ok(loc_uri) = doc_uri.parse::<Url>() {
                                locations.push(Location { uri: loc_uri, range: Range { start, end } });
                            }
                        }
                    }
                }
                col += 1;
            }
        }
    }

    locations
}

fn resolve_callable(
    state: &WorkspaceState,
    uri: &str,
    text: &str,
    name: &str,
    pos: Position,
) -> bool {
    if let Some(parsed) = state.get_parsed(uri) {
        for sym in &parsed.symbols {
            if sym.name == name && sym.line == pos.line {
                return is_func_kind(&sym.kind);
            }
        }
    }

    if let Some(line_str) = text.lines().nth(pos.line as usize) {
        let bytes = line_str.as_bytes();
        let col = pos.character as usize;
        let mut end = col;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        let mut j = end;
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'(' {
            return true;
        }
    }

    let mut found_as_func = false;
    let mut found_as_non_func = false;

    for entry in state.open_docs.iter() {
        if let Some(parsed) = state.get_parsed(entry.key().as_str()) {
            for sym in &parsed.symbols {
                if sym.name == name {
                    if is_func_kind(&sym.kind) {
                        found_as_func = true;
                    } else {
                        found_as_non_func = true;
                    }
                }
            }
        }
    }

    found_as_func && !found_as_non_func
}

#[inline]
fn is_func_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Native
            | SymbolKind::Public
            | SymbolKind::Stock
            | SymbolKind::Static
            | SymbolKind::Forward
            | SymbolKind::Plain
    )
}

fn word_at(text: &str, line: u32, col: u32) -> Option<String> {
    let line_str = text.lines().nth(line as usize)?;
    let bytes = line_str.as_bytes();
    let col = col as usize;
    if col > bytes.len() {
        return None;
    }
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut start = col;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some(line_str[start..end].to_string())
}
