use std::collections::HashSet;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::messages::{msg, Locale, MsgKey};
use crate::parser::lexer::strip_line_comments;
use crate::parser::{ParsedFile, SymbolKind};

use super::includes::ResolvedIncludes;
use super::{codes, diagnostic::PawnDiagnostic};

static RX_CALL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\s*\(").unwrap());
static RX_IDENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\b").unwrap());

#[derive(Clone, Copy)]
enum CollectMode {
    Calls,
    AllIdents,
    IdentsNoDefineLines,
}

fn collect_idents(text: &str, mode: CollectMode) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut in_block = false;

    for raw_line in text.split('\n') {
        let raw = raw_line.trim_end_matches('\r');
        let stripped = strip_line_comments(raw, in_block);
        in_block = stripped.in_block;

        let trimmed = stripped.text.trim_start();
        let trimmed_lower = trimmed.to_ascii_lowercase();

        if trimmed_lower.starts_with("#define") || trimmed_lower.starts_with("# define") {
            continue;
        }

        let kw = |k: &str| trimmed_lower.starts_with(&format!("{} ", k)) || trimmed_lower.starts_with(&format!("{}\t", k));
        let skip = match mode {
            CollectMode::Calls => kw("stock") || kw("public") || kw("static") || kw("native") || kw("forward"),
            CollectMode::AllIdents => kw("new") || kw("const") || (kw("static") && !stripped.text.contains('(')),
            CollectMode::IdentsNoDefineLines => trimmed.starts_with('#'),
        };

        if skip {
            continue;
        }

        let rx = match mode {
            CollectMode::Calls => &*RX_CALL,
            CollectMode::AllIdents | CollectMode::IdentsNoDefineLines => &*RX_IDENT,
        };

        for cap in rx.captures_iter(&stripped.text) {
            out.insert(cap[1].to_string());
        }
    }

    out
}

pub fn analyze_unused(
    text: &str,
    file_path: &Path,
    parsed: &ParsedFile,
    resolved: &ResolvedIncludes,
    warn_unused_in_inc: bool,
    workspace_root: Option<&Path>,
    locale: Locale,
) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let is_inc = is_include_file(file_path);

    let local_calls = collect_idents(text, CollectMode::Calls);
    let local_idents = collect_idents(text, CollectMode::AllIdents);

    if is_inc && !warn_unused_in_inc {
        return diags;
    }

    let mut all_calls = local_calls.clone();
    let mut all_idents_ws = local_idents.clone();
    let mut all_idents_no_directives = collect_idents(text, CollectMode::IdentsNoDefineLines);
    for fp in &resolved.paths {
        if let Some(entry) = resolved.files.get(fp) {
            all_calls.extend(collect_idents(&entry.text, CollectMode::Calls));
            all_idents_ws.extend(collect_idents(&entry.text, CollectMode::AllIdents));
            all_idents_no_directives.extend(collect_idents(&entry.text, CollectMode::IdentsNoDefineLines));
        }
    }
    if let Some(root) = workspace_root {
        collect_workspace_all(root, file_path, &mut all_calls, &mut all_idents_ws, &mut all_idents_no_directives);
    }

    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Variable)) {
        if sym.name.starts_with('_') { continue; }
        if !all_idents_ws.contains(&sym.name) {
            diags.push(PawnDiagnostic::unnecessary_warning(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0005,
                msg(locale, MsgKey::VarUnused).replace("{}", &sym.name),
            ));
        }
    }

    for sym in parsed.symbols.iter()
        .filter(|s| matches!(s.kind, SymbolKind::Stock | SymbolKind::Static))
    {
        if sym.name.starts_with('_') { continue; }
        if !all_calls.contains(&sym.name) {
            diags.push(PawnDiagnostic::unnecessary_warning(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0006,
                msg(locale, MsgKey::StockUnused).replace("{}", &sym.name),
            ));
        }
    }

    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Native)) {
        if sym.name.starts_with('_') { continue; }
        if !all_calls.contains(&sym.name) {
            diags.push(PawnDiagnostic::hint(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0014,
                msg(locale, MsgKey::NativeNeverCalled).replace("{}", &sym.name),
            ));
        }
    }

    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Forward)) {
        if sym.name.starts_with('_') { continue; }
        if !all_calls.contains(&sym.name) {
            diags.push(PawnDiagnostic::hint(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0015,
                msg(locale, MsgKey::ForwardNeverCalled).replace("{}", &sym.name),
            ));
        }
    }

    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Plain)) {
        if sym.name.starts_with('_') { continue; }
        if !all_calls.contains(&sym.name) {
            diags.push(PawnDiagnostic::unnecessary_warning(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0016,
                msg(locale, MsgKey::FuncNeverCalled).replace("{}", &sym.name),
            ));
        }
    }

    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Define)) {
        if sym.name.starts_with('_') { continue; }
        let used_as_ident = all_idents_no_directives.contains(&sym.name);
        let used_as_call = all_calls.contains(&sym.name);
        if !used_as_ident && !used_as_call {
            diags.push(PawnDiagnostic::hint(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0011,
                msg(locale, MsgKey::DefineUnused).replace("{}", &sym.name),
            ));
        }
    }

    for inc in &parsed.includes {
        let Some(rp) = find_resolved_path(inc, &resolved.paths) else { continue };

        let exported = collect_transitive_exports(rp, resolved);

        if exported.is_empty() { continue; }

        if !exported.iter().any(|name| all_idents_ws.contains(name)) {
            diags.push(PawnDiagnostic::hint(
                inc.line, inc.col, inc.col + inc.token.len() as u32,
                codes::PP0012,
                msg(locale, MsgKey::IncludeNoSymbolsUsed).replace("{}", &inc.token),
            ));
        }
    }

    diags
}

