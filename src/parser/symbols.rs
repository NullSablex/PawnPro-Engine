use once_cell::sync::Lazy;
use regex::Regex;

use super::{
    lexer::{has_inline_deprecated, strip_line_comments, update_brace_depth},
    types::{IncludeDirective, Param, ParsedFile, Symbol, SymbolKind},
};


// ─── Regex compilados (compilados uma única vez) ───────────────────────────

static RX_DEPRECATED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?://\s*@DEPRECATED|/\*\s*@DEPRECATED\s*\*/)\s*$").unwrap());

static RX_NATIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:forward\s+)?native\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

static RX_FORWARD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*forward\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// (public|stock) — cobre todos os casos:
//   stock Func(...)
//   stock DOF2::Func(...)         — namespace sem tag de retorno
//   stock Float: DOF2::Func(...)  — tag de retorno ANTES do namespace
//   stock bool: DOF2::Func(...)
// grupo 1: keyword (public|stock)
// grupo 2: namespace (ex: "DOF2::" ou vazio)
// grupo 3: nome da função
// grupo 4: params raw
static RX_PUBLIC_STOCK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(public|stock)(?:\s+(?:public|stock))?\s+(?:[A-Za-z_]\w*:)?\s*((?:[A-Za-z_]\w*::)*)(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// static stock [Tag:]Name(params) — deve ser testado ANTES de RX_STATIC_FUNC
static RX_STATIC_STOCK_FUNC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*static\s+stock\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// static [Tag:]Name(params) — funções com corpo
static RX_STATIC_FUNC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*static\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// static const Name ou static const Name[...] (array/constante)
static RX_STATIC_CONST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*static\s+const\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\b").unwrap()
});

// stock const [Tag:]Name — constante exportada
static RX_STOCK_CONST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*stock\s+const\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\b").unwrap()
});

// float/bool Name(params) — funções com tipo de retorno sem ":"
static RX_FLOAT_BOOL_FUNC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(float|bool)\s+([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// Função sem keyword ou com namespace: [Namespace::]* [Tag:] Name(params)
// Usado como fallback após todos os padrões com keyword falharem.
// grupo 1: namespace (ex: "BPLR::" ou vazio), grupo 2: nome, grupo 3: params
static RX_PLAIN_FUNC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*((?:[A-Za-z_]\w*::)*)(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// #define Name — captura o nome e tudo que vem depois (corpo da macro)
static RX_DEFINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)\b(.*)$").unwrap());

// Detecta macros do tipo PREFIX::%0(...) ou PREFIX:%0(...)
// — indicam que o prefixo é gerador de função quando o corpo tem forward/public.
static RX_MACRO_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)(?:::?)\s*%\d").unwrap());

// Detecta alias de namespace numa única linha: #define NS:: PREFIX_
static RX_NAMESPACE_ALIAS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)::\s+([A-Za-z_]\w*)").unwrap());

// Detecta abertura de alias de namespace com continuação de linha:
// `#define NS:: \` — o alias está na próxima linha
static RX_NAMESPACE_ALIAS_CONT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)::\s*\\").unwrap());

// Detecta abertura de função com qualquer keyword (ou sem keyword) com ( sem ) na mesma linha.
// Cobre: stock, public, static, static stock, native, forward, e funções plain.
// Grupo 1: keywords (pode ser vazio), grupo 2: namespace, grupo 3: nome, grupo 4: params parciais após (
static RX_FUNC_OPEN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*((?:(?:public|stock|static|native|forward)\s+)*)?(?:[A-Za-z_]\w*:)?\s*((?:[A-Za-z_]\w*::)*)(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)$").unwrap()
});

// enum [Tag:] [Name] [(opts)] [{] — grupo 1: nome opcional do enum
static RX_ENUM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*enum\s+(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)").unwrap());

// Membro de enum: [Tag:]Name (dentro do corpo)
static RX_ENUM_MEMBER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)").unwrap());

// #include / #tryinclude <token> ou "token" — grupo 1: "try", grupo 2: <token>, grupo 3: "token"
static RX_INCLUDE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*#\s*(try)?include\s*(?:<([^>]+)>|"([^"]+)")"#).unwrap());

