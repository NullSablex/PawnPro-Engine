use std::collections::HashSet;

use regex::Regex;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemTag, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, Position,
};

use crate::messages::{Locale, MsgKey, msg};
use crate::workspace::WorkspaceState;

use super::collect_all_symbols;

static RX_NEW: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?:^|;|\{)\s*(?:new|static)\s+(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)").unwrap()
});

struct KwSnippet {
    label: &'static str,
    detail_key: MsgKey,
    insert: &'static str,
    is_snippet: bool,
}

static KW_IN_BODY: &[KwSnippet] = &[
    KwSnippet {
        label: "if",
        detail_key: MsgKey::KwIf,
        insert: "if (${1:condition})\n\t$0",
        is_snippet: true,
    },
    KwSnippet {
        label: "if/else",
        detail_key: MsgKey::KwIfElse,
        insert: "if (${1:condition})\n\t$2\nelse\n\t$0",
        is_snippet: true,
    },
    KwSnippet {
        label: "else",
        detail_key: MsgKey::KwElse,
        insert: "else\n\t$0",
        is_snippet: true,
    },
    KwSnippet {
        label: "for",
        detail_key: MsgKey::KwFor,
        insert: "for (new ${1:i} = 0; ${1:i} < ${2:count}; ++${1:i})\n\t$0",
        is_snippet: true,
    },
    KwSnippet {
        label: "while",
        detail_key: MsgKey::KwWhile,
        insert: "while (${1:condition})\n\t$0",
        is_snippet: true,
    },
    KwSnippet {
        label: "do",
        detail_key: MsgKey::KwDo,
        insert: "do\n{\n\t$0\n}\nwhile (${1:condition});",
        is_snippet: true,
    },
    KwSnippet {
        label: "switch",
        detail_key: MsgKey::KwSwitch,
        insert: "switch (${1:value})\n{\n\tcase ${2:0}: $0\n}",
        is_snippet: true,
    },
    KwSnippet {
        label: "case",
        detail_key: MsgKey::KwCase,
        insert: "case ${1:value}: $0",
        is_snippet: true,
    },
    KwSnippet {
        label: "default",
        detail_key: MsgKey::KwDefault,
        insert: "default: $0",
        is_snippet: true,
    },
    KwSnippet {
        label: "return",
        detail_key: MsgKey::KwReturn,
        insert: "return ${1:value};",
        is_snippet: true,
    },
    KwSnippet {
        label: "break",
        detail_key: MsgKey::KwBreak,
        insert: "break;",
        is_snippet: false,
    },
    KwSnippet {
        label: "continue",
        detail_key: MsgKey::KwContinue,
        insert: "continue;",
        is_snippet: false,
    },
    KwSnippet {
        label: "goto",
        detail_key: MsgKey::KwGoto,
        insert: "goto ${1:label};",
        is_snippet: true,
    },
    KwSnippet {
        label: "exit",
        detail_key: MsgKey::KwExit,
        insert: "exit;",
        is_snippet: false,
    },
    KwSnippet {
        label: "new",
        detail_key: MsgKey::KwNewLocal,
        insert: "new ${1:name};",
        is_snippet: true,
    },
    // sizeof e tagof usam parênteses mas NÃO são chamadas de função
    KwSnippet {
        label: "sizeof",
        detail_key: MsgKey::KwSizeof,
        insert: "sizeof(${1:var})",
        is_snippet: true,
    },
    KwSnippet {
        label: "tagof",
        detail_key: MsgKey::KwTagof,
        insert: "tagof(${1:var})",
        is_snippet: true,
    },
    KwSnippet {
        label: "true",
        detail_key: MsgKey::KwTrue,
        insert: "true",
        is_snippet: false,
    },
    KwSnippet {
        label: "false",
        detail_key: MsgKey::KwFalse,
        insert: "false",
        is_snippet: false,
    },
    KwSnippet {
        label: "cellmax",
        detail_key: MsgKey::KwCellmax,
        insert: "cellmax",
        is_snippet: false,
    },
    KwSnippet {
        label: "cellmin",
        detail_key: MsgKey::KwCellmin,
        insert: "cellmin",
        is_snippet: false,
    },
    KwSnippet {
        label: "cellbits",
        detail_key: MsgKey::KwCellbits,
        insert: "cellbits",
        is_snippet: false,
    },
];

