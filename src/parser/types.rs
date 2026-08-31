#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Native,
    Forward,
    Public,
    Stock,
    Static,
    /// Função sem keyword — tratada pelo compilador como "global não-stock":
    /// não exportada no AMX, não isenta de warning 203 se não chamada internamente.
    Plain,
    /// Constante: membro de enum, `stock const`, `static const`
    StaticConst,
    /// Nome de enum declarado: `enum NomeDoEnum { ... }`
    Enum,
    Define,
    /// Variável declarada com `new` ou `static` (não constante)
    Variable,
    /// Constante declarada com `const`
    Const,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub tag: Option<String>, // ex: "Float" em "Float:x"
    pub is_variadic: bool,   // "..."
}

/// Marcação de `#pragma deprecated`: se o símbolo está descontinuado e, quando
/// a diretiva trouxe uma, a mensagem que acompanha o aviso de uso.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deprecation {
    pub is_deprecated: bool,
    pub message: Option<String>,
}

impl Deprecation {
    pub const NONE: Self = Self {
        is_deprecated: false,
        message: None,
    };

    pub fn marked(message: Option<String>) -> Self {
        Self {
            is_deprecated: true,
            message,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: Option<String>,
    pub params: Vec<Param>,
    pub deprecated: bool,
    /// Texto após `#pragma deprecated`, repassado no aviso de uso. `None`
    /// quando a diretiva não trouxe mensagem.
    pub deprecated_message: Option<String>,
    pub doc: Option<String>,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct IncludeDirective {
    pub token: String,
    pub is_angle: bool,
    /// ausência do arquivo não é erro
    pub is_try: bool,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Default, Clone)]
pub struct ParsedFile {
    pub symbols: Vec<Symbol>,
    pub includes: Vec<IncludeDirective>,
    pub macro_names: Vec<String>,
    pub deprecated_macros: Vec<String>,
    /// ex: `["CMD", "FN"]` — macros cujo corpo contém `forward` ou `public`
    pub func_macro_prefixes: Vec<String>,
    /// `NS` → `NS_`; detectado de `#define NS:: NS_`
    pub namespace_aliases: std::collections::HashMap<String, String>,
}
