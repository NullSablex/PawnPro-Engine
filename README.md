<div align="center">
  <img src="images/logo.png" alt="PawnPro Engine" />
</div>

Motor IntelliSense para a linguagem **Pawn** — servidor LSP em Rust integrado à extensão [PawnPro](https://github.com/NullSablex/PawnPro) para Visual Studio Code.

## O que é

`pawnpro-engine` é o núcleo de análise do PawnPro. Roda como processo separado e se comunica com o editor via **Language Server Protocol (LSP)** sobre stdin/stdout — o mesmo protocolo usado por `rust-analyzer` e `clangd`.

A extensão PawnPro inicia o motor automaticamente ao detectar o binário. Se o binário não estiver presente, a extensão cai em modo TypeScript como fallback transparente.

## Funcionalidades

### Diagnósticos

| Código | Descrição |
|--------|-----------|
| PP0001 | `#include` não encontrado |
| PP0002 | `native` com corpo `{}` |
| PP0003 | `forward` com corpo `{}` |
| PP0004 | `public`/`stock`/`static` sem corpo |
| PP0005 | Variável declarada e não utilizada |
| PP0006 | Função `stock`/`static` declarada e não utilizada |
| PP0007 | Uso de símbolo marcado com `@DEPRECATED` |
| PP0008 | `#include` de arquivo marcado com `@DEPRECATED` |

### IntelliSense

- **Completions** — `native`, `stock`, `public`, `forward`, `static`, `#define` de todos os includes transitivos; snippets com parâmetros; deprecated marcado visivelmente
- **Hover** — assinatura + comentário de doc; em `#include` exibe o caminho resolvido
- **Signature Help** — parâmetro ativo destacado ao digitar `(` e `,`
- **CodeLens** — contagem de referências para todas as funções; funciona em `.inc`

### Parser

- `native`, `forward`, `public`, `stock`, `static`, `static const`, `float`/`bool` como tipo de retorno
- Tags Pawn (`Float:`, `File:`, etc.) — corretamente ignoradas na detecção de nomes
- `#define`, `#include <token>` e `#include "caminho/relativo"`
- `// @DEPRECATED` e `/* @DEPRECATED */` (case-insensitive)
- Comentários de doc extraídos acima de cada declaração

## Plataformas

| Plataforma | Artefato |
|------------|----------|
| Linux x64 | `pawnpro-engine-linux-x64` |
| Linux arm64 | `pawnpro-engine-linux-arm64` |
| Windows x64 | `pawnpro-engine-win32-x64.exe` |
| macOS x64 | `pawnpro-engine-darwin-x64` |
| macOS arm64 | `pawnpro-engine-darwin-arm64` |

Os binários são compilados estáticos (musl no Linux) — sem dependências externas.

## Configuração

O motor lê automaticamente:
- `~/.pawnpro/config.json` — configuração global
- `.pawnpro/config.json` — configuração do projeto (sobrescreve global)

### Chaves relevantes

```json
{
  "includePaths": ["${workspaceFolder}/pawno/include"],
  "analysis": {
    "warnUnusedInInc": false
  }
}
```

- **`includePaths`** — diretórios para resolver `#include <token>`; suporta `${workspaceFolder}`
- **`analysis.warnUnusedInInc`** — habilita avisos de símbolos não usados em `.inc` (padrão: `false`)

## Desenvolvimento

### Pré-requisitos

- Rust stable (`rustup install stable`)

### Build

```bash
git clone https://github.com/NullSablex/PawnPro-Engine
cd pawnpro-engine
cargo build
```

O binário estará em `target/debug/pawnpro-engine`. A extensão PawnPro o detecta automaticamente se estiver na pasta irmã `../pawnpro-engine/target/debug/`.

### Testes

```bash
cargo test
```

### Lint

```bash
cargo clippy -- -D warnings
```

## Uso como LSP standalone

O motor segue o protocolo LSP padrão e pode ser integrado em qualquer editor com suporte a LSP:

```
pawnpro-engine
```

Lê de stdin e escreve em stdout. Não requer argumentos.

## Licença

PawnPro Engine License v1.0 — Source-Available (não Open Source).  
Uso comercial permitido ✅ · Venda proibida ❌ · Detalhes: [LICENSE.md](LICENSE.md)
