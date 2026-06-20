//! Adaptação LSP do formatador: monta os `TextEdit` de documento/seleção a
//! partir do motor estrutural ([`super::format_engine`]) e concentra os helpers
//! de espaçamento de operadores (validados contra o compilador open.mp), que o
//! motor reutiliza linha a linha.

use tower_lsp::lsp_types::{Position, Range, TextEdit};

use crate::parser::lexer::strip_line_comments;
use crate::util::to_u32;

pub fn format_document(text: &str, style: super::format_style::FormatStyle) -> Vec<TextEdit> {
    let formatted = super::format_engine::format(text, style);
    if formatted == text {
        return vec![];
    }
    vec![TextEdit {
        range: full_document_range(text),
        new_text: formatted,
    }]
}

/// Range que cobre exatamente o documento. Em LSP, um '\n' final cria uma linha
/// vazia adicional; o fim do documento é o início dessa linha (char 0). Calcular
/// pelo número de '\n' evita o off-by-one que inseria uma linha em branco extra
/// ao formatar (inclusive ao acionar a formatação sem seleção).
fn full_document_range(text: &str) -> Range {
    let newlines = to_u32(text.matches('\n').count());
    let last_line_len = if text.ends_with('\n') {
        0
    } else {
        to_u32(text.rsplit('\n').next().map_or(0, |l| l.chars().count()))
    };
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: newlines,
            character: last_line_len,
        },
    }
}

pub fn format_range(
    text: &str,
    range: Range,
    style: super::format_style::FormatStyle,
) -> Vec<TextEdit> {
    let lines: Vec<&str> = text.lines().collect();
    let start_line = range.start.line as usize;
    // Quando a seleção termina no início de uma linha (character 0) e cobre mais
    // de uma linha, essa última linha não faz parte do conteúdo selecionado —
    // o editor a inclui apenas como limite. Recuar evita formatar/alterar uma
    // linha que o usuário não selecionou.
    let mut raw_end = range.end.line as usize;
    if raw_end > start_line && range.end.character == 0 {
        raw_end -= 1;
    }
    let end_line = raw_end.min(lines.len().saturating_sub(1));

    // Nível de indentação herdado do contexto externo à seleção (chaves abertas
    // antes do início). O motor formata o trecho relativo a esta base.
    let base_level = u32::try_from(compute_indent_at_line(text, start_line).max(0)).unwrap_or(0);

    let slice: Vec<&str> = lines[start_line..=end_line].to_vec();
    let slice_text = slice.join("\n");
    let formatted_full = super::format_engine::format_with_base(&slice_text, style, base_level);
    // O motor sempre termina com '\n', mas o range selecionado não inclui a quebra
    // final — removê-la evita inserir uma linha em branco e faz a comparação de
    // "sem mudança" bater, para não emitir edit quando nada muda.
    let formatted_slice = formatted_full.strip_suffix('\n').unwrap_or(&formatted_full);

    if formatted_slice == slice_text {
        return vec![];
    }

    let end_char = to_u32(lines.get(end_line).map_or(0, |l| l.len()));
    vec![TextEdit {
        range: Range {
            start: Position {
                line: to_u32(start_line),
                character: 0,
            },
            end: Position {
                line: to_u32(end_line),
                character: end_char,
            },
        },
        new_text: formatted_slice.to_string(),
    }]
}

fn compute_indent_at_line(text: &str, target_line: usize) -> i32 {
    let mut depth = 0i32;
    let mut in_block = false;
    for (i, raw) in text.lines().enumerate() {
        if i >= target_line {
            break;
        }
        let stripped = strip_line_comments(raw, in_block);
        in_block = stripped.in_block;
        for ch in stripped.text.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth = (depth - 1).max(0),
                _ => {}
            }
        }
    }
    depth
}