fn is_include_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("inc") | Some("p") | Some("pawn")
    )
}

fn workspace_files<'a>(root: &'a Path, exclude: &'a Path) -> impl Iterator<Item = std::path::PathBuf> + 'a {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(move |e| {
            e.file_type().is_file() && {
                let p = e.path();
                p != exclude && matches!(
                    p.extension().and_then(|x| x.to_str()),
                    Some("pwn") | Some("inc") | Some("p") | Some("pawn")
                )
            }
        })
        .map(|e| e.into_path())
}

fn collect_workspace_all(
    root: &Path, exclude: &Path,
    calls: &mut HashSet<String>,
    all_idents: &mut HashSet<String>,
    no_directives: &mut HashSet<String>,
) {
    for path in workspace_files(root, exclude) {
        if let Ok(bytes) = std::fs::read(&path) {
            let text = crate::parser::lexer::decode_bytes(&bytes);
            calls.extend(collect_idents(&text, CollectMode::Calls));
            all_idents.extend(collect_idents(&text, CollectMode::AllIdents));
            no_directives.extend(collect_idents(&text, CollectMode::IdentsNoDefineLines));
        }
    }
}

fn collect_transitive_exports(
    root: &std::path::PathBuf,
    resolved: &ResolvedIncludes,
) -> HashSet<String> {
    let mut exported = HashSet::new();
    let mut visited: HashSet<&std::path::PathBuf> = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root);

    while let Some(path) = queue.pop_front() {
        if !visited.insert(path) {
            continue;
        }
        let Some(entry) = resolved.files.get(path) else { continue };
        exported.extend(entry.parsed.symbols.iter().map(|s| s.name.clone()));
        exported.extend(entry.parsed.macro_names.iter().cloned());

        for nested_inc in &entry.parsed.includes {
            if let Some(nested_path) = find_resolved_path(nested_inc, &resolved.paths) {
                queue.push_back(nested_path);
            }
        }
    }

    exported
}

fn find_resolved_path<'a>(
    inc: &crate::parser::IncludeDirective,
    paths: &'a [std::path::PathBuf],
) -> Option<&'a std::path::PathBuf> {
    paths.iter().find(|p| {
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let token_stem = std::path::Path::new(&inc.token)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&inc.token);
        stem.eq_ignore_ascii_case(token_stem) || p.to_string_lossy().contains(&*inc.token)
    })
}
