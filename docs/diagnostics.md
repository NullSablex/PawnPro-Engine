# Diagnósticos

O motor emite diagnósticos identificados por códigos `PP####`.

## Tabela completa

| Código | Severidade | Descrição |
|--------|------------|-----------|
| `PP0001` | Erro | `#include` não encontrado |
| `PP0002` | Erro | `native` com corpo `{}` |
| `PP0003` | Erro | `forward` com corpo `{}` |
| `PP0004` | Aviso | `public`/`stock`/`static` sem corpo `{}` |
| `PP0005` | Aviso¹ | Variável global declarada e não utilizada |
| `PP0006` | Aviso¹ | Função `stock`/`static` não utilizada |
| `PP0007` | Aviso | Uso de símbolo ou include marcado com `@DEPRECATED` |
| `PP0008` | Aviso | `#include` precedido de comentário `@DEPRECATED` |
| `PP0009` | Hint¹ | Parâmetro de função declarado e não utilizado |
| `PP0010` | Aviso | Função chamada não declarada em nenhum include ativo |
| `PP0011` | Hint¹ | `#define` declarado mas não utilizado |
| `PP0012` | Hint | `#include` cujos símbolos (diretos e transitivos) não são utilizados |
| `PP0013` | Hint | `#tryinclude` não resolvido (informativo) |
| `PP0014` | Hint | `native` declarada mas nunca chamada |
| `PP0015` | Hint | `forward` declarado mas nunca chamado |
| `PP0016` | Aviso¹ | Função sem keyword declarada mas nunca chamada |
| `PP0017` | Aviso | Indentação inconsistente dentro de um bloco |

> ¹ Marcados com `DiagnosticTag::UNNECESSARY` — o editor exibe o símbolo desbotado além do sublinhado diagnóstico.

## Detalhes

### PP0001 — `#include` não encontrado
Emitido quando um `#include <token>` ou `#include "caminho"` não é resolvido em nenhum dos `includePaths` configurados. A mensagem inclui os caminhos pesquisados. Verifique a chave `includePaths` em `.pawnpro/config.json`.

### PP0002 / PP0003 — `native`/`forward` com corpo
`native` e `forward` são declarações sem implementação. Um bloco `{}` após a assinatura é erro estrutural.

### PP0004 — Função sem corpo
`public`, `stock` e `static` requerem implementação. Uma declaração sem `{}` indica erro de digitação ou declaração incompleta.

### PP0005 / PP0006 — Não utilizados
- **PP0005** cobre variáveis globais (`new variavel;`).
- **PP0006** cobre funções `stock` e `static`.
- Ambos usam a flag `unnecessary`, que faz o editor exibir o símbolo com texto desbotado.
- Stocks em `.inc` são silenciados por padrão. Use `analysis.warnUnusedInInc: true` para habilitá-los.
- O motor verifica o workspace inteiro — um símbolo usado em qualquer arquivo `.pwn`/`.inc` do projeto não dispara o aviso.

### PP0007 / PP0008 — Depreciação
Aplique `// @DEPRECATED` (ou `/* @DEPRECATED */`) na linha anterior à declaração ou inline.

- **PP0007** é emitido na própria declaração depreciada (hint visual com strikethrough) e em cada uso posterior.
- Cobre: `native`, `stock`, `public`, `forward`, `static`, `#define` e variáveis globais.
- Depreciar um `forward` marca automaticamente o `public` par (e vice-versa), incluindo quando definidos em includes diferentes.
- **PP0008** é emitido na linha do `#include` depreciado; todos os símbolos daquele arquivo passam a emitir PP0007.

### PP0009 — Parâmetro não utilizado
Emitido como Hint para evitar ruído em callbacks com assinatura fixa definida pelo SA-MP/OMP SDK. Parâmetros com prefixo `_` e parâmetros variádicos são ignorados (convenção intencional).

### PP0010 — Função não declarada
O motor verifica que cada chamada de função possui uma declaração correspondente em algum dos includes transitivos, no próprio arquivo, ou no SDK configurado. Não é emitido em arquivos `.inc`.

### PP0011 — `#define` não utilizado
Emitido como Hint. Defines de biblioteca em `.inc` sem uso local são comuns — o nível Hint evita falsos alertas.

### PP0012 — `#include` sem uso
Emitido como Hint quando nenhum símbolo (função, macro ou variável) do include **e de seus sub-includes transitivos** é referenciado no arquivo atual. Um include que re-exporta símbolos de sub-includes não dispara este aviso enquanto qualquer um desses símbolos for usado. Não emitido se o include não foi resolvido (PP0001 já cobre esse caso). Não emitido em `.inc`.

### PP0013 — `#tryinclude` não resolvido
`#tryinclude` é uma diretiva opcional por definição. O motor emite Hint apenas para informar que o arquivo não foi encontrado, sem impedir a análise.

### PP0014 / PP0015 — `native`/`forward` nunca chamados
- **PP0014** cobre `native` declaradas no arquivo mas nunca invocadas em nenhum arquivo do workspace.
- **PP0015** cobre `forward` no mesmo critério.
- Emitidos como Hint — declarações de biblioteca são comuns em `.inc` e não devem gerar ruído.

### PP0016 — Função sem keyword nunca chamada
Funções globais sem `public`/`stock`/`static` (callbacks registrados externamente, por exemplo) que nunca aparecem chamadas no workspace. Emitido como Aviso com flag `unnecessary`.

### PP0017 — Indentação inconsistente
Emitido quando a indentação de uma linha dentro de um bloco diverge do padrão detectado no arquivo (spaces vs tabs, ou tamanho de tab diferente). O motor respeita `#pragma tabsize N` quando presente em qualquer include do workspace.
