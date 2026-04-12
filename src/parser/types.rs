/// Tipo de símbolo Pawn detectado durante o parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Native,
    Forward,
    Public,
    Stock,
    Static,
    StaticConst,
    Define,
    Variable, // new / static (variável) / const
}

/// Parâmetro de uma função.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub tag: Option<String>, // ex: "Float" em "Float:x"
    pub is_variadic: bool,   // "..."
}

/// Símbolo declarado em um arquivo.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// Assinatura completa para funções, ex: "CreateVehicle(modelid, Float:x, ...)"
    pub signature: Option<String>,
    pub params: Vec<Param>,
    /// Marcado com // @DEPRECATED ou /* @DEPRECATED */
    pub deprecated: bool,
    /// Comentário de documentação acima da declaração.
    pub doc: Option<String>,
    /// Linha (0-based).
    pub line: u32,
    /// Coluna do início do nome (0-based, em bytes UTF-8 do rawLine).
    pub col: u32,
}

/// Diretiva #include em um arquivo.
#[derive(Debug, Clone)]
pub struct IncludeDirective {
    /// Token como escrito: "a_samp" ou "../utils"
    pub token: String,
    /// true para <token>, false para "token"
    pub is_angle: bool,
    pub line: u32,
    pub col: u32,
}

/// Resultado do parsing de um arquivo.
#[derive(Debug, Default, Clone)]
pub struct ParsedFile {
    pub symbols: Vec<Symbol>,
    pub includes: Vec<IncludeDirective>,
    /// Nomes de macros (#define) — subconjunto de `symbols` para acesso rápido.
    pub macro_names: Vec<String>,
    /// Macros marcadas como depreciadas.
    pub deprecated_macros: Vec<String>,
}
