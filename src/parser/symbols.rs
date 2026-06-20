use regex::Regex;

use super::{
    lexer::{has_inline_deprecated, strip_line_comments, update_brace_depth},
    types::{IncludeDirective, Param, ParsedFile, Symbol, SymbolKind},
};
use crate::util::to_u32;

static RX_DEPRECATED: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(?://\s*@DEPRECATED|/\*\s*@DEPRECATED\s*\*/)\s*$").unwrap()
});

static RX_NATIVE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(?:forward\s+)?native\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

static RX_FORWARD: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"^\s*forward\s+(?:[A-Za-z_]\w*::)*(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)",
    )
    .unwrap()
});

// public|stock, com namespace e tag de retorno opcionais.
// Grupos: 1=keyword, 2=namespace (`NS::` ou vazio), 3=nome, 4=params.
static RX_PUBLIC_STOCK: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(public|stock)(?:\s+(?:public|stock))?\s+(?:[A-Za-z_]\w*:)?\s*((?:[A-Za-z_]\w*::)*)(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// static stock [Tag:]Name(params) — deve ser testado ANTES de RX_STATIC_FUNC
static RX_STATIC_STOCK_FUNC: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*static\s+stock\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// static [Tag:]Name(params) — funções com corpo
static RX_STATIC_FUNC: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*static\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// static const Name ou static const Name[...] (array/constante)
static RX_STATIC_CONST: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*static\s+const\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\b").unwrap()
});

// stock const [Tag:]Name — constante exportada
static RX_STOCK_CONST: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*stock\s+const\s+(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\b").unwrap()
});

// float/bool Name(params) — funções com tipo de retorno sem ":"
static RX_FLOAT_BOOL_FUNC: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(float|bool)\s+([A-Za-z_]\w*)\s*\(([^)]*)\)").unwrap()
});

// Função sem keyword, fallback após os padrões com keyword falharem.
// Grupos: 1=namespace (`NS::` ou vazio), 2=nome, 3=params.
static RX_PLAIN_FUNC: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*((?:[A-Za-z_]\w*::)*)(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)\)")
        .unwrap()
});

// #define Name — captura o nome e tudo que vem depois (corpo da macro)
static RX_DEFINE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)\b(.*)$").unwrap());

// Detecta macros do tipo PREFIX::%0(...) ou PREFIX:%0(...)
// — indicam que o prefixo é gerador de função quando o corpo tem forward/public.
static RX_MACRO_PREFIX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)(?:::?)\s*%\d").unwrap()
});

// Detecta alias de namespace numa única linha: #define NS:: PREFIX_
static RX_NAMESPACE_ALIAS: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)::\s+([A-Za-z_]\w*)").unwrap()
});

// Detecta abertura de alias de namespace com continuação de linha:
// `#define NS:: \` — o alias está na próxima linha
static RX_NAMESPACE_ALIAS_CONT: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)::\s*\\").unwrap());

// Detecta abertura de função com qualquer keyword (ou sem keyword) com ( sem ) na mesma linha.
// Cobre: stock, public, static, static stock, native, forward, e funções plain.
// Grupo 1: keywords (pode ser vazio), grupo 2: namespace, grupo 3: nome, grupo 4: params parciais após (
static RX_FUNC_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*((?:(?:public|stock|static|native|forward)\s+)*)?(?:[A-Za-z_]\w*:)?\s*((?:[A-Za-z_]\w*::)*)(?:[A-Za-z_]\w*:)?\s*([A-Za-z_]\w*)\s*\(([^)]*)$").unwrap()
});

// enum [Tag:] [Name] [(opts)] [{] — grupo 1: nome opcional do enum
static RX_ENUM: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*enum\s+(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)").unwrap()
});

// Membro de enum: [Tag:]Name (dentro do corpo)
static RX_ENUM_MEMBER: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^\s*(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)").unwrap());

// #include / #tryinclude <token> ou "token" — grupo 1: "try", grupo 2: <token>, grupo 3: "token"
static RX_INCLUDE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"^\s*#\s*(try)?include\s*(?:<([^>]+)>|"([^"]+)")"#).unwrap()
});

