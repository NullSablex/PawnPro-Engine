use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::messages::{Locale, MsgKey, msg};
use crate::parser::lexer::{has_inline_deprecated, strip_line_comments};
use crate::parser::types::IncludeDirective;
use crate::parser::{ParsedFile, SymbolKind};
use crate::util::to_u32;

use super::includes::{ResolvedIncludes, resolve_include};
use super::{codes, diagnostic::PawnDiagnostic};

#[derive(Clone)]
enum DepKind {
    Individual,
    FromFile,
}

#[derive(Clone)]
struct DepEntry {
    kind: DepKind,
    /// Linha (0-based) da declaração no arquivo atual — `None` para símbolos de includes.
    decl_line: Option<u32>,
}

static RX_DEPRECATED: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(?://\s*@DEPRECATED|/\*\s*@DEPRECATED\s*\*/)\s*$").unwrap()
});
static RX_INCLUDE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"^\s*#\s*include\s*(?:<([^>]+)>|"([^"]+)")"#).unwrap()
});
static RX_CALL: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"\b([A-Za-z_]\w*)\s*\(").unwrap());
static RX_IDENT: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"\b([A-Za-z_]\w*)\b").unwrap());
// Matches declaration prefixes to skip the symbol's own declaration line when scanning usages.
static RX_DECL_PREFIX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"\b(?:stock|native|public|forward|static)\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*$",
    )
    .unwrap()
});

/// Varre as linhas em busca de `#include` marcados como `@DEPRECATED` (na linha
/// anterior ou inline), emite o diagnóstico PP0008 e devolve o conjunto de
/// arquivos resolvidos como descontinuados, para propagar a marcação aos símbolos.
fn collect_deprecated_includes(
    lines: &[&str],
    file_dir: &Path,
    include_paths: &[PathBuf],
    locale: Locale,
    diags: &mut Vec<PawnDiagnostic>,
) -> HashSet<PathBuf> {
    let mut deprecated_files: HashSet<PathBuf> = HashSet::new();
    let mut pending_deprecated = false;
    let mut in_block = false;
    for (line_idx, raw_line) in lines.iter().enumerate() {
        let raw_line = raw_line.trim_end_matches('\r');

        if RX_DEPRECATED.is_match(raw_line) {
            pending_deprecated = true;
            let s = strip_line_comments(raw_line, in_block);
            in_block = s.in_block;
            continue;
        }

        let stripped = strip_line_comments(raw_line, in_block);
        in_block = stripped.in_block;
        let line = &stripped.text;
        if line.trim().is_empty() {
            continue;
        }

        let inline_deprecated = has_inline_deprecated(raw_line);
        if (pending_deprecated || inline_deprecated)
            && let Some(cap) = RX_INCLUDE.captures(line)
        {
            let token = cap.get(1).or(cap.get(2)).map_or("", |m| m.as_str().trim());
            let is_angle = cap.get(1).is_some();
            let dir = IncludeDirective {
                token: token.to_string(),
                is_angle,
                is_try: false,
                line: to_u32(line_idx),
                col: 0,
            };
            let col = to_u32(raw_line.find(token).unwrap_or(0));
            diags.push(PawnDiagnostic::warning(
                to_u32(line_idx),
                col,
                col + to_u32(token.len()),
                codes::PP0008,
                msg(locale, MsgKey::IncludeDeprecated).replace("{}", token),
            ));
            if let Some(resolved_path) = resolve_include(&dir, file_dir, include_paths) {
                let canon = resolved_path.canonicalize().unwrap_or(resolved_path);
                deprecated_files.insert(canon);
            }
        }
        pending_deprecated = false;
    }
    deprecated_files
}

