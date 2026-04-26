use tower_lsp::lsp_types::*;

use crate::parser::lexer::strip_line_comments;

pub struct FormatOptions {
    pub tab_size: u32,
    pub insert_spaces: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { tab_size: 4, insert_spaces: false }
    }
}

impl FormatOptions {
    pub fn from_lsp(opts: &FormattingOptions) -> Self {
        Self {
            tab_size: opts.tab_size,
            insert_spaces: opts.insert_spaces,
        }
    }

    fn indent_str(&self) -> String {
        if self.insert_spaces {
            " ".repeat(self.tab_size as usize)
        } else {
            "\t".to_string()
        }
    }
}

/// Formats the entire document. Returns a single TextEdit replacing the whole content.
pub fn format_document(text: &str, opts: &FormatOptions) -> Vec<TextEdit> {
    let formatted = format_text(text, opts);
    if formatted == text {
        return vec![];
    }
    let line_count = text.lines().count() as u32;
    let last_line = text.lines().last().unwrap_or("");
    vec![TextEdit {
        range: Range {
            start: Position { line: 0, character: 0 },
            end: Position {
                line: line_count,
                character: last_line.len() as u32,
            },
        },
        new_text: formatted,
    }]
}

/// Formats a specific range. Returns edits replacing only the affected lines.
pub fn format_range(text: &str, range: Range, opts: &FormatOptions) -> Vec<TextEdit> {
    let lines: Vec<&str> = text.lines().collect();
    let start_line = range.start.line as usize;
    let end_line = (range.end.line as usize).min(lines.len().saturating_sub(1));

    // Determine indent level at start_line by counting braces above it
    let indent_at_start = compute_indent_at_line(text, start_line);

    let slice: Vec<&str> = lines[start_line..=end_line].to_vec();
    let slice_text = slice.join("\n");
    let formatted_slice = format_text_with_initial_indent(&slice_text, opts, indent_at_start);

    if formatted_slice == slice_text {
        return vec![];
    }

    let end_char = lines.get(end_line).map(|l| l.len() as u32).unwrap_or(0);
    vec![TextEdit {
        range: Range {
            start: Position { line: start_line as u32, character: 0 },
            end: Position { line: end_line as u32, character: end_char },
        },
        new_text: formatted_slice,
    }]
}

fn compute_indent_at_line(text: &str, target_line: usize) -> i32 {
    let mut depth = 0i32;
    let mut in_block = false;
    for (i, raw) in text.lines().enumerate() {
        if i >= target_line { break; }
        let stripped = strip_line_comments(raw, in_block);
        in_block = stripped.in_block;
        for ch in stripped.text.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth = (depth - 1).max(0),
                _ => {}
            }
        }
    }
    depth
}

pub fn format_text(text: &str, opts: &FormatOptions) -> String {
    format_text_with_initial_indent(text, opts, 0)
}

fn format_text_with_initial_indent(text: &str, opts: &FormatOptions, initial_indent: i32) -> String {
    let indent_unit = opts.indent_str();
    let raw_lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(raw_lines.len());
    let mut depth = initial_indent;
    let mut in_block_comment = false;
    // Track if previous non-empty non-comment line was a function/public/stock opener
    let mut prev_was_top_level_decl = false;

    for (i, raw) in raw_lines.iter().enumerate() {
        let trimmed = raw.trim();

        // Blank line — preserve at most one consecutive blank
        if trimmed.is_empty() {
            if out.last().map(|l: &String| l.trim().is_empty()).unwrap_or(false) {
                continue; // collapse multiple blank lines into one
            }
            out.push(String::new());
            continue;
        }

        // Block comment continuation lines (start with * or */)
        let stripped = strip_line_comments(raw, in_block_comment);
        let is_full_block_comment_line = in_block_comment
            || trimmed.starts_with("/*")
            || (trimmed.starts_with('*') && !trimmed.starts_with("*/"));

        if in_block_comment || trimmed.starts_with("/*") {
            let next_in_block = stripped.in_block;
            // Preserve indentation of block comments relative to current depth
            let prefix = if trimmed.starts_with('*') || trimmed.starts_with("*/") {
                format!("{} ", indent(depth, &indent_unit))
            } else {
                indent(depth, &indent_unit)
            };
            out.push(format!("{}{}", prefix, trimmed));
            in_block_comment = next_in_block;
            continue;
        }
        in_block_comment = stripped.in_block;
        let _ = is_full_block_comment_line;

        // Preprocessor directives: keep as-is (just strip trailing whitespace)
        if trimmed.starts_with('#') {
            // Blank line before top-level declarations for readability
            if is_top_level_func_decl(trimmed) && i > 0 && !prev_was_top_level_decl {
                if out.last().map(|l: &String| !l.trim().is_empty()).unwrap_or(false) {
                    out.push(String::new());
                }
            }
            out.push(trimmed.to_string());
            prev_was_top_level_decl = false;
            continue;
        }

        // Closing brace(s) — dedent before printing
        let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
        let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;

        if trimmed.starts_with('}') {
            depth = (depth - closes + opens).max(0);
        }

        // Blank line before top-level function declarations
        let is_func = is_top_level_func_decl(trimmed);
        if is_func && depth == 0 && i > 0 {
            if out.last().map(|l: &String| !l.trim().is_empty()).unwrap_or(false) {
                out.push(String::new());
            }
        }

        // Format the line content
        let formatted_line = format_line(trimmed, &stripped.text.trim().to_string());
        out.push(format!("{}{}", indent(depth, &indent_unit), formatted_line));

        // Adjust depth after line for non-}-started lines
        if !trimmed.starts_with('}') {
            depth = (depth + opens - closes).max(0);
        }

        prev_was_top_level_decl = is_func;
    }

    // Ensure single trailing newline
    while out.last().map(|l: &String| l.trim().is_empty()).unwrap_or(false) {
        out.pop();
    }
    let mut result = out.join("\n");
    result.push('\n');
    result
}