// new/const com múltiplas variáveis: captura tudo após a keyword para split posterior
static RX_NEW_DECL: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^\s*(?:new|const)\s+(.+)").unwrap());
// static como variável (fora de corpo): static [Tag:]name = ...;
static RX_STATIC_VAR: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*static\s+(?:[A-Za-z_]\w*:)?([A-Za-z_]\w*)\s*(?:[=\[;])").unwrap()
});

// Palavras reservadas que nunca são nomes de variáveis
static RESERVED: std::sync::LazyLock<std::collections::HashSet<&'static str>> =
    std::sync::LazyLock::new(|| {
        [
            "true", "false", "null", "sizeof", "tagof", "Float", "bool", "char", "String", "new",
            "static", "const", "native", "forward", "public", "stock", "return", "if", "else",
            "for", "while", "do", "switch", "case", "break", "continue", "default",
        ]
        .into_iter()
        .collect()
    });

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

// `"Tag:a, b[10], Tag:c = 0"` → `["a", "b", "c"]`
fn extract_var_names(raw: &str) -> Vec<String> {
    let raw = raw.trim_end_matches(';').trim();
    let parts = split_params(raw);
    let mut names = Vec::new();
    for part in parts {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let name_part = if let Some(c) = p.find(':') {
            p[c + 1..].trim()
        } else {
            p
        };
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
        let stripped = t.trim_start_matches("const").trim_start_matches('&').trim();
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
            params.push(Param {
                name,
                tag,
                is_variadic: false,
            });
        }
    }

    params
}

fn extract_doc(lines: &[&str], line_idx: usize) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut found = false;
    // Caminha para cima a partir da linha anterior. Índices em `usize` com
    // decremento via `checked_sub` evitam o uso de `isize` (e os casts que ele
    // exigiria); ao chegar em 0 o loop termina.
    let mut i = line_idx.checked_sub(1);
    while let Some(idx) = i {
        let l = lines[idx].trim();
        if l.is_empty() {
            if found {
                break;
            }
            i = idx.checked_sub(1);
            continue;
        }
        if l.starts_with("//") {
            doc_lines.push(l.to_string());
            found = true;
        } else if l.ends_with("*/") {
            doc_lines.push(l.to_string());
            // busca o início do bloco
            let mut j = idx.checked_sub(1);
            while let Some(jdx) = j {
                let ll = lines[jdx].trim();
                doc_lines.push(ll.to_string());
                if ll.contains("/*") {
                    break;
                }
                j = jdx.checked_sub(1);
            }
            break;
        } else {
            break;
        }
        i = idx.checked_sub(1);
    }
    if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(doc_lines.join("\n"))
    }
}

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
    let col = to_u32(raw_line.find(name.as_str()).unwrap_or(0));
    result.symbols.push(Symbol {
        signature: Some(format!("{}({})", name, params_raw.trim())),
        name,
        kind,
        params,
        deprecated,
        doc: extract_doc(raw_lines, line_idx),
        line: to_u32(line_idx),
        col,
    });
}

/// Assinatura de função cuja `(...)` se estende por várias linhas, acumulada
/// entre o `(` de abertura e o `)` de fechamento:
/// `(line, col, name, params_acc, kind, deprecated, doc)`.
type MultilineFunc = (u32, u32, String, String, SymbolKind, bool, Option<String>);

/// Função `plain` (sem keyword) cuja `{` ainda não apareceu — pode estar na
/// linha seguinte: `(line, col, name, params, deprecated, doc, kind)`.
type PendingPlain = (u32, u32, String, String, bool, Option<String>, SymbolKind);

/// Concatena um trecho de parâmetros ao acumulador, inserindo `", "` como
/// separador apenas quando necessário — evita vírgula dupla (`a,, b`) quando o
/// trecho anterior já termina em `,` ou o novo começa com `,` (parâmetros em
/// linhas próprias).
fn append_param_chunk(acc: &mut String, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    if !acc.is_empty() {
        if acc.trim_end().ends_with(',') || chunk.starts_with(',') {
            // Já há vírgula no limite: insere só o espaço de separação.
            acc.push(' ');
        } else {
            acc.push_str(", ");
        }
    }
    acc.push_str(chunk);
}

