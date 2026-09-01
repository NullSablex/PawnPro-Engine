//! Validação das diretivas `#pragma`.
//!
//! O compilador rejeita um `#pragma` que não conheça (erro 207), mas só na
//! compilação; aqui o erro aparece enquanto se escreve. A lista abaixo espelha
//! `sc2.c` do compilador do open.mp.

use crate::analyzer::{codes, diagnostic::PawnDiagnostic};
use crate::messages::{Locale, MsgKey, msg};
use crate::parser::lexer::strip_line_comments;
use crate::util::to_u32;

/// Diretivas aceitas pelo compilador (`sc2.c`).
const KNOWN: &[&str] = &[
    "align",
    "amxlimit",
    "amxram",
    "codepage",
    "compat",
    "compress",
    "ctrlchar",
    "deprecated",
    "disable",
    "dynamic",
    "enable",
    "library",
    "naked",
    "nodestruct",
    "option",
    "pack",
    "pop",
    "push",
    "rational",
    "semicolon",
    "tabsize",
    "unread",
    "unused",
    "unwritten",
    "warning",
];

/// O que fazer com um `#pragma` malformado, além de apontá-lo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PragmaFix {
    /// Trocar o nome da diretiva pelo desta sugestão.
    Rename(String),
    /// Remover as aspas em volta da mensagem de `deprecated`.
    Unquote(String),
}

