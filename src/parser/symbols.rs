use once_cell::sync::Lazy;
use regex::Regex;

use super::{
    lexer::{build_line_offsets, strip_line_comments},
    types::{IncludeDirective, Param, ParsedFile, Symbol, SymbolKind},
};

// ─── Regex compilados (compilados uma única vez) ───────────────────────────

static RX_DEPRECATED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*(?://\s*@deprecated|/\*\s*@deprecated\s*\*/)\s*$").unwrap());

static RX_NATIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:forward\s+)?native\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

static RX_FORWARD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*forward\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

static RX_PUBLIC_STOCK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(public|stock)\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// static [Tag:]Name(params) — funções com corpo
static RX_STATIC_FUNC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*static\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// static const Name ou static const Name[...] (array/constante)
static RX_STATIC_CONST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*static\s+const\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\b").unwrap()
});

// float/bool Name(params) — funções com tipo de retorno sem ":"
static RX_FLOAT_BOOL_FUNC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(float|bool)\s+([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// #define Name
static RX_DEFINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)\b").unwrap());

// #include <token> ou #include "token"
static RX_INCLUDE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*#\s*include\s*(?:<([^>]+)>|"([^"]+)")"#).unwrap());

// new/static(var)/const com tag opcional: new [Tag:]name
static RX_VAR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(new|const)\s+(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)").unwrap()
});
// static como variável (fora de corpo): static [Tag:]name = ...;
static RX_STATIC_VAR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*static\s+(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)\s*(?:[=\[;])").unwrap()
});

// Palavras reservadas que nunca são nomes de variáveis
static RESERVED: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
    [
        "true", "false", "null", "sizeof", "tagof", "Float", "bool", "char",
        "String", "new", "static", "const", "native", "forward", "public",
        "stock", "return", "if", "else", "for", "while", "do", "switch",
        "case", "break", "continue", "default",
    ]
    .into_iter()
    .collect()
});

// ─── Helpers ──────────────────────────────────────────────────────────────

fn update_brace_depth(line: &str, mut depth: i32) -> i32 {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut in_char = false;
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        let prev = if i > 0 { bytes[i - 1] } else { 0 };
        if ch == b'"' && !in_char && prev != b'\\' {
            in_str = !in_str;
        } else if ch == b'\'' && !in_str && prev != b'\\' {
            in_char = !in_char;
        } else if !in_str && !in_char {
            if ch == b'{' {
                depth += 1;
            } else if ch == b'}' {
                depth = (depth - 1).max(0);
            }
        }
        i += 1;
    }
    depth
}

/// Divide parâmetros respeitando parênteses/colchetes aninhados.
pub fn split_params(raw: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut in_char = false;
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        let prev = if i > 0 { bytes[i - 1] } else { 0 };
        if ch == b'"' && !in_char && prev != b'\\' {
            in_str = !in_str;
            cur.push(ch as char);
        } else if ch == b'\'' && !in_str && prev != b'\\' {
            in_char = !in_char;
            cur.push(ch as char);
        } else if in_str || in_char {
            cur.push(ch as char);
        } else if ch == b'(' || ch == b'[' || ch == b'{' {
            depth += 1;
            cur.push(ch as char);
        } else if ch == b')' || ch == b']' || ch == b'}' {
            depth = (depth - 1).max(0);
            cur.push(ch as char);
        } else if ch == b',' && depth == 0 {
            let t = cur.trim().to_string();
            if !t.is_empty() {
                params.push(t);
            }
            cur.clear();
        } else {
            cur.push(ch as char);
        }
        i += 1;
    }
    let t = cur.trim().to_string();
    if !t.is_empty() {
        params.push(t);
    }
    params
}

