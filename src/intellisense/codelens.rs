use tower_lsp::lsp_types::*;

use crate::parser::types::SymbolKind;
use crate::workspace::WorkspaceState;

/// Retorna CodeLens com contagem de referências para funções no arquivo `uri`.
/// Conta ocorrências nos documentos abertos (exceto o próprio arquivo).
pub fn get_code_lens(state: &WorkspaceState, uri: &str) -> Vec<CodeLens> {
    let Some(parsed) = state.get_parsed(uri) else {
        return vec![];
    };

    // Apenas símbolos "chamáveis" recebem CodeLens
    let func_syms: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Native
                    | SymbolKind::Forward
                    | SymbolKind::Public
                    | SymbolKind::Stock
                    | SymbolKind::Static
                    | SymbolKind::StaticConst
            )
        })
        .collect();

    if func_syms.is_empty() {
        return vec![];
    }

    // Textos de todos os documentos abertos (exceto o próprio)
    let search_texts: Vec<String> = state
        .open_docs
        .iter()
        .filter(|e| e.key().as_str() != uri)
        .map(|e| e.text.clone())
        .collect();

    func_syms
        .iter()
        .map(|sym| {
            let refs: usize = search_texts
                .iter()
                .map(|t| count_word(sym.name.as_str(), t))
                .sum();

            let title = match refs {
                0 => "0 referências".to_string(),
                1 => "1 referência".to_string(),
                n => format!("{} referências", n),
            };

            let range = Range {
                start: Position { line: sym.line, character: sym.col },
                end: Position {
                    line: sym.line,
                    character: sym.col + sym.name.len() as u32,
                },
            };

            CodeLens {
                range,
                command: Some(Command {
                    title,
                    command: String::new(), // lens de leitura — sem ação
                    arguments: None,
                }),
                data: None,
            }
        })
        .collect()
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Conta ocorrências de `name` como palavra inteira (word boundary) em `text`.
fn count_word(name: &str, text: &str) -> usize {
    let nb = name.as_bytes();
    let tb = text.as_bytes();
    if tb.len() < nb.len() {
        return 0;
    }
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut count = 0usize;
    for i in 0..=(tb.len() - nb.len()) {
        if &tb[i..i + nb.len()] == nb {
            let before_ok = i == 0 || !is_ident(tb[i - 1]);
            let after_ok = i + nb.len() >= tb.len() || !is_ident(tb[i + nb.len()]);
            if before_ok && after_ok {
                count += 1;
            }
        }
    }
    count
}