/// Distância de edição, limitada a `max`: acima disso a sugestão viraria chute.
fn edit_distance_within(a: &str, b: &str, max: usize) -> Option<usize> {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(cur[j] + 1).min(prev[j + 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let d = prev[b.len()];
    (d <= max).then_some(d)
}

/// A diretiva conhecida mais próxima de `word`, quando há uma plausível.
fn closest_known(word: &str) -> Option<&'static str> {
    let lower = word.to_ascii_lowercase();
    // Um nome curto tolera menos erro: com `max` fixo, "pack" viraria sugestão
    // para qualquer palavra de quatro letras.
    let max = if lower.len() <= 4 { 1 } else { 2 };
    KNOWN
        .iter()
        .filter_map(|k| edit_distance_within(&lower, k, max).map(|d| (d, *k)))
        .min_by_key(|(d, k)| (*d, k.len()))
        .map(|(_, k)| k)
}

/// Uma diretiva `#pragma` malformada, já com a correção sugerida.
pub struct PragmaIssue {
    pub line: u32,
    pub col: u32,
    pub col_end: u32,
    /// A diretiva como escrita, para compor a mensagem.
    pub word: String,
    pub kind: IssueKind,
    pub fix: Option<PragmaFix>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueKind {
    Unknown,
    UnknownWithSuggestion(&'static str),
    DeprecatedQuoted,
}

impl PragmaIssue {
    /// Mensagem no idioma resolvido.
    pub fn message(&self, locale: Locale) -> String {
        match &self.kind {
            IssueKind::Unknown => msg(locale, MsgKey::PragmaUnknown).replace("{}", &self.word),
            IssueKind::UnknownWithSuggestion(s) => msg(locale, MsgKey::PragmaUnknownDidYouMean)
                .replace("{}", &self.word)
                .replace("{sug}", s),
            IssueKind::DeprecatedQuoted => msg(locale, MsgKey::PragmaDeprecatedQuoted).to_string(),
        }
    }
}

/// Verifica as diretivas `#pragma` do texto.
pub fn analyze_pragmas(text: &str, locale: Locale) -> Vec<PawnDiagnostic> {
    collect_issues(text)
        .into_iter()
        .map(|i| {
            let message = i.message(locale);
            PawnDiagnostic::warning(i.line, i.col, i.col_end, codes::PP0019, message)
        })
        .collect()
}

/// Separado de `analyze_pragmas` para que o quick fix reaproveite a análise.
pub fn collect_issues(text: &str) -> Vec<PragmaIssue> {
    let mut out = Vec::new();
    let mut in_block = false;
    for (idx, raw) in text.lines().enumerate() {
        let stripped = strip_line_comments(raw, in_block);
        in_block = stripped.in_block;
        let line = &stripped.text;

        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('#') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix("pragma") else {
            continue;
        };
        if !rest.is_empty() && !rest.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let after = rest.trim_start();
        // Coluna do nome da diretiva, contada no texto original.
        let name_col = line.len() - after.len();

        let word: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if word.is_empty() {
            continue;
        }

        if !KNOWN.contains(&word.to_ascii_lowercase().as_str()) {
            let suggestion = closest_known(&word);
            out.push(PragmaIssue {
                line: to_u32(idx),
                col: to_u32(name_col),
                col_end: to_u32(name_col + word.len()),
                kind: suggestion.map_or(IssueKind::Unknown, IssueKind::UnknownWithSuggestion),
                word,
                fix: suggestion.map(|s| PragmaFix::Rename(s.to_string())),
            });
            continue;
        }

        // `deprecated` toma o resto da linha como texto livre: aspas em volta
        // entram na mensagem em vez de delimitá-la.
        if word.eq_ignore_ascii_case("deprecated") {
            let arg = after[word.len()..].trim();
            if let Some(inner) = unquoted(arg) {
                let arg_col = line.len() - after.len() + (after.len() - after[word.len()..].len());
                let arg_start = arg_col + (after[word.len()..].len() - arg.len());
                out.push(PragmaIssue {
                    line: to_u32(idx),
                    col: to_u32(arg_start),
                    col_end: to_u32(arg_start + arg.len()),
                    word,
                    kind: IssueKind::DeprecatedQuoted,
                    fix: Some(PragmaFix::Unquote(inner.to_string())),
                });
            }
        }
    }
    out
}

/// O conteúdo de `s` quando ele está inteiro entre aspas duplas.
fn unquoted(s: &str) -> Option<&str> {
    let inner = s.strip_prefix('"')?.strip_suffix('"')?;
    // Aspas no meio do texto são literais legítimas, não delimitadores.
    (!inner.contains('"')).then_some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues(src: &str) -> Vec<PragmaIssue> {
        collect_issues(src)
    }

    #[test]
    fn known_pragmas_are_accepted() {
        for src in [
            "#pragma deprecated Use OutraFuncao",
            "#pragma tabsize 4",
            "#pragma unused x",
            "#pragma option -d3",
            "  #  pragma   semicolon 1",
        ] {
            assert!(issues(src).is_empty(), "{src}");
        }
    }

    #[test]
    fn typo_suggests_the_closest_directive() {
        let i = issues("#pragma deprected Use Outra");
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].fix, Some(PragmaFix::Rename("deprecated".into())));
        assert!(
            i[0].message(Locale::En).contains("deprecated"),
            "{}",
            i[0].message(Locale::En)
        );
    }

    #[test]
    fn unknown_without_a_close_match_has_no_fix() {
        let i = issues("#pragma zzzzzzzz");
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].fix, None);
    }

    #[test]
    fn quoted_deprecated_message_is_flagged() {
        let i = issues("#pragma deprecated \"Use BanPlayerFor\"");
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0].fix,
            Some(PragmaFix::Unquote("Use BanPlayerFor".into()))
        );
    }

    #[test]
    fn unquoted_message_is_fine_and_quotes_inside_are_kept() {
        assert!(issues("#pragma deprecated Use BanPlayerFor").is_empty());
        // Aspas no meio são texto legítimo, não delimitadores.
        assert!(issues(r#"#pragma deprecated diga "olá" a ela"#).is_empty());
    }

    #[test]
    fn deprecated_without_message_is_fine() {
        assert!(issues("#pragma deprecated").is_empty());
    }

    #[test]
    fn directive_inside_a_comment_is_ignored() {
        assert!(issues("// #pragma deprected x").is_empty());
        assert!(issues("/*\n#pragma deprected x\n*/").is_empty());
    }

    #[test]
    fn column_points_at_the_directive_name() {
        let i = issues("  #pragma deprected x");
        assert_eq!(i[0].col, 10);
        assert_eq!(i[0].col_end, 19);
    }
}
