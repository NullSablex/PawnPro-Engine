use std::fmt::Write as _;
use std::path::Path;

use regex::Regex;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::analyzer::includes::resolve_include;
use crate::messages::{MsgKey, msg};
use crate::parser::types::{IncludeDirective, Symbol, SymbolKind};
use crate::workspace::WorkspaceState;

use super::{collect_all_symbols, extract_word};

static RX_INCLUDE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"#\s*include\s*(?:<([^>]+)>|"([^"]+)")"#).unwrap());

pub fn get_hover(state: &WorkspaceState, uri: &str, position: Position) -> Option<Hover> {
    let locale = state.locale;
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

    if let Some(h) = hover_include(
        line,
        &file_path,
        &inc_paths,
        state.workspace_root.as_deref(),
    ) {
        return Some(h);
    }

    let word = extract_word(line, col)?;
    let all_syms = collect_all_symbols(state, &file_path, &inc_paths, &parsed);
    let sym = all_syms.iter().find(|s| s.name == word)?;

    Some(format_symbol(sym, locale))
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

    let dir = IncludeDirective {
        token: token.clone(),
        is_angle,
        is_try: false,
        line: 0,
        col: 0,
    };
    let file_dir = file_path.parent().unwrap_or(Path::new("."));
    resolve_include(&dir, file_dir, inc_paths)?;

    let _ = workspace_root;
    let md = format!("```\n{}\n```\n\n`{}`", line.trim(), token);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

pub(super) fn doc_labels(locale: crate::messages::Locale) -> super::DocLabels {
    super::DocLabels {
        params: msg(locale, MsgKey::HoverParams).to_string(),
        returns: msg(locale, MsgKey::HoverReturns).to_string(),
        remarks: msg(locale, MsgKey::HoverRemarks).to_string(),
    }
}

fn format_symbol(sym: &Symbol, locale: crate::messages::Locale) -> Hover {
    let kw = match sym.kind {
        SymbolKind::Native => "native",
        SymbolKind::Forward => "forward",
        SymbolKind::Public => "public",
        SymbolKind::Stock => "stock",
        SymbolKind::Static => "static",
        SymbolKind::Plain => "",
        SymbolKind::StaticConst | SymbolKind::Const => "const",
        SymbolKind::Enum => "enum",
        SymbolKind::Define => "#define",
        SymbolKind::Variable => "new",
    };

    let mut md = if let Some(sig) = &sym.signature {
        if kw.is_empty() {
            format!("```pawn\n{sig}\n```")
        } else {
            format!("```pawn\n{kw} {sig}\n```")
        }
    } else if kw.is_empty() {
        format!("```pawn\n{}\n```", sym.name)
    } else {
        format!("```pawn\n{} {}\n```", kw, sym.name)
    };

    if sym.deprecated {
        // Sem blockquote: o editor recua o bloco inteiro, e o `---` seguinte
        // passa a ser lido como continuação dele.
        let _ = write!(md, "\n\n{}", msg(locale, MsgKey::HoverDeprecated));
        // A mensagem da diretiva costuma dizer o que usar no lugar.
        if let Some(m) = sym.deprecated_message.as_deref().filter(|m| !m.is_empty()) {
            let _ = write!(md, " — {m}");
        }
    }

    if let Some(rendered) = sym
        .doc
        .as_deref()
        .map(super::parse_doc)
        .and_then(|d| d.to_markdown(&doc_labels(locale)))
    {
        let _ = write!(md, "\n\n---\n{rendered}");
    }

    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::Locale;
    use crate::parser::types::SymbolKind;

    fn sym(deprecated: bool, message: Option<&str>, doc: Option<&str>) -> Symbol {
        Symbol {
            name: "BanirComMotivo".into(),
            kind: SymbolKind::Stock,
            signature: Some("BanirComMotivo(playerid)".into()),
            params: vec![],
            deprecated,
            deprecated_message: message.map(str::to_string),
            doc: doc.map(str::to_string),
            line: 0,
            col: 0,
        }
    }

    fn markdown(h: &Hover) -> String {
        match &h.contents {
            HoverContents::Markup(m) => m.value.clone(),
            _ => panic!("esperado markup"),
        }
    }

    #[test]
    fn deprecation_is_not_a_blockquote() {
        // `>` faz o editor recuar o bloco e engolir o `---` seguinte como
        // continuação — foi o que deixava o hover torto.
        let md = markdown(&format_symbol(&sym(true, None, None), Locale::PtBr));
        assert!(!md.contains('>'), "{md}");
        assert!(md.contains("Depreciado"), "{md}");
    }

    #[test]
    fn deprecation_message_is_shown_in_the_hover() {
        let md = markdown(&format_symbol(
            &sym(true, Some("Use BanPlayerFor"), None),
            Locale::PtBr,
        ));
        assert!(md.contains("Use BanPlayerFor"), "{md}");
    }

    #[test]
    fn signature_comes_first_and_doc_after_the_rule() {
        let md = markdown(&format_symbol(
            &sym(false, None, Some("/**\n * Bane alguém.\n */")),
            Locale::PtBr,
        ));
        assert!(md.starts_with("```pawn\nstock BanirComMotivo(playerid)\n```"));
        assert!(md.contains("\n---\n"), "{md}");
        assert!(md.contains("Bane alguém."), "{md}");
    }

    #[test]
    fn a_symbol_without_doc_still_shows_its_signature() {
        let md = markdown(&format_symbol(&sym(false, None, None), Locale::PtBr));
        assert!(md.contains("BanirComMotivo(playerid)"));
        assert!(!md.contains("---"), "sem doc não há regra: {md}");
    }
}