/// Move chaves que dividem a linha com código para linhas próprias (estilo
/// Allman), ignorando strings/char/comentários. Dá ao motor uma linha por
/// `{`/`}`/statement. Blocos vazios `{}` são preservados.
pub(super) fn split_braces_to_own_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    // Estado de string que ATRAVESSA linhas: em Pawn, uma string literal terminada
    // com `\` no fim físico da linha continua na linha seguinte. Sem carregar isso,
    // o conteúdo continuado (ex.: `{38b170}` de uma cor) seria lido como código e
    // os `{`/`}` virariam blocos. Carrega-se apenas para strings ("), não chars.
    let mut str_continues = false;
    for raw in text.lines() {
        let mut seg = String::new();
        let mut in_str = str_continues;
        let mut in_char = false;
        let mut in_line_comment = false;
        let chars: Vec<char> = raw.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            if in_line_comment {
                seg.push(ch);
                i += 1;
                continue;
            }
            match ch {
                '"' if !in_char => {
                    in_str = !in_str;
                    seg.push(ch);
                }
                '\'' if !in_str => {
                    in_char = !in_char;
                    seg.push(ch);
                }
                '\\' if in_str || in_char => {
                    seg.push(ch);
                    if i + 1 < chars.len() {
                        seg.push(chars[i + 1]);
                        i += 1;
                    }
                }
                '/' if !in_str && !in_char && chars.get(i + 1) == Some(&'/') => {
                    in_line_comment = true;
                    seg.push(ch);
                }
                '{' if !in_str && !in_char && !in_line_comment => {
                    // Lookahead: '{' seguido (ignorando espaços) de '}' é bloco
                    // vazio. Mantém '{}' colado — inclusive ao código que o
                    // precede na mesma linha (ex.: 'if (a) {}'), por legibilidade.
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] == ' ' {
                        j += 1;
                    }
                    if chars.get(j) == Some(&'}') {
                        if seg.trim().is_empty() {
                            out.push_str("{}\n");
                        } else {
                            seg = format!("{} {{}}", seg.trim());
                        }
                        i = j + 1;
                        continue;
                    }
                    // Bloco não-vazio: '{' vai para linha própria (estilo Allman).
                    if !seg.trim().is_empty() {
                        out.push_str(seg.trim());
                        out.push('\n');
                    }
                    seg.clear();
                    out.push_str("{\n");
                }
                '}' if !in_str && !in_char && !in_line_comment => {
                    if !seg.trim().is_empty() {
                        out.push_str(seg.trim());
                        out.push('\n');
                    }
                    seg.clear();
                    out.push_str("}\n");
                }
                _ => seg.push(ch),
            }
            i += 1;
        }
        // A string continua na próxima linha quando ficou aberta e a linha física
        // termina com `\` (continuação). Nesse caso os espaços à esquerda da linha
        // seguinte são CONTEÚDO da string — não podem ser trimados.
        let was_continuation = str_continues;
        str_continues = in_str && raw.ends_with('\\');

        if was_continuation {
            // Linha (ou início dela) faz parte de uma string aberta na anterior:
            // preserva-se literal, sem trim e sem split de chaves.
            out.push_str(raw);
            out.push('\n');
        } else if !seg.trim().is_empty() {
            out.push_str(seg.trim());
            out.push('\n');
        } else if raw.trim().is_empty() {
            // Mantém linhas em branco originais (colapsadas depois).
            out.push('\n');
        }
    }
    out
}

/// Aplica a formatação intra-linha: espaçamento de operadores e de palavras-chave.
/// Quando `space_ops` é `false`, os operadores binários ficam colados (`a+b`);
/// o espaçamento de palavras-chave e o colapso de espaços continuam valendo.
pub(super) fn format_line(trimmed: &str, space_ops: bool) -> String {
    // Comentários e diretivas do preprocessador têm sintaxe própria (`<...>` de
    // include, `%0` de macro, continuação com `\`) e NÃO devem passar pelo
    // espaçamento de operadores — senão `#include <a>` viraria `#include < a >`.
    if trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('#')
    {
        return trimmed.to_string();
    }

    let s = if space_ops {
        format_operators(trimmed)
    } else {
        trimmed.to_string()
    };
    let s = format_keyword_spacing(&s);
    collapse_spaces(&s)
}

