use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::parser::{parse_file, IncludeDirective, ParsedFile};
use crate::parser::lexer::decode_bytes;

use super::{codes, diagnostic::PawnDiagnostic};

#[derive(Clone)]
pub struct IncludeEntry {
    pub text: String,
    pub parsed: ParsedFile,
}

pub struct ResolvedIncludes {
    pub paths: Vec<PathBuf>,
    pub files: HashMap<PathBuf, IncludeEntry>,
}

// Quotes search relative to the current file first, then fall back to include_paths —
// matching the Pawn compiler's own resolution order.
pub fn resolve_include(
    directive: &IncludeDirective,
    file_dir: &Path,
    include_paths: &[PathBuf],
) -> Option<PathBuf> {
    let token = &directive.token;

    if directive.is_angle {
        for base in include_paths {
            if let Some(p) = try_resolve(base.join(token)) {
                return Some(p);
            }
        }
    } else {
        if let Some(p) = try_resolve(file_dir.join(token)) {
            return Some(p);
        }
        for base in include_paths {
            if let Some(p) = try_resolve(base.join(token)) {
                return Some(p);
            }
        }
    }
    None
}

// On Linux, performs a case-insensitive directory scan when the exact path fails,
// covering mismatched casing like `evf.inc` vs `EVF.inc`.
fn try_resolve(path: PathBuf) -> Option<PathBuf> {
    if path.exists() {
        return Some(path);
    }
    let with_inc = PathBuf::from(format!("{}.inc", path.to_string_lossy()));
    if with_inc.exists() {
        return Some(with_inc);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let candidates = [&path, &with_inc];
        for candidate in candidates {
            if let (Some(parent), Some(file_name)) = (candidate.parent(), candidate.file_name()) {
                let needle = file_name.to_string_lossy().to_ascii_lowercase();
                if let Ok(entries) = std::fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        let entry_name = entry.file_name();
                        if entry_name.to_string_lossy().to_ascii_lowercase() == needle {
                            return Some(entry.path());
                        }
                    }
                }
            }
        }
    }

    None
}

pub fn analyze_includes(
    directives: &[IncludeDirective],
    file_path: &Path,
    include_paths: &[PathBuf],
    workspace_root: Option<&Path>,
) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let file_dir = file_path.parent().unwrap_or(Path::new("."));

    for dir in directives {
        let col_end = dir.col + dir.token.len() as u32;
        match resolve_include(dir, file_dir, include_paths) {
            None => {
                if dir.is_try {
                    // #tryinclude não resolvido é informativo, não um erro
                    diags.push(PawnDiagnostic::hint(
                        dir.line, dir.col, col_end,
                        codes::PP0013,
                        format!("\"{}\" não encontrado — #tryinclude ignorado pelo compilador", dir.token),
                    ));
                } else {
                    let msg = build_not_found_message(dir, file_dir, include_paths);
                    diags.push(PawnDiagnostic::error(dir.line, dir.col, col_end, codes::PP0001, msg));
                }
            }
            Some(resolved) => {
                // Sinaliza apenas se o caminho resolvido escapa o workspace root
                if let Some(root) = workspace_root {
                    let canon = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
                    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
                    if !canon.starts_with(&root_canon) {
                        diags.push(PawnDiagnostic::error(
                            dir.line, dir.col, col_end,
                            codes::PP0001,
                            format!("\"{}\" aponta para fora do workspace", dir.token),
                        ));
                    }
                }
            }
        }
    }
    diags
}

fn build_not_found_message(
    dir: &IncludeDirective,
    file_dir: &Path,
    include_paths: &[PathBuf],
) -> String {
    let mut msg = format!("Include não encontrada: \"{}\"", dir.token);
    msg.push_str(&format!(" (tentou também: {}.inc)", dir.token));
    if dir.is_angle {
        if include_paths.is_empty() {
            msg.push_str(". Nenhum includePaths configurado.");
        } else {
            let paths: Vec<String> = include_paths
                .iter()
                .take(2)
                .map(|p| p.display().to_string())
                .collect();
            let suffix = if include_paths.len() > 2 { "..." } else { "" };
            msg.push_str(&format!(". Buscado em: {}{}", paths.join(", "), suffix));
        }
    } else {
        msg.push_str(&format!(". Caminho relativo a: {}", file_dir.display()));
    }
    msg
}

// BFS ensures direct includes are processed before transitive ones,
// so max_files never cuts off first-level dependencies.
pub fn collect_included_files(
    file_path: &Path,
    include_paths: &[PathBuf],
    directives: &[IncludeDirective],
    max_depth: usize,
    max_files: usize,
) -> ResolvedIncludes {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let mut files: HashMap<PathBuf, IncludeEntry> = HashMap::new();
    let file_dir = file_path.parent().unwrap_or(Path::new(".")).to_path_buf();

    let mut queue: std::collections::VecDeque<(Vec<IncludeDirective>, PathBuf, usize)> =
        std::collections::VecDeque::new();
    queue.push_back((directives.to_vec(), file_dir, 1));

    while let Some((dirs, dir, depth)) = queue.pop_front() {
        for directive in &dirs {
            if out.len() >= max_files {
                break;
            }
            let Some(resolved) = resolve_include(directive, &dir, include_paths) else { continue };
            let norm = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
            if seen.contains(&norm) {
                continue;
            }
            seen.insert(norm.clone());
            out.push(norm.clone());

            if depth < max_depth {
                let entry = files.entry(norm.clone()).or_insert_with(|| {
                    let bytes = std::fs::read(&resolved).unwrap_or_default();
                    let text = decode_bytes(&bytes);
                    let parsed = parse_file(&text);
                    IncludeEntry { text, parsed }
                });
                let nested = entry.parsed.includes.clone();
                let nested_dir = resolved
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_path_buf();
                queue.push_back((nested, nested_dir, depth + 1));
            }
        }
    }

    ResolvedIncludes { paths: out, files }
}
