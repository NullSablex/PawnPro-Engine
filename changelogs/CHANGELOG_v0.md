# Changelog — Versões 0.x

---

## [0.1.0] - 12/04/2026

### Lançamento inicial do PawnPro Engine

Primeiro lançamento público do motor IntelliSense em Rust para a linguagem Pawn, integrado à extensão [PawnPro](https://github.com/NullSablex/PawnPro) v3.0.0.

### Adicionado

#### Parser
- Reconhecimento de `native`, `forward`, `public`, `stock`, `static`, `static const`
- Suporte a `float` e `bool` como tipo de retorno de funções
- Tags Pawn (`Float:`, `File:`, etc.) corretamente ignoradas na detecção de nomes de variáveis e funções
- `#define` e `#include` — suporte a `<token>` e `"caminho/relativo"` (com e sem extensão)
- Detecção de `// @DEPRECATED` e `/* @DEPRECATED */` (case-insensitive)
- Extração de comentários de doc acima de declarações

#### Diagnósticos
- **PP0001** — `#include` não encontrado (com lista dos caminhos buscados na mensagem)
- **PP0002** — `native` com corpo `{}`
- **PP0003** — `forward` com corpo `{}`
- **PP0004** — `public`/`stock`/`static` sem corpo
- **PP0005** — Variável declarada e não utilizada
- **PP0006** — Função `stock`/`static` declarada e não utilizada (suprimido em `.inc` por padrão)
- **PP0007** — Uso de símbolo marcado com `@DEPRECATED`
- **PP0008** — `#include` de arquivo marcado com `@DEPRECATED`

#### IntelliSense
- **Completions** (`textDocument/completion`) — funções e macros de todos os includes transitivos; snippets com placeholders por parâmetro; itens deprecated marcados com `CompletionItemTag::DEPRECATED`
- **Hover** (`textDocument/hover`) — assinatura + comentário de doc em bloco Pawn; em `#include` exibe o caminho resolvido
- **Signature Help** (`textDocument/signatureHelp`) — assinatura ativa ao digitar `(` e `,`, com parâmetro atual destacado
- **CodeLens** (`textDocument/codeLens`) — contagem de referências para `native/forward/public/stock/static/static const`; opera sobre documentos abertos

#### Configuração
- Leitura de `~/.pawnpro/config.json` (global) e `.pawnpro/config.json` (projeto)
- Expansão de `${workspaceFolder}` em `includePaths`
- Configuração `analysis.warnUnusedInInc` para avisos em `.inc` (padrão: `false`)

#### Infraestrutura
- Servidor LSP sobre stdin/stdout (`tower-lsp 0.20`)
- Cache de `ParsedFile` por caminho com `DashMap` — thread-safe, sem locks globais
- Build estático via `x86_64-unknown-linux-musl` e `aarch64-unknown-linux-musl` no Linux
- GitHub Actions: CI (`cargo check`, `cargo clippy -D warnings`, `cargo test`) e Release (5 plataformas)

### Plataformas

| Artefato | Target Rust |
|----------|-------------|
| `pawnpro-engine-linux-x64` | `x86_64-unknown-linux-musl` |
| `pawnpro-engine-linux-arm64` | `aarch64-unknown-linux-musl` |
| `pawnpro-engine-win32-x64.exe` | `x86_64-pc-windows-msvc` |
| `pawnpro-engine-darwin-x64` | `x86_64-apple-darwin` |
| `pawnpro-engine-darwin-arm64` | `aarch64-apple-darwin` |