/// Coleta `#include`s e símbolos descontinuados (locais e de includes resolvidos)
/// e emite os diagnósticos PP0007/PP0008. As fases de varredura por linha já
/// foram extraídas (`collect_deprecated_includes`, `emit_deprecated_usages`); o
/// que resta é o cruzamento entre símbolos do arquivo e dos includes, sequencial
/// e legível — daí o `allow` em vez de fragmentar a montagem dos mapas.
#[allow(clippy::too_many_lines)]
pub fn analyze_deprecated(
    text: &str,
    file_path: &Path,
    parsed: &ParsedFile,
    include_paths: &[PathBuf],
    resolved: &ResolvedIncludes,
    locale: Locale,
) -> Vec<PawnDiagnostic> {
    let mut diags = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let file_dir = file_path.parent().unwrap_or(Path::new("."));

    let deprecated_files =
        collect_deprecated_includes(&lines, file_dir, include_paths, locale, &mut diags);

    let mut dep_callables: HashMap<String, DepEntry> = HashMap::new();
    let mut dep_vars: HashMap<String, DepEntry> = HashMap::new();
    let mut dep_macros: HashMap<String, DepEntry> = HashMap::new();
    let mut deprecated_forward_names: HashSet<String> = HashSet::new();
    let mut deprecated_public_names: HashSet<String> = HashSet::new();

    for sym in &parsed.symbols {
        if sym.deprecated {
            classify_sym(
                &mut dep_callables,
                &mut dep_vars,
                &mut dep_macros,
                sym,
                DepKind::Individual,
                Some(sym.line),
            );
            if sym.kind == SymbolKind::Forward {
                deprecated_forward_names.insert(sym.name.clone());
            }
            if sym.kind == SymbolKind::Public {
                deprecated_public_names.insert(sym.name.clone());
            }
        }
    }
    for m in &parsed.deprecated_macros {
        dep_macros.entry(m.clone()).or_insert(DepEntry {
            kind: DepKind::Individual,
            decl_line: None,
        });
    }

    for sym in &parsed.symbols {
        if sym.kind == SymbolKind::Forward && deprecated_public_names.contains(&sym.name) {
            dep_callables.entry(sym.name.clone()).or_insert(DepEntry {
                kind: DepKind::Individual,
                decl_line: Some(sym.line),
            });
        }
    }

    let _ = include_paths;
    for fp in &resolved.paths {
        let canon = fp.canonicalize().unwrap_or(fp.clone());
        let all_deprecated = deprecated_files.contains(&canon);

        if let Some(entry) = resolved
            .files
            .get(fp)
            .or_else(|| resolved.files.get(&canon))
        {
            for sym in &entry.parsed.symbols {
                let kind = if all_deprecated && !sym.deprecated {
                    DepKind::FromFile
                } else if sym.deprecated {
                    DepKind::Individual
                } else {
                    continue;
                };
                if sym.kind == SymbolKind::Forward && matches!(kind, DepKind::Individual) {
                    deprecated_forward_names.insert(sym.name.clone());
                }
                // Símbolos de includes não têm linha de declaração no arquivo atual.
                classify_sym(
                    &mut dep_callables,
                    &mut dep_vars,
                    &mut dep_macros,
                    sym,
                    kind,
                    None,
                );
            }
            for m in &entry.parsed.deprecated_macros {
                let kind = if all_deprecated {
                    DepKind::FromFile
                } else {
                    DepKind::Individual
                };
                dep_macros.entry(m.clone()).or_insert(DepEntry {
                    kind,
                    decl_line: None,
                });
            }
        }
    }

    for sym in &parsed.symbols {
        if sym.kind == SymbolKind::Public && deprecated_forward_names.contains(&sym.name) {
            dep_callables.entry(sym.name.clone()).or_insert(DepEntry {
                kind: DepKind::Individual,
                decl_line: Some(sym.line),
            });
        }
    }

    for sym in &parsed.symbols {
        let is_deprecated = sym.deprecated
            || (sym.kind == SymbolKind::Public && deprecated_forward_names.contains(&sym.name))
            || (sym.kind == SymbolKind::Forward && deprecated_public_names.contains(&sym.name));
        if !is_deprecated {
            continue;
        }
        let col_end = sym.col + to_u32(sym.name.len());
        diags.push(PawnDiagnostic::deprecated_decl(
            sym.line,
            sym.col,
            col_end,
            codes::PP0007,
            msg(locale, MsgKey::SymDeprecated).replace("{}", &sym.name),
        ));
    }

    let any = !dep_callables.is_empty() || !dep_vars.is_empty() || !dep_macros.is_empty();
    if any {
        emit_deprecated_usages(
            &lines,
            &dep_callables,
            &dep_vars,
            &dep_macros,
            locale,
            &mut diags,
        );
    }

    diags
}

