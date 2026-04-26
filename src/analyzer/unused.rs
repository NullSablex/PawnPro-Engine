use std::collections::HashSet;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::parser::lexer::strip_line_comments;
use crate::parser::{ParsedFile, Symbol, SymbolKind};

use super::includes::ResolvedIncludes;
use super::{codes, diagnostic::PawnDiagnostic};

static RX_CALL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\s*\(").unwrap());
static RX_IDENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\b").unwrap());

#[derive(Clone, Copy)]
enum CollectMode {
    /// Coleta chamadas `name(`, ignorando linhas de declaração de funções
    Calls,
    /// Coleta todos os idents, ignorando linhas de declaração de variáveis
    AllIdents,
    /// Coleta todos os idents, ignorando linhas com `#` (diretivas)
    IdentsNoDeclLines,
}

fn collect_idents(text: &str, mode: CollectMode) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut in_block = false;

    for raw_line in text.split('\n') {
        let raw = raw_line.trim_end_matches('\r');
        let stripped = strip_line_comments(raw, in_block);
        in_block = stripped.in_block;

        let trimmed_lower = stripped.text.trim_start().to_ascii_lowercase();

        let skip = match mode {
            CollectMode::Calls => {
                trimmed_lower.starts_with("stock ") || trimmed_lower.starts_with("stock\t")
                    || trimmed_lower.starts_with("public ") || trimmed_lower.starts_with("public\t")
                    || trimmed_lower.starts_with("static ") || trimmed_lower.starts_with("static\t")
                    || trimmed_lower.starts_with("native ") || trimmed_lower.starts_with("native\t")
                    || trimmed_lower.starts_with("forward ") || trimmed_lower.starts_with("forward\t")
            }
            CollectMode::AllIdents => {
                trimmed_lower.starts_with("new ") || trimmed_lower.starts_with("new\t")
                    || trimmed_lower.starts_with("const ") || trimmed_lower.starts_with("const\t")
                    || ((trimmed_lower.starts_with("static ") || trimmed_lower.starts_with("static\t"))
                        && !stripped.text.contains('('))
            }
            CollectMode::IdentsNoDeclLines => stripped.text.trim_start().starts_with('#'),
        };

        if skip {
            continue;
        }

        let rx = match mode {
            CollectMode::Calls => &*RX_CALL,
            CollectMode::AllIdents | CollectMode::IdentsNoDeclLines => &*RX_IDENT,
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
) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let is_inc = has_extension(file_path, "inc");

    let local_calls = collect_idents(text, CollectMode::Calls);
    let local_idents = collect_idents(text, CollectMode::AllIdents);

    // PP0005 — variáveis não usadas (sempre, independente de ser .inc)
    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Variable)) {
        if sym.name.starts_with('_') { continue; }
        if !local_idents.contains(&sym.name) {
            diags.push(PawnDiagnostic::unnecessary_warning(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0005,
                format!("\"{}\" variável declarada mas não utilizada", sym.name),
            ));
        }
    }

    if is_inc && !warn_unused_in_inc {
        return diags;
    }

    // PP0006 — stocks/statics não usados
    let stock_syms: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Stock | SymbolKind::Static))
        .collect();

    if !stock_syms.is_empty() {
        let used = collect_used_stocks(&stock_syms, &local_calls, &resolved.paths, resolved);

        for sym in &stock_syms {
            if sym.name.starts_with('_') { continue; }
            if !used.contains(&sym.name) {
                diags.push(PawnDiagnostic::unnecessary_warning(
                    sym.line, sym.col, sym.col + sym.name.len() as u32,
                    codes::PP0006,
                    format!("\"{}\" função stock declarada mas não utilizada", sym.name),
                ));
            }
        }
    }

    // PP0011 — defines não usados
    if !parsed.macro_names.is_empty() {
        let idents_no_directives = collect_idents(text, CollectMode::IdentsNoDeclLines);
        for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Define)) {
            if sym.name.starts_with('_') { continue; }
            // Macros com parâmetros são chamadas como funções: checar em local_calls também
            let used_as_ident = idents_no_directives.contains(&sym.name);
            let used_as_call = local_calls.contains(&sym.name);
            if !used_as_ident && !used_as_call {
                diags.push(PawnDiagnostic::hint(
                    sym.line, sym.col, sym.col + sym.name.len() as u32,
                    codes::PP0011,
                    format!("\"{}\" definido mas não utilizado", sym.name),
                ));
            }
        }
    }

    // PP0012 — includes cujos símbolos não são usados
    for inc in &parsed.includes {
        let Some(rp) = find_resolved_path(inc, &resolved.paths) else { continue };
        let Some(entry) = resolved.files.get(rp) else { continue };

        let exported: HashSet<String> = entry.parsed.symbols.iter()
            .map(|s| s.name.clone())
            .chain(entry.parsed.macro_names.iter().cloned())
            .collect();

        if exported.is_empty() { continue; }

        if !exported.iter().any(|name| local_idents.contains(name)) {
            diags.push(PawnDiagnostic::hint(
                inc.line, inc.col, inc.col + inc.token.len() as u32,
                codes::PP0012,
                format!("\"{}\" incluído mas nenhum de seus símbolos é utilizado", inc.token),
            ));
        }
    }

    diags
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn collect_used_stocks(
    stocks: &[&Symbol],
    local_calls: &HashSet<String>,
    paths: &[std::path::PathBuf],
    resolved: &ResolvedIncludes,
) -> HashSet<String> {
    let mut used: HashSet<String> = stocks
        .iter()
        .filter(|s| local_calls.contains(&s.name))
        .map(|s| s.name.clone())
        .collect();

    if used.len() == stocks.len() {
        return used;
    }

    'outer: for fp in paths {
        let inc_calls = resolved.files.get(fp)
            .map(|e| collect_idents(&e.text, CollectMode::Calls))
            .unwrap_or_default();

        for sym in stocks {
            if !used.contains(&sym.name) && inc_calls.contains(&sym.name) {
                used.insert(sym.name.clone());
                if used.len() == stocks.len() {
                    break 'outer;
                }
            }
        }
    }

    used
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