/// Emite um `Symbol::Variable` para cada identificador de uma linha que faz
/// parte de uma declaração de variáveis múltipla (`new`/`static`/`static stock`
/// com nomes espalhados por várias linhas), ignorando palavras reservadas.
fn push_multi_var_names(
    result: &mut ParsedFile,
    raw_line: &str,
    trimmed: &str,
    line_idx: usize,
    deprecated: bool,
) {
    for name in extract_var_names(trimmed) {
        if RESERVED.contains(name.as_str()) {
            continue;
        }
        let col = to_u32(raw_line.find(&name).unwrap_or(0));
        result.symbols.push(Symbol {
            name,
            kind: SymbolKind::Variable,
            signature: None,
            params: vec![],
            deprecated,
            doc: None,
            line: to_u32(line_idx),
            col,
        });
    }
}

/// Extrai o alias de um `#define NS:: \` cuja definição está na linha seguinte.
/// Retorna `None` se a linha não inicia com um identificador válido.
fn continuation_alias(trimmed: &str) -> Option<String> {
    let alias = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches('\\')
        .trim();
    if !alias.is_empty()
        && alias
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    {
        Some(alias.to_string())
    } else {
        None
    }
}

/// Processa uma linha enquanto há uma assinatura multi-linha em aberto.
///
/// Acumula os parâmetros parciais e, ao encontrar o `)`, finaliza: empurra o
/// `Symbol` se a `{` está na mesma linha, ou guarda em `pending_plain` caso o
/// corpo abra adiante.
fn continue_multiline_func(
    line: &str,
    multiline_func: &mut Option<MultilineFunc>,
    result: &mut ParsedFile,
    pending_plain: &mut Option<PendingPlain>,
) {
    let Some(close_pos) = line.find(')') else {
        // Ainda dentro dos parâmetros: acumula o trecho desta linha.
        let mf = multiline_func.as_mut().expect("multiline_func presente");
        append_param_chunk(&mut mf.3, line.trim());
        return;
    };

    let partial = line[..close_pos].trim();
    if !partial.is_empty() {
        let mf = multiline_func.as_mut().expect("multiline_func presente");
        append_param_chunk(&mut mf.3, partial);
    }
    let (pidx, pcol, pname, pparams, pkind, pdep, pdoc) =
        multiline_func.take().expect("multiline_func presente");
    let rest = &line[close_pos + 1..];
    if rest.contains('{') {
        let parsed_params = parse_params(&pparams);
        result.symbols.push(Symbol {
            signature: Some(format!("{}({})", pname, pparams.trim())),
            name: pname,
            kind: pkind,
            params: parsed_params,
            deprecated: pdep,
            doc: pdoc,
            line: pidx,
            col: pcol,
        });
    } else {
        *pending_plain = Some((pidx, pcol, pname, pparams, pdep, pdoc, pkind));
    }
}

/// Estado acumulado durante o parsing linha-a-linha de um arquivo Pawn.
///
/// Agrupar o estado num struct evita arrastar ~10 variáveis mutáveis por toda a
/// função e permite separar a detecção de cada forma sintática em métodos
/// coesos, em vez de uma única função gigante.
///
/// As flags booleanas são modos independentes do autômato de parsing (em bloco
/// de comentário, em enum, em declaração múltipla...); agrupá-las num sub-struct
/// só as tornaria menos legíveis, daí o `allow`.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
struct ParserState {
    result: ParsedFile,
    in_block: bool,
    depth: i32,
    pending_deprecated: bool,
    /// Função cuja `{` ainda não apareceu (pode estar na linha seguinte).
    pending_plain: Option<PendingPlain>,
    /// Parâmetros acumulados de assinatura multi-linha entre `(` ... `)`.
    multiline_func: Option<MultilineFunc>,
    in_multi_var_decl: bool,
    in_enum: bool,
    pending_enum_open: bool,
    /// `#define NS:: \` — alias está na linha seguinte.
    pending_ns_alias: Option<String>,
}

pub fn parse_file(text: &str) -> ParsedFile {
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let mut st = ParserState::default();

    for (line_idx, raw_line) in raw_lines.iter().enumerate() {
        let raw_line = raw_line.trim_end_matches('\r');
        st.process_line(raw_line, line_idx, &raw_lines);
    }

    st.result
}

