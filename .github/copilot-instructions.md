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
- `dep_graph` usa `PathBuf` em ambos os lados — nunca URIs.
- Sem comentários óbvios — apenas comentários que explicam *por quê*.

## Estrutura

```
src/
  parser/       ← lexer.rs, symbols.rs, types.rs  (parsing puro)
  analyzer/     ← codes.rs, diagnostic.rs, includes.rs, semantic.rs,
                  unused.rs, hints.rs, deprecated.rs, undefined.rs
  intellisense/ ← completion, hover, signature, codelens, references,
                  semantic_tokens, formatter
  workspace.rs  ← orquestra parser + analyzers; gerencia cache e dep_graph
  server.rs     ← handlers LSP; usa ConfigUpdate para init e didChangeConfiguration
  config.rs     ← EngineConfig
```

## Cache e invalidação granular

`WorkspaceState` em `workspace.rs`:
- `parsed_cache: DashMap<PathBuf, Arc<ParsedFile>>` — chave é **PathBuf**, nunca String/URI
- `dep_graph: DashMap<PathBuf, HashSet<PathBuf>>` — grafo reverso puro em paths: `c.inc → {b.inc}`, `b.inc → {a.pwn}`
- `tabsize_cache: Mutex<Option<Option<u32>>>` — interior mutability para `&self`

Métodos de invalidação e republicação:
- `evict_dependents(path)` — BFS sobre `dep_graph`, remove do `parsed_cache` todos os dependentes transitivos
- `evict_path_from_cache(path)` — remove o path e seus dependentes (usado por `did_change_watched_files`)
- `open_dependents(uri)` — BFS sobre `dep_graph`, retorna URIs dos arquivos **abertos** que dependem transitivamente do arquivo alterado

`did_change`, `did_save` e `did_change_watched_files` em `server.rs` chamam `open_dependents` e republicam diagnósticos para cada dependente aberto — `main.pwn` recebe diagnósticos atualizados quando qualquer include que ele usa (direta ou transitivamente) muda.

`record_dependencies` propaga o `reverse_deps` completo de `collect_included_files` para o `dep_graph`, preservando a cadeia `c.inc → {b.inc}`, não apenas `c.inc → {main.pwn}`.

## Utilitários canônicos (`parser/lexer.rs`)

Não duplicar em outros módulos:
- `decode_bytes` — UTF-8 com fallback latin-1
- `strip_line_comments` — remove `//` e `/* */`, rastreia estado de bloco
- `update_brace_depth` — rastreia `{}` ignorando literais string/char
- `has_inline_deprecated` — detecta `@DEPRECATED` inline

## Diagnósticos

| Código | Severidade | Construtor |
|--------|------------|------------|
| PP0001–PP0003 | Error | `::error` |
| PP0004, PP0008, PP0010, PP0017 | Warning | `::warning` |
| PP0005–PP0006, PP0016 | Warning + unnecessary | `::unnecessary_warning` |
| PP0007 (declaração) | Warning + deprecated | `::deprecated_decl` |
| PP0007 (uso) | Warning + deprecated | `::deprecated_warning` |
| PP0009, PP0011–PP0015 | Hint | `::hint` |
| PP0012 | Hint | `::hint` — checar `collect_transitive_exports`, não só `entry.parsed.symbols` |

`deprecated: true` ativa `DiagnosticTag::DEPRECATED` (strikethrough no editor) — usar `deprecated_decl` na declaração, nunca `warning` simples.

## ConfigUpdate (`server.rs`)

Struct que elimina duplicação entre `initialize` e `did_change_configuration`:
- `from_init_options(value)` / `from_settings(value)` — parsing dos campos JSON (`includePaths`, `warnUnusedInInc`, `suppressDiagnosticsInInc`, `sdkFilePath`, `locale`)
- `apply_init(&mut state)` — aplica na inicialização
- `apply_change(&mut state) -> bool` — aplica em tempo real; retorna `true` se algo mudou (indica ao handler que deve republicar)

## Gotchas

- `Arc<ParsedFile>`: ao estender símbolos de um include, usar `.clone()` — `all.extend(inc_parsed.symbols.clone())`.
- PP0010 não é emitido em `.inc`, apenas em `.pwn`.
- PP0012 usa `collect_transitive_exports` (BFS sobre `ResolvedIncludes`) — um include que re-exporta símbolos de sub-includes não deve gerar PP0012 se qualquer desses símbolos for usado.
- `unused.rs` usa `collect_workspace_all()` — walkdir único, não três chamadas separadas.
- `open_docs` guarda chave como URI completa (`file:///...`) — nunca `format!("file://{}", key)`.
- `dep_graph` guarda `PathBuf → HashSet<PathBuf>` — `open_dependents` faz a conversão path→URI na saída.

## Build

```bash
cargo build --release
cargo test
cargo clippy -- -D warnings
```
