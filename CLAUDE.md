# PawnPro Engine — Guia para Agentes de IA

## O que é este projeto

Servidor LSP em Rust para a linguagem **Pawn** (SA-MP / open.mp). Comunica-se com a extensão [PawnPro](https://github.com/NullSablex/PawnPro) via stdin/stdout usando o Language Server Protocol.

---

## Regras absolutas

- **Zero `unwrap()` em código de produção** fora de `Lazy::new`. Usar `?`, `if let`, `unwrap_or`, ou `unwrap_or_else`.
- **Nunca usar `panic!` em caminhos de análise**. O motor precisa sobreviver a qualquer entrada malformada.
- **Regexes estáticas via `once_cell::sync::Lazy<Regex>`**. Nunca compilar regex dentro de loops.
- **Nunca interpolar input do usuário diretamente em `Regex::new`** sem escapar metacaracteres.
- **`cargo clippy -- -D warnings` deve passar sem erros** antes de qualquer commit.
- **Novos diagnósticos sempre em `analyzer/codes.rs`** com constante `pub const PP####`.
- **`parsed_cache` usa `PathBuf` como chave**, não `String` (URI). Converter URI → path antes de acessar.
- **`dep_graph` usa `PathBuf` em ambos os lados** — nunca `String`/URI. Converter antes de inserir.
- **Sem comentários óbvios**. Apenas comentários que explicam *por quê* — restrições ocultas, invariantes sutis, workarounds de bugs específicos.

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

### `WorkspaceState` (`workspace.rs`)
```rust
pub struct WorkspaceState {
    pub parsed_cache: DashMap<PathBuf, Arc<ParsedFile>>,
    pub dep_graph: DashMap<PathBuf, HashSet<PathBuf>>,  // include_path → paths que o incluem
    pub tabsize_cache: Mutex<Option<Option<u32>>>,
    // ...
}
```
- `parsed_cache` usa `PathBuf` como chave — **nunca String/URI**.
- `dep_graph` é o grafo reverso de dependências puro em paths: `c.inc → {b.inc}`, `b.inc → {a.pwn}`. Permite invalidar transitivamente pelo BFS.
- `tabsize_cache` usa `Mutex` para interior mutability em `&self`.

Métodos relevantes:
- `open_dependents(uri)` — BFS no `dep_graph` a partir de um URI, retorna URIs dos arquivos **abertos no editor** que dependem transitivamente dele.
- `evict_dependents(path)` — BFS no `dep_graph` removendo do `parsed_cache` todos os dependentes transitivos.
- `evict_path_from_cache(path)` — remove o path e seus dependentes do cache (usado por `did_change_watched_files`).

### `ParsedFile` (`parser/types.rs`)
Armazenado como `Arc<ParsedFile>` no cache. Contém:
- `symbols: Vec<Symbol>` — todas as declarações
- `includes: Vec<IncludeDirective>` — todas as diretivas `#include` / `#tryinclude`
- `macro_names: Vec<String>` — nomes de `#define`
- `deprecated_macros: Vec<String>` — macros marcadas com `@DEPRECATED`
- `func_macro_prefixes: Vec<String>` — prefixos como `CMD`, `BPR`
- `namespace_aliases: HashMap<String, String>`

### `ResolvedIncludes` (`analyzer/includes.rs`)
```rust
pub struct ResolvedIncludes {
    pub paths: Vec<PathBuf>,
    pub files: HashMap<PathBuf, IncludeEntry>,
    pub reverse_deps: HashMap<PathBuf, HashSet<PathBuf>>,
}
```
`reverse_deps` é construído durante a resolução BFS e usado por `record_dependencies` em `workspace.rs` para atualizar `dep_graph`.

### `Symbol` (`parser/types.rs`)
- `kind: SymbolKind` — `Native | Forward | Public | Stock | Static | Plain | StaticConst | Enum | Define | Variable | Const`
  - `Plain` — função sem keyword (global não-stock); não exportada no AMX
  - `StaticConst` — constante: membro de enum, `stock const`, `static const`
  - `Enum` — nome do enum declarado (`enum NomeDoEnum { ... }`)
  - `Const` — constante declarada com `const`
- `deprecated: bool` — marcado com `@DEPRECATED`
- `doc: Option<String>` — comentário de documentação acima da declaração
- `line: u32`, `col: u32` — posição 0-based em bytes UTF-8

### `PawnDiagnostic` (`analyzer/diagnostic.rs`)
Construtores disponíveis:
- `::error(...)` — Severity::Error
- `::warning(...)` — Severity::Warning
- `::unnecessary_warning(...)` — Warning + `unnecessary: true` (texto desbotado no editor)
- `::hint(...)` — Severity::Hint + `unnecessary: true`
- `::deprecated_decl(...)` — Warning + `deprecated: true` (na própria declaração)
- `::deprecated_warning(...)` — Warning + `deprecated: true` (nos usos)

### `ConfigUpdate` (`server.rs`)
Struct extraída para eliminar duplicação entre `initialize` e `did_change_configuration`:
- `from_init_options(value)` — lê de `initializationOptions`
- `from_settings(value)` — lê de `workspace/didChangeConfiguration`
- `apply_init(&mut config)` — aplica campos relevantes ao init
- `apply_change(&mut config)` — aplica campos relevantes à atualização

---

## Fluxo de análise (`workspace.rs`)

Para cada arquivo aberto ou alterado:
1. `decode_bytes(bytes)` — decodifica UTF-8 com fallback latin-1
2. `parse_file(text, path)` — extrai `ParsedFile`, armazena como `Arc<ParsedFile>` em `parsed_cache`
3. `resolve_includes(parsed, path, include_paths)` — resolve recursivamente todos os includes transitivos → `ResolvedIncludes` (inclui `reverse_deps`)
4. `record_dependencies(&resolved.reverse_deps)` — atualiza `dep_graph`
5. Cada analyzer recebe `(text, path, parsed, resolved)` e retorna `Vec<PawnDiagnostic>`
6. Diagnósticos publicados via `client.publish_diagnostics()`

### Invalidação granular de cache e republicação

`dep_graph: DashMap<PathBuf, HashSet<PathBuf>>` mapeia cada include para o conjunto de arquivos que o incluem diretamente. `record_dependencies` propaga o mapa completo de `reverse_deps` retornado por `collect_included_files` — preservando a estrutura `c.inc → {b.inc}`, `b.inc → {a.pwn}`.

Quando um include muda:
1. `evict_dependents(path)` — BFS sobre `dep_graph`, remove do `parsed_cache` todos os dependentes transitivos.
2. `open_dependents(uri)` — BFS sobre `dep_graph`, coleta os URIs dos arquivos abertos que dependem transitivamente do include alterado.
3. Os handlers `did_change`, `did_save` e `did_change_watched_files` chamam `open_dependents` e republicam diagnósticos para cada dependente aberto — garantindo que `main.pwn` receba diagnósticos atualizados quando `b.inc` ou `c.inc` mudam, mesmo que o usuário não edite `main.pwn`.

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

## Configuração

### `EngineConfig` (`config.rs`) — carregada do disco

```rust
pub struct EngineConfig {
    pub include_paths: Vec<String>,   // suporta ${workspaceFolder}
    pub analysis: AnalysisConfig,
}
pub struct AnalysisConfig {
    pub warn_unused_in_inc: bool,
    pub suppress_diagnostics_in_inc: bool,
    pub sdk: SdkConfig,
}
pub struct SdkConfig {
    pub platform: String,
    pub file_path: String,
}
```

Carregada automaticamente de `~/.pawnpro/config.json` (global) e `.pawnpro/config.json` (projeto). Merge: projeto sobrescreve global; valores não-default ganham. `${workspaceFolder}` é substituído em runtime por `resolved_include_paths()`.

### `ConfigUpdate` (`server.rs`) — recebida via LSP

Campos aceitos em `initializationOptions` e em `workspace/didChangeConfiguration`:

| Campo JSON | Tipo | Destino em `WorkspaceState` |
|------------|------|-----------------------------|
| `includePaths` | `string[]` | `include_paths_override` |
| `warnUnusedInInc` | `bool` | `config.analysis.warn_unused_in_inc` |
| `suppressDiagnosticsInInc` | `bool` | `config.analysis.suppress_diagnostics_in_inc` |
| `sdkFilePath` | `string` | `sdk_file` |
| `locale` | `string` | `locale` (`"pt-BR"` → `Locale::PtBr`) |

`apply_init` aplica na inicialização; `apply_change` aplica em tempo real e retorna `bool` indicando se algo mudou (para controlar o republish). Nunca duplicar lógica de parsing entre os dois caminhos — usar `ConfigUpdate`.

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
- `deprecated_decl` usa `deprecated: true` — isso ativa o strikethrough no editor via `DiagnosticTag::DEPRECATED`. Não usar `warning` simples para declarações depreciadas.
- `Arc<ParsedFile>`: ao estender símbolos de um include, usar `.clone()` — `all.extend(inc_parsed.symbols.clone())`.
- `unused.rs` usa `collect_workspace_all()` — função única que faz um único walkdir em vez de três chamadas separadas a `collect_workspace()`.
- `open_docs` na engine guarda a chave como URI completa (`file:///...`). Nunca fazer `format!("file://{}", key)` — já tem o prefixo.
- `dep_graph` guarda `PathBuf → HashSet<PathBuf>` — nunca URIs. `open_dependents` faz a conversão path→URI na saída, não na entrada.
- `collect_transitive_exports` em `unused.rs` faz BFS sobre `ResolvedIncludes` para coletar símbolos diretos e transitivos de um include antes de emitir PP0012 — não checar apenas `entry.parsed.symbols` diretamente.
- `did_change`/`did_save`/`did_change_watched_files` em `server.rs` republicam os dependentes abertos via `open_dependents`, não apenas o arquivo alterado.
