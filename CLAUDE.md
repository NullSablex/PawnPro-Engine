# PawnPro Engine — Guia para Agentes de IA

## O que é este projeto

Servidor LSP em Rust para a linguagem **Pawn** (SA-MP / open.mp). Comunica-se com a extensão VS Code [PawnPro](https://github.com/NullSablex/PawnPro) via stdin/stdout usando o Language Server Protocol.

---

## Regras absolutas

- **Zero `unwrap()` em código de produção** fora de `Lazy::new`. Usar `?`, `if let`, `unwrap_or`, ou `unwrap_or_else`.
- **Nunca usar `panic!` em caminhos de análise**. O motor precisa sobreviver a qualquer entrada malformada.
- **Regexes estáticas via `once_cell::sync::Lazy<Regex>`**. Nunca compilar regex dentro de loops.
- **Nunca interpolar input do usuário diretamente em `Regex::new`** sem escapar metacaracteres.
- **`cargo clippy -- -D warnings` deve passar sem erros** antes de qualquer commit.
- **Novos diagnósticos sempre em `analyzer/codes.rs`** com constante `pub const PP####`.

---

## Estrutura de módulos

```
src/
  main.rs              ← entry point LSP (tower-lsp)
  server.rs            ← handlers LSP: initialize, completion, hover, etc.
  workspace.rs         ← analisa um arquivo: chama parser + todos os analyzers
  config.rs            ← EngineConfig recebida via initializationOptions
  parser/
    lexer.rs           ← decode_bytes, strip_line_comments, update_brace_depth,
                          has_inline_deprecated — utilitários de texto
    symbols.rs         ← parser principal: extrai Symbol, IncludeDirective, macros
    types.rs           ← ParsedFile, Symbol, SymbolKind, IncludeDirective, Param
    mod.rs             ← re-exports públicos
  analyzer/
    codes.rs           ← constantes PP0001–PP0013
    diagnostic.rs      ← PawnDiagnostic, Severity, construtores
    includes.rs        ← PP0001, PP0013 — resolve #include / #tryinclude
    semantic.rs        ← PP0002, PP0003, PP0004 — erros estruturais
    unused.rs          ← PP0005, PP0006, PP0011, PP0012 — código morto
    hints.rs           ← PP0009 — parâmetros não utilizados
    deprecated.rs      ← PP0007, PP0008 — @DEPRECATED
    undefined.rs       ← PP0010 — funções não declaradas
    mod.rs             ← re-exports + função analyze() que orquestra tudo
  intellisense/
    completion.rs      ← textDocument/completion
    hover.rs           ← textDocument/hover
    signature.rs       ← textDocument/signatureHelp
    codelens.rs        ← textDocument/codeLens + codeLens/resolve
    references.rs      ← textDocument/references
    semantic_tokens.rs ← textDocument/semanticTokens/full
    mod.rs
```

---

## Tipos centrais

### `ParsedFile` (`parser/types.rs`)
Resultado do parsing de um arquivo. Contém:
- `symbols: Vec<Symbol>` — todas as declarações
- `includes: Vec<IncludeDirective>` — todas as diretivas `#include` / `#tryinclude`
- `macro_names: Vec<String>` — nomes de `#define` (subconjunto de symbols)
- `deprecated_macros: Vec<String>` — macros marcadas com `@DEPRECATED`
- `func_macro_prefixes: Vec<String>` — prefixos como `CMD`, `BPR` que geram `forward`/`public`
- `namespace_aliases: HashMap<String, String>` — ex: `"DOF2"` → `"DOF2_"`

### `Symbol` (`parser/types.rs`)
- `kind: SymbolKind` — `Native | Forward | Public | Stock | Static | StaticConst | Define | Variable`
- `deprecated: bool` — marcado com `@DEPRECATED`
- `doc: Option<String>` — comentário de documentação acima da declaração
- `line: u32`, `col: u32` — posição 0-based em bytes UTF-8

### `IncludeDirective` (`parser/types.rs`)
- `is_angle: bool` — `true` para `<token>`, `false` para `"caminho"`
- `is_try: bool` — `true` para `#tryinclude` (ausência não é erro)

### `PawnDiagnostic` (`analyzer/diagnostic.rs`)
Construtores disponíveis:
- `::error(...)` — Severity::Error
- `::warning(...)` — Severity::Warning
- `::unnecessary_warning(...)` — Warning + `unnecessary: true` (texto desbotado no editor)
- `::hint(...)` — Severity::Hint + `unnecessary: true`
- `::deprecated_decl(...)` — Warning + `deprecated: true` (na própria declaração)
- `::deprecated_warning(...)` — Warning + `deprecated: true` (nos usos)

---

## Fluxo de análise (`workspace.rs`)

Para cada arquivo aberto ou alterado:
1. `decode_bytes(bytes)` — decodifica UTF-8 com fallback latin-1
2. `parse_file(text, path)` — extrai `ParsedFile`
3. `resolve_includes(parsed, path, include_paths)` — resolve recursivamente todos os includes transitivos → `ResolvedIncludes`
4. Cada analyzer recebe `(text, path, parsed, resolved)` e retorna `Vec<PawnDiagnostic>`
5. Diagnósticos publicados via `client.publish_diagnostics()`

---

## Adicionando um novo diagnóstico

1. Adicionar constante em `analyzer/codes.rs`: `pub const PP00XX: &str = "PP00XX";`
2. Criar ou editar o analyzer correspondente em `analyzer/`
3. Chamar o analyzer em `analyzer/mod.rs` dentro de `analyze()`
4. Documentar em `docs/diagnostics.md`

---

## Adicionando uma nova capacidade LSP

1. Implementar em `intellisense/`
2. Registrar o handler em `server.rs`
3. Declarar a capability em `server_capabilities()` em `server.rs`
4. Documentar em `docs/lsp.md`

---

## Lexer (`parser/lexer.rs`)

Funções utilitárias canônicas — **não duplicar em outros módulos**:

| Função | Descrição |
|--------|-----------|
| `decode_bytes(bytes)` | UTF-8 com fallback latin-1 |
| `strip_line_comments(line, in_block)` | Remove `//` e `/* */`, rastreia estado de bloco |
| `update_brace_depth(ch, depth, in_str, in_char)` | Rastreia profundidade de `{}` ignorando literais |
| `has_inline_deprecated(line)` | Detecta `@DEPRECATED` inline na mesma linha |

---

## Configuração recebida via LSP

```rust
pub struct EngineConfig {
    pub include_paths: Vec<PathBuf>,
    pub warn_unused_in_inc: bool,
    pub sdk_file_path: Option<PathBuf>,
    pub workspace_folder: Option<PathBuf>,
}
```

Recebida em `initializationOptions` e atualizada via `workspace/didChangeConfiguration`.

---

## Build e testes

```bash
cargo build                    # debug
cargo build --release          # release (LTO + strip + panic=abort)
cargo test                     # testes unitários
cargo clippy -- -D warnings    # deve passar sem erros
```

---

## Gotchas

- `update_brace_depth` em `lexer.rs` é a versão canônica — lida com literais string/char. Não criar versões alternativas em outros módulos.
- Diagnósticos PP0005/PP0006 usam `unnecessary_warning`, não `warning` — isso ativa o estilo "desbotado" no editor.
- PP0010 não é emitido em arquivos `.inc` (apenas `.pwn`).
- PP0011/PP0012/PP0013 são emitidos como Hint, não Warning — evitar ruído em bibliotecas.
- `is_try: false` deve ser definido explicitamente em todos os literais `IncludeDirective` fora do parser.
- Semantic tokens detectam chamadas multiline: olham até 3 linhas adiante por `(`.
