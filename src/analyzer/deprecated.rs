use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::parser::lexer::strip_line_comments;
use crate::parser::{ParsedFile, SymbolKind};

use super::includes::collect_included_files;
use super::{codes, diagnostic::PawnDiagnostic};

static RX_DEPRECATED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*(?://\s*@deprecated|/\*\s*@deprecated\s*\*/)\s*$").unwrap());
static RX_INCLUDE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*#\s*include\s*(?:<([^>]+)>|"([^"]+)")"#).unwrap());
static RX_CALL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\s*\(").unwrap());
static RX_IDENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\b").unwrap());
static RX_DECL_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(?:stock|native|public|forward|static)\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*$").unwrap());

/// Analisa o arquivo atual para usos de símbolos depreciados provenientes dos includes.
pub fn analyze_deprecated(
    text: &str,
    file_path: &Path,
    parsed: &ParsedFile,
    include_paths: &[PathBuf],
) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();

    // 1. Coleta nomes depreciados de todos os includes
    let mut dep_funcs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut dep_macros: std::collections::HashSet<String> = std::collections::HashSet::new();

    let included = collect_included_files(file_path, include_paths, &parsed.includes, 5, 100);
    for fp in &included {
        if let Ok(inc_bytes) = std::fs::read(fp) {
            let inc_text = decode_text(&inc_bytes);
            let inc_parsed = crate::parser::parse_file(&inc_text);
            for sym in &inc_parsed.symbols {
                if sym.deprecated {
                    match sym.kind {
                        SymbolKind::Define => { dep_macros.insert(sym.name.clone()); }
                        _ => { dep_funcs.insert(sym.name.clone()); }
                    }
                }
            }
            for m in &inc_parsed.deprecated_macros {
                dep_macros.insert(m.clone());
            }
        }
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let mut line_offsets = vec![0u32];
    {
        let mut off = 0u32;
        for ch in text.chars() {
            off += ch.len_utf8() as u32;
            if ch == '\n' { line_offsets.push(off); }
        }
    }

    // 2. Escaneia o arquivo atual para usos de funções/macros depreciadas
    if !dep_funcs.is_empty() || !dep_macros.is_empty() {
        let mut in_block = false;
        for (line_idx, raw_line) in lines.iter().enumerate() {
            let raw_line = raw_line.trim_end_matches('\r');
            let stripped = strip_line_comments(raw_line, in_block);
            in_block = stripped.in_block;
            let line = &stripped.text;
            if line.trim().is_empty() { continue; }

            // Funções depreciadas (chamadas Name(...))
            for cap in RX_CALL.captures_iter(line) {
                let name = &cap[1];
                if !dep_funcs.contains(name) { continue; }
                // Ignora declarações (stock/native/public/forward antes do nome)
                let before = &line[..cap.get(0).unwrap().start()];
                if RX_DECL_PREFIX.is_match(before) { continue; }
                let col = raw_line.find(name).unwrap_or(0) as u32;
                diags.push(PawnDiagnostic::warning(
                    line_idx as u32, col, col + name.len() as u32,
                    codes::PP0007,
                    format!("\"{}\" está depreciado", name),
                ));
            }

            // Macros depreciadas (qualquer ocorrência de identificador)
            for cap in RX_IDENT.captures_iter(line) {
                let name = &cap[1];
                if !dep_macros.contains(name) { continue; }
                // Ignora a própria linha de #define
                if line.trim_start().starts_with("#") { continue; }
                let col = cap.get(1).unwrap().start() as u32;
                diags.push(PawnDiagnostic::warning(
                    line_idx as u32, col, col + name.len() as u32,
                    codes::PP0007,
                    format!("\"{}\" está depreciado", name),
                ));
            }
        }
    }

    // 3. Verifica #include depreciados no arquivo atual (// @DEPRECATED antes do #include)
    {
        let mut pending_deprecated = false;
        let mut in_block = false;
        for (line_idx, raw_line) in lines.iter().enumerate() {
            let raw_line = raw_line.trim_end_matches('\r');

            if RX_DEPRECATED.is_match(raw_line) {
                pending_deprecated = true;
                let s = strip_line_comments(raw_line, in_block);
                in_block = s.in_block;
                continue;
            }

            let stripped = strip_line_comments(raw_line, in_block);
            in_block = stripped.in_block;
            let line = &stripped.text;
            if line.trim().is_empty() { continue; }

            if pending_deprecated && let Some(cap) = RX_INCLUDE.captures(line) {
                let token = cap.get(1).or(cap.get(2)).map(|m| m.as_str().trim()).unwrap_or("");
                let col = raw_line.find(token).unwrap_or(0) as u32;
                diags.push(PawnDiagnostic::warning(
                    line_idx as u32, col, col + token.len() as u32,
                    codes::PP0008,
                    format!("\"{}\" está depreciado", token),
                ));
            }
            pending_deprecated = false;
        }
    }

    diags
}

fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}