static KW_TOP_LEVEL: &[KwSnippet] = &[
    KwSnippet {
        label: "stock",
        detail_key: MsgKey::KwStock,
        insert: "stock ${1:Name}(${2:params})\n{\n\t$0\n}",
        is_snippet: true,
    },
    KwSnippet {
        label: "public",
        detail_key: MsgKey::KwPublic,
        insert: "public ${1:Name}(${2:params})\n{\n\t$0\n}",
        is_snippet: true,
    },
    KwSnippet {
        label: "forward",
        detail_key: MsgKey::KwForward,
        insert: "forward ${1:Name}(${2:params});",
        is_snippet: true,
    },
    KwSnippet {
        label: "native",
        detail_key: MsgKey::KwNative,
        insert: "native ${1:Name}(${2:params});",
        is_snippet: true,
    },
    KwSnippet {
        label: "static",
        detail_key: MsgKey::KwStatic,
        insert: "static ${1:Name}(${2:params})\n{\n\t$0\n}",
        is_snippet: true,
    },
    KwSnippet {
        label: "enum",
        detail_key: MsgKey::KwEnum,
        insert: "enum ${1:Name}\n{\n\t${2:VALUE}\n}",
        is_snippet: true,
    },
    KwSnippet {
        label: "const",
        detail_key: MsgKey::KwConst,
        insert: "const ${1:NAME} = ${2:0};",
        is_snippet: true,
    },
    KwSnippet {
        label: "new",
        detail_key: MsgKey::KwNewGlobal,
        insert: "new ${1:name};",
        is_snippet: true,
    },
    KwSnippet {
        label: "#define",
        detail_key: MsgKey::KwDefine,
        insert: "#define ${1:NAME} ${2:value}",
        is_snippet: true,
    },
    KwSnippet {
        label: "#undef",
        detail_key: MsgKey::KwUndef,
        insert: "#undef ${1:NAME}",
        is_snippet: true,
    },
    KwSnippet {
        label: "#include",
        detail_key: MsgKey::KwInclude,
        insert: "#include <${1:file}>",
        is_snippet: true,
    },
    KwSnippet {
        label: "#tryinclude",
        detail_key: MsgKey::KwTryinclude,
        insert: "#tryinclude <${1:file}>",
        is_snippet: true,
    },
    KwSnippet {
        label: "#if",
        detail_key: MsgKey::KwIfDefined,
        insert: "#if defined ${1:MACRO}\n$0\n#endif",
        is_snippet: true,
    },
    KwSnippet {
        label: "#ifdef",
        detail_key: MsgKey::KwIfdef,
        insert: "#ifdef ${1:MACRO}\n$0\n#endif",
        is_snippet: true,
    },
    KwSnippet {
        label: "#ifndef",
        detail_key: MsgKey::KwIfndef,
        insert: "#ifndef ${1:MACRO}\n$0\n#endif",
        is_snippet: true,
    },
    KwSnippet {
        label: "#else",
        detail_key: MsgKey::KwElseDir,
        insert: "#else",
        is_snippet: false,
    },
    KwSnippet {
        label: "#endif",
        detail_key: MsgKey::KwEndif,
        insert: "#endif",
        is_snippet: false,
    },
    KwSnippet {
        label: "#pragma",
        detail_key: MsgKey::KwPragma,
        insert: "#pragma ${1:option}",
        is_snippet: true,
    },
    KwSnippet {
        label: "#assert",
        detail_key: MsgKey::KwAssert,
        insert: "#assert ${1:condition}",
        is_snippet: true,
    },
    KwSnippet {
        label: "#error",
        detail_key: MsgKey::KwError,
        insert: "#error ${1:message}",
        is_snippet: true,
    },
    KwSnippet {
        label: "#warning",
        detail_key: MsgKey::KwWarning,
        insert: "#warning ${1:message}",
        is_snippet: true,
    },
];

fn kw_to_item(kw: &KwSnippet, locale: Locale) -> CompletionItem {
    CompletionItem {
        label: kw.label.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some(msg(locale, kw.detail_key).to_string()),
        insert_text: Some(kw.insert.to_string()),
        insert_text_format: Some(if kw.is_snippet {
            InsertTextFormat::SNIPPET
        } else {
            InsertTextFormat::PLAIN_TEXT
        }),
        sort_text: Some(Rank::Keyword.sort_text(kw.label)),
        ..Default::default()
    }
}