impl ParserState {
    /// Processa uma única linha, despachando para o handler da fase/forma
    /// adequada. Cada handler devolve `true` quando consumiu a linha (equivalente
    /// ao antigo `continue`); ao final, a profundidade de chaves é atualizada.
    fn process_line(&mut self, raw_line: &str, line_idx: usize, raw_lines: &[&str]) {
        // @DEPRECATED deve ser verificado no rawLine, antes do strip
        if RX_DEPRECATED.is_match(raw_line) {
            self.pending_deprecated = true;
            let stripped = strip_line_comments(raw_line, self.in_block);
            self.in_block = stripped.in_block;
            self.depth = update_brace_depth(&stripped.text, self.depth);
            return;
        }

        let stripped = strip_line_comments(raw_line, self.in_block);
        self.in_block = stripped.in_block;
        let line = stripped.text.clone();
        let trimmed = line.trim().to_string();

        if self.multiline_func.is_some() {
            continue_multiline_func(
                &line,
                &mut self.multiline_func,
                &mut self.result,
                &mut self.pending_plain,
            );
            self.depth = update_brace_depth(&line, self.depth);
            return;
        }

        if let Some(ns) = self.pending_ns_alias.take() {
            if let Some(alias) = continuation_alias(&trimmed) {
                self.result.namespace_aliases.insert(ns, alias);
            }
            self.depth = update_brace_depth(&line, self.depth);
            return;
        }

        if trimmed.is_empty() {
            self.depth = update_brace_depth(&line, self.depth);
            return;
        }

        if self.depth == 0 {
            self.process_top_level(&line, &trimmed, raw_line, line_idx, raw_lines);
        } else {
            self.process_nested(&trimmed, raw_line, line_idx);
        }

        self.depth = update_brace_depth(&line, self.depth);
    }

