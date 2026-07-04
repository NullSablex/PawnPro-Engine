//! Estilo de formatação configurável.
//!
//! `FormatStyle` reúne as opções da saída do formatador. Os presets são conjuntos
//! pré-definidos dessas opções; `Custom` libera o ajuste individual.

/// Onde a chave de abertura de um bloco é colocada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BracePlacement {
    /// `{` em linha própria, alinhada com o controle (estilo Allman).
    NextLine,
    /// `{` na mesma linha do controle (estilo K&R).
    SameLine,
}

/// Preset de estilo. Define os valores-base; `Custom` permite sobrescrevê-los.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Allman,
    Knr,
    Compact,
    Custom,
}

impl Preset {
    /// Resolve o preset a partir do nome recebido na configuração (case-insensitive).
    /// Valores desconhecidos caem no padrão `Allman`.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "knr" | "k&r" | "kr" => Preset::Knr,
            "compact" | "compacto" => Preset::Compact,
            "custom" => Preset::Custom,
            _ => Preset::Allman,
        }
    }
}

/// Conjunto completo de opções de formatação já resolvidas (preset + overrides).
///
/// São opções de estilo independentes; agrupar os booleanos em sub-structs só
/// reduziria a clareza de uma struct de configuração plana, daí o `allow`.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
pub struct FormatStyle {
    /// Posição da chave de abertura de blocos.
    pub brace: BracePlacement,
    /// Largura de um nível de indentação (em caracteres, quando `insert_spaces`).
    pub tab_size: u32,
    /// `true` = indenta com espaços; `false` = com TAB.
    pub insert_spaces: bool,
    /// Espaço em volta de operadores binários (`a + b` vs `a+b`).
    pub space_around_operators: bool,
    /// Mantém blocos vazios colados ao controle (`if (a) {}`) em vez de quebrar.
    pub empty_block_same_line: bool,
    /// Colapsa o corpo de um controle de um único statement (sem chaves) na mesma
    /// linha: `if (x)\n foo();` vira `if (x) foo();`. Estilo do preset Compacto.
    pub collapse_single_body: bool,
    /// Preserva o alinhamento manual em colunas de inicializadores de array `{ }`
    /// multi-linha: as linhas internas saem intactas (sem colapsar os espaços de
    /// alinhamento nem re-indentar). Opt-in — o padrão é re-indentar como o resto.
    pub preserve_array_alignment: bool,
}

impl Default for FormatStyle {
    fn default() -> Self {
        Self::from_preset(Preset::Allman)
    }
}

impl FormatStyle {
    /// Opções-base de um preset. `Custom` parte de Allman; os campos individuais
    /// são então sobrescritos pela configuração do usuário.
    #[must_use]
    pub fn from_preset(preset: Preset) -> Self {
        let brace = match preset {
            Preset::Knr | Preset::Compact => BracePlacement::SameLine,
            Preset::Allman | Preset::Custom => BracePlacement::NextLine,
        };
        Self {
            brace,
            tab_size: 4,
            insert_spaces: false,
            space_around_operators: true,
            // Blocos vazios ficam colados em todos os presets, por legibilidade.
            empty_block_same_line: true,
            // Só o Compacto colapsa corpos de statement único na mesma linha.
            collapse_single_body: matches!(preset, Preset::Compact),
            // Opt-in em todos os presets; ligado pela config do usuário.
            preserve_array_alignment: false,
        }
    }

    /// String de um nível de indentação, conforme `insert_spaces`/`tab_size`.
    #[must_use]
    pub fn indent_unit(self) -> String {
        if self.insert_spaces {
            " ".repeat(self.tab_size as usize)
        } else {
            "\t".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_brace_defaults() {
        assert_eq!(
            FormatStyle::from_preset(Preset::Allman).brace,
            BracePlacement::NextLine
        );
        assert_eq!(
            FormatStyle::from_preset(Preset::Knr).brace,
            BracePlacement::SameLine
        );
        assert_eq!(
            FormatStyle::from_preset(Preset::Compact).brace,
            BracePlacement::SameLine
        );
    }

    #[test]
    fn preset_from_name_is_lenient() {
        assert_eq!(Preset::from_name("Allman"), Preset::Allman);
        assert_eq!(Preset::from_name("K&R"), Preset::Knr);
        assert_eq!(Preset::from_name("compacto"), Preset::Compact);
        assert_eq!(Preset::from_name("custom"), Preset::Custom);
        assert_eq!(Preset::from_name("xpto"), Preset::Allman); // fallback
    }

    #[test]
    fn indent_unit_tab_or_spaces() {
        let mut s = FormatStyle::default();
        assert_eq!(s.indent_unit(), "\t");
        s.insert_spaces = true;
        s.tab_size = 4;
        assert_eq!(s.indent_unit(), "    ");
    }

    #[test]
    fn indent_unit_respects_tab_size() {
        let two = FormatStyle {
            insert_spaces: true,
            tab_size: 2,
            ..Default::default()
        };
        assert_eq!(two.indent_unit(), "  ");
        let eight = FormatStyle {
            insert_spaces: true,
            tab_size: 8,
            ..Default::default()
        };
        assert_eq!(eight.indent_unit(), " ".repeat(8));
    }

    #[test]
    fn only_compact_collapses_single_body() {
        assert!(FormatStyle::from_preset(Preset::Compact).collapse_single_body);
        assert!(!FormatStyle::from_preset(Preset::Allman).collapse_single_body);
        assert!(!FormatStyle::from_preset(Preset::Knr).collapse_single_body);
        assert!(!FormatStyle::from_preset(Preset::Custom).collapse_single_body);
    }
}