/// Analisa os parâmetros brutos e retorna Vec<Param>.
fn parse_params(raw: &str) -> Vec<Param> {
    let parts = split_params(raw);
    let mut params = Vec::new();

    for p in &parts {
        let t = p.trim();
        if t.is_empty() {
            continue;
        }
        if t == "..." || t.ends_with("...") {
            params.push(Param {
                name: "...".to_string(),
                tag: None,
                is_variadic: true,
            });
            continue;
        }
        // Extrai nome e tag: "const Float:x[]" → name="x", tag="Float"
        // Remove qualificadores: const, &
        let stripped = t
            .trim_start_matches("const")
            .trim_start_matches('&')
            .trim();
        // Tenta capturar "Tag:name" ou apenas "name"
        let (tag, name) = if let Some(colon) = stripped.find(':') {
            let tag_part = stripped[..colon].trim().to_string();
            let name_part = stripped[colon + 1..]
                .trim()
                .trim_end_matches(']')
                .trim_end_matches('[')
                .trim()
                .to_string();
            (Some(tag_part), name_part)
        } else {
            // Nome pode ter sufixo []
            let name_raw = stripped
                .split_whitespace()
                .last()
                .unwrap_or(stripped)
                .trim_end_matches(']')
                .trim_end_matches('[')
                .trim()
                .to_string();
            (None, name_raw)
        };
        // Mantém apenas a parte identificador do nome (sem sufixos)
        let name = name
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();
        if !name.is_empty() {
            params.push(Param { name, tag, is_variadic: false });
        }
    }

    params
}

/// Extrai o comentário de documentação acima de uma linha (busca para trás).
fn extract_doc(lines: &[&str], line_idx: usize) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut found = false;
    let mut i = line_idx as isize - 1;
    while i >= 0 {
        let l = lines[i as usize].trim();
        if l.is_empty() {
            if found {
                break;
            }
            i -= 1;
            continue;
        }
        if l.starts_with("//") {
            doc_lines.push(l.to_string());
            found = true;
        } else if l.ends_with("*/") {
            doc_lines.push(l.to_string());
            // busca o início do bloco
            let mut j = i - 1;
            while j >= 0 {
                let ll = lines[j as usize].trim();
                doc_lines.push(ll.to_string());
                if ll.contains("/*") {
                    break;
                }
                j -= 1;
            }
            break;
        } else {
            break;
        }
        i -= 1;
    }
    if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(doc_lines.join("\n"))
    }
}

// ─── Parser principal ──────────────────────────────────────────────────────

