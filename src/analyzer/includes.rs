use std::path::{Path, PathBuf};

use crate::parser::IncludeDirective;

use super::{codes, diagnostic::PawnDiagnostic};

/// Tenta resolver um token de include para um caminho de arquivo real.
/// - `<token>` → busca em `include_paths`; tenta adicionar `.inc` se sem extensão
/// - `"path"` → resolve relativo a `file_dir`; respeita a extensão presente
pub fn resolve_include(
    directive: &IncludeDirective,
    file_dir: &Path,
    include_paths: &[PathBuf],
) -> Option<PathBuf> {
    let token = &directive.token;

    if directive.is_angle {
        // Busca em include_paths
        for base in include_paths {
            if let Some(p) = try_resolve(base.join(token)) {
                return Some(p);
            }
        }
    } else {
        // Relativo ao diretório do arquivo atual
        if let Some(p) = try_resolve(file_dir.join(token)) {
            return Some(p);
        }
    }
    None
}

/// Tenta o caminho como-está, depois com `.inc` adicionado.
fn try_resolve(path: PathBuf) -> Option<PathBuf> {
    if path.exists() {
        return Some(path);
    }
    // Tenta adicionar .inc se não tem extensão
    if path.extension().is_none() {
        let with_inc = path.with_extension("inc");
        if with_inc.exists() {
            return Some(with_inc);
        }
    }
    None
}

/// Analisa as diretivas #include de um arquivo e gera diagnósticos para os não encontrados.
pub fn analyze_includes(
    directives: &[IncludeDirective],
    file_path: &Path,
    include_paths: &[PathBuf],
) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let file_dir = file_path.parent().unwrap_or(Path::new("."));

    for dir in directives {
        if resolve_include(dir, file_dir, include_paths).is_none() {
            let msg = build_not_found_message(dir, file_dir, include_paths);
            let col_end = dir.col + dir.token.len() as u32;
            diags.push(PawnDiagnostic::error(dir.line, dir.col, col_end, codes::PP0001, msg));
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
    if Path::new(&dir.token).extension().is_none() {
        msg.push_str(&format!(" (tentou: {}.inc)", dir.token));
    }
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

/// Resolve todas as includes recursivamente e retorna os caminhos.
pub fn collect_included_files(
    file_path: &Path,
    include_paths: &[PathBuf],
    directives: &[IncludeDirective],
    max_depth: usize,
    max_files: usize,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let file_dir = file_path.parent().unwrap_or(Path::new("."));

    collect_recursive(directives, file_dir, include_paths, &mut out, &mut seen, 1, max_depth, max_files);
    out
}

fn collect_recursive(
    directives: &[IncludeDirective],
    file_dir: &Path,
    include_paths: &[PathBuf],
    out: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
    depth: usize,
    max_depth: usize,
    max_files: usize,
) {
    if out.len() >= max_files {
        return;
    }
    for dir in directives {
        let Some(resolved) = resolve_include(dir, file_dir, include_paths) else { continue };
        let norm = resolved.canonicalize().unwrap_or(resolved.clone());
        if seen.contains(&norm) {
            continue;
        }
        seen.insert(norm.clone());
        out.push(resolved.clone());

        if depth < max_depth {
            // Lê e parseia o arquivo incluído para seguir suas includes
            if let Ok(text) = std::fs::read(&resolved) {
                let text = decode_text(&text);
                let nested = crate::parser::parse_file(&text);
                let nested_dir = resolved.parent().unwrap_or(Path::new("."));
                collect_recursive(&nested.includes, nested_dir, include_paths, out, seen, depth + 1, max_depth, max_files);
            }
        }
    }
}

/// Decodifica bytes como UTF-8 (com fallback para latin-1 se muitos erros).
fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}
