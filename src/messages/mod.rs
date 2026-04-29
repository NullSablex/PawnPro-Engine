mod en;
mod pt_br;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    PtBr,
    #[default]
    En,
}

impl Locale {
    pub fn from_str(s: &str) -> Self {
        let s = s.to_ascii_lowercase();
        if s.starts_with("pt") {
            Self::PtBr
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
}

pub fn msg(locale: Locale, key: MsgKey) -> &'static str {
    match locale {
        Locale::PtBr => pt_br::get(key),
        Locale::En   => en::get(key),
    }
}
