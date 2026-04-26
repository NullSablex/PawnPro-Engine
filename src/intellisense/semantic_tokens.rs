use tower_lsp::lsp_types::*;

use crate::parser::SymbolKind;
use crate::workspace::WorkspaceState;

// Token type index — must match the legend declared in ServerCapabilities
const TOKEN_TYPE_FUNCTION: u32 = 0;

pub fn get_semantic_tokens(state: &WorkspaceState, uri: &str) -> Option<SemanticTokens> {
    let text = state.get_text(uri)?;

    let sdk = state.sdk_parsed.as_ref()?;

    // Build callable set directly from the SDK — natives, forwards, stocks, publics
    let sdk_callables: std::collections::HashSet<&str> = sdk
        .symbols
        .iter()
        .filter(|s| !matches!(s.kind, SymbolKind::Variable | SymbolKind::Define | SymbolKind::Enum | SymbolKind::StaticConst))
        .map(|s| s.name.as_str())
        .collect();

    if sdk_callables.is_empty() {
        return None;
    }

    let mut tokens: Vec<SemanticToken> = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    let lines: Vec<&str> = text.lines().collect();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    for (line_idx, line) in lines.iter().enumerate() {
        let bytes = line.as_bytes();
        let mut col = 0usize;

        while col < bytes.len() {
            if !is_ident(bytes[col]) {
                col += 1;
                continue;
            }
            let start = col;
            while col < bytes.len() && is_ident(bytes[col]) {
                col += 1;
            }
            let word = &line[start..col];

            if sdk_callables.contains(word) {
                // Check for '(' on the same line first (after optional spaces/tabs)
                let mut j = col;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let has_paren = if j < bytes.len() && bytes[j] == b'(' {
                    true
                } else if j >= bytes.len() {
                    // End of line — look ahead on subsequent non-empty lines
                    let mut found = false;
                    for next_line in lines.iter().skip(line_idx + 1).take(3) {
                        let trimmed = next_line.trim_start();
                        if trimmed.is_empty() {
                            continue;
                        }
                        found = trimmed.starts_with('(');
                        break;
                    }
                    found
                } else {
                    false
                };

                if has_paren {
                    let cur_line = line_idx as u32;
                    let cur_start = start as u32;

                    let delta_line = cur_line - prev_line;
                    let delta_start = if delta_line == 0 {
                        cur_start - prev_start
                    } else {
                        cur_start
                    };

                    tokens.push(SemanticToken {
                        delta_line,
                        delta_start,
                        length: word.len() as u32,
                        token_type: TOKEN_TYPE_FUNCTION,
                        token_modifiers_bitset: 0,
                    });

                    prev_line = cur_line;
                    prev_start = cur_start;
                }
            }
        }
    }

    Some(SemanticTokens {
        result_id: None,
        data: tokens,
    })
}

pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![SemanticTokenType::FUNCTION],
        token_modifiers: vec![],
    }
}