/// Operadores que recebem espaço em volta, ordenados do mais longo para o mais
/// curto (longest-match, como o `lex()` do compilador). A ordem importa: sem ela
/// `>>>=` seria fatiado em `>>` + `>=` + `=`.
const SPACED_OPS: &[&str] = &[
    ">>>=", // taSHRU
    "<<=", ">>=", ">>>", // taSHL / taSHR / tSHRU
    "*=", "/=", "%=", "+=", "-=", "&=", "^=", "|=", // atribuições compostas
    "||", "&&", "==", "!=", "<=", ">=", "<<", ">>", // lógicos / comparação / shift
    "=", "<", ">", "&", "|", "^", // de um caractere
];

/// Operadores que NÃO recebem espaço em volta (pós-fixos, ranges, escopo).
/// '...'/'..' são tELLIPS/tDBLDOT; '::' é acesso de escopo (`lex()` distingue de
/// label pelo segundo ':'). '++'/'--' são pós/pré-fixos e devem ficar colados.
const TIGHT_OPS: &[&str] = &["...", "::", "++", "--", ".."];

fn starts_with_at(chars: &[char], i: usize, pat: &str) -> bool {
    let pc: Vec<char> = pat.chars().collect();
    if i + pc.len() > chars.len() {
        return false;
    }
    chars[i..i + pc.len()] == pc[..]
}

/// Emite um operador ambíguo (`+ - * %`) já posicionado: se for unário (no
/// início ou logo após `(`, `,`, `=` ou outro operador) fica colado; se for
/// binário, recebe espaço em volta. Avança `i` para o próximo caractere.
fn push_ambiguous_operator(out: &mut String, chars: &[char], i: &mut usize, ch: char) {
    let is_unary = last_non_space(out).is_none_or(|c| "([{,=+-*/%&|^!<>?:;".contains(c));
    if is_unary {
        out.push(ch);
    } else {
        ensure_space_before(out);
        out.push(ch);
        out.push(' ');
        while *i + 1 < chars.len() && chars[*i + 1] == ' ' {
            *i += 1;
        }
    }
    *i += 1;
}

fn format_operators(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len() + 16);
    let mut i = 0;
    let mut in_str = false;
    let mut in_char = false;

    while i < chars.len() {
        let ch = chars[i];

        match ch {
            '"' if !in_char => {
                in_str = !in_str;
                out.push(ch);
                i += 1;
                continue;
            }
            '\'' if !in_str => {
                in_char = !in_char;
                out.push(ch);
                i += 1;
                continue;
            }
            '\\' if in_str || in_char => {
                out.push(ch);
                if i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            _ if in_str || in_char => {
                out.push(ch);
                i += 1;
                continue;
            }
            _ => {}
        }

        // Operadores "colados" têm prioridade de longest-match e não ganham espaço.
        if let Some(op) = TIGHT_OPS.iter().find(|op| starts_with_at(&chars, i, op)) {
            out.push_str(op);
            i += op.chars().count();
            continue;
        }

        // Operadores com espaço em volta (longest-match pela tabela do compilador).
        if let Some(op) = SPACED_OPS.iter().find(|op| starts_with_at(&chars, i, op)) {
            ensure_space_before(&mut out);
            out.push_str(op);
            out.push(' ');
            i += op.chars().count();
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            continue;
        }

        match ch {
            // '+'/'-'/'*'/'%' são ambíguos: binários ganham espaço, unários
            // (após '(', ',', '=', outro operador, ou início) ficam colados.
            // '/' fica de fora (sempre colado) para não conflitar com '//' e '/*'.
            '+' | '-' | '*' | '%' => {
                push_ambiguous_operator(&mut out, &chars, &mut i, ch);
            }
            ',' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(',');
                out.push(' ');
                i += 1;
                while i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
            }
            ';' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(';');
                i += 1;
                while i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                // Espaço após ';' só quando há mais conteúdo na linha (separadores
                // de 'for(;;)') e o próximo token não fecha o grupo: 'for (;;)'
                // não deve virar 'for (;; )'.
                if i < chars.len() && chars[i] != ')' {
                    out.push(' ');
                }
            }
            // Demais caracteres (incluindo '/', que nunca é espaçado para não
            // colidir com comentários '//' e '/*') são copiados sem alteração.
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }

    out
}

