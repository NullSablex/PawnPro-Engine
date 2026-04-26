mod completion;
mod codelens;
mod formatter;
mod hover;
mod references;
mod semantic_tokens;
mod signature;

pub use completion::{get_at_completions, get_completions};
pub use codelens::get_code_lens;
pub use formatter::{format_document, format_range, FormatOptions};
pub use hover::get_hover;
pub use references::get_references;
pub use semantic_tokens::{get_semantic_tokens, semantic_tokens_legend};
pub use signature::get_signature_help;

use std::path::{Path, PathBuf};

use crate::analyzer::includes::collect_included_files;
use crate::parser::types::Symbol;
use crate::parser::ParsedFile;
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
            all.extend(inc_parsed.symbols);
        }
    }
    all
}
