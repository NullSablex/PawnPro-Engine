//! Sugestão determinística de nome para um identificador sinalizado.
//!
//! Não inventa semântica: converte caixa para o estilo-alvo e, na falta de base,
//! devolve nada (o usuário renomeia manualmente). Conservador por desenho —
//! melhor não sugerir do que sugerir algo pior.

use super::style::Case;

/// Sugere uma forma de `name` no estilo `target`. `None` quando já está no
/// estilo ou não há conversão sensata (ex.: nome de uma só letra sem palavras).
#[must_use]
pub fn to_style(name: &str, target: Case) -> Option<String> {
    let words = split_words(name);
    if words.is_empty() {
        return None;
    }
    let candidate = join_words(&words, target);
    (candidate != name).then_some(candidate)
}

/// Quebra um identificador em palavras, lidando com `snake_case`, `camelCase`,
/// `PascalCase` e `UPPER_CASE`. Dígitos ficam grudados na palavra anterior.
fn split_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;

    for ch in name.chars() {
        if ch == '_' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        // Maiúscula após minúscula inicia nova palavra (fronteira camelCase).
        if ch.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(ch.to_ascii_lowercase());
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn join_words(words: &[String], target: Case) -> String {
    match target {
        Case::Snake => words.join("_"),
        Case::Upper => words.join("_").to_ascii_uppercase(),
        Case::Camel => {
            let mut out = String::new();
            for (idx, w) in words.iter().enumerate() {
                if idx == 0 {
                    out.push_str(w);
                } else {
                    out.push_str(&capitalize(w));
                }
            }
            out
        }
        Case::Pascal => words.iter().map(|w| capitalize(w)).collect(),
        Case::CapSnake => words
            .iter()
            .map(|w| capitalize(w))
            .collect::<Vec<_>>()
            .join("_"),
    }
}

fn capitalize(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_to_camel() {
        assert_eq!(
            to_style("player_health", Case::Camel).as_deref(),
            Some("playerHealth")
        );
    }

    #[test]
    fn camel_to_snake() {
        assert_eq!(
            to_style("playerHealth", Case::Snake).as_deref(),
            Some("player_health")
        );
    }

    #[test]
    fn to_pascal() {
        assert_eq!(
            to_style("player_state", Case::Pascal).as_deref(),
            Some("PlayerState")
        );
    }

    #[test]
    fn to_upper() {
        assert_eq!(
            to_style("maxPlayers", Case::Upper).as_deref(),
            Some("MAX_PLAYERS")
        );
    }

    #[test]
    fn to_cap_snake() {
        assert_eq!(
            to_style("carregar_lixeiras", Case::CapSnake).as_deref(),
            Some("Carregar_Lixeiras")
        );
        assert_eq!(
            to_style("loadTrashCans", Case::CapSnake).as_deref(),
            Some("Load_Trash_Cans")
        );
    }

    #[test]
    fn keeps_digits_attached() {
        assert_eq!(
            to_style("slot1count", Case::Pascal).as_deref(),
            Some("Slot1count")
        );
    }

    #[test]
    fn none_when_already_in_style() {
        assert!(to_style("playerHealth", Case::Camel).is_none());
    }

    #[test]
    fn none_for_single_letter() {
        // Uma letra vira uma palavra; converter para o mesmo estilo não muda nada.
        assert!(to_style("x", Case::Camel).is_none());
    }
}