    /// Detecção no nível raiz (`depth == 0`): declarações, diretivas e funções.
    /// Cada bloco que "consome" a linha retorna cedo (equivalente ao antigo
    /// `continue`); a profundidade é atualizada por `process_line`.
    ///
    /// É uma cascata plana de detecções independentes (uma por forma sintática);
    /// o grosso das funções já foi extraído para `try_function_decl`. O que resta
    /// é linear e legível, mas excede o limite por listar muitas formas — daí o
    /// `allow`, em vez de fragmentar em sub-métodos que só repetiriam a assinatura.
    #[allow(clippy::too_many_lines)]
    fn process_top_level(
        &mut self,
        line: &str,
        trimmed: &str,
        raw_line: &str,
        line_idx: usize,
        raw_lines: &[&str],
    ) {
        // Um enum aberto na raiz não persiste entre linhas top-level.
        self.in_enum = false;

        if self.pending_enum_open {
            if trimmed.starts_with('{') {
                self.in_enum = true;
                self.pending_enum_open = false;
                self.pending_plain = None;
                return;
            }
            self.pending_enum_open = false; // linha inesperada, abandona
        }

        if self.in_multi_var_decl {
            self.pending_plain = None;
            push_multi_var_names(
                &mut self.result,
                raw_line,
                trimmed,
                line_idx,
                self.pending_deprecated,
            );
            if trimmed.contains(';') || trimmed.contains('{') {
                self.in_multi_var_decl = false;
            }
            return;
        }

        if matches!(trimmed, "static" | "static stock" | "new") {
            self.in_multi_var_decl = true;
            return;
        }

        if let Some((pidx, pcol, pname, pparams, pdep, pdoc, pkind)) = self.pending_plain.take()
            && trimmed.starts_with('{')
        {
            let params = parse_params(&pparams);
            self.result.symbols.push(Symbol {
                signature: Some(format!("{}({})", pname, pparams.trim())),
                name: pname,
                kind: pkind,
                params,
                deprecated: pdep,
                doc: pdoc,
                line: pidx,
                col: pcol,
            });
            return;
        }

        let inline_deprecated = has_inline_deprecated(raw_line);
        let deprecated = self.pending_deprecated || inline_deprecated;
        self.pending_deprecated = false;

        if let Some(cap) = RX_INCLUDE.captures(line) {
            let is_try = cap.get(1).is_some();
            let (token, is_angle) = if let Some(m) = cap.get(2) {
                (m.as_str().trim().to_string(), true)
            } else {
                (cap.get(3).unwrap().as_str().trim().to_string(), false)
            };
            let col = to_u32(raw_line.find(&token).unwrap_or(0));
            self.result.includes.push(IncludeDirective {
                token,
                is_angle,
                is_try,
                line: to_u32(line_idx),
                col,
            });
            return;
        }

        if let Some(enum_cap) = RX_ENUM.captures(line) {
            let enum_name = enum_cap.get(1).map_or("", |m| m.as_str());
            if !enum_name.is_empty() && !RESERVED.contains(enum_name) {
                let col = to_u32(raw_line.find(enum_name).unwrap_or(0));
                self.result.symbols.push(Symbol {
                    name: enum_name.to_string(),
                    kind: SymbolKind::Enum,
                    signature: None,
                    params: vec![],
                    deprecated,
                    doc: extract_doc(raw_lines, line_idx),
                    line: to_u32(line_idx),
                    col,
                });
            }
            if let Some(open) = line.find('{') {
                let body_start = open + 1;
                let body_end = line[body_start..]
                    .find('}')
                    .map_or(line.len(), |i| body_start + i);
                let members_str = &line[body_start..body_end];
                if members_str.trim().is_empty() {
                    self.in_enum = true;
                } else {
                    for name in extract_var_names(members_str) {
                        if !RESERVED.contains(name.as_str()) {
                            let col = to_u32(raw_line.find(&name).unwrap_or(0));
                            self.result.symbols.push(Symbol {
                                name,
                                kind: SymbolKind::StaticConst,
                                signature: None,
                                params: vec![],
                                deprecated: false,
                                doc: None,
                                line: to_u32(line_idx),
                                col,
                            });
                        }
                    }
                    if !line[open..].contains('}') {
                        self.in_enum = true;
                    }
                }
            } else {
                self.pending_enum_open = true;
            }
            return;
        }

        if let Some(cap) = RX_DEFINE.captures(line) {
            let name = cap[1].to_string();
            let body = cap.get(2).map_or("", |m| m.as_str());
            let col = to_u32(raw_line.find(&name).unwrap_or(0));

            let is_func_macro = RX_MACRO_PREFIX.is_match(line) && {
                let b = body.to_ascii_lowercase();
                b.contains("forward") || b.contains("public")
            };

            self.result.macro_names.push(name.clone());
            if deprecated {
                self.result.deprecated_macros.push(name.clone());
            }
            if is_func_macro {
                if !self.result.func_macro_prefixes.contains(&name) {
                    self.result.func_macro_prefixes.push(name);
                }
            } else {
                self.result.symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Define,
                    signature: None,
                    params: vec![],
                    deprecated,
                    doc: extract_doc(raw_lines, line_idx),
                    line: to_u32(line_idx),
                    col,
                });
                // Detecta alias de namespace: #define NS:: NS_  (inline)
                if let Some(ac) = RX_NAMESPACE_ALIAS.captures(raw_line) {
                    self.result
                        .namespace_aliases
                        .insert(ac[1].to_string(), ac[2].to_string());
                // Ou com continuação de linha: `#define NS:: \` → alias na linha seguinte
                } else if let Some(ac) = RX_NAMESPACE_ALIAS_CONT.captures(raw_line) {
                    self.pending_ns_alias = Some(ac[1].to_string());
                }
            }
            return;
        }

        if self.try_function_decl(line, raw_line, line_idx, raw_lines, deprecated) {
            return;
        }

        if let Some(cap) = RX_NEW_DECL.captures(line) {
            let is_const = line.trim_start().to_ascii_lowercase().starts_with("const");
            let kind = if is_const {
                SymbolKind::Const
            } else {
                SymbolKind::Variable
            };
            for name in extract_var_names(&cap[1]) {
                if !RESERVED.contains(name.as_str()) {
                    let col = to_u32(raw_line.find(&name).unwrap_or(0));
                    self.result.symbols.push(Symbol {
                        name,
                        kind: kind.clone(),
                        signature: None,
                        params: vec![],
                        deprecated,
                        doc: None,
                        line: to_u32(line_idx),
                        col,
                    });
                }
            }
        }

        if let Some(cap) = RX_STATIC_VAR.captures(line) {
            let name = cap[1].to_string();
            if !RESERVED.contains(name.as_str()) {
                let col = to_u32(raw_line.find(&name).unwrap_or(0));
                self.result.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Variable,
                    signature: None,
                    params: vec![],
                    deprecated,
                    doc: None,
                    line: to_u32(line_idx),
                    col,
                });
            }
        }
    }

    /// Detecção dentro de um corpo aninhado (`depth > 0`): basicamente membros
    /// de `enum` quando ele se estende por várias linhas.
    fn process_nested(&mut self, trimmed: &str, raw_line: &str, line_idx: usize) {
        self.pending_deprecated = false;
        self.in_multi_var_decl = false;

        if self.in_enum
            && self.depth == 1
            && !trimmed.starts_with('}')
            && !trimmed.starts_with('{')
            && let Some(cap) = RX_ENUM_MEMBER.captures(trimmed)
        {
            let name = cap[1].to_string();
            if !RESERVED.contains(name.as_str()) {
                let col = to_u32(raw_line.find(&name).unwrap_or(0));
                self.result.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::StaticConst,
                    signature: None,
                    params: vec![],
                    deprecated: false,
                    doc: None,
                    line: to_u32(line_idx),
                    col,
                });
            }
        }
    }

    /// Detecta declarações de função em todas as formas (native, forward,
    /// public/stock, static, float/bool e sem-keyword), além de constantes
    /// `static`/`stock const`. Retorna `true` se consumiu a linha.
    ///
    /// Cada forma é um `if let RX_*` independente; o tamanho reflete a variedade
    /// de sintaxes do Pawn, não complexidade entrelaçada.
    #[allow(clippy::too_many_lines)]
    fn try_function_decl(
        &mut self,
        line: &str,
        raw_line: &str,
        line_idx: usize,
        raw_lines: &[&str],
        deprecated: bool,
    ) -> bool {
        if let Some(cap) = RX_NATIVE.captures(line) {
            let params_raw = cap.get(2).map_or("", |m| m.as_str());
            push_func(
                &mut self.result,
                raw_lines,
                raw_line,
                line_idx,
                cap[1].to_string(),
                params_raw,
                SymbolKind::Native,
                deprecated,
            );
            return true;
        }

        if let Some(cap) = RX_FORWARD.captures(line) {
            let params_raw = cap.get(2).map_or("", |m| m.as_str());
            push_func(
                &mut self.result,
                raw_lines,
                raw_line,
                line_idx,
                cap[1].to_string(),
                params_raw,
                SymbolKind::Forward,
                deprecated,
            );
            return true;
        }

        if let Some(cap) = RX_PUBLIC_STOCK.captures(line) {
            let kind = if &cap[1] == "public" {
                SymbolKind::Public
            } else {
                SymbolKind::Stock
            };
            let namespace_raw = cap.get(2).map_or("", |m| m.as_str()); // ex: "NS::"
            let func_name = cap[3].to_string();
            let params_raw = cap.get(4).map_or("", |m| m.as_str());

            // Se existe alias de namespace (NS:: → NS_), registra com nome expandido
            let effective_name = if namespace_raw.is_empty() {
                func_name.clone()
            } else {
                let ns = namespace_raw.trim_end_matches(':').trim_end_matches(':');
                if let Some(alias) = self.result.namespace_aliases.get(ns) {
                    format!("{alias}{func_name}")
                } else {
                    func_name.clone()
                }
            };

            push_func(
                &mut self.result,
                raw_lines,
                raw_line,
                line_idx,
                effective_name,
                params_raw,
                kind,
                deprecated,
            );
            return true;
        }

        if let Some(cap) = RX_STATIC_CONST.captures(line) {
            let name = cap[1].to_string();
            let col = to_u32(raw_line.find(&name).unwrap_or(0));
            self.result.symbols.push(Symbol {
                name,
                kind: SymbolKind::StaticConst,
                signature: None,
                params: vec![],
                deprecated,
                doc: extract_doc(raw_lines, line_idx),
                line: to_u32(line_idx),
                col,
            });
            return true;
        }

        if let Some(cap) = RX_STOCK_CONST.captures(line) {
            let name = cap[1].to_string();
            let col = to_u32(raw_line.find(&name).unwrap_or(0));
            self.result.symbols.push(Symbol {
                name,
                kind: SymbolKind::StaticConst,
                signature: None,
                params: vec![],
                deprecated,
                doc: extract_doc(raw_lines, line_idx),
                line: to_u32(line_idx),
                col,
            });
            return true;
        }

        if let Some(cap) = RX_STATIC_STOCK_FUNC.captures(line) {
            let params_raw = cap.get(2).map_or("", |m| m.as_str());
            push_func(
                &mut self.result,
                raw_lines,
                raw_line,
                line_idx,
                cap[1].to_string(),
                params_raw,
                SymbolKind::Stock,
                deprecated,
            );
            return true;
        }

        if let Some(cap) = RX_STATIC_FUNC.captures(line) {
            let params_raw = cap.get(2).map_or("", |m| m.as_str());
            push_func(
                &mut self.result,
                raw_lines,
                raw_line,
                line_idx,
                cap[1].to_string(),
                params_raw,
                SymbolKind::Static,
                deprecated,
            );
            return true;
        }

        if let Some(cap) = RX_FLOAT_BOOL_FUNC.captures(line) {
            let name = cap[2].to_string();
            let params_raw = cap.get(3).map_or("", |m| m.as_str());
            let params = parse_params(params_raw);
            let col = to_u32(raw_line.find(name.as_str()).unwrap_or(0));
            let signature = Some(format!("{}:{} ({})", &cap[1], name, params_raw.trim()));
            self.result.symbols.push(Symbol {
                name,
                kind: SymbolKind::Stock,
                signature,
                params,
                deprecated,
                doc: extract_doc(raw_lines, line_idx),
                line: to_u32(line_idx),
                col,
            });
            return true;
        }

        if let Some(cap) = RX_PLAIN_FUNC.captures(line) {
            let name = cap[2].to_string();
            if !RESERVED.contains(name.as_str()) {
                let params_raw = cap.get(3).map_or("", |m| m.as_str());
                let col = to_u32(raw_line.find(name.as_str()).unwrap_or(0));
                // Funções sem keyword: o compilador as trata como "global não-stock"
                // (usage=uDEFINE apenas, sem uSTOCK). Para o LSP, mapeamos para Public
                // para que PP0006 não seja emitido — callbacks externos (ex: OnPlayerConnect
                // sem `public`) nunca teriam chamada interna e seriam falsos positivos.
                let kind = SymbolKind::Plain;
                if line.contains('{') {
                    push_func(
                        &mut self.result,
                        raw_lines,
                        raw_line,
                        line_idx,
                        name,
                        params_raw,
                        kind,
                        deprecated,
                    );
                } else {
                    self.pending_plain = Some((
                        to_u32(line_idx),
                        col,
                        name,
                        params_raw.to_string(),
                        deprecated,
                        extract_doc(raw_lines, line_idx),
                        kind,
                    ));
                }
                return true;
            }
        }

        if line.contains('(')
            && !line.contains(')')
            && let Some(cap) = RX_FUNC_OPEN.captures(line)
        {
            let func_name = cap[3].to_string();
            if !func_name.is_empty() && !RESERVED.contains(func_name.as_str()) {
                let keywords = cap.get(1).map_or("", |m| m.as_str()).to_ascii_lowercase();
                let kind = if keywords.contains("public") {
                    SymbolKind::Public
                } else if keywords.contains("native") {
                    SymbolKind::Native
                } else if keywords.contains("forward") {
                    SymbolKind::Forward
                } else if keywords.contains("static") {
                    SymbolKind::Static
                } else if keywords.contains("stock") {
                    SymbolKind::Stock
                }
                // Sem keyword: global não-stock
                else {
                    SymbolKind::Plain
                };
                let namespace_raw = cap.get(2).map_or("", |m| m.as_str());
                let effective_name = if namespace_raw.is_empty() {
                    func_name
                } else {
                    let ns = namespace_raw.trim_end_matches(':');
                    if let Some(alias) = self.result.namespace_aliases.get(ns) {
                        format!("{alias}{func_name}")
                    } else {
                        func_name
                    }
                };
                let partial = cap.get(4).map_or("", |m| m.as_str().trim());
                let col = to_u32(raw_line.find(effective_name.as_str()).unwrap_or(0));
                self.multiline_func = Some((
                    to_u32(line_idx),
                    col,
                    effective_name,
                    partial.to_string(),
                    kind,
                    deprecated,
                    extract_doc(raw_lines, line_idx),
                ));
                return true;
            }
        }
        false
    }
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
        assert!(
            !f.symbols
                .iter()
                .any(|s| s.name == "pattern" && matches!(s.kind, SymbolKind::Variable))
        );
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
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "MyHelper" && matches!(s.kind, SymbolKind::Static))
        );
    }

    #[test]
    fn parses_static_const() {
        let src = "static const MAX_ZONES = 10;";
        let f = parse_file(src);
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "MAX_ZONES" && matches!(s.kind, SymbolKind::StaticConst))
        );
    }

    #[test]
    fn parses_deprecated() {
        let src = "// @DEPRECATED\nstock OldFunc() {}";
        let f = parse_file(src);
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "OldFunc" && s.deprecated)
        );
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
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "GetDist" && matches!(s.kind, SymbolKind::Stock))
        );
    }

    #[test]
    fn parses_multi_var_new() {
        let src = "new Slots[MAX_PLAYERS], Float:Coords[MAX_PLAYERS], bool:active;";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "Slots"));
        assert!(f.symbols.iter().any(|s| s.name == "Coords"));
        assert!(f.symbols.iter().any(|s| s.name == "active"));
    }

    #[test]
    fn parses_enum_name() {
        let src = "enum E_Item\n{\n    ItemId,\n    DropTimer\n};";
        let f = parse_file(src);
        // O nome do enum é registrado com SymbolKind::Enum (kind dedicado)
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "E_Item" && matches!(s.kind, SymbolKind::Enum))
        );
    }

    #[test]
    fn parses_enum_name_with_tag() {
        let src = "enum E_ZONES: { E_ZONE_ID, E_ZONE_NAME }";
        let f = parse_file(src);
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "E_ZONES" && matches!(s.kind, SymbolKind::Enum))
        );
    }

    #[test]
    fn parses_enum_members() {
        let src = "enum E_Item\n{\n    Float:Pos[3],\n    ItemId,\n    DropTimer\n};";
        let f = parse_file(src);
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "Pos" && matches!(s.kind, SymbolKind::StaticConst))
        );
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "ItemId" && matches!(s.kind, SymbolKind::StaticConst))
        );
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "DropTimer" && matches!(s.kind, SymbolKind::StaticConst))
        );
    }

    #[test]
    fn parses_static_stock_multiline_vars() {
        let src =
            "static stock\n    bool:zcmd_g_HasOPCS = false,\n    bool:zcmd_g_HasOPCE = false;";
        let f = parse_file(src);
        assert!(f.symbols.iter().any(|s| s.name == "zcmd_g_HasOPCS"));
        assert!(f.symbols.iter().any(|s| s.name == "zcmd_g_HasOPCE"));
    }

    #[test]
    fn parses_public_stock_func() {
        let src = "public stock DoSomething(playerid) {}";
        let f = parse_file(src);
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "DoSomething" && matches!(s.kind, SymbolKind::Public))
        );
    }

    #[test]
    fn parses_stock_const() {
        let src = r#"stock const SSCANF_QUIET[] = "SSCANF_QUIET";"#;
        let f = parse_file(src);
        assert!(
            f.symbols
                .iter()
                .any(|s| s.name == "SSCANF_QUIET" && matches!(s.kind, SymbolKind::StaticConst))
        );
    }

    // Assinatura de função distribuída em várias linhas, um parâmetro por linha.
    // Regressão: o acumulador não pode gerar vírgula dupla ('a,, b').
    #[test]
    fn parses_multiline_func_signature() {
        let src = "stock\n    MyFunc(\n        a,\n        b\n    )\n{\n    return 1;\n}";
        let f = parse_file(src);
        let sym = f
            .symbols
            .iter()
            .find(|s| s.name == "MyFunc")
            .expect("MyFunc deve ser detectada");
        assert_eq!(sym.signature.as_deref(), Some("MyFunc(a, b)"));
        assert_eq!(
            sym.params.len(),
            2,
            "deve ter 2 parâmetros, não vírgula dupla"
        );
    }

    // Assinatura multi-linha com parâmetros separados por vírgula na mesma linha.
    #[test]
    fn parses_multiline_func_mixed() {
        let src = "stock MyFunc(a,\n    b, c)\n{\n}";
        let f = parse_file(src);
        let sym = f
            .symbols
            .iter()
            .find(|s| s.name == "MyFunc")
            .expect("MyFunc deve ser detectada");
        assert_eq!(sym.params.len(), 3);
    }

    // Alias de namespace inline: `#define NS:: NS_`.
    #[test]
    fn parses_namespace_alias_inline() {
        let src = "#define NS:: NS_";
        let f = parse_file(src);
        assert_eq!(f.namespace_aliases.get("NS"), Some(&"NS_".to_string()));
    }
}
