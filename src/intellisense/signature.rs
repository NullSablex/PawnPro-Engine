use tower_lsp::lsp_types::*;

use crate::workspace::WorkspaceState;

use super::collect_all_symbols;

/// Retorna SignatureHelp para a posição `position` no arquivo `uri`.
/// Acionado quando o usuário digita `(` ou `,`.
pub fn get_signature_help(
    state: &WorkspaceState,
    uri: &str,
    position: Position,
) -> Option<SignatureHelp> {
    let text = state.get_text(uri)?;
    let file_path = crate::workspace::uri_to_path(uri)?;
    let inc_paths = state.include_paths();
    let parsed = state.get_parsed(uri)?;

    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    let col = position.character as usize;

    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];
    let prefix = &line[..col.min(line.len())];

    // Descobre qual função está sendo chamada e qual o parâmetro ativo
    let (func_name, active_param) = find_call_context(prefix)?;

    let all_syms = collect_all_symbols(state, &file_path, &inc_paths, &parsed);
    let sym = all_syms
        .iter()
        .find(|s| s.name == func_name && s.signature.is_some())?;

    // Monta ParameterInformation para cada parâmetro
    let param_infos: Vec<ParameterInformation> = sym
        .params
        .iter()
        .map(|p| {
            let label = if let Some(tag) = &p.tag {
                format!("{}:{}", tag, p.name)
            } else {
                p.name.clone()
            };
            ParameterInformation {
                label: ParameterLabel::Simple(label),
                documentation: None,
            }
        })
        .collect();

    let active_idx = (active_param as u32)
        .min(param_infos.len().saturating_sub(1) as u32);

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label: sym.signature.clone().unwrap_or_default(),
            documentation: sym.doc.as_ref().map(|d| {
                Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: d.clone(),
                })
            }),
            parameters: Some(param_infos),
            active_parameter: Some(active_idx),
        }],
        active_signature: Some(0),
        active_parameter: Some(active_idx),
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Percorre o prefixo de trás para frente, localizando a chamada de função ativa
/// e contando vírgulas para determinar o índice do parâmetro atual.
fn find_call_context(prefix: &str) -> Option<(String, usize)> {
    let chars: Vec<char> = prefix.chars().collect();
    let mut depth = 0i32;
    let mut active_param = 0usize;
    let mut in_str = false;
    let mut in_char = false;

    let mut i = chars.len() as isize - 1;
    while i >= 0 {
        let ch = chars[i as usize];
        // Rastreamento básico de strings/chars (sem escape multi-linha)
        match ch {
            '"' if !in_char => in_str = !in_str,
            '\'' if !in_str => in_char = !in_char,
            _ if in_str || in_char => {}
            ')' | ']' => depth += 1,
            '[' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            '(' => {
                if depth == 0 {
                    let name = name_before(&chars, i as usize)?;
                    return Some((name, active_param));
                }
                depth -= 1;
            }
            ',' if depth == 0 => active_param += 1,
            _ => {}
        }
        i -= 1;
    }
    None
}

/// Extrai o nome do identificador imediatamente antes de `paren_pos`.
fn name_before(chars: &[char], paren_pos: usize) -> Option<String> {
    if paren_pos == 0 {
        return None;
    }
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut end = paren_pos;
    while end > 0 && chars[end - 1] == ' ' {
        end -= 1;
    }
    if end == 0 || !is_ident(chars[end - 1]) {
        return None;
    }
    let mut start = end;
    while start > 0 && is_ident(chars[start - 1]) {
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some(chars[start..end].iter().collect())
}
