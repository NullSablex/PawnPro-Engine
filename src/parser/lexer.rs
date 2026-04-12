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
        // SAFETY: apenas bytes ASCII são manipulados, strings UTF-8 multi-byte são copiadas
        // intactas (nunca divididas em strings/chars fora de comentários).
        text: unsafe { String::from_utf8_unchecked(out) },
        in_block,
    }
}

/// Constrói tabela de offsets de início de cada linha (0-based byte offset).
pub fn build_line_offsets(text: &str) -> Vec<u32> {
    let mut offsets = vec![0u32];
    let mut offset: u32 = 0;
    for ch in text.chars() {
        offset += ch.len_utf8() as u32;
        if ch == '\n' {
            offsets.push(offset);
        }
    }
    offsets
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
