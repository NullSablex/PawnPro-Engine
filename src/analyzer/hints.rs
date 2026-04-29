use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::messages::{msg, Locale, MsgKey};
use crate::parser::lexer::strip_line_comments;
use crate::parser::types::{Symbol, SymbolKind};

use super::{codes, diagnostic::PawnDiagnostic};

static RX_WORD: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\b").unwrap());

pub fn analyze_hints(text: &str, symbols: &[Symbol], locale: Locale) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();

    let funcs: Vec<&Symbol> = symbols
        .iter()
        .filter(|s| {
            matches!(s.kind, SymbolKind::Public | SymbolKind::Stock | SymbolKind::Static | SymbolKind::Plain)
                && !s.params.is_empty()
        })
        .collect();

    if funcs.is_empty() {
        return diags;
    }

    let raw_lines: Vec<&str> = text.split('\n').collect();

    for sym in funcs {
        let body_lines = extract_body_lines(&raw_lines, sym.line as usize);
        if body_lines.is_empty() {
            continue;
        }

        let used = collect_idents(&body_lines);

        for param in &sym.params {
            if param.is_variadic || param.name.starts_with('_') || param.name == "..." {
                continue;
            }
            if !used.contains(&param.name) {
                let col = find_param_col(&raw_lines, sym.line as usize, &param.name);
                diags.push(PawnDiagnostic::hint(
                    sym.line,
                    col,
                    col + param.name.len() as u32,
                    codes::PP0009,
                    msg(locale, MsgKey::ParamUnused).replace("{}", &param.name),
                ));
            }
        }
    }

    diags
}

fn extract_body_lines<'t>(raw_lines: &[&'t str], decl_line: usize) -> Vec<&'t str> {
    let mut result = Vec::new();
    let mut depth: i32 = 0;
    let mut found_open = false;
    let mut in_block = false;

    for raw in raw_lines.iter().skip(decl_line) {
        let stripped = strip_line_comments(raw.trim_end_matches('\r'), in_block);
        in_block = stripped.in_block;

        for ch in stripped.text.chars() {
            match ch {
                '{' => { depth += 1; found_open = true; }
                '}' => { depth = (depth - 1).max(0); }
                _ => {}
            }
        }

        result.push(*raw);

        if found_open && depth == 0 {
            break;
        }
    }

    if !found_open { result.clear(); }
    result
}

fn collect_idents(body_lines: &[&str]) -> HashSet<String> {
    let mut idents = HashSet::new();
    let mut in_block = false;

    for (i, raw) in body_lines.iter().enumerate() {
        let stripped = strip_line_comments(raw.trim_end_matches('\r'), in_block);
        in_block = stripped.in_block;

        if i == 0 { continue; }

        for cap in RX_WORD.captures_iter(&stripped.text) {
            idents.insert(cap[1].to_string());
        }
    }

    idents
}

fn find_param_col(raw_lines: &[&str], decl_line: usize, param_name: &str) -> u32 {
    for raw in raw_lines.iter().skip(decl_line).take(8) {
        if let Some(col) = word_col_in_line(raw, param_name) {
            return col;
        }
        if raw.contains('{') {
            break;
        }
    }
    0
}

fn word_col_in_line(line: &str, word: &str) -> Option<u32> {
    let bytes = line.as_bytes();
    let wbytes = word.as_bytes();
    let wlen = wbytes.len();
    if wlen == 0 || wlen > bytes.len() {
        return None;
    }
    for i in 0..=(bytes.len() - wlen) {
        if &bytes[i..i + wlen] != wbytes {
            continue;
        }
        let is_ident_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
        let after_ok = i + wlen == bytes.len() || !is_ident_byte(bytes[i + wlen]);
        if before_ok && after_ok {
            return Some(i as u32);
        }
    }
    None
}
