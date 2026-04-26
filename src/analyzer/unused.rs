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
    /// Coleta chamadas `name(`, ignorando linhas de declaração de funções e #define
    Calls,
    /// Coleta todos os idents, ignorando linhas de declaração de variáveis e #define
    AllIdents,
    /// Coleta todos os idents de linhas não-diretiva (sem `#`), exceto #define — usado para defines
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

        // Linhas #define são sempre skippadas em todos os modos — evita auto-match do nome
        if trimmed_lower.starts_with("#define") || trimmed_lower.starts_with("# define") {
            continue;
        }

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
            // Para IdentsNoDefineLines: aceita só linhas sem qualquer diretiva `#`
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
) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let is_inc = is_include_file(file_path);

    let local_calls = collect_idents(text, CollectMode::Calls);
    let local_idents = collect_idents(text, CollectMode::AllIdents);

    if is_inc && !warn_unused_in_inc {
        return diags;
    }

    // Computa all_calls e all_idents uma única vez — reutilizado por todos os diagnósticos abaixo.
    // Escopo: arquivo atual + includes transitivos + todos os arquivos do workspace.
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
        collect_workspace_calls(root, file_path, &mut all_calls);
        collect_workspace_idents(root, file_path, &mut all_idents_ws);
        collect_workspace_idents_no_directives(root, file_path, &mut all_idents_no_directives);
    }

    // PP0005 — variáveis declaradas mas não utilizadas
    // Varre workspace inteiro: uma variável global em .inc pode ser lida por qualquer .pwn.
    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Variable)) {
        if sym.name.starts_with('_') { continue; }
        if !all_idents_ws.contains(&sym.name) {
            diags.push(PawnDiagnostic::unnecessary_warning(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0005,
                format!("\"{}\" variável declarada mas não utilizada", sym.name),
            ));
        }
    }

    // PP0006 — stocks/statics não usados
    for sym in parsed.symbols.iter()
        .filter(|s| matches!(s.kind, SymbolKind::Stock | SymbolKind::Static))
    {
        if sym.name.starts_with('_') { continue; }
        if !all_calls.contains(&sym.name) {
            diags.push(PawnDiagnostic::unnecessary_warning(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0006,
                format!("\"{}\" função stock declarada mas não utilizada", sym.name),
            ));
        }
    }

    // PP0014 — natives declaradas mas nunca chamadas em nenhum arquivo do workspace
    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Native)) {
        if sym.name.starts_with('_') { continue; }
        if !all_calls.contains(&sym.name) {
            diags.push(PawnDiagnostic::hint(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0014,
                format!("\"{}\" native declarada mas nunca chamada", sym.name),
            ));
        }
    }

    // PP0015 — forwards declarados mas nunca chamados
    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Forward)) {
        if sym.name.starts_with('_') { continue; }
        if !all_calls.contains(&sym.name) {
            diags.push(PawnDiagnostic::hint(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0015,
                format!("\"{}\" forward declarado mas nunca chamado", sym.name),
            ));
        }
    }

    // PP0016 — funções plain (sem keyword) declaradas mas nunca chamadas
    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Plain)) {
        if sym.name.starts_with('_') { continue; }
        if !all_calls.contains(&sym.name) {
            diags.push(PawnDiagnostic::unnecessary_warning(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0016,
                format!("\"{}\" função declarada mas nunca chamada", sym.name),
            ));
        }
    }

    // PP0011 — defines não usados (varredura workspace para evitar falsos positivos em .inc)
    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Define)) {
        if sym.name.starts_with('_') { continue; }
        let used_as_ident = all_idents_no_directives.contains(&sym.name);
        let used_as_call = all_calls.contains(&sym.name);
        if !used_as_ident && !used_as_call {
            diags.push(PawnDiagnostic::hint(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0011,
                format!("\"{}\" definido mas não utilizado", sym.name),
            ));
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

        if !exported.iter().any(|name| all_idents_ws.contains(name)) {
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

/// Retorna true para qualquer extensão que o compilador trata como include file:
/// .inc, .p, .pawn — nunca compilados diretamente, sempre incluídos por um .pwn.
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

/// Varre todos os arquivos do workspace e acumula idents excluindo linhas de diretivas.
fn collect_workspace_idents_no_directives(root: &Path, exclude: &Path, out: &mut HashSet<String>) {
    for path in workspace_files(root, exclude) {
        if let Ok(bytes) = std::fs::read(&path) {
            let text = crate::parser::lexer::decode_bytes(&bytes);
            out.extend(collect_idents(&text, CollectMode::IdentsNoDefineLines));
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
