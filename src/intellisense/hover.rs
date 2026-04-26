use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;
use tower_lsp::lsp_types::*;

use crate::analyzer::includes::resolve_include;
use crate::parser::types::{IncludeDirective, Symbol, SymbolKind};
use crate::workspace::WorkspaceState;

use super::{collect_all_symbols, extract_word};

static RX_INCLUDE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"#\s*include\s*(?:<([^>]+)>|"([^"]+)")"#).unwrap()
});

pub fn get_hover(state: &WorkspaceState, uri: &str, position: Position) -> Option<Hover> {
    let text = state.get_text(uri)?;
    let file_path = crate::workspace::uri_to_path(uri)?;
    let inc_paths = state.include_paths();
    let parsed = state.get_parsed(uri)?;

    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    let col = position.character as usize;

    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];

    if let Some(h) = hover_include(line, &file_path, &inc_paths, state.workspace_root.as_deref()) {
        return Some(h);
    }

    let word = extract_word(line, col)?;
    let all_syms = collect_all_symbols(state, &file_path, &inc_paths, &parsed);
    let sym = all_syms.iter().find(|s| s.name == word)?;

    Some(format_symbol(sym))
}

fn hover_include(
    line: &str,
    file_path: &Path,
    inc_paths: &[std::path::PathBuf],
    workspace_root: Option<&Path>,
) -> Option<Hover> {
    if !line.trim().starts_with('#') {
        return None;
    }
    let cap = RX_INCLUDE.captures(line)?;
    let (token, is_angle) = if let Some(m) = cap.get(1) {
        (m.as_str().to_string(), true)
    } else {
        (cap.get(2)?.as_str().to_string(), false)
    };

    let dir = IncludeDirective { token: token.clone(), is_angle, is_try: false, line: 0, col: 0 };
    let file_dir = file_path.parent().unwrap_or(Path::new("."));
    resolve_include(&dir, file_dir, inc_paths)?;

    let _ = workspace_root;
    let md = format!("```\n{}\n```\n\n`{}`", line.trim(), token);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: md }),
        range: None,
    })
}

fn format_symbol(sym: &Symbol) -> Hover {
    let kw = match sym.kind {
        SymbolKind::Native => "native",
        SymbolKind::Forward => "forward",
        SymbolKind::Public => "public",
        SymbolKind::Stock => "stock",
        SymbolKind::Static => "static",
        SymbolKind::Plain => "",
        SymbolKind::StaticConst => "const",
        SymbolKind::Enum => "enum",
        SymbolKind::Define => "#define",
        SymbolKind::Variable => "new",
        SymbolKind::Const => "const",
    };

    let mut md = if let Some(sig) = &sym.signature {
        if kw.is_empty() {
            format!("```pawn\n{}\n```", sig)
        } else {
            format!("```pawn\n{} {}\n```", kw, sig)
        }
    } else if kw.is_empty() {
        format!("```pawn\n{}\n```", sym.name)
    } else {
        format!("```pawn\n{} {}\n```", kw, sym.name)
    };

    if sym.deprecated {
        md.push_str("\n\n> ⚠️ **Deprecated**");
    }

    if let Some(doc) = &sym.doc {
        let clean: Vec<&str> = doc
            .lines()
            .map(|l| {
                l.trim()
                    .trim_start_matches("//")
                    .trim_start_matches('*')
                    .trim_start_matches('/')
                    .trim()
            })
            .filter(|l| !l.is_empty() && !l.starts_with("/*") && !l.starts_with("*/"))
            .collect();
        if !clean.is_empty() {
            md.push_str(&format!("\n\n---\n{}", clean.join("\n")));
        }
    }

    Hover {
        contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: md }),
        range: None,
    }
}
