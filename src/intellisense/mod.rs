mod codelens;
mod completion;
mod docs;
mod format_engine;
mod format_indent;
mod format_style;
mod formatter;
mod hover;
mod quickfix;
mod references;
mod rename;
mod semantic_tokens;
mod signature;

pub use codelens::get_code_lens;
pub use completion::{MAX_COMPLETION_ITEMS, get_at_completions, get_completions};
pub use docs::{DocLabels, parse_doc};
pub use format_style::{BracePlacement, FormatStyle, Preset};
pub use formatter::{format_document, format_range};
pub use hover::get_hover;
pub use quickfix::{RemovalKind, removal_kind, removal_range};
pub use references::get_references;
pub use rename::{get_rename, prepare_rename};
pub use semantic_tokens::{get_semantic_tokens, semantic_tokens_legend};
pub use signature::get_signature_help;

use std::path::{Path, PathBuf};

use crate::analyzer::includes::collect_included_files;
use crate::parser::ParsedFile;
use crate::parser::types::Symbol;
use crate::workspace::WorkspaceState;

pub(crate) fn extract_word(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';

    let mut start = col.min(chars.len());
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

/// O símbolo conhecido mais parecido com `name`, para sugerir em PP0010.
///
/// Considera os símbolos do arquivo e de todos os includes transitivos — é o
/// mesmo universo que o autocomplete oferece, então a sugestão nunca aponta
/// para algo que o arquivo não enxerga.
pub fn suggest_symbol(
    state: &WorkspaceState,
    file_path: &Path,
    inc_paths: &[PathBuf],
    parsed: &ParsedFile,
    name: &str,
) -> Option<String> {
    let all = collect_all_symbols(state, file_path, inc_paths, parsed);
    let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
    crate::similar::closest(name, names).map(str::to_string)
}

pub(crate) fn collect_all_symbols(
    state: &WorkspaceState,
    file_path: &Path,
    inc_paths: &[PathBuf],
    parsed: &ParsedFile,
) -> Vec<Symbol> {
    let mut all = parsed.symbols.clone();
    let resolved = collect_included_files(file_path, inc_paths, &parsed.includes, 16, 1000);
    for inc_path in &resolved.paths {
        if let Some(entry) = resolved.files.get(inc_path) {
            all.extend(entry.parsed.symbols.clone());
        } else if let Some(inc_parsed) = state.get_parsed_by_path(inc_path) {
            all.extend(inc_parsed.symbols.clone());
        }
    }
    all
}
