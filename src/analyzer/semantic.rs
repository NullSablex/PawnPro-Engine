use once_cell::sync::Lazy;
use regex::Regex;

use crate::parser::lexer::{build_line_offsets, strip_line_comments};

use super::{codes, diagnostic::PawnDiagnostic};

static RX_NATIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:forward\s+)?native\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\([^)]*\)\s*([;{])?").unwrap()
});
static RX_FORWARD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*forward\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\([^)]*\)\s*([;{])?").unwrap()
});
static RX_PUBLIC_STOCK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(public|stock|static)\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\([^)]*\)\s*([;{])?").unwrap()
});

fn update_brace_depth(line: &str, depth: i32) -> i32 {
    let mut d = depth;
    for ch in line.chars() {
        match ch {
            '{' => d += 1,
            '}' => d = (d - 1).max(0),
            _ => {}
        }
    }
    d
}

/// Analisa a estrutura semântica de declarações:
/// - native/forward com corpo → erro
/// - public/stock/static sem corpo → warning
pub fn analyze_semantics(text: &str) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let line_offsets = build_line_offsets(text);
    let _ = line_offsets; // usado para futura referência de offset exato

    let mut in_block = false;
    let mut depth: i32 = 0;

    for (line_idx, raw_line) in lines.iter().enumerate() {
        let raw_line = raw_line.trim_end_matches('\r');
        let stripped = strip_line_comments(raw_line, in_block);
        in_block = stripped.in_block;
        let line = &stripped.text;

        if depth == 0 {
            // native com corpo
            if let Some(cap) = RX_NATIVE.captures(line) {
                let name = &cap[1];
                let terminator = cap.get(2).map(|m| m.as_str()).unwrap_or(";");
                if terminator == "{" {
                    let col = raw_line.find(name).unwrap_or(0) as u32;
                    diags.push(PawnDiagnostic::error(
                        line_idx as u32, col, col + name.len() as u32,
                        codes::PP0002,
                        format!("Função native \"{}\" não pode ter corpo", name),
                    ));
                }
            }
            // forward com corpo
            else if let Some(cap) = RX_FORWARD.captures(line) {
                let name = &cap[1];
                let terminator = cap.get(2).map(|m| m.as_str()).unwrap_or(";");
                if terminator == "{" {
                    let col = raw_line.find(name).unwrap_or(0) as u32;
                    diags.push(PawnDiagnostic::error(
                        line_idx as u32, col, col + name.len() as u32,
                        codes::PP0003,
                        format!("Declaração forward \"{}\" não pode ter corpo", name),
                    ));
                }
            }
            // public / stock / static sem corpo
            else if let Some(cap) = RX_PUBLIC_STOCK.captures(line) {
                let kw = &cap[1];
                let name = &cap[2];
                let terminator = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                if terminator == ";" || terminator.is_empty() {
                    let col = raw_line.find(name).unwrap_or(0) as u32;
                    diags.push(PawnDiagnostic::warning(
                        line_idx as u32, col, col + name.len() as u32,
                        codes::PP0004,
                        format!("Declaração {} \"{}\" sem corpo. Use \"forward\" para protótipos.", kw, name),
                    ));
                }
            }
        }

        depth = update_brace_depth(line, depth);
    }

    diags
}
