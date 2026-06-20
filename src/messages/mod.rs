mod langs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    PtBr,
    Es,
    Ru,
    Ro,
    #[default]
    En,
}

impl Locale {
    /// Resolve o `Locale` a partir de uma tag de idioma (ex.: "pt-BR", "es", "ru").
    /// Casa pelo prefixo de duas letras; desconhecidos caem em inglês.
    pub fn from_str(s: &str) -> Self {
        let s = s.to_ascii_lowercase();
        if s.starts_with("pt") {
            Self::PtBr
        } else if s.starts_with("es") {
            Self::Es
        } else if s.starts_with("ru") {
            Self::Ru
        } else if s.starts_with("ro") {
            Self::Ro
        } else {
            Self::En
        }
    }
}

#[derive(Clone, Copy)]
pub enum MsgKey {
    IncludeNotFound,
    IncludeTried,
    IncludeNoPathsConfigured,
    IncludeSearchedIn,
    IncludeRelativeTo,
    NativeHasBody,
    ForwardHasBody,
    DeclNoBody,
    VarUnused,
    StockUnused,
    SymDeprecated,
    SymDeprecatedUsage,
    SymFromDeprecatedFile,
    IncludeDeprecated,
    ParamUnused,
    SymbolUndeclared,
    DefineUnused,
    IncludeNoSymbolsUsed,
    TryIncludeNotFound,
    NativeNeverCalled,
    ForwardNeverCalled,
    FuncNeverCalled,
    IndentInconsistent,
    RefsZero,
    RefsOne,
    RefsMany,
    HoverDeprecated,
    KwIf,
    KwIfElse,
    KwElse,
    KwFor,
    KwWhile,
    KwDo,
    KwSwitch,
    KwCase,
    KwDefault,
    KwReturn,
    KwBreak,
    KwContinue,
    KwGoto,
    KwExit,
    KwNewLocal,
    KwSizeof,
    KwTagof,
    KwTrue,
    KwFalse,
    KwCellmax,
    KwCellmin,
    KwCellbits,
    KwStock,
    KwPublic,
    KwForward,
    KwNative,
    KwStatic,
    KwEnum,
    KwConst,
    KwNewGlobal,
    KwDefine,
    KwUndef,
    KwInclude,
    KwTryinclude,
    KwIfDefined,
    KwIfdef,
    KwIfndef,
    KwElseDir,
    KwEndif,
    KwPragma,
    KwAssert,
    KwError,
    KwWarning,
    KwAtDeprecated,
    KwLocal,
    NameTooShort,
    NamePlaceholder,
    NameStyle,
}

pub fn msg(locale: Locale, key: MsgKey) -> &'static str {
    match locale {
        Locale::PtBr => langs::pt_br::get(key),
        Locale::Es => langs::es::get(key),
        Locale::Ru => langs::ru::get(key),
        Locale::Ro => langs::ro::get(key),
        Locale::En => langs::en::get(key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_from_tag_prefix() {
        assert_eq!(Locale::from_str("pt-BR"), Locale::PtBr);
        assert_eq!(Locale::from_str("es"), Locale::Es);
        assert_eq!(Locale::from_str("ru-RU"), Locale::Ru);
        assert_eq!(Locale::from_str("ro"), Locale::Ro);
        assert_eq!(Locale::from_str("en-US"), Locale::En);
        assert_eq!(Locale::from_str("xx"), Locale::En); // desconhecido → inglês
    }

    #[test]
    fn every_locale_resolves_a_message() {
        // Toda variante deve devolver texto não vazio para uma chave qualquer.
        for loc in [Locale::PtBr, Locale::Es, Locale::Ru, Locale::Ro, Locale::En] {
            assert!(!msg(loc, MsgKey::HoverDeprecated).is_empty());
        }
    }
}
