/// Atualiza a profundidade de chaves `{}` para uma linha, ignorando literais string/char.
pub fn update_brace_depth(line: &str, mut depth: i32) -> i32 {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut in_char = false;
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        let prev = if i > 0 { bytes[i - 1] } else { 0 };
        if ch == b'"' && !in_char && prev != b'\\' {
            in_str = !in_str;
        } else if ch == b'\'' && !in_str && prev != b'\\' {
            in_char = !in_char;
        } else if !in_str && !in_char {
            if ch == b'{' { depth += 1; }
            else if ch == b'}' { depth = (depth - 1).max(0); }
        }
        i += 1;
    }
    depth
}

/// Decodifica bytes como UTF-8, com fallback latin-1 para arquivos Windows-1252.
pub fn decode_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// Retorna `true` se a linha contém `@DEPRECATED` dentro de um comentário `//` ou `/* */`.
pub fn has_inline_deprecated(raw_line: &str) -> bool {
    if let Some(p) = raw_line.find("//")
        && raw_line[p..].contains("@DEPRECATED") { return true; }
    if let Some(p) = raw_line.find("/*") {
        let end = raw_line[p..].find("*/").map(|q| p + q).unwrap_or(raw_line.len());
        if raw_line[p..end].contains("@DEPRECATED") { return true; }
    }
    false
}

/// Resultado do strip de comentários em uma linha.
#[derive(Debug)]
pub struct StripResult {
    /// Texto com comentários substituídos por espaços (preserva colunas).
    pub text: String,
    /// true se a linha termina dentro de um bloco /* ... */
    pub in_block: bool,
}

/// Strip de comentários `//` e `/* */` em uma linha, preservando posições de coluna.
/// Strings `"..."` e chars `'...'` são preservados integralmente.
pub fn strip_line_comments(line: &str, in_block: bool) -> StripResult {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut out = Vec::with_capacity(len);
    let mut in_block = in_block;
    let mut in_string = false;
    let mut in_char = false;
    let mut i = 0;

    while i < len {
        if in_block {
            if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                out.push(b' ');
                out.push(b' ');
                i += 2;
                in_block = false;
            } else {
                out.push(b' ');
                i += 1;
            }
        } else if in_string {
            let ch = bytes[i];
            if ch == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
            out.push(ch);
            i += 1;
        } else if in_char {
            let ch = bytes[i];
            if ch == b'\'' && (i == 0 || bytes[i - 1] != b'\\') {
                in_char = false;
            }
            out.push(ch);
            i += 1;
        } else {
            // fora de qualquer literal ou bloco
            if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                // comentário de linha: preenche o resto com espaços
                while out.len() < len {
                    out.push(b' ');
                }
                break;
            } else if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                out.push(b' ');
                out.push(b' ');
                i += 2;
                in_block = true;
            } else if bytes[i] == b'"' {
                in_string = true;
                out.push(bytes[i]);
                i += 1;
            } else if bytes[i] == b'\'' {
                in_char = true;
                out.push(bytes[i]);
                i += 1;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }

    // Garante o mesmo comprimento do original para preservar offsets
    while out.len() < len {
        out.push(b' ');
    }

    StripResult {
        // SAFETY: `out` é UTF-8 válido porque:
        //   1. Dentro de string/char literals, bytes são copiados 1:1 do input (já UTF-8 válido).
        //   2. Dentro de comentários, cada byte é substituído por b' ' (0x20, ASCII válido).
        //   3. Fora de literais/comentários, bytes são copiados 1:1.
        // Nenhum byte multi-byte UTF-8 é dividido: substituições só ocorrem dentro de
        // comentários onde não há literais de texto — portanto a sequência nunca é cortada.
        text: unsafe { String::from_utf8_unchecked(out) },
        in_block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_line_comment() {
        let r = strip_line_comments("new x = 1; // unused", false);
        assert!(r.text.starts_with("new x = 1; "));
        assert!(!r.in_block);
    }

    #[test]
    fn preserves_string_with_double_slash() {
        let r = strip_line_comments(r#"new s = "url://foo";"#, false);
        assert!(r.text.contains("url://foo"));
    }

    #[test]
    fn handles_block_comment_start() {
        let r = strip_line_comments("native foo(); /* doc", false);
        assert!(r.in_block);
    }

    #[test]
    fn handles_block_comment_end() {
        let r = strip_line_comments("   end */ native bar();", true);
        assert!(!r.in_block);
        assert!(r.text.contains("native bar()"));
    }
}