fn cursor_is_inside_function(text: &str, cursor_line: usize) -> bool {
    let mut depth = 0i32;
    let mut in_block = false;
    for (i, raw) in text.lines().enumerate() {
        if i >= cursor_line {
            break;
        }
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

fn collect_locals(text: &str, cursor_line: usize) -> (Vec<String>, Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    if cursor_line >= lines.len() {
        return (vec![], vec![]);
    }

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
                        let header_search_start = if i > 0 { i - 1 } else { 0 };
                        for j in (0..=header_search_start).rev() {
                            let hln = lines[j].trim();
                            if hln.is_empty()
                                || hln.starts_with("//")
                                || hln.starts_with("/*")
                                || hln.starts_with('*')
                            {
                                continue;
                            }
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
        if func_body_start.is_some() {
            break;
        }
    }

    let header_idx = func_header_line.unwrap_or(func_body_start.unwrap_or(0));
    let body_start = func_body_start.unwrap_or(0);

    let mut params: Vec<String> = Vec::new();
    if let Some(header_ln) = lines.get(header_idx)
        && let Some(paren_open) = header_ln.find('(')
    {
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

    let mut local_vars: Vec<String> = Vec::new();
    for ln in lines
        .iter()
        .take(cursor_line.min(lines.len().saturating_sub(1)) + 1)
        .skip(body_start)
    {
        let ln = *ln;
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
    let part = part
        .trim_start_matches("const")
        .trim()
        .trim_start_matches('&')
        .trim();
    let part = if let Some(colon) = part.rfind(':') {
        &part[colon + 1..]
    } else {
        part
    };
    let name = part
        .trim_start()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or("");
    if name == "..." {
        String::new()
    } else {
        name.to_string()
    }
}

/// Completions do trigger `@`: as tags de documentação Javadoc, úteis apenas
/// dentro de um comentário. Fora dele, `@` não inicia nada em Pawn.
pub fn get_at_completions(in_comment: bool, locale: Locale) -> Vec<CompletionItem> {
    if !in_comment {
        return Vec::new();
    }
    [
        (
            "param",
            MsgKey::DocTagParam,
            "param ${1:nome}  ${2:descrição}",
        ),
        ("return", MsgKey::DocTagReturn, "return ${1:descrição}"),
        ("remarks", MsgKey::DocTagRemarks, "remarks ${1:nota}"),
    ]
    .into_iter()
    .map(|(label, detail_key, insert)| CompletionItem {
        label: format!("@{label}"),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some(msg(locale, detail_key).to_string()),
        // O `@` já foi digitado e disparou o trigger.
        insert_text: Some(insert.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        sort_text: Some(format!("0_{label}")),
        ..Default::default()
    })
    .collect()
}

// `locale` (i18n) e `locals` (variáveis locais) são termos corretos do domínio;
// a semelhança ortográfica é incidental.
#[allow(clippy::similar_names)]
pub fn get_completions(
    state: &WorkspaceState,
    uri: &str,
    position: Position,
) -> Vec<CompletionItem> {
    let locale = state.locale;
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

    // Os símbolos do próprio arquivo vêm primeiro em `all_syms`; conhecê-los
    // por nome permite ranquear o resto como vindo de include.
    let own: HashSet<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    for sym in &all_syms {
        if !seen.insert(sym.name.clone()) {
            continue;
        }
        let rank = if sym.deprecated {
            Rank::Deprecated
        } else if own.contains(sym.name.as_str()) {
            Rank::File
        } else {
            Rank::Included
        };
        items.push(build_symbol_item(sym, rank));
    }

    if let Some(text) = state.get_text(uri) {
        let cursor_line = position.line as usize;
        let in_function = cursor_is_inside_function(&text, cursor_line);

        if in_function {
            let (params, locals) = collect_locals(&text, cursor_line);
            for name in params.iter().chain(locals.iter()) {
                if !seen.insert(name.clone()) {
                    continue;
                }
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some(msg(locale, MsgKey::KwLocal).to_string()),
                    insert_text: Some(name.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    sort_text: Some(Rank::Local.sort_text(name)),
                    ..Default::default()
                });
            }
        }

        let kw_list = if in_function {
            KW_IN_BODY
        } else {
            KW_TOP_LEVEL
        };
        for kw in kw_list {
            if !seen.contains(kw.label) {
                items.push(kw_to_item(kw, locale));
            }
        }
    }

    // Ordena aqui (e não só via `sort_text`) para que o corte abaixo mantenha os
    // itens mais próximos do cursor, e não os primeiros a serem coletados.
    items.sort_by(|a, b| a.sort_text.cmp(&b.sort_text));
    items
}

/// Teto de itens devolvidos por chamada.
///
/// Um projeto com muitos includes chega a milhares de símbolos, e mandá-los a
/// cada tecla trava a digitação. Com o corte, a resposta é marcada como
/// incompleta e o editor pede de novo conforme o prefixo cresce — que é o
/// mecanismo do LSP para listas grandes.
pub const MAX_COMPLETION_ITEMS: usize = 1000;

/// Posição de um item na lista, do mais próximo do cursor ao mais distante.
///
/// O `sort_text` que o editor usa para ordenar começa por este dígito, então a
/// ordem entre grupos é decidida aqui e não pelo alfabeto. Um símbolo
/// descontinuado cai para `Deprecated`, seja qual for sua origem: aparece na
/// lista (às vezes é mesmo o que se quer), mas nunca à frente de uma
/// alternativa viva.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// Parâmetros e variáveis locais da função sob o cursor.
    Local,
    /// Símbolos declarados no próprio arquivo.
    File,
    /// Símbolos vindos dos includes.
    Included,
    /// Palavras-chave da linguagem.
    Keyword,
    /// Qualquer símbolo marcado com `#pragma deprecated`.
    Deprecated,
}

impl Rank {
    fn prefix(self) -> char {
        match self {
            Rank::Local => '0',
            Rank::File => '1',
            Rank::Included => '2',
            Rank::Keyword => '3',
            Rank::Deprecated => '9',
        }
    }

    /// `sort_text` do item: o grupo decide primeiro, o nome desempata.
    fn sort_text(self, label: &str) -> String {
        format!("{}_{}", self.prefix(), label.to_ascii_lowercase())
    }
}

fn build_symbol_item(sym: &crate::parser::types::Symbol, rank: Rank) -> CompletionItem {
    use crate::parser::types::SymbolKind::{
        Const, Define, Enum, Forward, Native, Plain, Public, Static, StaticConst, Stock, Variable,
    };

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
        // Na lista de autocomplete cabe o resumo, não o bloco inteiro; o hover
        // mostra a doc completa.
        documentation: sym
            .doc
            .as_deref()
            .map(super::parse_doc)
            .and_then(|d| d.short())
            .map(|v| {
                Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: v,
                })
            }),
        insert_text,
        insert_text_format,
        sort_text: Some(rank.sort_text(&sym.name)),
        ..Default::default()
    };

    if sym.deprecated {
        item.tags = Some(vec![CompletionItemTag::DEPRECATED]);
    }

    item
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::types::{Symbol, SymbolKind};

    fn sym(name: &str, deprecated: bool) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Native,
            signature: Some(format!("{name}()")),
            params: vec![],
            deprecated,
            deprecated_message: None,
            doc: None,
            line: 0,
            col: 0,
        }
    }

    #[test]
    fn rank_orders_from_the_cursor_outwards() {
        let mut ranks = [
            Rank::Deprecated,
            Rank::Keyword,
            Rank::Included,
            Rank::File,
            Rank::Local,
        ];
        ranks.sort_unstable();
        assert_eq!(
            ranks,
            [
                Rank::Local,
                Rank::File,
                Rank::Included,
                Rank::Keyword,
                Rank::Deprecated
            ]
        );
    }

    #[test]
    fn sort_text_puts_locals_before_includes() {
        // O que importa é a ordem entre grupos, não o alfabeto: uma local
        // chamada "zz" ainda vence uma nativa chamada "aa".
        let local = Rank::Local.sort_text("zz");
        let included = Rank::Included.sort_text("aa");
        assert!(local < included, "{local} deveria vir antes de {included}");
    }

    #[test]
    fn deprecated_sinks_below_everything_else() {
        let dep = Rank::Deprecated.sort_text("aaa");
        for rank in [Rank::Local, Rank::File, Rank::Included, Rank::Keyword] {
            let other = rank.sort_text("zzz");
            assert!(other < dep, "{other} deveria vir antes de {dep}");
        }
    }

    #[test]
    fn sort_text_is_case_insensitive_within_a_group() {
        // Sem normalizar a caixa, "Zebra" viria antes de "alfa" (ASCII).
        let upper = Rank::Included.sort_text("Zebra");
        let lower = Rank::Included.sort_text("alfa");
        assert!(lower < upper);
    }

    #[test]
    fn deprecated_symbol_is_ranked_and_tagged() {
        let item = build_symbol_item(&sym("BanEx", true), Rank::Deprecated);
        assert!(item.sort_text.unwrap().starts_with('9'));
        assert_eq!(item.tags, Some(vec![CompletionItemTag::DEPRECATED]));
    }

    #[test]
    fn live_symbol_keeps_its_group_and_has_no_tag() {
        let item = build_symbol_item(&sym("BanPlayerFor", false), Rank::File);
        assert!(item.sort_text.unwrap().starts_with('1'));
        assert_eq!(item.tags, None);
    }
}
