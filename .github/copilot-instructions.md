# Copilot Instructions — PawnPro Engine

## Contexto

Servidor LSP em Rust para a linguagem Pawn (SA-MP / open.mp). Comunica com a extensão VS Code via stdin/stdout usando o Language Server Protocol (tower-lsp).

## Regras de código

- Zero `unwrap()` em código de produção fora de `Lazy::new`. Usar `?`, `if let`, `unwrap_or`.
- Nunca `panic!` em caminhos de análise — o motor precisa sobreviver a qualquer entrada.
- Regexes sempre como `static Lazy<Regex>` — nunca compilar dentro de loops.
- Nunca interpolar input não escapado em `Regex::new`.
- `cargo clippy -- -D warnings` deve passar sem erros.
- Novos diagnósticos sempre com constante em `analyzer/codes.rs`.

## Estrutura

```
src/
  parser/     ← lexer.rs, symbols.rs, types.rs  (parsing puro)
  analyzer/   ← codes.rs, diagnostic.rs, includes.rs, semantic.rs,
                unused.rs, hints.rs, deprecated.rs, undefined.rs
  intellisense/ ← completion, hover, signature, codelens, references,
                  semantic_tokens
  workspace.rs  ← orquestra parser + analyzers para cada arquivo
  server.rs     ← handlers LSP
  config.rs     ← EngineConfig
```

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
| PP0004, PP0007(uso), PP0008, PP0010 | Warning | `::warning` / `::deprecated_warning` |
| PP0005–PP0006 | Warning + unnecessary | `::unnecessary_warning` |
| PP0007(decl) | Warning + deprecated | `::deprecated_decl` |
| PP0009, PP0011–PP0013 | Hint | `::hint` |

## Build

```bash
cargo build --release
cargo test
cargo clippy -- -D warnings
```
