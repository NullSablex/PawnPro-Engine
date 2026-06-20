//! Rename de símbolos. Reaproveita `get_references` para localizar todas as
//! ocorrências e as converte num `WorkspaceEdit`. Sem renomear arquivos nem
//! tocar em símbolos de bibliotecas — apenas texto nas ocorrências encontradas.

use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

use super::references::get_references;
use crate::text::word_range_at;
use crate::workspace::WorkspaceState;

/// Valida que há um identificador na posição e devolve seu intervalo (para o
/// editor destacar o alvo do rename). `None` quando não há palavra.
#[must_use]
pub fn prepare_rename(state: &WorkspaceState, uri: &str, pos: Position) -> Option<Range> {
    let doc = state.open_docs.get(uri)?;
    word_range_at(&doc.text, pos)
}

/// Produz o `WorkspaceEdit` que renomeia todas as ocorrências do símbolo sob o
/// cursor para `new_name`. `None` se não houver o que renomear.
#[must_use]
pub fn get_rename(
    state: &WorkspaceState,
    uri: &str,
    pos: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let locations = get_references(state, uri, pos);
    if locations.is_empty() {
        return None;
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for loc in locations {
        changes.entry(loc.uri).or_default().push(TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        });
    }

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const URI: &str = "file:///test.pwn";

    fn state_with(text: &str) -> WorkspaceState {
        let st = WorkspaceState::new();
        st.open_document(URI.to_string(), text.to_string(), 1);
        st
    }

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn prepare_returns_word_range() {
        let st = state_with("stock count() {}\n");
        let range = prepare_rename(&st, URI, at(0, 8)).unwrap();
        assert_eq!(range.start.character, 6); // início de "count"
        assert_eq!(range.end.character, 11); // fim de "count"
    }

    #[test]
    fn prepare_none_off_identifier() {
        let st = state_with("stock count() {}\n");
        // coluna 5 = o espaço entre "stock" e "count" não toca identificador? Na
        // verdade encosta no fim de "stock"; uso um ponto claramente vazio.
        assert!(prepare_rename(&st, URI, at(0, 13)).is_none()); // dentro de "()"
    }

    #[test]
    fn rename_renames_all_occurrences() {
        // `n` aparece como parâmetro e no corpo; ambos devem ser renomeados.
        let text = "stock dbl(n) { return n + n; }\n";
        let st = state_with(text);
        let edit = get_rename(&st, URI, at(0, 10), "value").unwrap();
        let changes = edit.changes.unwrap();
        let edits = &changes[&URI.parse::<Url>().unwrap()];
        // 3 ocorrências de `n`: parâmetro + duas no corpo.
        assert_eq!(edits.len(), 3, "esperava 3 edições, got: {edits:?}");
        assert!(edits.iter().all(|e| e.new_text == "value"));
    }

    #[test]
    fn rename_none_when_no_word() {
        let st = state_with("stock f() {}\n");
        assert!(get_rename(&st, URI, at(0, 9), "x").is_none()); // dentro de "()"
    }

    // Fluxo do code action de estilo: dado um nome fora da convenção, a sugestão
    // (camelCase) vira um rename de todas as ocorrências. Une suggestions_for +
    // get_rename como o handler de code action faz.
    #[test]
    fn style_suggestion_drives_rename() {
        use crate::config::{NamingConfig, StyleConfig};

        let text = "stock get_thing() { return get_thing(); }\n";
        let st = state_with(text);
        let cfg = NamingConfig {
            enabled: true,
            style: StyleConfig {
                functions: vec!["camelCase".to_string()],
                ..StyleConfig::default()
            },
            ..NamingConfig::default()
        };

        let suggestions = crate::naming::suggestions_for("get_thing", &cfg);
        assert_eq!(suggestions, vec!["getThing"]);

        // Renomear para a sugestão atinge declaração + chamada.
        let edit = get_rename(&st, URI, at(0, 7), &suggestions[0]).unwrap();
        let edits = &edit.changes.unwrap()[&URI.parse::<Url>().unwrap()];
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_text == "getThing"));
    }
}