/// Faz o parse de um arquivo Pawn e retorna os símbolos e includes encontrados.
pub fn parse_file(text: &str) -> ParsedFile {
    let mut result = ParsedFile::default();
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let _line_offsets = build_line_offsets(text);

    let mut in_block = false;
    let mut depth: i32 = 0;
    let mut pending_deprecated = false;

    for (line_idx, raw_line) in raw_lines.iter().enumerate() {
        // Remove \r do final (Windows line endings)
        let raw_line = raw_line.trim_end_matches('\r');

        // Verifica @DEPRECATED ANTES do strip (o comentário está no rawLine)
        if RX_DEPRECATED.is_match(raw_line) {
            pending_deprecated = true;
            let stripped = strip_line_comments(raw_line, in_block);
            in_block = stripped.in_block;
            depth = update_brace_depth(&stripped.text, depth);
            continue;
        }

        let stripped = strip_line_comments(raw_line, in_block);
        in_block = stripped.in_block;
        let line = &stripped.text;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            // linha vazia: mantém pending_deprecated
            depth = update_brace_depth(line, depth);
            continue;
        }

        let top_level = depth == 0;

        if top_level {
            let deprecated = pending_deprecated;
            pending_deprecated = false;

            // #include
            if let Some(cap) = RX_INCLUDE.captures(line) {
                let (token, is_angle) = if let Some(m) = cap.get(1) {
                    (m.as_str().trim().to_string(), true)
                } else {
                    (cap.get(2).unwrap().as_str().trim().to_string(), false)
                };
                // Posição do token no rawLine
                let col = raw_line.find(&token).unwrap_or(0) as u32;
                result.includes.push(IncludeDirective {
                    token,
                    is_angle,
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // #define
            if let Some(cap) = RX_DEFINE.captures(line) {
                let name = cap[1].to_string();
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.macro_names.push(name.clone());
                if deprecated {
                    result.deprecated_macros.push(name.clone());
                }
                result.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Define,
                    signature: None,
                    params: vec![],
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // native
            if let Some(cap) = RX_NATIVE.captures(line) {
                let name = cap[1].to_string();
                let params_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                let params = parse_params(params_raw);
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Native,
                    signature: Some(format!("{}({})", name, params_raw.trim())),
                    params,
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // forward
            if let Some(cap) = RX_FORWARD.captures(line) {
                let name = cap[1].to_string();
                let params_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                let params = parse_params(params_raw);
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Forward,
                    signature: Some(format!("{}({})", name, params_raw.trim())),
                    params,
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // public / stock
            if let Some(cap) = RX_PUBLIC_STOCK.captures(line) {
                let kw = &cap[1];
                let name = cap[2].to_string();
                let params_raw = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                let params = parse_params(params_raw);
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                let kind = if kw == "public" { SymbolKind::Public } else { SymbolKind::Stock };
                result.symbols.push(Symbol {
                    name: name.clone(),
                    kind,
                    signature: Some(format!("{}({})", name, params_raw.trim())),
                    params,
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // static const (deve vir antes de static func)
            if let Some(cap) = RX_STATIC_CONST.captures(line) {
                let name = cap[1].to_string();
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::StaticConst,
                    signature: None,
                    params: vec![],
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // static function
            if let Some(cap) = RX_STATIC_FUNC.captures(line) {
                let name = cap[1].to_string();
                let params_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                let params = parse_params(params_raw);
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Static,
                    signature: Some(format!("{}({})", name, params_raw.trim())),
                    params,
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // float/bool Name(params)
            if let Some(cap) = RX_FLOAT_BOOL_FUNC.captures(line) {
                let name = cap[2].to_string();
                let params_raw = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                let params = parse_params(params_raw);
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Stock, // tratado como stock (tem corpo)
                    signature: Some(format!("{}:{} ({})", &cap[1], name, params_raw.trim())),
                    params,
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // Variáveis: new/const [Tag:]name  (tag corretamente ignorada)
            for cap in RX_VAR.captures_iter(line) {
                let name = cap[2].to_string();
                if RESERVED.contains(name.as_str()) {
                    continue;
                }
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Variable,
                    signature: None,
                    params: vec![],
                    deprecated,
                    doc: None,
                    line: line_idx as u32,
                    col,
                });
            }

            // static como variável: static [Tag:]name = ...
            if let Some(cap) = RX_STATIC_VAR.captures(line) {
                let name = cap[1].to_string();
                if !RESERVED.contains(name.as_str()) {
                    let col = raw_line.find(&name).unwrap_or(0) as u32;
                    result.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Variable,
                        signature: None,
                        params: vec![],
                        deprecated,
                        doc: None,
                        line: line_idx as u32,
                        col,
                    });
                }
            }
        } else {
            // dentro de bloco: qualquer linha não-vazia reseta pending_deprecated
            pending_deprecated = false;
        }

        depth = update_brace_depth(line, depth);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_native() {
        let src = "native fexist(const pattern[]);";
        let f = parse_file(src);
        assert_eq!(f.symbols.len(), 1);
        assert_eq!(f.symbols[0].name, "fexist");
        assert!(matches!(f.symbols[0].kind, SymbolKind::Native));
        // Parâmetro "pattern" detectado mas NÃO como variável separada
        assert!(!f.symbols.iter().any(|s| s.name == "pattern" && matches!(s.kind, SymbolKind::Variable)));
    }

    #[test]
    fn parses_tagged_variable() {
        let src = "new File:f = fopen(\"test\", io_read);";
        let f = parse_file(src);
        // Deve capturar "f", não "File"
        assert!(f.symbols.iter().any(|s| s.name == "f"));
        assert!(!f.symbols.iter().any(|s| s.name == "File"));
    }

    #[test]
    fn parses_static_func() {
        let src = "static MyHelper(a, b) {}";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "MyHelper" && matches!(s.kind, SymbolKind::Static)));
    }

    #[test]
    fn parses_static_const() {
        let src = "static const MAX_ZONES = 10;";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "MAX_ZONES" && matches!(s.kind, SymbolKind::StaticConst)));
    }

    #[test]
    fn parses_deprecated() {
        let src = "// @DEPRECATED\nstock OldFunc() {}";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "OldFunc" && s.deprecated));
    }

    #[test]
    fn parses_include_angle() {
        let src = "#include <a_samp>";
        let f = parse_file(src);
        assert_eq!(f.includes.len(), 1);
        assert_eq!(f.includes[0].token, "a_samp");
        assert!(f.includes[0].is_angle);
    }

    #[test]
    fn parses_include_relative() {
        let src = r#"#include "../utils/helpers.inc""#;
        let f = parse_file(src);
        assert_eq!(f.includes[0].token, "../utils/helpers.inc");
        assert!(!f.includes[0].is_angle);
    }

    #[test]
    fn parses_float_bool_func() {
        let src = "float GetDistance(Float:x, Float:y) {}";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "GetDistance"));
    }
}
