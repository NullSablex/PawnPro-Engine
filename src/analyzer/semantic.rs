use once_cell::sync::Lazy;
use regex::Regex;

use crate::parser::lexer::{strip_line_comments, update_brace_depth};

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

fn next_nonempty_starts_brace(lines: &[&str], from: usize) -> bool {
    for raw in lines.iter().skip(from + 1) {
        let s = strip_line_comments(raw.trim_end_matches('\r'), false);
        let t = s.text.trim().to_string();
        if !t.is_empty() {
            return t.starts_with('{');
        }
    }
    false
}

pub fn analyze_semantics(text: &str) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut in_block = false;
    let mut depth: i32 = 0;

    for (line_idx, raw_line) in lines.iter().enumerate() {
        let raw_line = raw_line.trim_end_matches('\r');
        let stripped = strip_line_comments(raw_line, in_block);
        in_block = stripped.in_block;
        let line = &stripped.text;

        if depth == 0 {
            if let Some(cap) = RX_NATIVE.captures(line) {
                let name = &cap[1];
                let terminator = cap.get(2).map(|m| m.as_str()).unwrap_or(";");
                let has_body = terminator == "{"
                    || (terminator.is_empty() && next_nonempty_starts_brace(&lines, line_idx));
                if has_body {
                    let col = raw_line.find(name).unwrap_or(0) as u32;
                    diags.push(PawnDiagnostic::error(
                        line_idx as u32, col, col + name.len() as u32,
                        codes::PP0002,
                        format!("Função native \"{}\" não pode ter corpo", name),
                    ));
                }
            } else if let Some(cap) = RX_FORWARD.captures(line) {
                let name = &cap[1];
                let terminator = cap.get(2).map(|m| m.as_str()).unwrap_or(";");
                let has_body = terminator == "{"
                    || (terminator.is_empty() && next_nonempty_starts_brace(&lines, line_idx));
                if has_body {
                    let col = raw_line.find(name).unwrap_or(0) as u32;
                    diags.push(PawnDiagnostic::error(
                        line_idx as u32, col, col + name.len() as u32,
                        codes::PP0003,
                        format!("Declaração forward \"{}\" não pode ter corpo", name),
                    ));
                }
            } else if let Some(cap) = RX_PUBLIC_STOCK.captures(line) {
                let kw = &cap[1];
                let name = &cap[2];
                let terminator = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                let no_body = terminator == ";"
                    || (terminator.is_empty() && !next_nonempty_starts_brace(&lines, line_idx));
                if no_body {
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
