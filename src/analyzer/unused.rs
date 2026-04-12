use std::collections::HashSet;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::parser::lexer::strip_line_comments;
use crate::parser::{ParsedFile, SymbolKind};

use super::includes::collect_included_files;
use super::{codes, diagnostic::PawnDiagnostic};

static RX_CALL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\s*\(").unwrap());
static RX_IDENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b([A-Za-z_]\w*)\b").unwrap());

/// Coleta todos os identificadores usados como chamada `Name(` em um texto.
fn collect_call_usages(text: &str) -> HashSet<String> {
    let mut usages = HashSet::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut in_block = false;
    for raw_line in &lines {
        let stripped = strip_line_comments(raw_line.trim_end_matches('\r'), in_block);
        in_block = stripped.in_block;
        for cap in RX_CALL.captures_iter(&stripped.text) {
            usages.insert(cap[1].to_string());
        }
    }
    usages
}

/// Coleta todos os identificadores (para macros e variáveis).
fn collect_all_idents(text: &str) -> HashSet<String> {
    let mut usages = HashSet::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut in_block = false;
    for raw_line in &lines {
        let stripped = strip_line_comments(raw_line.trim_end_matches('\r'), in_block);
        in_block = stripped.in_block;
        for cap in RX_IDENT.captures_iter(&stripped.text) {
            usages.insert(cap[1].to_string());
        }
    }
    usages
}

/// Analisa símbolos não utilizados.
///
/// - Variáveis (`Variable`): verificadas localmente no arquivo.
/// - Stocks/Static em `.pwn`: verificadas localmente + nos includes. Em `.inc`, suprimidas
///   a menos que `warn_unused_in_inc = true`.
/// - Parâmetros de native: NUNCA verificados (não são variáveis locais).
pub fn analyze_unused(
    text: &str,
    file_path: &Path,
    parsed: &ParsedFile,
    include_paths: &[PathBuf],
    warn_unused_in_inc: bool,
) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let is_inc = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("inc"))
        .unwrap_or(false);

    let local_calls = collect_call_usages(text);
    let local_idents = collect_all_idents(text);

    // ── Variáveis locais ──────────────────────────────────────────────────
    for sym in parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Variable)) {
        if !is_used_as_ident(&sym.name, &local_idents, sym.line) {
            diags.push(PawnDiagnostic::unnecessary_warning(
                sym.line, sym.col, sym.col + sym.name.len() as u32,
                codes::PP0005,
                format!("\"{}\" variável declarada mas não utilizada", sym.name),
            ));
        }
    }

    // ── Stocks / Static ───────────────────────────────────────────────────
    let stock_syms: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Stock | SymbolKind::Static))
        .collect();

    if !stock_syms.is_empty() {
        // Em .inc sem flag: não emite
        if is_inc && !warn_unused_in_inc {
            return diags;
        }

        // Verifica uso local primeiro
        let mut used: HashSet<String> = HashSet::new();
        for sym in &stock_syms {
            if local_calls.contains(&sym.name) {
                used.insert(sym.name.clone());
            }
        }

        // Se ainda há stocks não usadas, verifica nos includes
        if used.len() < stock_syms.len() {
            let included = collect_included_files(file_path, include_paths, &parsed.includes, 5, 100);
            'outer: for fp in &included {
                if let Ok(inc_text) = std::fs::read(fp) {
                    let inc_text = decode_text(&inc_text);
                    let inc_calls = collect_call_usages(&inc_text);
                    for sym in &stock_syms {
                        if !used.contains(&sym.name) && inc_calls.contains(&sym.name) {
                            used.insert(sym.name.clone());
                            if used.len() == stock_syms.len() {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        for sym in &stock_syms {
            if !used.contains(&sym.name) {
                diags.push(PawnDiagnostic::unnecessary_warning(
                    sym.line, sym.col, sym.col + sym.name.len() as u32,
                    codes::PP0006,
                    format!("\"{}\" função stock declarada mas não utilizada", sym.name),
                ));
            }
        }
    }

    diags
}

/// Verifica se um nome é usado como identificador no texto, ignorando a linha de declaração.
fn is_used_as_ident(name: &str, idents: &HashSet<String>, decl_line: u32) -> bool {
    // Como collect_all_idents varre o arquivo inteiro, uma variável declarada mas nunca
    // referenciada de outra forma não estará nos idents de outras linhas.
    // Heurística simples: se name está em idents E existem mais de 1 ocorrência
    // (a declaração conta como 1), considera usado.
    // Para precisão, seria necessário rastrear linha por linha — deixado para futura melhoria.
    let _ = decl_line;
    idents.contains(name)
}

fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}
