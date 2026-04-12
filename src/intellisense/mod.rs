mod completion;
mod codelens;
mod hover;
mod signature;

pub use completion::get_completions;
pub use codelens::get_code_lens;
pub use hover::get_hover;
pub use signature::get_signature_help;

use std::path::{Path, PathBuf};

use crate::analyzer::includes::collect_included_files;
use crate::parser::types::Symbol;
use crate::parser::ParsedFile;
use crate::workspace::WorkspaceState;

// ─── Helpers compartilhados ───────────────────────────────────────────────────

/// Extrai o identificador (palavra) na posição `col` de uma linha.
pub(crate) fn extract_word(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';

    // Posiciona dentro dos limites
    let mut start = col.min(chars.len());
    // Se col aponta para após o último char ou para um não-ident, recua um
    if start == chars.len() || !is_ident(chars[start]) {
        if start == 0 {
            return None;
        }
        start -= 1;
        if !is_ident(chars[start]) {
            return None;
        }
    }
    while start > 0 && is_ident(chars[start - 1]) {
        start -= 1;
    }
    let mut end = start;
    while end < chars.len() && is_ident(chars[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some(chars[start..end].iter().collect())
}

/// Coleta todos os símbolos do arquivo atual + includes (transitivam., com cache).
pub(crate) fn collect_all_symbols(
    state: &WorkspaceState,
    file_path: &Path,
    inc_paths: &[PathBuf],
    parsed: &ParsedFile,
) -> Vec<Symbol> {
    let mut all = parsed.symbols.clone();
    let included = collect_included_files(file_path, inc_paths, &parsed.includes, 10, 200);
    for inc_path in &included {
        if let Some(inc_parsed) = state.get_parsed_by_path(inc_path) {
            all.extend(inc_parsed.symbols);
        }
    }
    all
}

