<div align="center" markdown>

![PawnPro Engine](logo.png)

[![CI](https://img.shields.io/github/actions/workflow/status/NullSablex/PawnPro-Engine/ci.yml?style=flat-square&label=CI)](https://github.com/NullSablex/PawnPro-Engine/actions)
[![Release](https://img.shields.io/github/v/release/NullSablex/PawnPro-Engine?style=flat-square&label=release)](https://github.com/NullSablex/PawnPro-Engine/releases)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![LSP](https://img.shields.io/badge/protocolo-LSP-informational?style=flat-square)](https://microsoft.github.io/language-server-protocol/)
[![Clippy](https://img.shields.io/github/actions/workflow/status/NullSablex/PawnPro-Engine/ci.yml?style=flat-square&label=Clippy&logo=rust)](https://github.com/NullSablex/PawnPro-Engine/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/licença-Source--Available-blue?style=flat-square)](https://github.com/NullSablex/PawnPro-Engine/blob/master/LICENSE.md)

![Windows x64](https://img.shields.io/badge/Windows-x64-0078D4?style=flat-square&logo=windows11&logoColor=white)
![Linux x64 · arm64](https://img.shields.io/badge/Linux-x64%20·%20arm64-FCC624?style=flat-square&logo=linux&logoColor=black)
![macOS x64 · arm64](https://img.shields.io/badge/macOS-x64%20·%20arm64-000000?style=flat-square&logo=apple&logoColor=white)

</div>

Motor IntelliSense para a linguagem **Pawn** — servidor LSP em Rust integrado à extensão [PawnPro](https://github.com/NullSablex/PawnPro).

## O que é

`pawnpro-engine` é o núcleo de análise do PawnPro. Roda como processo separado e se comunica com o editor via **Language Server Protocol (LSP)** sobre stdin/stdout — o mesmo protocolo usado por `rust-analyzer` e `clangd`.

A extensão PawnPro inicia o motor automaticamente ao detectar o binário. Se o binário não estiver presente, a extensão recua para o modo TypeScript como fallback transparente.

## Capacidades

- **Diagnósticos** — 17 códigos `PP####` cobrindo erros de estrutura, código morto, símbolos não declarados, depreciação e indentação (ver [Diagnósticos](diagnostics.md)).
- **Completions** — símbolos de todos os includes transitivos com snippets de parâmetros; itens depreciados marcados.
- **Hover** — assinatura e comentário de documentação; em `#include` mostra o caminho resolvido.
- **Signature Help** — parâmetro ativo destacado ao digitar `(` e `,`.
- **CodeLens** — contagem de referências para todas as funções; clicável.
- **References** — `textDocument/references` (Shift+F12).
- **Semantic Tokens** — coloração semântica com suporte a chamadas multiline.
- **Formatação** — documento inteiro e seleção de intervalo.
- **Invalidação granular** — ao salvar um include, o motor republica automaticamente os diagnósticos de todos os arquivos abertos que dependem dele, transitivamente.

Para detalhes do protocolo e das opções de configuração, consulte [Protocolo LSP](lsp.md).

## Plataformas

| Plataforma | Artefato |
|------------|----------|
| Linux x64 | `pawnpro-engine-linux-x64` |
| Linux arm64 | `pawnpro-engine-linux-arm64` |
| Windows x64 | `pawnpro-engine-win32-x64.exe` |
| macOS x64 | `pawnpro-engine-darwin-x64` |
| macOS arm64 | `pawnpro-engine-darwin-arm64` |

Os binários são compilados com LTO, strip e `panic = abort` — sem dependências externas em runtime.

## Desenvolvimento

**Pré-requisitos:** Rust stable (`rustup install stable`)

```bash
git clone https://github.com/NullSablex/PawnPro-Engine
cd pawnpro-engine
cargo build          # debug
cargo build --release
cargo test
cargo clippy -- -D warnings
```

O binário de debug é detectado automaticamente pela extensão PawnPro se estiver em `../pawnpro-engine/target/debug/` ou `../pawnpro-engine/target/release/`.

## Licença

PawnPro Engine License v1.0 — Source-Available (não Open Source).  
Uso pessoal e comercial permitido ✅ · Redistribuição e venda proibidas ❌ · Detalhes: [LICENSE.md](https://github.com/NullSablex/PawnPro-Engine/blob/master/LICENSE.md)
