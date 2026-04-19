# Changelog
Todas as mudanças notáveis neste projeto serão documentadas aqui.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.0.0/),
e este projeto adere ao [Semantic Versioning](https://semver.org/lang/pt-BR/).

---

## [1.0.0] - 14/04/2026

### Adicionado

- **Parser:** reconhecimento de funções sem keyword (`Func(params){}`) e com namespace (`NS::Func(params){}`) — tratadas como `stock`
- **PP0009** — hint de parâmetro declarado e não utilizado no corpo da função; prefixo `_` e parâmetros variádicos são ignorados (convenção intencional)
- **Completions:** autocomplete `@DEPRECATED` ao digitar `@`; fora de comentário insere automaticamente `// @DEPRECATED`

### Aprimorado

- **Deprecação — validação:** `@DEPRECATED` só é reconhecido em maiúsculas e obrigatoriamente dentro de comentário (`//` ou `/* */`)
- **Deprecação — declaração:** símbolo marcado com `@DEPRECATED` exibe aviso amarelo (Warning) na própria linha de declaração, sem tachado
- **Deprecação — uso:** chamadas a símbolos depreciados exibem aviso amarelo com tachado (`DiagnosticTag::DEPRECATED`)
- **Deprecação — propagação bidirecional:** `forward @DEPRECATED` propaga para o `public` correspondente e vice-versa
- **Deprecação — includes:** forwards depreciados definidos em includes propagam corretamente para os publics do arquivo atual
- **References:** comentários (`//` e `/* */`) são ignorados na contagem de referências
- **References:** detecção de callable com três camadas elimina falsos positivos de nomes de parâmetros
- **CodeLens:** comentários ignorados na contagem de chamadas; funções usam `name(`, `static const` usa ocorrência de palavra

### Corrigido

- `@deprecated` em caixa baixa ou sem comentário era aceito como marcação válida
- `forward` depreciado em include não propagava aviso para o `public` correspondente no arquivo atual
- `public @DEPRECATED` sem `forward` correspondente não exibia aviso na linha de declaração
- Referências dentro de comentários eram contabilizadas como usos reais
- Parâmetros de funções geravam falsos positivos no painel de referências
- Código comentado era contabilizado nas contagens do CodeLens
- **`extract_doc`** — `found = true` dentro do bloco `*/` removido; causava detecção prematura do início de bloco de documentação em alguns casos
- `cargo clippy -D warnings` — `collapsible_if` em `deprecated.rs`, `symbols.rs` e `server.rs`; `too_many_arguments` nas funções privadas `build()` e `push_func()`

### Alterado

- **`Param`** — campo `has_default` removido (não era usado em nenhum analyzer ou handler LSP)
- **`Symbol`** — campos `min_args` e `max_args` removidos; `parse_params` simplificada para retornar `Vec<Param>` diretamente (validação de aridade não implementada)
- **`Document`** — campo `uri` removido de `workspace.rs` (redundante com a chave do `DashMap`)
- **`Severity`** — variante `Info` removida de `diagnostic.rs`; branch correspondente removida do match em `server.rs` (nenhum diagnóstico usava `Info`)
- **`collect_recursive`** — refatorada com struct `CollectCtx` para consolidar os parâmetros `include_paths`, `out`, `seen`, `max_depth`, `max_files`; elimina o aviso `too_many_arguments` do clippy
- **`release.yml`** — runner `macos-13` migrado para `macos-latest`; `actions/upload-artifact` atualizado para `v6`; `actions/download-artifact` atualizado para `v7`

### Detalhe importante

- Podem haver alguns dados que não foram mencionados ou que foram esquecidos de serem adicionados a este arquivo, não intencionalmente mas sim pelo fator humano.

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

---

## Licença e Repositório

- **Repositório:** [github.com/NullSablex/pawnpro-engine](https://github.com/NullSablex/PawnPro-Engine)
- **Extensão:** [github.com/NullSablex/PawnPro](https://github.com/NullSablex/PawnPro)
- **Licença:** PawnPro Engine License v1.0 — Source-Available
- **Feedback:** Use as Issues do GitHub para reportar bugs ou sugerir melhorias
