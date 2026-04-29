# Copilot Instructions — PawnPro Engine

## Contexto

Servidor LSP em Rust para a linguagem Pawn (SA-MP / open.mp). Comunica com a extensão PawnPro via stdin/stdout usando o Language Server Protocol (tower-lsp).

## Regras de código

- Zero `unwrap()` em código de produção fora de `Lazy::new`. Usar `?`, `if let`, `unwrap_or`.
- Nunca `panic!` em caminhos de análise — o motor precisa sobreviver a qualquer entrada.
- Regexes sempre como `static Lazy<Regex>` — nunca compilar dentro de loops.
- Nunca interpolar input não escapado em `Regex::new`.
- `cargo clippy -- -D warnings` deve passar sem erros.
- Novos diagnósticos sempre com constante em `analyzer/codes.rs`.
- Sem comentários óbvios — apenas comentários que explicam *por quê*.

## Estrutura

```
src/
  parser/       ← lexer.rs, symbols.rs, types.rs  (parsing puro)
  analyzer/     ← codes.rs, diagnostic.rs, includes.rs, semantic.rs,
                  unused.rs, hints.rs, deprecated.rs, undefined.rs
  intellisense/ ← completion, hover, signature, codelens, references,
                  semantic_tokens
  workspace.rs  ← orquestra parser + analyzers; gerencia cache e dep_graph
  server.rs     ← handlers LSP; usa ConfigUpdate para init e didChangeConfiguration
  config.rs     ← EngineConfig
```

## Cache e invalidação granular

`WorkspaceState` em `workspace.rs`:
- `parsed_cache: DashMap<PathBuf, Arc<ParsedFile>>` — chave é **PathBuf**, nunca String/URI
- `dep_graph: DashMap<PathBuf, HashSet<String>>` — grafo reverso: include_path → URIs que dependem dele
- `tabsize_cache: Mutex<Option<Option<u32>>>` — interior mutability para `&self`

Quando um include muda: `evict_dependents(uri)` invalida só os arquivos que dependem dele, não o cache inteiro.

`ResolvedIncludes` contém `reverse_deps` construído durante BFS — usado por `record_dependencies` para atualizar `dep_graph`.

## Utilitários canônicos (`parser/lexer.rs`)

Não duplicar em outros módulos:
- `decode_bytes` — UTF-8 com fallback latin-1
- `strip_line_comments` — remove `//` e `/* */`, rastreia estado de bloco
- `update_brace_depth` — rastreia `{}` ignorando literais string/char
- `has_inline_deprecated` — detecta `@DEPRECATED` inline

## Diagnósticos

| Código | Severidade | Construtor |
|--------|------------|------------|
| PP0001 | Error | `::error` |
| PP0002–PP0003 | Error | `::error` |
| PP0004, PP0008, PP0010 | Warning | `::warning` / `::deprecated_warning` |
| PP0005–PP0006 | Warning + unnecessary | `::unnecessary_warning` |
| PP0007 (declaração) | Warning + deprecated | `::deprecated_decl` |
| PP0007 (uso) | Warning + deprecated | `::deprecated_warning` |
| PP0009, PP0011–PP0013 | Hint | `::hint` |

`deprecated: true` ativa `DiagnosticTag::DEPRECATED` (strikethrough no editor) — usar `deprecated_decl` na declaração, nunca `warning` simples.

## ConfigUpdate (`server.rs`)

Struct que elimina duplicação entre `initialize` e `did_change_configuration`:
- `from_init_options(value)` / `from_settings(value)` — parsing
- `apply_init(&mut config)` / `apply_change(&mut config)` — aplicação

## Gotchas

- `Arc<ParsedFile>`: ao estender símbolos de um include, usar `.clone()` — `all.extend(inc_parsed.symbols.clone())`.
- PP0010 não é emitido em `.inc`, apenas em `.pwn`.
- `unused.rs` usa `collect_workspace_all()` — walkdir único, não três chamadas separadas.
- `open_docs` guarda chave como URI completa (`file:///...`) — nunca `format!("file://{}", key)`.

## Build

```bash
cargo build --release
cargo test
cargo clippy -- -D warnings
```
