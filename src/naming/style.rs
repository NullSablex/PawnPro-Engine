//! Classificação e checagem de estilo de caixa (`camelCase`, `snake_case`, …).
//!
//! Determinístico e tolerante: um `_` inicial e dígitos não desqualificam um
//! estilo. Na dúvida, não acusa.

/// Estilo de caixa esperado para uma categoria de identificador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Camel,
    Snake,
    Pascal,
    Upper,
    /// Cada palavra capitalizada, separada por `_` (ex.: `Carregar_Lixeiras`).
    CapSnake,
}

impl Case {
    /// Resolve a partir do valor de configuração. Desconhecido/`"off"` → `None`
    /// (a categoria não é checada).
    #[must_use]
    pub fn from_config(s: &str) -> Option<Self> {
        match s {
            "camelCase" => Some(Self::Camel),
            "snake_case" => Some(Self::Snake),
            "PascalCase" => Some(Self::Pascal),
            "UPPER_CASE" => Some(Self::Upper),
            "Capitalized_Snake" => Some(Self::CapSnake),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Camel => "camelCase",
            Self::Snake => "snake_case",
            Self::Pascal => "PascalCase",
            Self::Upper => "UPPER_CASE",
            Self::CapSnake => "Capitalized_Snake",
        }
    }
}

/// `true` se `name` está conforme `expected`. Um `_` inicial (descarte/privado)
/// é removido antes da checagem; nomes vazios após isso são considerados
/// conformes (nada a dizer). Prefixos como `g` de global não recebem tratamento
/// especial — `gName` já é `camelCase` válido, `g_name` já é `snake_case` válido.
#[must_use]
pub fn matches(name: &str, expected: Case) -> bool {
    let core = name.strip_prefix('_').unwrap_or(name);
    if core.is_empty() {
        return true;
    }
    match expected {
        Case::Camel => is_camel(core),
        Case::Snake => is_snake(core),
        Case::Pascal => is_pascal(core),
        Case::Upper => is_upper(core),
        Case::CapSnake => is_cap_snake(core),
    }
}

/// Rótulo legível do estilo, para a mensagem ao usuário.
#[must_use]
pub fn label(expected: Case) -> &'static str {
    expected.label()
}

fn is_camel(s: &str) -> bool {
    let mut chars = s.chars();
    // Primeiro caractere minúsculo; sem `_`; não totalmente em maiúsculas.
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && !s.contains('_')
        && s.chars().any(|c| c.is_ascii_lowercase())
}

fn is_pascal(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && !s.contains('_')
        && s.chars().any(|c| c.is_ascii_lowercase())
}

fn is_snake(s: &str) -> bool {
    // Tudo minúsculo/dígito, separado por `_`; sem maiúsculas.
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && s.chars().any(|c| c.is_ascii_lowercase())
}

fn is_upper(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && s.chars().any(|c| c.is_ascii_uppercase())
}

/// `Carregar_Lixeiras` e também `Palavrao` (uma palavra): cada segmento separado
/// por `_` começa com maiúscula e contém ao menos uma minúscula. O `_` é
/// opcional — uma palavra só capitalizada degenera nesse estilo, exatamente como
/// em `PascalCase`. A exigência de minúscula em cada segmento distingue de
/// `UPPER_CASE` (`MAX_PLAYERS`), onde os segmentos são todos maiúsculos.
fn is_cap_snake(s: &str) -> bool {
    if !s.chars().any(|c| c.is_ascii_lowercase()) {
        return false;
    }
    s.split('_').all(|seg| {
        seg.is_empty()
            || (seg.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && seg.chars().all(|c| c.is_ascii_alphanumeric())
                && seg.chars().any(|c| c.is_ascii_lowercase()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_values() {
        assert_eq!(Case::from_config("camelCase"), Some(Case::Camel));
        assert_eq!(Case::from_config("snake_case"), Some(Case::Snake));
        assert_eq!(Case::from_config("PascalCase"), Some(Case::Pascal));
        assert_eq!(Case::from_config("UPPER_CASE"), Some(Case::Upper));
        assert_eq!(Case::from_config("off"), None);
        assert_eq!(Case::from_config(""), None);
    }

    #[test]
    fn camel_accepts_and_rejects() {
        assert!(matches("playerHealth", Case::Camel));
        assert!(matches("count", Case::Camel));
        assert!(!matches("PlayerHealth", Case::Camel)); // Pascal
        assert!(!matches("player_health", Case::Camel)); // snake
        assert!(!matches("MAX_VALUE", Case::Camel));
    }

    #[test]
    fn snake_accepts_and_rejects() {
        assert!(matches("player_health", Case::Snake));
        assert!(matches("count", Case::Snake));
        assert!(!matches("playerHealth", Case::Snake));
        assert!(!matches("MAX", Case::Snake));
    }

    #[test]
    fn pascal_accepts_and_rejects() {
        assert!(matches("PlayerState", Case::Pascal));
        assert!(!matches("playerState", Case::Pascal));
        assert!(!matches("Player_State", Case::Pascal));
    }

    #[test]
    fn upper_accepts_and_rejects() {
        assert!(matches("MAX_PLAYERS", Case::Upper));
        assert!(matches("LIMIT", Case::Upper));
        assert!(!matches("maxPlayers", Case::Upper));
    }

    #[test]
    fn cap_snake_accepts_and_rejects() {
        assert!(matches("Carregar_Lixeiras", Case::CapSnake));
        assert!(matches("Carregar_Caixa_Eletronico", Case::CapSnake));
        // Uma palavra capitalizada degenera no estilo (o `_` é opcional), igual a
        // PascalCase — ambíguo por natureza, ambos aceitam.
        assert!(matches("Palavrao", Case::CapSnake));
        assert!(matches("CarregarLixeiras", Case::CapSnake));
        assert!(!matches("carregar_lixeiras", Case::CapSnake)); // snake
        assert!(!matches("MAX_PLAYERS", Case::CapSnake)); // Upper
        assert!(!matches("Carregar_LIXEIRAS", Case::CapSnake)); // segmento todo maiúsculo
    }

    #[test]
    fn cap_snake_config_and_suggestion() {
        assert_eq!(Case::from_config("Capitalized_Snake"), Some(Case::CapSnake));
        assert_eq!(label(Case::CapSnake), "Capitalized_Snake");
    }

    #[test]
    fn underscore_prefix_is_ignored() {
        assert!(matches("_count", Case::Camel));
        assert!(matches("_playerHealth", Case::Camel));
    }

    #[test]
    fn global_prefix_is_ignored() {
        // `g_name` (snake-ish) e `gName` (camel-ish) não são penalizados pelo `g`.
        assert!(matches("gPlayerCount", Case::Camel));
        assert!(matches("g_player_count", Case::Snake));
    }
}
