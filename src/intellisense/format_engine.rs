//! Motor de formatação guiado por estrutura.
//!
//! A indentação vem da [`StmtTree`], que conhece o aninhamento real do código; o
//! conteúdo de cada linha é preservado do original (incluindo comentários), apenas
//! re-indentado e com operadores normalizados.

use std::collections::HashMap;

use crate::intellisense::format_indent::indent_levels;
use crate::intellisense::format_style::{BracePlacement, FormatStyle};
use crate::parser::stmt_parser::parse_stmts;
use crate::parser::token_lexer::tokenize_with_tabsize;
use crate::util::to_u32;

use super::formatter::{format_line, split_braces_to_own_lines};

/// Formata o texto inteiro segundo o estilo dado, devolvendo o resultado com um
/// `\n` final canônico.
#[must_use]
pub fn format(text: &str, style: FormatStyle) -> String {
    format_with_base(text, style, 0)
}

/// Formata o texto somando `base_level` a todos os níveis. Usado pela formatação
/// de seleção: o trecho selecionado pode estar dentro de blocos, então recebe o
/// nível de indentação herdado do contexto externo.
#[must_use]
pub fn format_with_base(text: &str, style: FormatStyle, base_level: u32) -> String {
    // Com `preserve_array_alignment`, inicializadores de array `{ ... }` quebrados
    // em várias linhas saem intactos (alinhamento manual em colunas preservado).
    // São protegidos do pipeline e reinseridos no fim, com a 1ª linha re-indentada
    // ao nível estrutural e o miolo verbatim.
    if style.preserve_array_alignment
        && let Some(blocks) = array_init_blocks(text)
    {
        return format_preserving(text, style, base_level, &blocks);
    }

    // Cada '{', '}' e statement em sua própria linha, para a indentação (que é por
    // statement) mapear 1-para-1. O preset K&R rejunta as chaves depois.
    let normalized = split_braces_to_own_lines(text);
    let line_info = analyze_lines(&normalized);
    let unit = style.indent_unit();

    // Cada entrada de `lines` descreve uma linha física do texto de entrada.
    let mut out: Vec<String> = Vec::with_capacity(line_info.len());
    for info in &line_info {
        if info.is_blank {
            // Colapsa sequências de linhas em branco em no máximo uma.
            if out.last().is_some_and(String::is_empty) {
                continue;
            }
            out.push(String::new());
            continue;
        }
        let content = format_line(&info.trimmed, style.space_around_operators);
        let level = (info.level + base_level) as usize;
        out.push(format!("{}{}", unit.repeat(level), content));
    }

    if style.collapse_single_body {
        collapse_braced_single(&mut out);
        collapse_single_bodies(&mut out);
    }
    if style.brace == BracePlacement::SameLine {
        join_braces_knr(&mut out);
    }

    // Remove linhas em branco finais e garante exatamente um '\n' ao final.
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    let mut result = out.join("\n");
    result.push('\n');
    result
}

/// Descrição de uma linha física da entrada, já com o nível de indentação
/// estrutural resolvido.
struct LineInfo {
    trimmed: String,
    level: u32,
    is_blank: bool,
}

/// Intervalo de linhas físicas `[start, end]` (inclusivo) de um inicializador de
/// array multi-linha a preservar verbatim.
struct ArrayBlock {
    start: usize,
    end: usize,
}

