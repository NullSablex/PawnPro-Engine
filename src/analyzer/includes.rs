use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::messages::{msg, Locale, MsgKey};
use crate::parser::lexer::decode_bytes;
use crate::parser::{parse_file, IncludeDirective, ParsedFile};

use super::{codes, diagnostic::PawnDiagnostic};

#[derive(Clone)]
pub struct IncludeEntry {
    pub text: String,
    pub parsed: ParsedFile,
}

pub struct ResolvedIncludes {
    pub paths: Vec<PathBuf>,
    pub files: HashMap<PathBuf, IncludeEntry>,
    /// Maps each resolved include path to the set of file paths that directly include it.
    pub reverse_deps: HashMap<PathBuf, HashSet<PathBuf>>,
}

// Extensions tried in order, mirroring the real compiler (sc2.c plungequalifiedfile):
// exact path first, then .inc, .p, .pawn, .pwn
static EXTENSIONS: &[&str] = &["", ".inc", ".p", ".pawn", ".pwn"];

// Quotes search relative to the current file first, then fall back to include_paths —
// matching the Pawn compiler's own resolution order.
pub fn resolve_include(
    directive: &IncludeDirective,
    file_dir: &Path,
    include_paths: &[PathBuf],
) -> Option<PathBuf> {
    let token = &directive.token;

    if directive.is_angle {
        include_paths.iter().find_map(|base| try_resolve(base.join(token)))
    } else {
        try_resolve(file_dir.join(token))
            .or_else(|| include_paths.iter().find_map(|base| try_resolve(base.join(token))))
    }
}

// On Linux, performs a case-insensitive directory scan when the exact path fails,
// covering mismatched casing like `evf.inc` vs `EVF.inc`.
fn try_resolve(path: PathBuf) -> Option<PathBuf> {
    let base = path.to_string_lossy();

    for ext in EXTENSIONS {
        let candidate = if ext.is_empty() {
            path.clone()
        } else {
            PathBuf::from(format!("{}{}", base, ext))
        };

        if candidate.exists() {
            return Some(candidate);
        }

        #[cfg(not(target_os = "windows"))]
        if let (Some(parent), Some(file_name)) = (candidate.parent(), candidate.file_name()) {
            let needle = file_name.to_string_lossy().to_ascii_lowercase();
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    if entry.file_name().to_string_lossy().to_ascii_lowercase() == needle {
                        return Some(entry.path());
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
    locale: Locale,
) -> Vec<PawnDiagnostic> {
    let _ = workspace_root;
    let file_dir = file_path.parent().unwrap_or(Path::new("."));

    directives
        .iter()
        .filter_map(|dir| {
            let col_end = dir.col + dir.token.len() as u32;
            if resolve_include(dir, file_dir, include_paths).is_some() {
                return None;
            }
            let diag = if dir.is_try {
                PawnDiagnostic::hint(
                    dir.line, dir.col, col_end,
                    codes::PP0013,
                    msg(locale, MsgKey::TryIncludeNotFound).replace("{}", &dir.token),
                )
            } else {
                let message = build_not_found_message(dir, file_dir, include_paths, locale);
                PawnDiagnostic::error(dir.line, dir.col, col_end, codes::PP0001, message)
            };
            Some(diag)
        })
        .collect()
}

fn build_not_found_message(
    dir: &IncludeDirective,
    file_dir: &Path,
    include_paths: &[PathBuf],
    locale: Locale,
) -> String {
    let mut out = msg(locale, MsgKey::IncludeNotFound).replace("{}", &dir.token);
    out.push_str(&msg(locale, MsgKey::IncludeTried).replace("{}", &dir.token));

    if dir.is_angle {
        if include_paths.is_empty() {
            out.push_str(msg(locale, MsgKey::IncludeNoPathsConfigured));
        } else {
            let paths: Vec<String> = include_paths
                .iter()
                .take(2)
                .map(|p| p.display().to_string())
                .collect();
            let suffix = if include_paths.len() > 2 { "..." } else { "" };
            let template = msg(locale, MsgKey::IncludeSearchedIn);
            out.push_str(&template.replacen("{}", &paths.join(", "), 1).replacen("{}", suffix, 1));
        }
    } else {
        out.push_str(&msg(locale, MsgKey::IncludeRelativeTo).replace("{}", &file_dir.display().to_string()));
    }

    out
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
    let mut ordered: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut files: HashMap<PathBuf, IncludeEntry> = HashMap::new();
    let mut reverse_deps: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();

    let root_canon = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
    let file_dir = file_path.parent().unwrap_or(Path::new(".")).to_path_buf();

    let mut queue: VecDeque<(Vec<IncludeDirective>, PathBuf, PathBuf, usize)> =
        VecDeque::new();
    queue.push_back((directives.to_vec(), file_dir, root_canon, 1));

    while let Some((dirs, dir, parent_canon, depth)) = queue.pop_front() {
        for directive in &dirs {
            if ordered.len() >= max_files {
                break;
            }

            let Some(resolved) = resolve_include(directive, &dir, include_paths) else {
                continue;
            };
            let norm = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

            reverse_deps
                .entry(norm.clone())
                .or_default()
                .insert(parent_canon.clone());

            if seen.contains(&norm) {
                continue;
            }
            seen.insert(norm.clone());
            ordered.push(norm.clone());

            if depth < max_depth {
                let entry = files.entry(norm.clone()).or_insert_with(|| {
                    let bytes = std::fs::read(&resolved).unwrap_or_default();
                    let text = decode_bytes(&bytes);
                    let parsed = parse_file(&text);
                    IncludeEntry { text, parsed }
                });
                let nested_dirs = entry.parsed.includes.clone();
                let nested_dir = resolved
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_path_buf();
                queue.push_back((nested_dirs, nested_dir, norm, depth + 1));
            }
        }
    }

    ResolvedIncludes { paths: ordered, files, reverse_deps }
}
