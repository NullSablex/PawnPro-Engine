# Protocolo LSP

O `pawnpro-engine` implementa o [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) sobre stdin/stdout.

## Inicialização

O motor é iniciado sem argumentos e negocia capacidades via `initialize` / `initialized`.

### `initializationOptions`

A extensão envia as configurações do projeto como `initializationOptions` no request `initialize`:

```json
{
  "includePaths": ["${workspaceFolder}/pawno/include"],
  "warnUnusedInInc": false,
  "suppressDiagnosticsInInc": false,
  "sdkFilePath": "/caminho/para/open.mp.inc",
  "locale": "pt-BR"
}
```

Todos os campos são opcionais. `sdkFilePath` aponta para o arquivo raiz do SDK (ex: `open.mp.inc`) — o motor resolve seus sub-includes transitivamente e disponibiliza todos os símbolos para análise e completions.

## Atualização de configuração

Após a inicialização, a extensão envia novas configurações via `workspace/didChangeConfiguration`. O motor reanalisa todos os arquivos abertos em paralelo quando qualquer campo relevante muda.

```json
{
  "settings": {
    "includePaths": ["..."],
    "warnUnusedInInc": false,
    "suppressDiagnosticsInInc": false,
    "sdkFilePath": "...",
    "locale": "pt-BR"
  }
}
```

## Capacidades implementadas

| Capacidade LSP | Descrição |
|----------------|-----------|
| `textDocument/publishDiagnostics` | Diagnósticos em tempo real |
| `textDocument/completion` | Auto-complete com snippets por parâmetro |
| `textDocument/hover` | Assinatura e documentação; em `#include` mostra o caminho resolvido |
| `textDocument/signatureHelp` | Parâmetro ativo destacado ao digitar `(` e `,` |
| `textDocument/codeLens` | Contagem de referências para todas as funções |
| `textDocument/references` | Localizar todas as referências (Shift+F12) |
| `textDocument/semanticTokens/full` | Coloração semântica |
| `textDocument/formatting` | Formatação de documento inteiro |
| `textDocument/rangeFormatting` | Formatação de seleção |
| `textDocument/didSave` | Reanálise dos dependentes ao salvar um include |
| `workspace/didChangeConfiguration` | Atualização de configuração em tempo real |
| `workspace/didChangeWatchedFiles` | Invalidação de cache ao modificar includes no disco |

## Completion

O motor registra três caracteres de disparo:

| Trigger | Comportamento |
|---------|--------------|
| `.` | Completions de namespace (aliases definidos via `#define NAMESPACE:: PREFIX_`) |
| `#` | Completions de diretivas (`#include`, `#define`, `#if`, `#ifdef`, etc.) com snippets |
| `@` | Completion de `@DEPRECATED` — em comentários insere a tag; fora de comentários insere `// @DEPRECATED` |

Completions normais (sem trigger) listam símbolos de todos os includes transitivos com snippets de parâmetros. Itens marcados com `@DEPRECATED` aparecem com tag de depreciação.

## Sincronização de documentos

O motor usa sincronização `FULL` — o cliente envia o texto completo a cada mudança. Não há suporte a sincronização incremental (`INCREMENTAL`).

Ao receber `textDocument/didChange` ou `textDocument/didSave` em um arquivo que é include de outros, o motor republica automaticamente os diagnósticos de todos os arquivos abertos que dependem dele (transitivamente), sem necessidade de o usuário salvar ou editar o arquivo principal.

O mesmo ocorre via `workspace/didChangeWatchedFiles` para arquivos `.pwn` e `.inc` modificados fora do editor.

## Grafo de dependências

O motor mantém um grafo reverso de dependências (`dep_graph`) atualizado a cada análise. Quando `a.pwn` inclui `b.inc` que inclui `c.inc`, o grafo registra:

```
c.inc → {b.inc}
b.inc → {a.pwn}
```

Ao modificar `c.inc`, o motor percorre o grafo por BFS, invalida o cache de `b.inc` e `a.pwn`, e republica os diagnósticos de `a.pwn` (o único arquivo aberto no editor que é compilation unit).

## Tokens semânticos

| Tipo | Modificadores | Cobertura |
|------|--------------|-----------|
| `function` | nenhum | Chamadas a símbolos do SDK (`sdkFilePath`) seguidas de `(` |

Apenas símbolos do SDK configurado (`sdkFilePath`) recebem coloração semântica. O `(` pode estar em linha separada — o motor olha até 3 linhas adiante para detectar a chamada.

## Locale

O motor aceita o campo `locale` tanto em `initializationOptions` quanto em `workspace/didChangeConfiguration`. Valores aceitos:

- `"pt-BR"` ou qualquer string começando com `"pt"` — português do Brasil
- qualquer outro valor — inglês (padrão)

## Uso standalone

O motor pode ser integrado em qualquer editor com suporte a LSP:

```
pawnpro-engine
```

Lê de stdin, escreve em stdout. Sem argumentos. Sem dependências dinâmicas em runtime.