fn ensure_space_before(out: &mut String) {
    if !out.ends_with(' ') && !out.is_empty() {
        out.push(' ');
    }
}

fn last_non_space(s: &str) -> Option<char> {
    s.chars().rev().find(|&c| c != ' ')
}

fn format_keyword_spacing(line: &str) -> String {
    // sizeof/tagof são operadores em Pawn, não keywords de controle: a forma
    // idiomática é 'sizeof(x)'. Inserir espaço ('sizeof (x)') gera aviso no compilador.
    static KW: &[&str] = &["if", "else", "for", "while", "do", "switch", "return"];
    let mut s = line.to_string();
    for kw in KW {
        let pat = format!("{kw}(");
        let rep = format!("{kw} (");
        s = replace_whole_word(&s, &pat, &rep);
    }
    s = replace_whole_word(&s, "else{", "else {");
    s = replace_whole_word(&s, "else if", "else if");
    s
}

fn replace_whole_word(s: &str, from: &str, to: &str) -> String {
    if !s.contains(from) {
        return s.to_string();
    }
    let kw = &from[..from.len() - 1]; // keyword part (without the trailing char)
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find(from) {
        let before = if pos > 0 {
            rest[..pos].chars().last()
        } else {
            None
        };
        let is_word_boundary = before.is_none_or(|c| !c.is_alphanumeric() && c != '_');
        result.push_str(&rest[..pos]);
        if is_word_boundary {
            result.push_str(to);
        } else {
            result.push_str(from);
        }
        rest = &rest[pos + from.len()..];
        let _ = kw;
    }
    result.push_str(rest);
    result
}

