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

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: Option<String>,
    pub params: Vec<Param>,
    pub deprecated: bool,
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
    /// ex: ["BPR", "CMD"] — macros cujo corpo contém `forward` ou `public`
    pub func_macro_prefixes: Vec<String>,
    /// "DOF2" → "DOF2_"; detectado de `#define NAMESPACE:: PREFIX_`
    pub namespace_aliases: std::collections::HashMap<String, String>,
}
