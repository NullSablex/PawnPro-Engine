use regex::Regex;

use crate::messages::{Locale, MsgKey, msg};
use crate::parser::lexer::{strip_line_comments, update_brace_depth};

use super::{codes, diagnostic::PawnDiagnostic};
use crate::util::to_u32;

static RX_NATIVE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(?:forward\s+)?native\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\([^)]*\)\s*([;{])?").unwrap()
});
static RX_FORWARD: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*forward\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\([^)]*\)\s*([;{])?").unwrap()
});
static RX_PUBLIC_STOCK: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
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

pub fn analyze_semantics(text: &str, locale: Locale) -> Vec<PawnDiagnostic> {
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
                let terminator = cap.get(2).map_or(";", |m| m.as_str());
                let has_body = terminator == "{"
                    || (terminator.is_empty() && next_nonempty_starts_brace(&lines, line_idx));
                if has_body {
                    let col = to_u32(raw_line.find(name).unwrap_or(0));
                    diags.push(PawnDiagnostic::error(
                        to_u32(line_idx),
                        col,
                        col + to_u32(name.len()),
                        codes::PP0002,
                        msg(locale, MsgKey::NativeHasBody).replace("{}", name),
                    ));
                }
            } else if let Some(cap) = RX_FORWARD.captures(line) {
                let name = &cap[1];
                let terminator = cap.get(2).map_or(";", |m| m.as_str());
                let has_body = terminator == "{"
                    || (terminator.is_empty() && next_nonempty_starts_brace(&lines, line_idx));
                if has_body {
                    let col = to_u32(raw_line.find(name).unwrap_or(0));
                    diags.push(PawnDiagnostic::error(
                        to_u32(line_idx),
                        col,
                        col + to_u32(name.len()),
                        codes::PP0003,
                        msg(locale, MsgKey::ForwardHasBody).replace("{}", name),
                    ));
                }
            } else if let Some(cap) = RX_PUBLIC_STOCK.captures(line) {
                let kw = &cap[1];
                let name = &cap[2];
                let terminator = cap.get(3).map_or("", |m| m.as_str());
                let no_body = terminator == ";"
                    || (terminator.is_empty() && !next_nonempty_starts_brace(&lines, line_idx));
                if no_body {
                    let col = to_u32(raw_line.find(name).unwrap_or(0));
                    let template = msg(locale, MsgKey::DeclNoBody);
                    let message = template.replacen("{}", kw, 1).replacen("{}", name, 1);
                    diags.push(PawnDiagnostic::warning(
                        to_u32(line_idx),
                        col,
                        col + to_u32(name.len()),
                        codes::PP0004,
                        message,
                    ));
                }
            }
        }

        depth = update_brace_depth(line, depth);
    }

    diags
}
