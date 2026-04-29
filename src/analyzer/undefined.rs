use std::collections::HashSet;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::messages::{msg, Locale, MsgKey};
use crate::parser::lexer::strip_line_comments;
use crate::parser::{ParsedFile, SymbolKind};

use super::includes::ResolvedIncludes;
use super::{codes, diagnostic::PawnDiagnostic};

static RX_CALL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:([A-Za-z_]\w*)::)?([A-Za-z_]\w*)\s*\(").unwrap()
});

// Limite de caracteres do compilador real (sNAMEMAX = 31)
const SNAME_MAX: usize = 31;

fn truncate_name(name: &str) -> &str {
    if name.len() <= SNAME_MAX { name } else { &name[..SNAME_MAX] }
}

static RESERVED: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "if", "else", "for", "while", "do", "switch", "case", "return",
        "sizeof", "tagof", "state", "goto", "assert", "break", "continue",
        "exit", "sleep", "new", "static", "const", "public", "stock",
        "native", "forward",
    ]
    .into_iter()
    .collect()
});


fn mask_string_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' || ch == '\'' {
            out.push(ch);
            let quote = ch;
            loop {
                match chars.next() {
                    None => break,
                    Some('\\') => {
                        out.push(' ');
                        if chars.next().is_some() { out.push(' '); }
                    }
                    Some(c) if c == quote => { out.push(c); break; }
                    Some(_) => out.push(' '),
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn analyze_undefined(
    text: &str,
    file_path: &Path,
    parsed: &ParsedFile,
    resolved: &ResolvedIncludes,
    sdk_parsed: Option<&ParsedFile>,
    locale: Locale,
) -> Vec<PawnDiagnostic> {
    // PP0010 só faz sentido em compilation units (.pwn).
    // .inc, .p, .pawn são include files — nunca compilados diretamente.
    let is_include = matches!(
        file_path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("inc") | Some("p") | Some("pawn")
    );
    if is_include {
        return vec![];
    }

    let mut known: HashSet<String> = HashSet::new();
    let mut func_prefixes: HashSet<String> = HashSet::new();

    let sources: Vec<&ParsedFile> = {
        let mut v: Vec<&ParsedFile> = Vec::new();
        if let Some(sdk) = sdk_parsed { v.push(sdk); }
        v.push(parsed);
        v
    };

    for p in &sources {
        for sym in &p.symbols {
            if !matches!(sym.kind, SymbolKind::Variable) {
                known.insert(truncate_name(&sym.name).to_string());
            }
        }
        for name in &p.macro_names {
            known.insert(truncate_name(name).to_string());
        }
        for prefix in &p.func_macro_prefixes {
            func_prefixes.insert(truncate_name(prefix).to_string());
        }
    }
    for fp in &resolved.paths {
        if let Some(entry) = resolved.files.get(fp) {
            for sym in &entry.parsed.symbols {
                if !matches!(sym.kind, SymbolKind::Variable) {
                    known.insert(truncate_name(&sym.name).to_string());
                }
            }
            for name in &entry.parsed.macro_names {
                known.insert(truncate_name(name).to_string());
            }
            for prefix in &entry.parsed.func_macro_prefixes {
                func_prefixes.insert(truncate_name(prefix).to_string());
            }
        }
    }

    let mut diags = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut in_block = false;

    for (line_idx, raw_line) in lines.iter().enumerate() {
        let raw_line = raw_line.trim_end_matches('\r');
        let stripped = strip_line_comments(raw_line, in_block);
        in_block = stripped.in_block;
        let line = mask_string_literals(&stripped.text);
        let line = &line;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        for cap in RX_CALL.captures_iter(line) {
            let namespace = cap.get(1).map(|m| m.as_str());
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            if name.is_empty() { continue; }

            // Aplica truncagem igual ao compilador real (sNAMEMAX=31)
            let name_trunc = truncate_name(name);

            if RESERVED.contains(name_trunc) || known.contains(name_trunc) {
                continue;
            }

            if let Some(ns) = namespace {
                let ns_trunc = truncate_name(ns);
                let expanded = format!("{}_{}", ns_trunc, name_trunc);
                if known.contains(expanded.as_str()) || known.contains(ns_trunc) || func_prefixes.contains(ns_trunc) {
                    continue;
                }
                continue; // unknown namespace — macro may use other patterns
            }

            let col = raw_line.find(name).unwrap_or(0) as u32;
            diags.push(PawnDiagnostic::warning(
                line_idx as u32, col, col + name.len() as u32,
                codes::PP0010,
                msg(locale, MsgKey::SymbolUndeclared).replace("{}", name),
            ));
        }
    }

    diags
}