// new/const com múltiplas variáveis: captura tudo após a keyword para split posterior
static RX_NEW_DECL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:new|const)\s+(.+)").unwrap()
});
// static como variável (fora de corpo): static [Tag:]name = ...;
static RX_STATIC_VAR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*static\s+(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)\s*(?:[=\[;])").unwrap()
});

// Palavras reservadas que nunca são nomes de variáveis
static RESERVED: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
    [
        "true", "false", "null", "sizeof", "tagof", "Float", "bool", "char",
        "String", "new", "static", "const", "native", "forward", "public",
        "stock", "return", "if", "else", "for", "while", "do", "switch",
        "case", "break", "continue", "default",
    ]
    .into_iter()
    .collect()
});

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Divide parâmetros respeitando parênteses/colchetes aninhados.
pub fn split_params(raw: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut in_char = false;
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        let prev = if i > 0 { bytes[i - 1] } else { 0 };
        if ch == b'"' && !in_char && prev != b'\\' {
            in_str = !in_str;
            cur.push(ch as char);
        } else if ch == b'\'' && !in_str && prev != b'\\' {
            in_char = !in_char;
            cur.push(ch as char);
        } else if in_str || in_char {
            cur.push(ch as char);
        } else if ch == b'(' || ch == b'[' || ch == b'{' {
            depth += 1;
            cur.push(ch as char);
        } else if ch == b')' || ch == b']' || ch == b'}' {
            depth = (depth - 1).max(0);
            cur.push(ch as char);
        } else if ch == b',' && depth == 0 {
            let t = cur.trim().to_string();
            if !t.is_empty() {
                params.push(t);
            }
            cur.clear();
        } else {
            cur.push(ch as char);
        }
        i += 1;
    }
    let t = cur.trim().to_string();
    if !t.is_empty() {
        params.push(t);
    }
    params
}