fn collapse_spaces(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut prev_space = false;
    let mut in_str = false;
    let mut in_char = false;

    for ch in line.chars() {
        match ch {
            '"' if !in_char => {
                in_str = !in_str;
                out.push(ch);
                prev_space = false;
            }
            '\'' if !in_str => {
                in_char = !in_char;
                out.push(ch);
                prev_space = false;
            }
            ' ' if !in_str && !in_char => {
                if !prev_space {
                    out.push(' ');
                }
                prev_space = true;
            }
            _ => {
                out.push(ch);
                prev_space = false;
            }
        }
    }

    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range {
            start: Position {
                line: sl,
                character: sc,
            },
            end: Position {
                line: el,
                character: ec,
            },
        }
    }

    // format_range não deve inserir '\n' extra após o bloco selecionado, nem
    // emitir edit quando o conteúdo já está formatado.
    #[test]
    fn range_no_trailing_newline_and_noop() {
        let text = "main()\n{\n\tfoo();\n}\n";
        // Seleção exatamente sobre o bloco já formatado: nenhum edit.
        let edits = format_range(
            text,
            rng(0, 0, 3, 1),
            super::super::format_style::FormatStyle::default(),
        );
        assert!(edits.is_empty(), "bloco já formatado não deve gerar edit");

        // Bloco desalinhado: gera edit, mas sem '\n' final e com end correto.
        let text2 = "main()\n{\nfoo();\n}\n";
        let edits2 = format_range(
            text2,
            rng(0, 0, 3, 1),
            super::super::format_style::FormatStyle::default(),
        );
        assert_eq!(edits2.len(), 1);
        assert!(
            !edits2[0].new_text.ends_with('\n'),
            "não pode terminar com '\\n'"
        );
        assert_eq!(
            edits2[0].range.end,
            Position {
                line: 3,
                character: 1
            }
        );
    }

    // Range que termina no início da linha seguinte (character 0) não deve
    // incluir/alterar essa linha — evita criar linha nova ao acionar sem seleção.
    // Usa contexto de bloco real ('{') para que a profundidade seja calculada.
    #[test]
    fn range_excludes_trailing_boundary_line() {
        let text = "main()\n{\n\tfoo();\n\tbar();\n}\n";
        // Cursor na linha do foo() (linha 2); editor manda end na linha 3, char 0.
        let edits = format_range(
            text,
            rng(2, 0, 3, 0),
            super::super::format_style::FormatStyle::default(),
        );
        // foo() já está formatado → nenhum edit, e bar() não é tocado.
        assert!(
            edits.is_empty(),
            "linha já formatada e fora da seleção real não deve gerar edit"
        );
    }

    fn fmt(src: &str) -> String {
        super::super::format_engine::format(src, super::super::format_style::FormatStyle::default())
    }

    // Regressão: '++'/'--' não podem ser separados em '+ +' / '- -'.
    #[test]
    fn preserves_increment_decrement() {
        assert_eq!(
            fmt("for(new i=0;i<3;i++) {}\n"),
            "for (new i = 0; i < 3; i++) {}\n"
        );
        assert_eq!(fmt("while(i--) {}\n"), "while (i--) {}\n");
        assert_eq!(fmt("x++;\n"), "x++;\n");
    }

    // Regressão: sizeof/tagof são operadores; 'sizeof(x)' não vira 'sizeof (x)'.
    #[test]
    fn keeps_sizeof_tagof_tight() {
        assert_eq!(
            fmt("format(s, sizeof(s), \"%d\", n);\n"),
            "format(s, sizeof(s), \"%d\", n);\n"
        );
        assert_eq!(fmt("new x = tagof(y);\n"), "new x = tagof(y);\n");
    }

    // Keywords de controle ainda recebem espaço antes do '('.
    #[test]
    fn control_keywords_get_space() {
        assert_eq!(fmt("if(a) {}\n"), "if (a) {}\n");
        assert_eq!(fmt("while(a) {}\n"), "while (a) {}\n");
    }

    // Tags ('Float:x') e operadores binários básicos permanecem corretos.
    #[test]
    fn binary_operators_and_tags() {
        assert_eq!(
            fmt("new Float:d = a*b + c;\n"),
            "new Float:d = a * b + c;\n"
        );
        assert_eq!(fmt("new Float:x1;\n"), "new Float:x1;\n");
    }

    // Corpo de controle sem chaves (if/for) indenta a próxima statement em +1.
    // Agora atendido pelo motor de formatação estrutural (AST).
    #[test]
    fn implicit_single_body_indents() {
        assert_eq!(fmt("if (a)\nfoo();\n"), "if (a)\n\tfoo();\n");
        assert_eq!(fmt("for (;;)\nbar();\n"), "for (;;)\n\tbar();\n");
        // 'for' contendo 'if' contendo bloco: cada nível indenta +1 e o '}'
        // alinha com seu '{' (caso clássico do warning 217 do compilador).
        let src = "for (i)\nif (x)\n{ a();\nb(); }\nc();\n";
        let expected = "for (i)\n\tif (x)\n\t{\n\t\ta();\n\t\tb();\n\t}\nc();\n";
        assert_eq!(fmt(src), expected);
    }

    // Chave de abertura com statement na mesma linha é normalizada para Allman,
    // eliminando a divergência de indentação 'stmt_sameline' do compilador.
    #[test]
    fn brace_with_inline_statement_is_split() {
        assert_eq!(fmt("{ foo(); }\n"), "{\n\tfoo();\n}\n");
        // Bloco vazio é preservado colado.
        assert_eq!(fmt("if (a) {}\n"), "if (a) {}\n");
    }

    // Regressão (descoberta via sc_tokens do compilador): shifts/atribuições
    // multi-caractere não podem ser fatiados.
    #[test]
    fn multichar_shift_and_assign_ops() {
        assert_eq!(fmt("x>>>=2;\n"), "x >>>= 2;\n");
        assert_eq!(fmt("new a = x>>>1;\n"), "new a = x >>> 1;\n");
        assert_eq!(fmt("x<<=3;\n"), "x <<= 3;\n");
        assert_eq!(fmt("x&=mask;\n"), "x &= mask;\n");
        assert_eq!(fmt("x|=flag;\n"), "x |= flag;\n");
        assert_eq!(fmt("x^=k;\n"), "x ^= k;\n");
    }

    // '...' (tELLIPS) e '..' (tDBLDOT) ficam intactos.
    #[test]
    fn ellipsis_and_range() {
        assert_eq!(fmt("MyFunc(Float:x, ...);\n"), "MyFunc(Float:x, ...);\n");
        assert_eq!(fmt("case 1..5:\n"), "case 1..5:\n");
    }

    // '::' (acesso de escopo) não vira ': :'.
    #[test]
    fn scope_resolution() {
        assert_eq!(fmt("tag::member = 5;\n"), "tag::member = 5;\n");
    }

    // Negação unária e complemento não ganham espaço espúrio.
    #[test]
    fn unary_operators() {
        assert_eq!(fmt("x = -y;\n"), "x = -y;\n");
        assert_eq!(fmt("x &= ~mask;\n"), "x &= ~mask;\n");
        assert_eq!(fmt("new a = (b)? -1 : 1;\n"), "new a = (b)? -1 : 1;\n");
    }
}

