use std::collections::HashSet;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::parser::lexer::strip_line_comments;
use crate::parser::{ParsedFile, SymbolKind};

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
    workspace_root: Option<&Path>,
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
    // Para .inc: varrer todos os .pwn do workspace — a stock pode ser usada por qualquer
    // arquivo que inclua este .inc, não apenas pelos includes transitivos do próprio arquivo.
    let stock_syms: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Stock | SymbolKind::Static))
        .collect();

    if !stock_syms.is_empty() {
        // Coleta chamadas do arquivo atual + includes transitivos + todos os arquivos do workspace.
        // Isso espelha o compilador real: ele avalia "não usada" no contexto da compilation
        // unit inteira, que pode incluir qualquer arquivo que referencie este.
        let mut all_calls = local_calls.clone();
        for fp in &resolved.paths {
            if let Some(entry) = resolved.files.get(fp) {
                all_calls.extend(collect_idents(&entry.text, CollectMode::Calls));
            }
        }
        if let Some(root) = workspace_root {
            collect_workspace_calls(root, file_path, &mut all_calls);
        }

        for sym in &stock_syms {
            if sym.name.starts_with('_') { continue; }
            if !all_calls.contains(&sym.name) {
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
    // Usa all_calls que já inclui o workspace inteiro, mais local_idents para não-funções.
    let mut all_idents = local_idents.clone();
    if let Some(root) = workspace_root {
        collect_workspace_idents(root, file_path, &mut all_idents);
    }
    for inc in &parsed.includes {
        let Some(rp) = find_resolved_path(inc, &resolved.paths) else { continue };
        let Some(entry) = resolved.files.get(rp) else { continue };

        let exported: HashSet<String> = entry.parsed.symbols.iter()
            .map(|s| s.name.clone())
            .chain(entry.parsed.macro_names.iter().cloned())
            .collect();

        if exported.is_empty() { continue; }

        if !exported.iter().any(|name| all_idents.contains(name)) {
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
                    Some("pwn") | Some("inc")
                )
            }
        })
        .map(|e| e.into_path())
}

/// Varre todos os arquivos .pwn e .inc do workspace (exceto o próprio arquivo)
/// e acumula todas as chamadas de função encontradas.
fn collect_workspace_calls(root: &Path, exclude: &Path, out: &mut HashSet<String>) {
    for path in workspace_files(root, exclude) {
        if let Ok(bytes) = std::fs::read(&path) {
            let text = crate::parser::lexer::decode_bytes(&bytes);
            out.extend(collect_idents(&text, CollectMode::Calls));
        }
    }
}

/// Varre todos os arquivos do workspace e acumula todos os identificadores.
fn collect_workspace_idents(root: &Path, exclude: &Path, out: &mut HashSet<String>) {
    for path in workspace_files(root, exclude) {
        if let Ok(bytes) = std::fs::read(&path) {
            let text = crate::parser::lexer::decode_bytes(&bytes);
            out.extend(collect_idents(&text, CollectMode::AllIdents));
        }
    }
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