/// Extrai nomes de variáveis de um fragmento de declaração `new`/`const`/`static`,
/// suportando múltiplas variáveis separadas por vírgula e tags opcionais.
///
/// `"Tag:a, b[10], Tag:c = 0"` → `["a", "b", "c"]`
fn extract_var_names(raw: &str) -> Vec<String> {
    let raw = raw.trim_end_matches(';').trim();
    let parts = split_params(raw);
    let mut names = Vec::new();
    for part in parts {
        let p = part.trim();
        if p.is_empty() { continue; }
        let name_part = if let Some(c) = p.find(':') { p[c + 1..].trim() } else { p };
        let name: String = name_part
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
}

/// Analisa os parâmetros brutos e retorna Vec<Param>.
fn parse_params(raw: &str) -> Vec<Param> {
    let parts = split_params(raw);
    let mut params = Vec::new();

    for p in &parts {
        let t = p.trim();
        if t.is_empty() {
            continue;
        }
        if t == "..." || t.ends_with("...") {
            params.push(Param {
                name: "...".to_string(),
                tag: None,
                is_variadic: true,
            });
            continue;
        }
        // Extrai nome e tag: "const Float:x[]" → name="x", tag="Float"
        // Remove qualificadores: const, &
        let stripped = t
            .trim_start_matches("const")
            .trim_start_matches('&')
            .trim();
        // Tenta capturar "Tag:name" ou apenas "name"
        let (tag, name) = if let Some(colon) = stripped.find(':') {
            let tag_part = stripped[..colon].trim().to_string();
            let name_part = stripped[colon + 1..]
                .trim()
                .trim_end_matches(']')
                .trim_end_matches('[')
                .trim()
                .to_string();
            (Some(tag_part), name_part)
        } else {
            // Nome pode ter sufixo []
            let name_raw = stripped
                .split_whitespace()
                .last()
                .unwrap_or(stripped)
                .trim_end_matches(']')
                .trim_end_matches('[')
                .trim()
                .to_string();
            (None, name_raw)
        };
        // Mantém apenas a parte identificador do nome (sem sufixos)
        let name = name
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();
        if !name.is_empty() {
            params.push(Param { name, tag, is_variadic: false });
        }
    }

    params
}

/// Extrai o comentário de documentação acima de uma linha (busca para trás).
fn extract_doc(lines: &[&str], line_idx: usize) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut found = false;
    let mut i = line_idx as isize - 1;
    while i >= 0 {
        let l = lines[i as usize].trim();
        if l.is_empty() {
            if found {
                break;
            }
            i -= 1;
            continue;
        }
        if l.starts_with("//") {
            doc_lines.push(l.to_string());
            found = true;
        } else if l.ends_with("*/") {
            doc_lines.push(l.to_string());
            // busca o início do bloco
            let mut j = i - 1;
            while j >= 0 {
                let ll = lines[j as usize].trim();
                doc_lines.push(ll.to_string());
                if ll.contains("/*") {
                    break;
                }
                j -= 1;
            }
            break;
        } else {
            break;
        }
        i -= 1;
    }
    if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(doc_lines.join("\n"))
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

// ─── Helper de símbolo ────────────────────────────────────────────────────

/// Cria um símbolo de função e o adiciona ao ParsedFile.
#[allow(clippy::too_many_arguments)]
fn push_func(
    result: &mut ParsedFile,
    raw_lines: &[&str],
    raw_line: &str,
    line_idx: usize,
    name: String,
    params_raw: &str,
    kind: SymbolKind,
    deprecated: bool,
) {
    let params = parse_params(params_raw);
    let col = raw_line.find(name.as_str()).unwrap_or(0) as u32;
    result.symbols.push(Symbol {
        signature: Some(format!("{}({})", name, params_raw.trim())),
        name,
        kind,
        params,
        deprecated,
        doc: extract_doc(raw_lines, line_idx),
        line: line_idx as u32,
        col,
    });
}

// ─── Parser principal ──────────────────────────────────────────────────────

/// Faz o parse de um arquivo Pawn e retorna os símbolos e includes encontrados.
pub fn parse_file(text: &str) -> ParsedFile {
    let mut result = ParsedFile::default();
    let raw_lines: Vec<&str> = text.split('\n').collect();

    let mut in_block = false;
    let mut depth: i32 = 0;
    let mut pending_deprecated = false;
    // Candidato de função simples pendente quando { ainda não apareceu na linha
    // (line_idx, col, name, params_raw, deprecated, doc, kind)
    #[allow(clippy::type_complexity)]
    let mut pending_plain: Option<(u32, u32, String, String, bool, Option<String>, SymbolKind)> = None;
    // Função com assinatura multi-linha: params acumulados entre ( ... )
    // (line_idx, col, name, params_buf, kind, deprecated, doc)
    #[allow(clippy::type_complexity)]
    let mut multiline_func: Option<(u32, u32, String, String, SymbolKind, bool, Option<String>)> = None;
    // true quando estamos dentro de uma declaração multi-linha: static stock\n  var1,\n  var2;
    let mut in_multi_var_decl = false;
    // controle de corpo de enum
    let mut in_enum = false;
    let mut pending_enum_open = false;
    // Namespace alias com continuação de linha: `#define NS:: \` → próxima linha tem o alias
    let mut pending_ns_alias: Option<String> = None;

    for (line_idx, raw_line) in raw_lines.iter().enumerate() {
        // Remove \r do final (Windows line endings)
        let raw_line = raw_line.trim_end_matches('\r');

        // Verifica @DEPRECATED ANTES do strip (o comentário está no rawLine)
        if RX_DEPRECATED.is_match(raw_line) {
            pending_deprecated = true;
            let stripped = strip_line_comments(raw_line, in_block);
            in_block = stripped.in_block;
            depth = update_brace_depth(&stripped.text, depth);
            continue;
        }

        let stripped = strip_line_comments(raw_line, in_block);
        in_block = stripped.in_block;
        let line = &stripped.text;
        let trimmed = line.trim();

        // Acumula parâmetros de função com assinatura multi-linha
        if let Some(ref mut mf) = multiline_func {
            if let Some(close_pos) = line.find(')') {
                let partial = line[..close_pos].trim();
                if !partial.is_empty() {
                    if !mf.3.is_empty() { mf.3.push_str(", "); }
                    mf.3.push_str(partial);
                }
                let (pidx, pcol, pname, pparams, pkind, pdep, pdoc) = multiline_func.take().unwrap();
                let rest = &line[close_pos + 1..];
                if rest.contains('{') {
                    let params = parse_params(&pparams);
                    result.symbols.push(Symbol {
                        signature: Some(format!("{}({})", pname, pparams.trim())),
                        name: pname, kind: pkind, params, deprecated: pdep,
                        doc: pdoc, line: pidx, col: pcol,
                    });
                } else {
                    pending_plain = Some((pidx, pcol, pname, pparams, pdep, pdoc, pkind));
                }
            } else {
                let chunk = line.trim();
                if !chunk.is_empty() {
                    if !mf.3.is_empty() { mf.3.push_str(", "); }
                    mf.3.push_str(chunk);
                }
            }
            depth = update_brace_depth(line, depth);
            continue;
        }

        // Resolve namespace alias de continuação: linha anterior tinha `#define NS:: \`
        if let Some(ns) = pending_ns_alias.take() {
            let alias = trimmed.split_whitespace().next().unwrap_or("").trim_end_matches('\\').trim();
            if !alias.is_empty() && alias.chars().next().map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false) {
                result.namespace_aliases.insert(ns, alias.to_string());
            }
            depth = update_brace_depth(line, depth);
            continue;
        }

        if trimmed.is_empty() {
            // linha vazia: mantém pending_deprecated
            depth = update_brace_depth(line, depth);
            continue;
        }

        let top_level = depth == 0;

        if top_level {
            // ── Enum fechou: depth voltou a 0 ────────────────────────────────────
            if in_enum {
                in_enum = false;
            }

            // ── Abertura de enum pendente (enum Name\n{) ─────────────────────────
            if pending_enum_open {
                if trimmed.starts_with('{') {
                    in_enum = true;
                    pending_enum_open = false;
                    pending_plain = None;
                    depth = update_brace_depth(line, depth);
                    continue;
                }
                pending_enum_open = false; // linha inesperada, abandona
            }

            // ── Declarações multi-linha: static stock\n  Tag:var = val,\n  ...; ──
            if in_multi_var_decl {
                pending_plain = None;
                for name in extract_var_names(trimmed) {
                    if !RESERVED.contains(name.as_str()) {
                        let col = raw_line.find(&name).unwrap_or(0) as u32;
                        result.symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Variable,
                            signature: None,
                            params: vec![],
                            deprecated: pending_deprecated,
                            doc: None,
                            line: line_idx as u32,
                            col,
                        });
                    }
                }
                if trimmed.contains(';') || trimmed.contains('{') {
                    in_multi_var_decl = false;
                }
                depth = update_brace_depth(line, depth);
                continue;
            }

            // ── Início de declaração multi-linha (static stock / static / new isolados) ─
            if matches!(trimmed, "static" | "static stock" | "new") {
                in_multi_var_decl = true;
                depth = update_brace_depth(line, depth);
                continue;
            }

            // Resolve candidato de função simples cuja { aparece nesta linha
            if let Some((pidx, pcol, pname, pparams, pdep, pdoc, pkind)) = pending_plain.take()
                && trimmed.starts_with('{') {
                let params = parse_params(&pparams);
                result.symbols.push(Symbol {
                    signature: Some(format!("{}({})", pname, pparams.trim())),
                    name: pname,
                    kind: pkind,
                    params,
                    deprecated: pdep,
                    doc: pdoc,
                    line: pidx,
                    col: pcol,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // Válido apenas em maiúsculas e dentro de comentário (// ou /* */)
            let inline_deprecated = has_inline_deprecated(raw_line);
            let deprecated = pending_deprecated || inline_deprecated;
            pending_deprecated = false;

            // #include / #tryinclude
            if let Some(cap) = RX_INCLUDE.captures(line) {
                let is_try = cap.get(1).is_some();
                let (token, is_angle) = if let Some(m) = cap.get(2) {
                    (m.as_str().trim().to_string(), true)
                } else {
                    (cap.get(3).unwrap().as_str().trim().to_string(), false)
                };
                let col = raw_line.find(&token).unwrap_or(0) as u32;
                result.includes.push(IncludeDirective {
                    token,
                    is_angle,
                    is_try,
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // enum — captura nome e membros
            if let Some(enum_cap) = RX_ENUM.captures(line) {
                // Nome do enum como tipo/tag — kind dedicado para hover correto
                let enum_name = enum_cap.get(1).map(|m| m.as_str()).unwrap_or("");
                if !enum_name.is_empty() && !RESERVED.contains(enum_name) {
                    let col = raw_line.find(enum_name).unwrap_or(0) as u32;
                    result.symbols.push(Symbol {
                        name: enum_name.to_string(),
                        kind: SymbolKind::Enum,
                        signature: None, params: vec![],
                        deprecated, doc: extract_doc(&raw_lines, line_idx),
                        line: line_idx as u32, col,
                    });
                }
                // Enum numa única linha: enum { A, B, C } ou enum Name { A, B }
                if let Some(open) = line.find('{') {
                    let body_start = open + 1;
                    let body_end = line[body_start..].find('}')
                        .map(|i| body_start + i)
                        .unwrap_or(line.len());
                    let members_str = &line[body_start..body_end];
                    if members_str.trim().is_empty() {
                        // Corpo está nas linhas seguintes
                        in_enum = true;
                    } else {
                        for name in extract_var_names(members_str) {
                            if !RESERVED.contains(name.as_str()) {
                                let col = raw_line.find(&name).unwrap_or(0) as u32;
                                result.symbols.push(Symbol {
                                    name,
                                    kind: SymbolKind::StaticConst,
                                    signature: None, params: vec![],
                                    deprecated: false, doc: None,
                                    line: line_idx as u32, col,
                                });
                            }
                        }
                        // Se `}` não fecha a linha, ainda pode haver mais membros
                        if !line[open..].contains('}') {
                            in_enum = true;
                        }
                    }
                } else {
                    pending_enum_open = true;
                }
                depth = update_brace_depth(line, depth);
                continue;
            }

            // #define
            if let Some(cap) = RX_DEFINE.captures(line) {
                let name = cap[1].to_string();
                let body = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                let col = raw_line.find(&name).unwrap_or(0) as u32;

                // Detecta macros geradoras de função (BPLR::, CMD:, CALLBACK::, etc.)
                let is_func_macro = RX_MACRO_PREFIX.is_match(line) && {
                    let b = body.to_ascii_lowercase();
                    b.contains("forward") || b.contains("public")
                };

                result.macro_names.push(name.clone());
                if deprecated {
                    result.deprecated_macros.push(name.clone());
                }
                if is_func_macro {
                    // Não expõe como símbolo (evita "#define BPLR" nas completions)
                    if !result.func_macro_prefixes.contains(&name) {
                        result.func_macro_prefixes.push(name);
                    }
                } else {
                    result.symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Define,
                        signature: None,
                        params: vec![],
                        deprecated,
                        doc: extract_doc(&raw_lines, line_idx),
                        line: line_idx as u32,
                        col,
                    });
                    // Detecta alias de namespace: #define DOF2:: DOF2_  (inline)
                    if let Some(ac) = RX_NAMESPACE_ALIAS.captures(raw_line) {
                        result.namespace_aliases.insert(ac[1].to_string(), ac[2].to_string());
                    // Ou com continuação de linha: `#define DOF2:: \` → alias na próxima linha
                    } else if let Some(ac) = RX_NAMESPACE_ALIAS_CONT.captures(raw_line) {
                        pending_ns_alias = Some(ac[1].to_string());
                    }
                }
                depth = update_brace_depth(line, depth);
                continue;
            }

            // native
            if let Some(cap) = RX_NATIVE.captures(line) {
                let params_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                push_func(&mut result, &raw_lines, raw_line, line_idx, cap[1].to_string(), params_raw, SymbolKind::Native, deprecated);
                depth = update_brace_depth(line, depth);
                continue;
            }

            // forward
            if let Some(cap) = RX_FORWARD.captures(line) {
                let params_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                push_func(&mut result, &raw_lines, raw_line, line_idx, cap[1].to_string(), params_raw, SymbolKind::Forward, deprecated);
                depth = update_brace_depth(line, depth);
                continue;
            }

            // public / stock (incluindo "public stock" e "stock public")
            if let Some(cap) = RX_PUBLIC_STOCK.captures(line) {
                let kind = if &cap[1] == "public" { SymbolKind::Public } else { SymbolKind::Stock };
                let namespace_raw = cap.get(2).map(|m| m.as_str()).unwrap_or(""); // ex: "DOF2::"
                let func_name = cap[3].to_string();
                let params_raw = cap.get(4).map(|m| m.as_str()).unwrap_or("");

                // Se existe alias de namespace (DOF2:: → DOF2_), registra com nome expandido
                let effective_name = if !namespace_raw.is_empty() {
                    let ns = namespace_raw.trim_end_matches(':').trim_end_matches(':');
                    if let Some(alias) = result.namespace_aliases.get(ns) {
                        format!("{}{}", alias, func_name)
                    } else {
                        func_name.clone()
                    }
                } else {
                    func_name.clone()
                };

                push_func(&mut result, &raw_lines, raw_line, line_idx, effective_name, params_raw, kind, deprecated);
                depth = update_brace_depth(line, depth);
                continue;
            }

            // static const (deve vir antes de static func)
            if let Some(cap) = RX_STATIC_CONST.captures(line) {
                let name = cap[1].to_string();
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::StaticConst,
                    signature: None,
                    params: vec![],
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // stock const (deve vir antes de static stock func)
            if let Some(cap) = RX_STOCK_CONST.captures(line) {
                let name = cap[1].to_string();
                let col = raw_line.find(&name).unwrap_or(0) as u32;
                result.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::StaticConst,
                    signature: None,
                    params: vec![],
                    deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32,
                    col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // static stock function (deve vir antes de static function)
            if let Some(cap) = RX_STATIC_STOCK_FUNC.captures(line) {
                let params_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                push_func(&mut result, &raw_lines, raw_line, line_idx, cap[1].to_string(), params_raw, SymbolKind::Stock, deprecated);
                depth = update_brace_depth(line, depth);
                continue;
            }

            // static function
            if let Some(cap) = RX_STATIC_FUNC.captures(line) {
                let params_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                push_func(&mut result, &raw_lines, raw_line, line_idx, cap[1].to_string(), params_raw, SymbolKind::Static, deprecated);
                depth = update_brace_depth(line, depth);
                continue;
            }

            // float/bool Name(params) — com tipo de retorno sem ":"
            if let Some(cap) = RX_FLOAT_BOOL_FUNC.captures(line) {
                let name = cap[2].to_string();
                let params_raw = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                let params = parse_params(params_raw);
                let col = raw_line.find(name.as_str()).unwrap_or(0) as u32;
                let signature = Some(format!("{}:{} ({})", &cap[1], name, params_raw.trim()));
                result.symbols.push(Symbol {
                    name, kind: SymbolKind::Stock, signature, params, deprecated,
                    doc: extract_doc(&raw_lines, line_idx),
                    line: line_idx as u32, col,
                });
                depth = update_brace_depth(line, depth);
                continue;
            }

            // Funções sem keyword / com namespace (fallback — após todos os padrões keyword)
            if let Some(cap) = RX_PLAIN_FUNC.captures(line) {
                let _ns_raw = cap.get(1).map(|m| m.as_str()).unwrap_or(""); // ex: "BPLR::"
                let name = cap[2].to_string();
                if !RESERVED.contains(name.as_str()) {
                    let params_raw = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                    let col = raw_line.find(name.as_str()).unwrap_or(0) as u32;
                    // Funções sem keyword: o compilador as trata como "global não-stock"
                    // (usage=uDEFINE apenas, sem uSTOCK). Para o LSP, mapeamos para Public
                    // para que PP0006 não seja emitido — callbacks externos (ex: OnPlayerConnect
                    // sem `public`) nunca teriam chamada interna e seriam falsos positivos.
                    let kind = SymbolKind::Plain;
                    if line.contains('{') {
                        push_func(&mut result, &raw_lines, raw_line, line_idx, name, params_raw, kind, deprecated);
                    } else {
                        pending_plain = Some((
                            line_idx as u32, col, name, params_raw.to_string(),
                            deprecated, extract_doc(&raw_lines, line_idx), kind,
                        ));
                    }
                    depth = update_brace_depth(line, depth);
                    continue;
                }
            }

            // Fallback multi-linha: qualquer declaração com ( sem ) na mesma linha
            // Cobre stock, public, static, native, forward e funções plain
            if line.contains('(') && !line.contains(')')
                && let Some(cap) = RX_FUNC_OPEN.captures(line)
            {
                let func_name = cap[3].to_string();
                if !func_name.is_empty() && !RESERVED.contains(func_name.as_str()) {
                    let keywords = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_ascii_lowercase();
                    let kind = if keywords.contains("public") { SymbolKind::Public }
                        else if keywords.contains("native") { SymbolKind::Native }
                        else if keywords.contains("forward") { SymbolKind::Forward }
                        else if keywords.contains("static") { SymbolKind::Static }
                        else if keywords.contains("stock") { SymbolKind::Stock }
                        // Sem keyword: global não-stock
                        else { SymbolKind::Plain };
                    let namespace_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                    let effective_name = if !namespace_raw.is_empty() {
                        let ns = namespace_raw.trim_end_matches(':');
                        if let Some(alias) = result.namespace_aliases.get(ns) {
                            format!("{}{}", alias, func_name)
                        } else { func_name }
                    } else { func_name };
                    let partial = cap.get(4).map(|m| m.as_str().trim()).unwrap_or("");
                    let col = raw_line.find(effective_name.as_str()).unwrap_or(0) as u32;
                    multiline_func = Some((
                        line_idx as u32, col, effective_name, partial.to_string(),
                        kind, deprecated, extract_doc(&raw_lines, line_idx),
                    ));
                    depth = update_brace_depth(line, depth);
                    continue;
                }
            }

            // Variáveis: new/const com suporte a múltiplas declarações (new a, b, c)
            if let Some(cap) = RX_NEW_DECL.captures(line) {
                let is_const = line.trim_start().to_ascii_lowercase().starts_with("const");
                let kind = if is_const { SymbolKind::Const } else { SymbolKind::Variable };
                for name in extract_var_names(&cap[1]) {
                    if !RESERVED.contains(name.as_str()) {
                        let col = raw_line.find(&name).unwrap_or(0) as u32;
                        result.symbols.push(Symbol {
                            name,
                            kind: kind.clone(),
                            signature: None,
                            params: vec![],
                            deprecated,
                            doc: None,
                            line: line_idx as u32,
                            col,
                        });
                    }
                }
            }

            // static como variável: static [Tag:]name = ...
            if let Some(cap) = RX_STATIC_VAR.captures(line) {
                let name = cap[1].to_string();
                if !RESERVED.contains(name.as_str()) {
                    let col = raw_line.find(&name).unwrap_or(0) as u32;
                    result.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Variable,
                        signature: None,
                        params: vec![],
                        deprecated,
                        doc: None,
                        line: line_idx as u32,
                        col,
                    });
                }
            }
        } else {
            // dentro de bloco: qualquer linha não-vazia reseta flags de declaração
            pending_deprecated = false;
            in_multi_var_decl = false;

            // ── Membros de enum (depth == 1, dentro do corpo do enum) ────────────
            if in_enum && depth == 1 && !trimmed.starts_with('}') && !trimmed.starts_with('{')
                && let Some(cap) = RX_ENUM_MEMBER.captures(trimmed) {
                let name = cap[1].to_string();
                if !RESERVED.contains(name.as_str()) {
                    let col = raw_line.find(&name).unwrap_or(0) as u32;
                    result.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::StaticConst,
                        signature: None, params: vec![],
                        deprecated: false, doc: None,
                        line: line_idx as u32, col,
                    });
                }
            }
        }

        depth = update_brace_depth(line, depth);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_native() {
        let src = "native fexist(const pattern[]);";
        let f = parse_file(src);
        assert_eq!(f.symbols.len(), 1);
        assert_eq!(f.symbols[0].name, "fexist");
        assert!(matches!(f.symbols[0].kind, SymbolKind::Native));
        // Parâmetro "pattern" detectado mas NÃO como variável separada
        assert!(!f.symbols.iter().any(|s| s.name == "pattern" && matches!(s.kind, SymbolKind::Variable)));
    }

    #[test]
    fn parses_tagged_variable() {
        let src = "new File:f = fopen(\"test\", io_read);";
        let f = parse_file(src);
        // Deve capturar "f", não "File"
        assert!(f.symbols.iter().any(|s| s.name == "f"));
        assert!(!f.symbols.iter().any(|s| s.name == "File"));
    }

    #[test]
    fn parses_static_func() {
        let src = "static MyHelper(a, b) {}";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "MyHelper" && matches!(s.kind, SymbolKind::Static)));
    }

    #[test]
    fn parses_static_const() {
        let src = "static const MAX_ZONES = 10;";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "MAX_ZONES" && matches!(s.kind, SymbolKind::StaticConst)));
    }

    #[test]
    fn parses_deprecated() {
        let src = "// @DEPRECATED\nstock OldFunc() {}";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "OldFunc" && s.deprecated));
    }

    #[test]
    fn parses_include_angle() {
        let src = "#include <a_samp>";
        let f = parse_file(src);
        assert_eq!(f.includes.len(), 1);
        assert_eq!(f.includes[0].token, "a_samp");
        assert!(f.includes[0].is_angle);
    }

    #[test]
    fn parses_include_relative() {
        let src = r#"#include "../utils/helpers.inc""#;
        let f = parse_file(src);
        assert_eq!(f.includes[0].token, "../utils/helpers.inc");
        assert!(!f.includes[0].is_angle);
    }

    #[test]
    fn parses_float_bool_func() {
        let src = "float GetDistance(Float:x, Float:y) {}";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "GetDistance"));
    }

    #[test]
    fn parses_tryinclude() {
        let src = "#tryinclude <y_iterate>";
        let f = parse_file(src);
        assert_eq!(f.includes.len(), 1);
        assert_eq!(f.includes[0].token, "y_iterate");
    }

    #[test]
    fn parses_static_stock_func() {
        let src = "static stock Float:GetDist(Float:x, Float:y) { return 0.0; }";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "GetDist" && matches!(s.kind, SymbolKind::Stock)));
    }

    #[test]
    fn parses_multi_var_new() {
        let src = "new TransportX[MAX_PLAYERS], Float:bombax[MAX_PLAYERS], bool:ativo;";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "TransportX"));
        assert!(f.symbols.iter().any(|s| s.name == "bombax"));
        assert!(f.symbols.iter().any(|s| s.name == "ativo"));
    }

    #[test]
    fn parses_enum_name() {
        let src = "enum dItEnum\n{\n    ObjtID,\n    droptTimer\n};";
        let f = parse_file(src);
        // O nome do enum deve ser registrado como StaticConst (usado como tag/tipo)
        assert!(f.symbols.iter().any(|s| s.name == "dItEnum" && matches!(s.kind, SymbolKind::StaticConst)));
    }

    #[test]
    fn parses_enum_name_with_tag() {
        let src = "enum E_ZONES: { E_ZONE_ID, E_ZONE_NAME }";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "E_ZONES" && matches!(s.kind, SymbolKind::StaticConst)));
    }

    #[test]
    fn parses_enum_members() {
        let src = "enum dItEnum\n{\n    Float:ObjtPos[3],\n    ObjtID,\n    droptTimer\n};";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "ObjtPos" && matches!(s.kind, SymbolKind::StaticConst)));
        assert!(f.symbols.iter().any(|s| s.name == "ObjtID" && matches!(s.kind, SymbolKind::StaticConst)));
        assert!(f.symbols.iter().any(|s| s.name == "droptTimer" && matches!(s.kind, SymbolKind::StaticConst)));
    }

    #[test]
    fn parses_static_stock_multiline_vars() {
        let src = "static stock\n    bool:zcmd_g_HasOPCS = false,\n    bool:zcmd_g_HasOPCE = false;";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "zcmd_g_HasOPCS"));
        assert!(f.symbols.iter().any(|s| s.name == "zcmd_g_HasOPCE"));
    }

    #[test]
    fn parses_public_stock_func() {
        let src = "public stock DoSomething(playerid) {}";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "DoSomething" && matches!(s.kind, SymbolKind::Public)));
    }

    #[test]
    fn parses_stock_const() {
        let src = r#"stock const SSCANF_QUIET[] = "SSCANF_QUIET";"#;
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "SSCANF_QUIET" && matches!(s.kind, SymbolKind::StaticConst)));
    }
}
