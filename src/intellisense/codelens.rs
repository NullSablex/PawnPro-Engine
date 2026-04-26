use std::collections::HashMap;

use serde_json::json;
use tower_lsp::lsp_types::*;

use crate::parser::lexer::strip_line_comments;
use crate::parser::types::SymbolKind;
use crate::workspace::WorkspaceState;

pub fn get_code_lens(state: &WorkspaceState, uri: &str) -> Vec<CodeLens> {
    let Some(parsed) = state.get_parsed(uri) else {
        return vec![];
    };

    let func_syms: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Native
                    | SymbolKind::Public
                    | SymbolKind::Stock
                    | SymbolKind::Static
                    | SymbolKind::Plain
            )
        })
        .collect();

    if func_syms.is_empty() {
        return vec![];
    }

    let maps: Vec<(HashMap<String, usize>, HashMap<String, usize>)> = state
        .open_docs
        .iter()
        .map(|e| build_freq_maps(&e.text))
        .collect();

    func_syms
        .iter()
        .map(|sym| {
            let total: usize = maps
                .iter()
                .map(|(_, call_m)| call_m.get(sym.name.as_str()).copied().unwrap_or(0))
                .sum();

            let refs = total.saturating_sub(1);

            let title = match refs {
                0 => "0 referências".to_string(),
                1 => "1 referência".to_string(),
                n => format!("{n} referências"),
            };

            let range = Range {
                start: Position { line: sym.line, character: sym.col },
                end: Position {
                    line: sym.line,
                    character: sym.col + sym.name.len() as u32,
                },
            };

            let command = if refs > 0 {
                Some(Command {
                    title,
                    command: "pawnpro.findReferences".to_string(),
                    arguments: Some(vec![
                        json!(uri),
                        json!(sym.line),
                        json!(sym.col),
                    ]),
                })
            } else {
                Some(Command {
                    title,
                    command: String::new(),
                    arguments: None,
                })
            };

            CodeLens { range, command, data: None }
        })
        .collect()
}

fn build_freq_maps(text: &str) -> (HashMap<String, usize>, HashMap<String, usize>) {
    let mut word_map: HashMap<String, usize> = HashMap::new();
    let mut call_map: HashMap<String, usize> = HashMap::new();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    let mut in_block = false;
    for raw_line in text.lines() {
        let stripped = strip_line_comments(raw_line.trim_end_matches('\r'), in_block);
        in_block = stripped.in_block;
        let bytes = stripped.text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if is_ident(bytes[i]) {
                let start = i;
                while i < bytes.len() && is_ident(bytes[i]) {
                    i += 1;
                }
                let word = &stripped.text[start..i];
                *word_map.entry(word.to_string()).or_insert(0) += 1;
                let mut j = i;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'(' {
                    *call_map.entry(word.to_string()).or_insert(0) += 1;
                }
            } else {
                i += 1;
            }
        }
    }
    (word_map, call_map)
}