fn indent(depth: i32, unit: &str) -> String {
    unit.repeat(depth as usize)
}

fn is_top_level_func_decl(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("public ") || lower.starts_with("public\t")
        || lower.starts_with("stock ") || lower.starts_with("stock\t")
        || lower.starts_with("static ") || lower.starts_with("static\t")
        || lower.starts_with("forward ") || lower.starts_with("forward\t")
        || lower.starts_with("native ") || lower.starts_with("native\t")
        || (trimmed.contains('(') && !lower.starts_with("new ")
            && !lower.starts_with("if") && !lower.starts_with("while")
            && !lower.starts_with("for") && !lower.starts_with("switch")
            && !lower.starts_with("//") && !lower.starts_with("return"))
}

/// Applies intra-line formatting: operator spacing, keyword spacing, etc.
/// `raw_content` is the stripped (no comments) version for analysis;
/// we format `trimmed` (which may have inline comments).
fn format_line(trimmed: &str, _stripped: &str) -> String {
    // Lines we should not touch internally:
    // - single-line comments
    // - preprocessor (already handled above)
    // - string-heavy lines (risk of mangling string contents)
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return trimmed.to_string();
    }

    let s = format_operators(trimmed);
    let s = format_keyword_spacing(&s);
    let s = collapse_spaces(&s);
    s
}

