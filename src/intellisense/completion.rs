use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;
use tower_lsp::lsp_types::*;

use crate::workspace::WorkspaceState;

use super::collect_all_symbols;

static RX_NEW: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:^|;|\{)\s*(?:new|static)\s+(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)").unwrap()
});

// ─── Keyword snippets ─────────────────────────────────────────────────────────

struct KwSnippet {
    label:  &'static str,
    detail: &'static str,
    insert: &'static str,
    is_snippet: bool,
}

/// Keywords válidas dentro do corpo de uma função
static KW_IN_BODY: &[KwSnippet] = &[
    // ── Controle de fluxo ─────────────────────────────────────────────────────
    KwSnippet { label: "if",       detail: "if (condição) { }",                insert: "if (${1:condition})\n\t$0", is_snippet: true },
    KwSnippet { label: "if/else",  detail: "if/else",                          insert: "if (${1:condition})\n\t$2\nelse\n\t$0", is_snippet: true },
    KwSnippet { label: "else",     detail: "else",                             insert: "else\n\t$0", is_snippet: true },
    KwSnippet { label: "for",      detail: "for (new i = 0; i < n; ++i)",      insert: "for (new ${1:i} = 0; ${1:i} < ${2:count}; ++${1:i})\n\t$0", is_snippet: true },
    KwSnippet { label: "while",    detail: "while (condição) { }",             insert: "while (${1:condition})\n\t$0", is_snippet: true },
    KwSnippet { label: "do",       detail: "do { } while (condição)",          insert: "do\n{\n\t$0\n}\nwhile (${1:condition});", is_snippet: true },
    KwSnippet { label: "switch",   detail: "switch (valor) { case: }",         insert: "switch (${1:value})\n{\n\tcase ${2:0}: $0\n}", is_snippet: true },
    KwSnippet { label: "case",     detail: "case valor:",                      insert: "case ${1:value}: $0", is_snippet: true },
    KwSnippet { label: "default",  detail: "default: (switch)",                insert: "default: $0", is_snippet: true },
    KwSnippet { label: "return",   detail: "return valor",                     insert: "return ${1:value};", is_snippet: true },
    KwSnippet { label: "break",    detail: "break — sai do loop/switch",       insert: "break;", is_snippet: false },
    KwSnippet { label: "continue", detail: "continue — próxima iteração",      insert: "continue;", is_snippet: false },
    KwSnippet { label: "goto",     detail: "goto label",                       insert: "goto ${1:label};", is_snippet: true },
    KwSnippet { label: "exit",     detail: "exit — encerra o script",          insert: "exit;", is_snippet: false },

    // ── Declarações locais ────────────────────────────────────────────────────
    KwSnippet { label: "new",      detail: "new variável local",               insert: "new ${1:name};", is_snippet: true },

    // ── Operadores do compilador (sintaxe especial, não são chamadas) ─────────
    // sizeof e tagof usam parênteses mas NÃO são chamadas de função
    KwSnippet { label: "sizeof",   detail: "sizeof variável — tamanho",        insert: "sizeof(${1:var})", is_snippet: true },
    KwSnippet { label: "tagof",    detail: "tagof variável — tag numérica",    insert: "tagof(${1:var})", is_snippet: true },

    // ── Literais ──────────────────────────────────────────────────────────────
    KwSnippet { label: "true",     detail: "true (1)",                         insert: "true", is_snippet: false },
    KwSnippet { label: "false",    detail: "false (0)",                        insert: "false", is_snippet: false },
    KwSnippet { label: "cellmax",  detail: "valor máximo de cell",             insert: "cellmax", is_snippet: false },
    KwSnippet { label: "cellmin",  detail: "valor mínimo de cell",             insert: "cellmin", is_snippet: false },
    KwSnippet { label: "cellbits", detail: "bits por cell",                    insert: "cellbits", is_snippet: false },
];