/// Percorre as linhas emitindo PP0007 para cada uso de um símbolo descontinuado
/// (chamável, variável ou macro), ignorando a própria linha de declaração e
/// contextos que não são usos reais (ex.: prefixo de declaração).
fn emit_deprecated_usages(
    lines: &[&str],
    dep_callables: &HashMap<String, DepEntry>,
    dep_vars: &HashMap<String, DepEntry>,
    dep_macros: &HashMap<String, DepEntry>,
    locale: Locale,
    diags: &mut Vec<PawnDiagnostic>,
) {
    let mut in_block = false;
    for (line_idx, raw_line) in lines.iter().enumerate() {
        let raw_line = raw_line.trim_end_matches('\r');
        let stripped = strip_line_comments(raw_line, in_block);
        in_block = stripped.in_block;
        let line = &stripped.text;
        if line.trim().is_empty() {
            continue;
        }
        let is_directive = line.trim_start().starts_with('#');

        if !dep_callables.is_empty() {
            for cap in RX_CALL.captures_iter(line) {
                let name = &cap[1];
                let Some(entry) = dep_callables.get(name) else {
                    continue;
                };
                if entry.decl_line == Some(to_u32(line_idx)) {
                    continue;
                }
                let before = &line[..cap.get(0).unwrap().start()];
                if RX_DECL_PREFIX.is_match(before) {
                    continue;
                }
                let col = to_u32(raw_line.find(name).unwrap_or(0));
                diags.push(PawnDiagnostic::deprecated_warning(
                    to_u32(line_idx),
                    col,
                    col + to_u32(name.len()),
                    codes::PP0007,
                    dep_msg(name, &entry.kind, locale),
                ));
            }
        }

        if (!dep_vars.is_empty() || !dep_macros.is_empty()) && !is_directive {
            for cap in RX_IDENT.captures_iter(line) {
                let name = &cap[1];
                let entry = dep_vars.get(name).or_else(|| dep_macros.get(name));
                let Some(entry) = entry else { continue };
                if entry.decl_line == Some(to_u32(line_idx)) {
                    continue;
                }
                let col = to_u32(cap.get(1).unwrap().start());
                diags.push(PawnDiagnostic::deprecated_warning(
                    to_u32(line_idx),
                    col,
                    col + to_u32(name.len()),
                    codes::PP0007,
                    dep_msg(name, &entry.kind, locale),
                ));
            }
        }
    }
}

fn classify_sym(
    callables: &mut HashMap<String, DepEntry>,
    vars: &mut HashMap<String, DepEntry>,
    macros: &mut HashMap<String, DepEntry>,
    sym: &crate::parser::types::Symbol,
    kind: DepKind,
    decl_line: Option<u32>,
) {
    let entry = DepEntry { kind, decl_line };
    match sym.kind {
        SymbolKind::Define => {
            macros.entry(sym.name.clone()).or_insert(entry);
        }
        SymbolKind::Variable => {
            vars.entry(sym.name.clone()).or_insert(entry);
        }
        _ => {
            callables.entry(sym.name.clone()).or_insert(entry);
        }
    }
}

fn dep_msg(name: &str, kind: &DepKind, locale: Locale) -> String {
    match kind {
        DepKind::Individual => msg(locale, MsgKey::SymDeprecatedUsage).replace("{}", name),
        DepKind::FromFile => msg(locale, MsgKey::SymFromDeprecatedFile).replace("{}", name),
    }
}
