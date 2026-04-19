use std::collections::HashSet;

use tower_lsp::lsp_types::*;

use crate::workspace::WorkspaceState;

use super::collect_all_symbols;

pub fn get_at_completions(in_comment: bool, line: u32, at_col: u32) -> Vec<CompletionItem> {
    if in_comment {
        vec![CompletionItem {
            label: "@DEPRECATED".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Marca o símbolo seguinte como depreciado".to_string()),
            insert_text: Some("DEPRECATED".to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            sort_text: Some("0_DEPRECATED".to_string()),
            ..Default::default()
        }]
    } else {
        let range = Range {
            start: Position { line, character: at_col },
            end:   Position { line, character: at_col + 1 },
        };
        vec![CompletionItem {
            label: "@DEPRECATED".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Marca o símbolo seguinte como depreciado".to_string()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range,
                new_text: "// @DEPRECATED".to_string(),
            })),
            sort_text: Some("0_DEPRECATED".to_string()),
            ..Default::default()
        }]
    }
}

pub fn get_completions(state: &WorkspaceState, uri: &str) -> Vec<CompletionItem> {
    let Some(file_path) = crate::workspace::uri_to_path(uri) else {
        return vec![];
    };
    let Some(parsed) = state.get_parsed(uri) else {
        return vec![];
    };
    let inc_paths = state.include_paths();
    let all_syms = collect_all_symbols(state, &file_path, &inc_paths, &parsed);

    let mut items: Vec<CompletionItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for sym in &all_syms {
        if !seen.insert(sym.name.clone()) {
            continue;
        }
        items.push(build_symbol_item(sym));
    }

    items
}

fn build_symbol_item(sym: &crate::parser::types::Symbol) -> CompletionItem {
    use crate::parser::types::SymbolKind::*;

    let kind = Some(match sym.kind {
        Native | Forward | Public | Stock | Static => CompletionItemKind::FUNCTION,
        StaticConst | Define => CompletionItemKind::CONSTANT,
        Variable => CompletionItemKind::VARIABLE,
    });

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
        sort_text: Some(format!("0_{}", sym.name)),
        ..Default::default()
    };

    if sym.deprecated {
        item.tags = Some(vec![CompletionItemTag::DEPRECATED]);
    }

    item
}