/// Keywords válidas fora de funções (nível de arquivo)
static KW_TOP_LEVEL: &[KwSnippet] = &[
    KwSnippet { label: "stock",       detail: "stock function",                      insert: "stock ${1:Name}(${2:params})\n{\n\t$0\n}", is_snippet: true },
    KwSnippet { label: "public",      detail: "public function (callback)",           insert: "public ${1:Name}(${2:params})\n{\n\t$0\n}", is_snippet: true },
    KwSnippet { label: "forward",     detail: "forward declaração",                  insert: "forward ${1:Name}(${2:params});", is_snippet: true },
    KwSnippet { label: "native",      detail: "native declaração",                   insert: "native ${1:Name}(${2:params});", is_snippet: true },
    KwSnippet { label: "static",      detail: "static function/variável",            insert: "static ${1:Name}(${2:params})\n{\n\t$0\n}", is_snippet: true },
    KwSnippet { label: "enum",        detail: "enum declaração",                     insert: "enum ${1:Name}\n{\n\t${2:VALUE}\n}", is_snippet: true },
    KwSnippet { label: "const",       detail: "constante global",                    insert: "const ${1:NAME} = ${2:0};", is_snippet: true },
    KwSnippet { label: "new",         detail: "variável global",                     insert: "new ${1:name};", is_snippet: true },
    // Diretivas — só válidas fora de funções e no início da linha
    KwSnippet { label: "#define",     detail: "#define macro",                       insert: "#define ${1:NAME} ${2:value}", is_snippet: true },
    KwSnippet { label: "#undef",      detail: "#undef macro",                        insert: "#undef ${1:NAME}", is_snippet: true },
    KwSnippet { label: "#include",    detail: "#include <arquivo>",                  insert: "#include <${1:file}>", is_snippet: true },
    KwSnippet { label: "#tryinclude", detail: "#tryinclude <arquivo> (se existir)",  insert: "#tryinclude <${1:file}>", is_snippet: true },
    KwSnippet { label: "#if",         detail: "#if defined MACRO … #endif",          insert: "#if defined ${1:MACRO}\n$0\n#endif", is_snippet: true },
    KwSnippet { label: "#ifdef",      detail: "#ifdef MACRO … #endif",               insert: "#ifdef ${1:MACRO}\n$0\n#endif", is_snippet: true },
    KwSnippet { label: "#ifndef",     detail: "#ifndef MACRO … #endif",              insert: "#ifndef ${1:MACRO}\n$0\n#endif", is_snippet: true },
    KwSnippet { label: "#else",       detail: "#else (dentro de #if)",               insert: "#else", is_snippet: false },
    KwSnippet { label: "#endif",      detail: "#endif",                              insert: "#endif", is_snippet: false },
    KwSnippet { label: "#pragma",     detail: "#pragma opção",                       insert: "#pragma ${1:option}", is_snippet: true },
    KwSnippet { label: "#assert",     detail: "#assert condição (compile-time)",     insert: "#assert ${1:condition}", is_snippet: true },
    KwSnippet { label: "#error",      detail: "#error mensagem (compile-time)",      insert: "#error ${1:message}", is_snippet: true },
    KwSnippet { label: "#warning",    detail: "#warning mensagem (compile-time)",    insert: "#warning ${1:message}", is_snippet: true },
];

fn kw_to_item(kw: &KwSnippet) -> CompletionItem {
    CompletionItem {
        label: kw.label.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some(kw.detail.to_string()),
        insert_text: Some(kw.insert.to_string()),
        insert_text_format: Some(if kw.is_snippet {
            InsertTextFormat::SNIPPET
        } else {
            InsertTextFormat::PLAIN_TEXT
        }),
        sort_text: Some(format!("9_{}", kw.label)),
        ..Default::default()
    }
}

// ─── Context detection ───────────────────────────────────────────────────────

/// Returns true if `cursor_line` is inside a function body (brace depth > 0).
fn cursor_is_inside_function(text: &str, cursor_line: usize) -> bool {
    let mut depth = 0i32;
    let mut in_block = false;
    for (i, raw) in text.lines().enumerate() {
        if i >= cursor_line { break; }
        let stripped = crate::parser::lexer::strip_line_comments(raw, in_block);
        in_block = stripped.in_block;
        for ch in stripped.text.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth = (depth - 1).max(0),
                _ => {}
            }
        }
    }
    depth > 0
}

// ─── Local variable / parameter extraction ───────────────────────────────────