/// Adds spaces around binary operators where missing, respects strings/chars.
fn format_operators(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len() + 16);
    let mut i = 0;
    let mut in_str = false;
    let mut in_char = false;

    while i < chars.len() {
        let ch = chars[i];

        // Track string/char literals to avoid mangling them
        match ch {
            '"' if !in_char => {
                in_str = !in_str;
                out.push(ch);
                i += 1;
                continue;
            }
            '\'' if !in_str => {
                in_char = !in_char;
                out.push(ch);
                i += 1;
                continue;
            }
            '\\' if in_str || in_char => {
                out.push(ch);
                if i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            _ if in_str || in_char => {
                out.push(ch);
                i += 1;
                continue;
            }
            _ => {}
        }

        // Multi-char operators: ==, !=, <=, >=, +=, -=, *=, /=, %=, &&, ||, ++, --, ->, <<, >>
        let next = chars.get(i + 1).copied();
        let two_char_op: Option<&str> = match (ch, next) {
            ('=', Some('=')) => Some("=="),
            ('!', Some('=')) => Some("!="),
            ('<', Some('=')) => Some("<="),
            ('>', Some('=')) => Some(">="),
            ('+', Some('=')) => Some("+="),
            ('-', Some('=')) => Some("-="),
            ('*', Some('=')) => Some("*="),
            ('/', Some('=')) => Some("/="),
            ('%', Some('=')) => Some("%="),
            ('&', Some('&')) => Some("&&"),
            ('|', Some('|')) => Some("||"),
            ('<', Some('<')) => Some("<<"),
            ('>', Some('>')) => Some(">>"),
            _ => None,
        };

        if let Some(op) = two_char_op {
            // ++ and -- are unary, don't pad them
            if op == "++" || op == "--" {
                out.push(ch);
                out.push(next.unwrap());
                i += 2;
                continue;
            }
            ensure_space_before(&mut out);
            out.push_str(op);
            out.push(' ');
            i += 2;
            // skip spaces already present after op
            while i < chars.len() && chars[i] == ' ' { i += 1; }
            continue;
        }

        // Single-char binary operators: = + - * / % < > & | ^ !
        // Skip: unary minus/plus (after operator or open paren), pointer-like contexts
        match ch {
            '=' | '<' | '>' | '&' | '|' | '^' => {
                ensure_space_before(&mut out);
                out.push(ch);
                out.push(' ');
                i += 1;
                while i < chars.len() && chars[i] == ' ' { i += 1; }
            }
            '+' | '-' => {
                // Unary: after (, [, ,, =, operator chars, or at start
                let prev = last_non_space(&out);
                let is_unary = prev.map(|c| "([,=+-*/%&|^!<>".contains(c)).unwrap_or(true);
                if is_unary {
                    out.push(ch);
                } else {
                    ensure_space_before(&mut out);
                    out.push(ch);
                    out.push(' ');
                    while i + 1 < chars.len() && chars[i + 1] == ' ' { i += 1; }
                }
                i += 1;
            }
            '*' | '/' | '%' => {
                let prev = last_non_space(&out);
                let is_unary = prev.map(|c| "([,=+-*/%&|^!<>".contains(c)).unwrap_or(true);
                if is_unary || ch == '/' {
                    // '/' can start a comment — don't pad
                    out.push(ch);
                } else {
                    ensure_space_before(&mut out);
                    out.push(ch);
                    out.push(' ');
                    while i + 1 < chars.len() && chars[i + 1] == ' ' { i += 1; }
                }
                i += 1;
            }
            ',' => {
                // Remove space before comma, ensure space after
                while out.ends_with(' ') { out.pop(); }
                out.push(',');
                out.push(' ');
                i += 1;
                while i < chars.len() && chars[i] == ' ' { i += 1; }
            }
            ';' => {
                while out.ends_with(' ') { out.pop(); }
                out.push(';');
                i += 1;
            }
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }

    out
}

fn ensure_space_before(out: &mut String) {
    if !out.ends_with(' ') && !out.is_empty() {
        out.push(' ');
    }
}

fn last_non_space(s: &str) -> Option<char> {
    s.chars().rev().find(|&c| c != ' ')
}

/// Ensures a space between keywords and `(`: `if(` → `if (`, `while(` → `while (`.
fn format_keyword_spacing(line: &str) -> String {
    // Keywords that must be followed by a space before `(`
    static KW: &[&str] = &[
        "if", "else", "for", "while", "do", "switch", "return", "sizeof", "tagof",
    ];
    let mut s = line.to_string();
    for kw in KW {
        let pat = format!("{}(", kw);
        let rep = format!("{} (", kw);
        // Only replace when the keyword is a whole word (preceded by non-ident or start)
        s = replace_whole_word(&s, &pat, &rep);
    }
    // Ensure space after `else` when followed by `{` or `if`
    s = replace_whole_word(&s, "else{", "else {");
    s = replace_whole_word(&s, "else if", "else if");
    s
}

fn replace_whole_word(s: &str, from: &str, to: &str) -> String {
    if !s.contains(from) { return s.to_string(); }
    let kw = &from[..from.len() - 1]; // keyword part (without the trailing char)
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find(from) {
        // Check the char before pos is not an ident char
        let before = if pos > 0 { rest[..pos].chars().last() } else { None };
        let is_word_boundary = before.map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true);
        result.push_str(&rest[..pos]);
        if is_word_boundary {
            result.push_str(to);
        } else {
            result.push_str(from);
        }
        rest = &rest[pos + from.len()..];
        let _ = kw;
    }
    result.push_str(rest);
    result
}

/// Collapse multiple consecutive spaces into one (outside strings).
fn collapse_spaces(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut prev_space = false;
    let mut in_str = false;
    let mut in_char = false;

    for ch in line.chars() {
        match ch {
            '"' if !in_char => { in_str = !in_str; out.push(ch); prev_space = false; }
            '\'' if !in_str => { in_char = !in_char; out.push(ch); prev_space = false; }
            ' ' if !in_str && !in_char => {
                if !prev_space { out.push(' '); }
                prev_space = true;
            }
            _ => { out.push(ch); prev_space = false; }
        }
    }

    out.trim_end().to_string()
}
