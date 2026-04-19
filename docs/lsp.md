# Protocolo LSP

O `pawnpro-engine` implementa o [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) sobre stdin/stdout.

## Inicialização

O motor é iniciado sem argumentos e negocia capacidades via `initialize` / `initialized`.

### `initializationOptions`

A extensão envia as configurações do projeto como `initializationOptions` no request `initialize`:

```json
{
  "includePaths": ["${workspaceFolder}/pawno/include"],
  "analysis": {
    "warnUnusedInInc": false,
    "sdk": {
      "platform": "omp",
      "filePath": ""
    }
  }
}
```

## Capacidades implementadas

| Capacidade LSP | Descrição |
|----------------|-----------|
| `textDocument/publishDiagnostics` | Diagnósticos em tempo real |
| `textDocument/completion` | Auto-complete com snippets |
| `textDocument/hover` | Hover com assinatura e documentação |
| `textDocument/signatureHelp` | Signature help com parâmetro ativo |
| `textDocument/codeLens` | CodeLens com contagem de referências |
| `codeLens/resolve` | Resolução de CodeLens |
| `textDocument/references` | Localizar todas as referências |
| `textDocument/semanticTokens/full` | Tokens semânticos (coloração) |
| `workspace/didChangeConfiguration` | Atualização de configuração em tempo real |

## Tokens semânticos

O motor produz tokens semânticos para as seguintes categorias:

| Tipo | Modificadores possíveis | Exemplos |
|------|-------------------------|---------|
| `function` | `declaration`, `deprecated` | `stock Func()`, `public OnGameModeInit()` |
| `macro` | `declaration`, `deprecated` | `#define MAX_PLAYERS 500` |
| `parameter` | — | Parâmetros de função |

Chamadas de função são detectadas mesmo quando o `(` está em linha separada (até 3 linhas adiante).

## Atualização de configuração

Após a inicialização, a extensão pode enviar novas configurações via `workspace/didChangeConfiguration`. O motor reanalisa todos os arquivos abertos imediatamente.

## Uso standalone

O motor pode ser integrado em qualquer editor com suporte a LSP:

```
pawnpro-engine
```

Lê de stdin, escreve em stdout. Não requer argumentos. Sem dependências dinâmicas em runtime.