/// Encontra inicializadores de array `... = { ... };` que se estendem por mais de
/// uma linha. Só considera o caso multi-linha — um inicializador numa linha só não
/// precisa de proteção. Retorna `None` se não houver nenhum (caminho normal).
///
/// Heurística textual (sem AST): a abertura é uma linha cujo conteúdo de código
/// contém `=` seguido de `{` sem o `}` correspondente na mesma linha; o bloco vai
/// até a linha que zera o balanço de `{}`. Strings, chars e comentários são
/// respeitados via [`brace_delta`].
fn array_init_blocks(text: &str) -> Option<Vec<ArrayBlock>> {
    let phys: Vec<&str> = text.split('\n').collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < phys.len() {
        let line = phys[i];
        if opens_array_init(line) && brace_delta(line) > 0 {
            let mut depth = brace_delta(line);
            let mut j = i + 1;
            while j < phys.len() && depth > 0 {
                depth += brace_delta(phys[j]);
                j += 1;
            }
            let end = (j - 1).min(phys.len() - 1);
            if end > i {
                blocks.push(ArrayBlock { start: i, end });
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    (!blocks.is_empty()).then_some(blocks)
}

/// A linha abre um inicializador de array: tem um `=` seguido (mais à frente) de
/// `{`, fora de string/char/comentário. Captura `new x[] = {`, `... = {`, etc.
fn opens_array_init(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let mut in_str = false;
    let mut in_char = false;
    let mut seen_eq = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' if !in_char => in_str = !in_str,
            '\'' if !in_str => in_char = !in_char,
            '\\' if in_str || in_char => i += 1,
            '/' if !in_str && !in_char && chars.get(i + 1) == Some(&'/') => break,
            '=' if !in_str && !in_char => {
                // `==`/`<=`/`>=`/`!=` não são atribuição. Basta checar vizinhos.
                let prev = i.checked_sub(1).and_then(|p| chars.get(p)).copied();
                let next = chars.get(i + 1).copied();
                if next != Some('=') && !matches!(prev, Some('=' | '!' | '<' | '>')) {
                    seen_eq = true;
                }
            }
            '{' if !in_str && !in_char && seen_eq => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Saldo de `{` menos `}` numa linha, ignorando string/char/comentário.
fn brace_delta(line: &str) -> i32 {
    let chars: Vec<char> = line.chars().collect();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut in_char = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' if !in_char => in_str = !in_str,
            '\'' if !in_str => in_char = !in_char,
            '\\' if in_str || in_char => i += 1,
            '/' if !in_str && !in_char && chars.get(i + 1) == Some(&'/') => break,
            '{' if !in_str && !in_char => depth += 1,
            '}' if !in_str && !in_char => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    depth
}

/// Formata o texto preservando os blocos de inicializador de array. As linhas fora
/// dos blocos passam pelo formatador normal; cada bloco é reinserido verbatim, com
/// a linha de abertura re-indentada ao nível estrutural que o formatador deu a ela.
fn format_preserving(
    text: &str,
    style: FormatStyle,
    base_level: u32,
    blocks: &[ArrayBlock],
) -> String {
    let phys: Vec<&str> = text.split('\n').collect();

    // Substitui cada bloco por um placeholder de UMA linha: um statement simples
    // SEM chaves (`__pp_arrayN__;`), que o pipeline indenta como qualquer outro e
    // o `split_braces_to_own_lines` não desmonta. O placeholder serve só para o
    // formatador resolver o NÍVEL de indentação da declaração; o conteúdo real
    // (abertura + miolo + fecho) é reinserido verbatim do original.
    let mut reduced = Vec::with_capacity(phys.len());
    let mut i = 0;
    while i < phys.len() {
        if let Some((bi, b)) = blocks.iter().enumerate().find(|(_, b)| b.start == i) {
            reduced.push(format!("__pp_array{bi}__;"));
            i = b.end + 1;
        } else {
            reduced.push(phys[i].to_string());
            i += 1;
        }
    }

    // Formata sem a flag (evita recursão infinita).
    let mut sub_style = style;
    sub_style.preserve_array_alignment = false;
    let formatted = format_with_base(&reduced.join("\n"), sub_style, base_level);

    // Reinsere os blocos. A indentação que o formatador deu ao placeholder vira a
    // indentação da linha de ABERTURA (conteúdo trimado do original); o miolo e o
    // fecho saem verbatim, sem qualquer re-indentação.
    let mut out: Vec<String> = Vec::new();
    for fline in formatted.trim_end_matches('\n').split('\n') {
        if let Some(bi) = placeholder_index(fline.trim()) {
            let b = &blocks[bi];
            let indent = leading_ws(fline);
            out.push(format!("{indent}{}", phys[b.start].trim()));
            for line in &phys[b.start + 1..=b.end] {
                out.push((*line).to_string());
            }
        } else {
            out.push(fline.to_string());
        }
    }

    let mut result = out.join("\n");
    result.push('\n');
    result
}

/// Índice do bloco se a linha for exatamente um placeholder `__pp_arrayN__;`.
fn placeholder_index(trimmed: &str) -> Option<usize> {
    let rest = trimmed.strip_prefix("__pp_array")?.strip_suffix("__;")?;
    rest.parse::<usize>().ok()
}

/// Resolve o nível de indentação de cada linha física do texto.
fn analyze_lines(text: &str) -> Vec<LineInfo> {
    let tree = parse_stmts(tokenize_with_tabsize(text, 8));
    let levels = indent_levels(&tree);

    // Nível por linha que inicia um statement (a 1ª vence, se houver várias).
    let mut line_level: HashMap<u32, u32> = HashMap::new();
    for (st, lvl) in tree.stmts.iter().zip(&levels) {
        line_level.entry(st.line).or_insert(*lvl);
    }

    let phys: Vec<&str> = text.split('\n').collect();

    // Comentário alinha com o código que documenta: nível do próximo statement.
    let mut next_level = vec![0u32; phys.len()];
    let mut carry = 0u32;
    for i in (0..phys.len()).rev() {
        if let Some(&lvl) = line_level.get(&to_u32(i)) {
            carry = lvl;
        }
        next_level[i] = carry;
    }

    let mut result = Vec::with_capacity(phys.len());
    let mut last_level = 0u32;
    // Balanço de `(`/`[` acumulado até o INÍCIO da linha. Linhas que começam
    // dentro de um grupo aberto são continuação de assinatura/expressão e
    // indentam +1 (ex.: parâmetros de função quebrados em várias linhas).
    let mut paren_balance = 0i32;
    // Uma declaração `new`/`static`/`const`/`decl` cuja lista de variáveis é
    // quebrada por vírgulas em várias linhas é UM statement só (o parser para no
    // `;`), então o `line_level` só marca a 1ª linha. As linhas seguintes da lista
    // são continuação lógica e indentam +1 — como já se faz com parênteses
    // abertos. Fica ligado até o `;` que fecha a declaração.
    let mut decl_continues = false;

    for (i, raw) in phys.iter().enumerate() {
        let trimmed_raw = raw.trim();
        let is_blank = trimmed_raw.is_empty();
        let is_comment = trimmed_raw.starts_with("//") || trimmed_raw.starts_with('*');

        // Nível-base: do statement que começa aqui; comentário mira o próximo
        // statement; continuação herda o nível lógico corrente.
        let base = if let Some(&lvl) = line_level.get(&to_u32(i)) {
            last_level = lvl;
            lvl
        } else if is_comment {
            next_level[i]
        } else {
            last_level
        };

        // Continuação dentro de parênteses abertos OU de uma lista de declaração
        // multi-linha: +1. Exceto se a linha COMEÇA fechando o grupo (ex.: o `)`
        // que encerra a lista de parâmetros), que alinha com a linha de abertura.
        let starts_closing = trimmed_raw.starts_with(')') || trimmed_raw.starts_with(']');
        let is_stmt_start = line_level.contains_key(&to_u32(i));
        let decl_cont = decl_continues && !is_stmt_start && !is_blank;
        let continuation = u32::from((paren_balance > 0 || decl_cont) && !starts_closing);
        let level = base + continuation;

        // Atualiza o estado da declaração multi-linha. Liga quando um statement de
        // declaração não termina com `;` na própria linha; desliga ao encontrar o
        // `;` (fora de parênteses/strings — `decl_open` checa isso).
        if is_stmt_start && !is_blank && !is_comment {
            decl_continues = starts_decl(trimmed_raw) && decl_open(trimmed_raw);
        } else if decl_continues && !decl_open(trimmed_raw) {
            decl_continues = false;
        }

        paren_balance = (paren_balance + paren_delta(trimmed_raw)).max(0);

        result.push(LineInfo {
            trimmed: trimmed_raw.to_string(),
            level,
            is_blank,
        });
    }
    result
}

/// A linha inicia uma declaração de variável (`new`/`static`/`const`/`decl`)?
/// O token-chave deve aparecer isolado (seguido de espaço), não como prefixo de
/// um identificador (ex.: `newValue`).
fn starts_decl(line: &str) -> bool {
    ["new ", "static ", "const ", "decl "]
        .iter()
        .any(|kw| line.starts_with(kw))
}

/// A declaração segue aberta após esta linha? Ou seja, NÃO há um `;` que a feche
/// (fora de strings, chars e comentário). Uma lista `new a,\n b;` mantém-se aberta
/// na 1ª linha (sem `;`) e fecha na que traz o `;`.
fn decl_open(line: &str) -> bool {
    let mut in_str = false;
    let mut in_char = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' if !in_char => in_str = !in_str,
            '\'' if !in_str => in_char = !in_char,
            '\\' if in_str || in_char => i += 1, // pula o caractere escapado
            '/' if !in_str && !in_char && chars.get(i + 1) == Some(&'/') => break,
            ';' if !in_str && !in_char => return false,
            _ => {}
        }
        i += 1;
    }
    true
}

/// Saldo de `(`/`[` menos `)`/`]` numa linha, ignorando os que estão em strings,
/// chars ou comentário de linha.
fn paren_delta(line: &str) -> i32 {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut in_char = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' if !in_char => in_str = !in_str,
            '\'' if !in_str => in_char = !in_char,
            '\\' if in_str || in_char => i += 1, // pula o caractere escapado
            '/' if !in_str && !in_char && chars.get(i + 1) == Some(&'/') => break,
            '(' | '[' if !in_str && !in_char => depth += 1,
            ')' | ']' if !in_str && !in_char => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    depth
}

/// Colapsa um bloco `{` + UM statement simples + `}` (que o pré-passo separou em
/// 3 linhas) de volta para uma linha, anexando-o ao cabeçalho anterior quando
/// houver: `case X:` / `{` / `id = 0;` / `}` vira `case X: { id = 0; }`.
///
/// É o que dá ao preset Compacto o comportamento esperado em `switch`/`case` e em
/// blocos triviais, em vez de empilhar 3 linhas por corpo.
fn collapse_braced_single(out: &mut Vec<String>) {
    let mut i = 0;
    while i + 2 < out.len() {
        let open = out[i].trim();
        let body = out[i + 1].trim();
        let close = out[i + 2].trim();
        // Bloco isolado de um único statement: `{` / `stmt;` / `}`.
        if open == "{" && close == "}" && is_simple_statement(body) {
            let one_line = format!("{{ {body} }}");
            // Se há um cabeçalho na linha anterior (case/default/if/...), o bloco
            // sobe para junto dele; senão, fica como bloco de uma linha.
            if i > 0 && header_takes_inline_block(out[i - 1].trim()) {
                let head = out[i - 1].trim_end().to_string();
                out[i - 1] = format!("{head} {one_line}");
                out.drain(i..=i + 2);
                i = i.saturating_sub(1);
            } else {
                let indent = leading_ws(&out[i]);
                out[i] = format!("{indent}{one_line}");
                out.drain(i + 1..=i + 2);
            }
        } else {
            i += 1;
        }
    }
}

/// Indentação (espaços/tabs) inicial de uma linha, preservada ao recompor.
fn leading_ws(line: &str) -> String {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// Cabeçalho que pode receber um bloco `{ ... }` na mesma linha: etiquetas de
/// switch (`case`/`default`) e controles (`if`/`for`/`while`/`else`). Não pode já
/// terminar em `{` nem em comentário de linha.
fn header_takes_inline_block(line: &str) -> bool {
    let t = line.trim();
    if t.ends_with('{') || ends_with_line_comment(t) {
        return false;
    }
    let kw = t
        .split(|c: char| c == '(' || c == ':' || c.is_whitespace())
        .next()
        .unwrap_or("");
    matches!(kw, "case" | "default" | "if" | "for" | "while" | "else")
        && (t.ends_with(')') || t.ends_with(':') || t == "else")
}

/// Colapsa o corpo de um controle de um único statement (sem chaves) para a
/// mesma linha do controle: `if (x)` seguido de `foo();` vira `if (x) foo();`.
/// Conservador: só junta quando o corpo é um statement simples — termina em `;`,
/// não abre/fecha bloco e não é outro controle (preserva a estrutura aninhada).
fn collapse_single_bodies(out: &mut Vec<String>) {
    let mut i = 0;
    while i + 1 < out.len() {
        let head = out[i].trim_end().to_string();
        let body = out[i + 1].trim().to_string();
        if is_control_header(&head) && is_simple_statement(&body) {
            out[i] = format!("{head} {body}");
            out.remove(i + 1);
            // Não avança: a linha combinada pode ser, ela mesma, o corpo único de
            // um controle acima (ex.: `else if (x)` recém-formado). Reavalia.
            i = i.saturating_sub(1);
        } else {
            i += 1;
        }
    }
}

/// Cabeçalho de controle cujo corpo (sem chaves) pode ser colapsado: `if`/`for`/
/// `while` terminando em `)`, ou `else` isolado. Não abre bloco (`{`).
fn is_control_header(line: &str) -> bool {
    let t = line.trim();
    if t.ends_with('{') {
        return false;
    }
    let kw = t
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("");
    (matches!(kw, "if" | "for" | "while") && t.ends_with(')')) || t == "else"
}

/// Statement simples que pode subir para a linha do controle: termina em `;`, não
/// mexe em blocos e não é, ele próprio, um controle (que abriria nova estrutura).
fn is_simple_statement(line: &str) -> bool {
    let t = line.trim();
    if !t.ends_with(';') || t.contains('{') || t.contains('}') {
        return false;
    }
    let kw = t
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("");
    !matches!(kw, "if" | "for" | "while" | "else" | "do" | "switch")
}

/// Junta a chave de abertura `{` (que o Allman deixa em linha própria) à linha do
/// controle anterior, produzindo o estilo K&R: `controle {`.
fn join_braces_knr(out: &mut Vec<String>) {
    let mut i = 0;
    while i < out.len() {
        if out[i].trim() == "{" && i > 0 {
            let prev = out[i - 1].trim_end();
            // Não junta se a linha anterior está vazia, é ela mesma '{', ou termina
            // em comentário de linha — colar o '{' após '//' o tornaria comentado.
            if !prev.is_empty() && prev != "{" && !ends_with_line_comment(prev) {
                out[i - 1] = format!("{prev} {{");
                out.remove(i);
                continue;
            }
        }
        i += 1;
    }
}

/// `true` se a linha termina dentro de um comentário de linha (`//...`) fora de
/// string/char — caso em que nada pode ser anexado ao seu final.
fn ends_with_line_comment(line: &str) -> bool {
    let mut in_str = false;
    let mut in_char = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' if !in_char => in_str = !in_str,
            '\'' if !in_str => in_char = !in_char,
            '\\' if in_str || in_char => i += 1,
            '/' if !in_str && !in_char && chars.get(i + 1) == Some(&'/') => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intellisense::format_style::Preset;

    fn fmt(src: &str, preset: Preset) -> String {
        format(src, FormatStyle::from_preset(preset))
    }

    #[test]
    fn for_if_block_allman() {
        // O caso que motivou o motor: chave do bloco alinha com o controle,
        // conteúdo +1, '}' alinha com a chave.
        let src = "main()\n{\nfor (new i = 0; i < 3; i++)\nif (x)\n{\nfoo();\n}\n}\n";
        let out = fmt(src, Preset::Allman);
        let expected =
            "main()\n{\n\tfor (new i = 0; i < 3; i++)\n\t\tif (x)\n\t\t{\n\t\t\tfoo();\n\t\t}\n}\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn knr_joins_braces() {
        let src = "main()\n{\nfoo();\n}\n";
        let out = fmt(src, Preset::Knr);
        // A chave de abertura da função sobe para a linha do cabeçalho.
        assert_eq!(out, "main() {\n\tfoo();\n}\n");
    }

    #[test]
    fn idempotent_allman() {
        let src = "main()\n{\nif (a)\n{\nfoo();\n}\n}\n";
        let once = fmt(src, Preset::Allman);
        let twice = format(&once, FormatStyle::from_preset(Preset::Allman));
        assert_eq!(once, twice, "formatar de novo não deve mudar nada");
    }

    // Lista `new` multi-linha (vírgulas encadeadas): as linhas de continuação
    // indentam +1 sob a declaração, e o `;` em linha própria acompanha. O formato
    // do usuário (declaração quebrada) é preservado, não achatado num único nível.
    #[test]
    fn multiline_new_decl_continuation() {
        let src = "f()\n{\nnew msg[128],\ngenreIndex = -1,\nfound = false\n;\nbar();\n}\n";
        let out = fmt(src, Preset::Allman);
        let expected = "f()\n{\n\tnew msg[128],\n\t\tgenreIndex = -1,\n\t\tfound = false\n\t\t;\n\tbar();\n}\n";
        assert_eq!(out, expected);
    }

    // Variante com o `;` colado na última variável: a continuação fecha na linha
    // do `;` e o statement seguinte volta ao nível-base.
    #[test]
    fn multiline_new_decl_semicolon_attached() {
        let src = "f()\n{\nnew bar[64],\nbaz = 0;\nqux();\n}\n";
        let out = fmt(src, Preset::Allman);
        let expected = "f()\n{\n\tnew bar[64],\n\t\tbaz = 0;\n\tqux();\n}\n";
        assert_eq!(out, expected);
    }

    // Com `preserve_array_alignment`, o miolo de um inicializador de array
    // multi-linha sai INTACTO (alinhamento manual em colunas preservado); só a
    // linha de declaração é re-indentada ao nível estrutural.
    #[test]
    fn preserves_array_alignment_when_enabled() {
        let src = "f()\n{\nnew genres[][12] = {\n        \"axe\",     \"blues\",\n        \"forro\",   \"funk\"\n    };\nbar();\n}\n";
        let mut style = FormatStyle::from_preset(Preset::Allman);
        style.preserve_array_alignment = true;
        let out = format(src, style);
        let expected = "f()\n{\n\tnew genres[][12] = {\n        \"axe\",     \"blues\",\n        \"forro\",   \"funk\"\n    };\n\tbar();\n}\n";
        assert_eq!(out, expected);
    }

    // Caso real do usuário: `new const` com várias colunas e o `;` em linha
    // própria. Tudo do `{` ao `}` sai byte-a-byte igual ao original; só a 1ª linha
    // (declaração) é re-indentada. Confirma idempotência também.
    #[test]
    fn preserves_real_world_array_block() {
        // Miolo (elementos alinhados em colunas) + a linha de fecho `};` colado,
        // exatamente como o usuário escreve.
        let inner = "        \"axe\",     \"blues\",      \"country\",   \"eletronica\",\n        \"forro\",   \"funk\",       \"hiphop\",    \"jazz\",\n        \"mpb\",     \"mclassica\",  \"noticia\",   \"pagode\",\n        \"pop\",     \"rap\",        \"reggae\",    \"rock\",\n        \"samba\",   \"religiao\"\n    };";
        let src = format!("stock f()\n{{\nnew const genre_keys[][12] = {{\n{inner}\nbar();\n}}\n");
        let mut style = FormatStyle::from_preset(Preset::Allman);
        style.preserve_array_alignment = true;
        let out = format(&src, style);
        // Cada linha do miolo (com seus espaços de coluna) aparece intacta.
        for line in inner.split('\n') {
            assert!(out.contains(line), "miolo alterado: faltou {line:?}\n{out}");
        }
        // Idempotente: re-formatar não muda nada.
        assert_eq!(out, format(&out, style));
    }

    // Sem a flag (padrão), o comportamento atual permanece: o bloco passa pelo
    // pipeline normal (não é preservado verbatim). Garante que a opção é opt-in.
    #[test]
    fn array_alignment_not_preserved_by_default() {
        let src = "f()\n{\nnew genres[][12] = {\n        \"axe\",     \"blues\"\n    };\n}\n";
        let out = fmt(src, Preset::Allman);
        // O alinhamento em colunas (espaços múltiplos) NÃO sobrevive ao default.
        assert!(!out.contains("\"axe\",     \"blues\""));
    }

    // Compacto em switch/case: cada `case X: { stmt; }` fica em uma única linha,
    // em vez de empilhar 3 linhas por caso.
    #[test]
    fn compact_switch_cases_single_line() {
        let src = "f()\n{\nswitch (x)\n{\ncase 0 .. 9: { id = 0; }\ndefault: { id = 1; }\n}\n}\n";
        let out = fmt(src, Preset::Compact);
        let expected = "f() {\n\tswitch (x) {\n\t\tcase 0 .. 9: { id = 0; }\n\t\tdefault: { id = 1; }\n\t}\n}\n";
        assert_eq!(out, expected);
    }

    // Diretivas do preprocessador são preservadas: `#include <a>` não pode virar
    // `#include < a >` pelo espaçamento de operadores.
    #[test]
    fn directives_are_not_operator_spaced() {
        let src = "#include <a_samp>\nmain()\n{\nnew x = a<b;\n}\n";
        let out = fmt(src, Preset::Allman);
        assert!(out.contains("#include <a_samp>"), "include preservado");
        assert!(out.contains("a < b"), "operador real ainda espaçado");
    }

    // Preset Compacto: corpo de statement único colapsa numa linha — tanto sem
    // chaves (`if (x) foo();`) quanto com chaves triviais (`if (y) { bar(); }`).
    #[test]
    fn compact_collapses_single_bodies() {
        let src = "main()\n{\nif (x)\nfoo();\nif (y)\n{\nbar();\n}\n}\n";
        let out = fmt(src, Preset::Compact);
        let expected = "main() {\n\tif (x) foo();\n\tif (y) { bar(); }\n}\n";
        assert_eq!(out, expected);
    }

    // Regressão: ao juntar a chave de abertura (K&R/Compacto), uma linha que
    // termina em comentário não pode receber o '{' (ficaria comentado). O '{'
    // permanece em linha própria nesse caso.
    #[test]
    fn knr_keeps_brace_after_line_comment() {
        let src = "main()\n{\nif (x) // nota\n{\nfoo();\n}\n}\n";
        let out = fmt(src, Preset::Knr);
        let expected = "main() {\n\tif (x) // nota\n\t{\n\t\tfoo();\n\t}\n}\n";
        assert_eq!(out, expected);
    }

    // Assinatura de função multi-linha: parâmetros indentam +1; o ')' alinha com
    // o cabeçalho; o corpo segue normalmente.
    #[test]
    fn multiline_signature() {
        let src = "stock fn(\n    const a[][],\n    b\n)\n{\n    return b;\n}\n";
        let out = fmt(src, Preset::Allman);
        let expected = "stock fn(\n\tconst a[][],\n\tb\n)\n{\n\treturn b;\n}\n";
        assert_eq!(out, expected);
    }

    // Comentário alinha com o código que documenta (próximo statement), não com
    // o anterior.
    #[test]
    fn comment_aligns_with_next_stmt() {
        let src = "main()\n{\nfor (i)\n{\n// doc\nfoo();\n}\n}\n";
        let out = fmt(src, Preset::Allman);
        let expected = "main()\n{\n\tfor (i)\n\t{\n\t\t// doc\n\t\tfoo();\n\t}\n}\n";
        assert_eq!(out, expected);
    }

    // Regressão: um corpo implícito que não vaza entre funções/blocos. A segunda
    // função deve indentar do zero, sem resíduo da primeira.
    #[test]
    fn no_implicit_body_leak_between_functions() {
        let src = "a()\n{\nif (x)\nfoo();\n}\nb()\n{\nfor (i)\n{\nbar();\n}\n}\n";
        let out = fmt(src, Preset::Allman);
        let expected =
            "a()\n{\n\tif (x)\n\t\tfoo();\n}\nb()\n{\n\tfor (i)\n\t{\n\t\tbar();\n\t}\n}\n";
        assert_eq!(out, expected);
    }

    // Indentação com espaços em vez de TAB: cada nível usa `tab_size` espaços.
    #[test]
    fn indent_with_spaces() {
        let mut style = FormatStyle::from_preset(Preset::Allman);
        style.insert_spaces = true;
        style.tab_size = 4;
        let out = format("main()\n{\nfoo();\n}\n", style);
        assert_eq!(out, "main()\n{\n    foo();\n}\n");
    }

    // space_around_operators = false: o formatador não insere espaço em volta de
    // operadores (preserva o que o autor escreveu). Com a opção true, o mesmo
    // input ganharia os espaços.
    #[test]
    fn no_space_around_operators() {
        let src = "main()\n{\nnew x=a+b;\n}\n";
        let mut tight = FormatStyle::from_preset(Preset::Allman);
        tight.space_around_operators = false;
        assert!(
            format(src, tight).contains("new x=a+b;"),
            "sem espaço forçado em operadores"
        );
        // Contraste: com a opção ligada (padrão), os espaços são inseridos.
        let spaced = FormatStyle::from_preset(Preset::Allman);
        assert!(
            format(src, spaced).contains("new x = a + b;"),
            "operadores espaçados quando a opção está ligada"
        );
    }

    // Chaves dentro de string/char literal não são tratadas como blocos: a linha
    // permanece intacta e não dispara split/indentação.
    #[test]
    fn braces_in_string_literal_are_untouched() {
        let src = "main()\n{\nprint(\"{ }\");\n}\n";
        let out = format(src, FormatStyle::from_preset(Preset::Allman));
        assert_eq!(out, "main()\n{\n\tprint(\"{ }\");\n}\n");
    }

    // Regressão: string literal continuada com `\` no fim da linha. O conteúdo
    // continuado (incluindo `{cor}` de embeds tipo dialog) NÃO pode ser lido como
    // código — os `{`/`}` não viram blocos e os espaços à esquerda (conteúdo da
    // string) são preservados.
    #[test]
    fn line_continued_string_is_not_split() {
        let src = "main()\n{\nf(\"{38b170}Axe\\n\\\n{8bcffa}Blues\");\n}\n";
        let out = fmt(src, Preset::Allman);
        // A linha de continuação fica literal; nenhum `{`/`}` da string vira bloco.
        assert!(out.contains("{8bcffa}Blues"), "saída: {out:?}");
        assert!(!out.contains("{\n8bcffa"), "cor virou bloco: {out:?}");
        assert!(!out.contains("8bcffa\n}"), "cor virou bloco: {out:?}");
    }

    // Idempotência em todos os presets: formatar a saída não a altera.
    #[test]
    fn idempotent_all_presets() {
        let src =
            "main()\n{\nswitch (x)\n{\ncase 0: { a = 1; }\ndefault: foo();\n}\nif (y)\nbar();\n}\n";
        for preset in [Preset::Allman, Preset::Knr, Preset::Compact] {
            let once = fmt(src, preset);
            let twice = format(&once, FormatStyle::from_preset(preset));
            assert_eq!(once, twice, "preset {preset:?} não é idempotente");
        }
    }
}