/// Returns (params, local_vars) visible at `cursor_line` in `text`.
/// Scans backwards from cursor to find the enclosing function, collects its
/// parameter names and all `new`/`static` declarations up to the cursor.
fn collect_locals(text: &str, cursor_line: usize) -> (Vec<String>, Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    if cursor_line >= lines.len() {
        return (vec![], vec![]);
    }

    // Walk backwards to find enclosing function opening brace
    let mut brace_depth = 0i32;
    let mut func_header_line: Option<usize> = None;
    let mut func_body_start: Option<usize> = None;

    for i in (0..=cursor_line).rev() {
        let ln = lines[i];
        for ch in ln.chars().rev() {
            match ch {
                '}' => brace_depth += 1,
                '{' => {
                    brace_depth -= 1;
                    if brace_depth < 0 {
                        func_body_start = Some(i);
                        // look for function header on same or previous lines
                        let header_search_start = if i > 0 { i - 1 } else { 0 };
                        for j in (0..=header_search_start).rev() {
                            let hln = lines[j].trim();
                            if hln.is_empty() || hln.starts_with("//") || hln.starts_with("/*") || hln.starts_with('*') {
                                continue;
                            }
                            // Check if this line looks like a function declaration
                            if hln.contains('(') {
                                func_header_line = Some(j);
                            }
                            break;
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
        if func_body_start.is_some() { break; }
    }

    let header_idx = func_header_line.unwrap_or(func_body_start.unwrap_or(0));
    let body_start = func_body_start.unwrap_or(0);

    // Extract parameter names from function header
    let mut params: Vec<String> = Vec::new();
    if let Some(header_ln) = lines.get(header_idx) {
        if let Some(paren_open) = header_ln.find('(') {
            let after = &header_ln[paren_open + 1..];
            let raw_params = if let Some(close) = after.find(')') {
                &after[..close]
            } else {
                after
            };
            for part in raw_params.split(',') {
                let name = extract_param_name(part.trim());
                if !name.is_empty() {
                    params.push(name);
                }
            }
        }
    }

    // Collect `new`/`static` local declarations from body start to cursor
    let mut local_vars: Vec<String> = Vec::new();
    for i in body_start..=cursor_line.min(lines.len().saturating_sub(1)) {
        let ln = lines[i];
        for cap in RX_NEW.captures_iter(ln) {
            let name = cap[1].to_string();
            if !name.is_empty() && !params.contains(&name) {
                local_vars.push(name);
            }
        }
    }

    (params, local_vars)
}

fn extract_param_name(part: &str) -> String {
    // "Float:x" → "x", "&playerid" → "playerid", "const name[]" → "name", "..." → ""
    let part = part.trim_start_matches("const").trim().trim_start_matches('&').trim();
    let part = if let Some(colon) = part.rfind(':') { &part[colon + 1..] } else { part };
    let name = part.trim_start().split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("");
    if name == "..." { String::new() } else { name.to_string() }
}

// ─── @ completions ────────────────────────────────────────────────────────────

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

// ─── Main completion entry point ──────────────────────────────────────────────

pub fn get_completions(state: &WorkspaceState, uri: &str, position: Position) -> Vec<CompletionItem> {
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

    // 1. Symbols from file + includes (functions, defines, globals, etc.)
    for sym in &all_syms {
        if !seen.insert(sym.name.clone()) {
            continue;
        }
        items.push(build_symbol_item(sym));
    }

    // 2 & 3. Contexto baseado na posição do cursor
    if let Some(text) = state.get_text(uri) {
        let cursor_line = position.line as usize;
        let in_function = cursor_is_inside_function(&text, cursor_line);

        if in_function {
            // Variáveis locais e parâmetros só fazem sentido dentro de função
            let (params, locals) = collect_locals(&text, cursor_line);
            for name in params.iter().chain(locals.iter()) {
                if !seen.insert(name.clone()) {
                    continue;
                }
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("local".to_string()),
                    insert_text: Some(name.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    sort_text: Some(format!("1_{}", name)),
                    ..Default::default()
                });
            }
        }

        let kw_list = if in_function { KW_IN_BODY } else { KW_TOP_LEVEL };
        for kw in kw_list {
            if !seen.contains(kw.label) {
                items.push(kw_to_item(kw));
            }
        }
    }

    items
}

// ─── Symbol item builder ──────────────────────────────────────────────────────

fn build_symbol_item(sym: &crate::parser::types::Symbol) -> CompletionItem {
    use crate::parser::types::SymbolKind::*;

    let kind = Some(match sym.kind {
        Native | Forward | Public | Stock | Static | Plain => CompletionItemKind::FUNCTION,
        StaticConst | Enum | Define | Const => CompletionItemKind::CONSTANT,
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
