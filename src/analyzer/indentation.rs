use crate::messages::{msg, Locale, MsgKey};
use crate::parser::token_lexer::{tokenize, tokenize_with_tabsize};
use crate::parser::stmt_parser::{parse_stmts, StmtKind};

use super::{codes, diagnostic::PawnDiagnostic};

fn extract_tabsize(text: &str) -> Option<u32> {
    let mut result = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("#pragma") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix("tabsize")
                && let Ok(n) = rest.trim().parse::<u32>()
            {
                result = Some(n);
            }
        }
    }
    result
}

pub fn analyze_indentation(text: &str, include_texts: &[&str], global_tabsize: Option<u32>, locale: Locale) -> Vec<PawnDiagnostic> {
    // Prioridade: #pragma tabsize no próprio texto > includes diretos > tabsize global dos inc paths.
    let mut effective_tabsize = global_tabsize;
    for inc in include_texts {
        if let Some(n) = extract_tabsize(inc) {
            effective_tabsize = Some(n);
        }
    }
    if let Some(n) = extract_tabsize(text) {
        effective_tabsize = Some(n);
    }

    let stream = match effective_tabsize {
        Some(ts) => tokenize_with_tabsize(text, ts),
        None => tokenize(text),
    };
    let tree = parse_stmts(stream);

    let mut diags = Vec::new();
    let mut block_indent: Vec<Option<u32>> = vec![None];
    let mut depth: u32 = 0;
    // Quando um if/while/for/else não é seguido de '{', o próximo statement é o body
    // implícito — exatamente como o compilador chama statement() recursivamente sem
    // incrementar o bloco. Esse statement deve ser ignorado na checagem do bloco pai.
    let mut skip_next_as_implicit_body = false;

    let stmts = &tree.stmts;
    let mut i = 0;
    while i < stmts.len() {
        let stmt = &stmts[i];
        i += 1;

        match stmt.kind {
            StmtKind::BlockOpen => {
                skip_next_as_implicit_body = false;
                depth += 1;
                if block_indent.len() <= depth as usize {
                    block_indent.push(None);
                } else {
                    block_indent[depth as usize] = None;
                }
                continue;
            }
            StmtKind::BlockClose => {
                skip_next_as_implicit_body = false;
                depth = depth.saturating_sub(1);
                continue;
            }
            StmtKind::Pragma | StmtKind::Include | StmtKind::Define | StmtKind::Label => continue,
            _ => {}
        }

        let is_control = matches!(stmt.kind, StmtKind::If | StmtKind::While | StmtKind::For | StmtKind::Else | StmtKind::Do);
        let next_is_block = matches!(stmts.get(i).map(|s| &s.kind), Some(StmtKind::BlockOpen));

        if skip_next_as_implicit_body {
            skip_next_as_implicit_body = false;
            // Se o stmt skipado é ele mesmo uma estrutura de controle (ex: o `if` de `else if`),
            // seu body também é implícito e deve ser skipado — a menos que seja seguido de {}.
            if is_control && !next_is_block {
                skip_next_as_implicit_body = true;
            }
            continue;
        }

        if is_control && !next_is_block {
            skip_next_as_implicit_body = true;
        }

        if depth == 0 {
            continue;
        }

        let idx = depth as usize;
        let si = stmt.stmt_indent;

        if let Some(last) = block_indent.get(idx).copied().flatten() {
            if si != last {
                let message = msg(locale, MsgKey::IndentInconsistent)
                    .replacen("{}", &last.to_string(), 1)
                    .replacen("{}", &si.to_string(), 1);
                diags.push(PawnDiagnostic::warning(
                    stmt.line,
                    stmt.col,
                    stmt.col + 1,
                    codes::PP0017,
                    message,
                ));
            }
        } else if idx < block_indent.len() {
            block_indent[idx] = Some(si);
        }
    }

    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag_lines(src: &str) -> Vec<u32> {
        analyze_indentation(src, &[], None, Locale::En).iter().map(|d| d.line).collect()
    }

    #[test]
    fn consistent_indent_no_warning() {
        let src = "main()\n{\n\tfoo();\n\tbar();\n}";
        assert!(diag_lines(src).is_empty());
    }

    #[test]
    fn inconsistent_indent_warns() {
        let src = "main()\n{\n\tfoo();\n    bar();\n}";
        let lines = diag_lines(src);
        assert!(!lines.is_empty(), "esperava PP0017 mas não gerou");
        assert!(lines.contains(&3), "deve apontar para a linha de bar()");
    }

    #[test]
    fn nested_blocks_consistent_no_warning() {
        let src = "main()\n{\n\tif (x)\n\t{\n\t\tfoo();\n\t\tbar();\n\t}\n}";
        assert!(diag_lines(src).is_empty());
    }

    #[test]
    fn gamemode_template_no_warning() {
        let src = concat!(
            "#include <open.mp>\n",
            "\n",
            "main()\n",
            "{\n",
            "}\n",
            "\n",
            "public OnGameModeInit()\n",
            "{\n",
            "\tSetGameModeText(\"Blank Script\");\n",
            "\tAddPlayerClass(0, 1958.3783, 1343.1572, 15.3746, 269.1425, 0, 0, 0, 0, 0, 0);\n",
            "\treturn 1;\n",
            "}\n",
            "\n",
            "public OnGameModeExit()\n",
            "{\n",
            "\treturn 1;\n",
            "}\n",
        );
        let lines = diag_lines(src);
        assert!(lines.is_empty(), "template gamemode não deve ter PP0017: {:?}", lines);
    }

    #[test]
    fn tabsize_from_include_normalizes_mixed() {
        let foreach_inc = "#pragma tabsize 4\n";
        let src = "public OnGameModeInit()\n{\n    SetGameModeText(\"test\");\n\tUsePlayerPedAnims();\n\treturn 1;\n}\n";
        let diags = analyze_indentation(src, &[foreach_inc], None, Locale::En);
        assert!(
            diags.is_empty(),
            "com tabsize=4 do include, tab e 4 espaços são equivalentes: {:?}",
            diags.iter().map(|d| format!("L{}: {}", d.line, d.message)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn pragma_tabsize_in_file_takes_effect() {
        let src = "#pragma tabsize 4\nmain()\n{\n    foo();\n    bar();\n}";
        assert!(diag_lines(src).is_empty());
    }

    #[test]
    fn if_without_braces_no_warning() {
        let src = concat!(
            "for (new i = 0; i < MAX; i++)\n",
            "{\n",
            "    if (arr[i] == 0)\n",
            "        continue;\n",
            "    foo(i);\n",
            "}\n",
        );
        assert!(diag_lines(src).is_empty(), "body implícito de if não deve gerar PP0017");
    }

    #[test]
    fn else_if_without_braces_no_warning() {
        let src = concat!(
            "main()\n",
            "{\n",
            "    if (lixos == 0)\n",
            "        strcat(s, \"Vazia\");\n",
            "    else if (lixos >= 10)\n",
            "        strcat(s, \"Cheia\");\n",
            "    foo();\n",
            "}\n",
        );
        let lines = diag_lines(src);
        assert!(lines.is_empty(), "else if sem chaves não deve gerar PP0017: linhas {:?}", lines);
    }

    #[test]
    fn else_with_braces_tab_inside_no_warning() {
        let foreach_inc = "#pragma tabsize 4\n";
        let src = concat!(
            "public OnFoo(playerid)\n",
            "{\n",
            "    if (response) {\n",
            "        foo();\n",
            "    } else {\n",
            "        bar();\n",
            "    }\n",
            "}\n",
        );
        let diags = analyze_indentation(src, &[foreach_inc], None, Locale::En);
        assert!(diags.is_empty(), "else {{ }} com indentação uniforme não deve gerar PP0017: {:?}",
            diags.iter().map(|d| format!("L{}: {}", d.line, d.message)).collect::<Vec<_>>());
    }

    #[test]
    fn else_block_tab_mixed_with_tabsize4() {
        let foreach_inc = "#pragma tabsize 4\n";
        let src = concat!(
            "    if (response) {\n",
            "        foo();\n",
            "    } else {\n",
            "        SendClientMessage(playerid, -1, \"msg\");\n",
            "\t\tShowPlayerDialog(playerid, 0, 0, \"x\", \"y\", \"a\", \"b\");\n",
            "    }\n",
        );
        let diags = analyze_indentation(src, &[foreach_inc], None, Locale::En);
        assert!(diags.is_empty(),
            "tab e espaços equivalentes com tabsize=4 não devem gerar PP0017: {:?}",
            diags.iter().map(|d| format!("L{}: {}", d.line, d.message)).collect::<Vec<_>>());
    }

    #[test]
    fn func_call_paren_on_next_line_no_warning() {
        let src = concat!(
            "public OnPlayerRequestClass(playerid, classid)\n",
            "{\n",
            "    new direction = 1;\n",
            "    SetPlayerCameraPos\n",
            "    (\n",
            "        playerid,\n",
            "        arr[ idx{playerid} ][0],\n",
            "        arr[ idx{playerid} ][1],\n",
            "        arr[ idx{playerid} ][2]\n",
            "    );\n",
            "    return true;\n",
            "}\n",
        );
        let diags = analyze_indentation(src, &[], None, Locale::En);
        assert!(diags.is_empty(), "( na próxima linha com {{}} em args não deve gerar PP0017: {:?}",
            diags.iter().map(|d| format!("L{}: {}", d.line, d.message)).collect::<Vec<_>>());
    }

    #[test]
    fn inconsistent_warns_even_with_tabsize4() {
        let foreach_inc = "#pragma tabsize 4\n";
        let src = "main()\n{\n\tfoo();\n  bar();\n}";
        let diags = analyze_indentation(src, &[foreach_inc], None, Locale::En);
        assert!(!diags.is_empty(), "2 espaços != tab(4) mesmo com tabsize=4");
    }
}
