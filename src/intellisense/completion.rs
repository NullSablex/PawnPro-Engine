use std::collections::HashSet;

use tower_lsp::lsp_types::*;

use crate::parser::types::SymbolKind;
use crate::workspace::WorkspaceState;

use super::collect_all_symbols;

/// Retorna completions para o arquivo identificado por `uri`.
/// Inclui símbolos do próprio arquivo + todos os includes transitivos.
pub fn get_completions(state: &WorkspaceState, uri: &str) -> Vec<CompletionItem> {
    let Some(file_path) = crate::workspace::uri_to_path(uri) else {
        return vec![];
    };
    let Some(parsed) = state.get_parsed(uri) else {
        return vec![];
    };
    let inc_paths = state.include_paths();

    let all_syms = collect_all_symbols(state, &file_path, &inc_paths, &parsed);

    let mut items = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for sym in &all_syms {
        // Variáveis locais não são oferecidas como completions globais
        if matches!(sym.kind, SymbolKind::Variable) {
            continue;
        }
        if !seen.insert(sym.name.clone()) {
            continue; // deduplicação
        }
        items.push(build_item(sym));
    }

    items
}

fn build_item(sym: &crate::parser::types::Symbol) -> CompletionItem {
    use crate::parser::types::SymbolKind::*;

    let kind = Some(match sym.kind {
        Native | Forward | Public | Stock | Static => CompletionItemKind::FUNCTION,
        StaticConst | Define => CompletionItemKind::CONSTANT,
        Variable => CompletionItemKind::VARIABLE,
    });

    // Snippet com placeholders para cada parâmetro
    let (insert_text, insert_text_format) = if sym.signature.is_some() && !sym.params.is_empty() {
        let parts: Vec<String> = sym
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if p.is_variadic {
                    format!("${}", i + 1)
                } else {
                    format!("${{{}:{}}}", i + 1, p.name)
                }
            })
            .collect();
        (
            Some(format!("{}({})", sym.name, parts.join(", "))),
            Some(InsertTextFormat::SNIPPET),
        )
    } else if sym.signature.is_some() {
        // Função sem parâmetros
        (
            Some(format!("{}()", sym.name)),
            Some(InsertTextFormat::PLAIN_TEXT),
        )
    } else {
        (None, None)
    };

    let mut item = CompletionItem {
        label: sym.name.clone(),
        kind,
        detail: sym.signature.clone(),
        documentation: sym.doc.as_ref().map(|d| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: d.clone(),
            })
        }),
        insert_text,
        insert_text_format,
        ..Default::default()
    };

    if sym.deprecated {
        item.tags = Some(vec![CompletionItemTag::DEPRECATED]);
    }

    item
}