#[cfg(test)]
mod doc_format_check {
    use super::*;

    // Garante que format_document (arquivo inteiro) tem as MESMAS garantias já
    // validadas em format_range: sem linha em branco extra no fim, idempotência,
    // e range cobrindo exatamente o documento de ENTRADA (não o de saída).
    fn style() -> super::super::format_style::FormatStyle {
        super::super::format_style::FormatStyle::default()
    }

    #[test]
    fn document_no_trailing_blank_and_idempotent() {
        let src = "stock f()\n{\n    for (new i; i<3; i++)\n    if (x)\n    { a();\n    b(); }\n    return x;\n}\n";
        let edits = format_document(src, style());
        assert_eq!(edits.len(), 1);
        let out = &edits[0].new_text;
        assert!(
            out.ends_with('\n') && !out.ends_with("\n\n"),
            "sem linha em branco no fim"
        );

        // Idempotente: reformatar o resultado não gera mais edits.
        let again = format_document(out, style());
        assert!(again.is_empty(), "formatar de novo não deve mudar nada");

        // Range cobre o documento de entrada (8 '\n' -> termina em (8, 0)).
        let src_newlines = to_u32(src.matches('\n').count());
        assert_eq!(
            edits[0].range.end,
            Position {
                line: src_newlines,
                character: 0
            }
        );
    }

    // Documento inteiro e seleção do bloco inteiro produzem o MESMO texto.
    #[test]
    fn document_and_full_range_agree() {
        let src = "main()\n{\nfoo();\nbar();\n}\n";
        let doc = super::super::format_engine::format(src, style());
        let lines = to_u32(src.matches('\n').count());
        let r = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: lines,
                character: 0,
            },
        };
        let range_edits = format_range(src, r, style());
        let range_out = format!("{}\n", range_edits[0].new_text);
        assert_eq!(doc, range_out, "documento e seleção total devem concordar");
    }
}
