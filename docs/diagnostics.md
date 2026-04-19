# Diagnósticos

O motor emite diagnósticos identificados por códigos `PP####`. Todos os diagnósticos respeitam o filtro de supressão via comentário `// pawnpro-disable PP####` (feature futura).

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
| `PP0009` | Hint | Parâmetro de função declarado e não utilizado |
| `PP0010` | Aviso | Função chamada não declarada em nenhum include ativo |
| `PP0011` | Hint | `#define` declarado mas não utilizado no arquivo |
| `PP0012` | Hint | `#include` incluído mas nenhum de seus símbolos é utilizado |
| `PP0013` | Hint | `#tryinclude` não resolvido (informativo) |

> ¹ Marcados com `unnecessary` — o VS Code exibe o símbolo desbotado/riscado além do sublinhado de aviso.

## Detalhes

### PP0001 — `#include` não encontrado
Emitido quando um `#include <token>` ou `#include "caminho"` não é resolvido em nenhum dos `includePaths` configurados. Considere verificar a chave `includePaths` em `.pawnpro/config.json`.

### PP0002 / PP0003 — `native`/`forward` com corpo
Em Pawn, `native` e `forward` são declarações sem implementação. Se um bloco `{}` for encontrado logo após a assinatura, o motor emite este erro.

### PP0004 — Função sem corpo
`public`, `stock` e `static` requerem implementação. Uma declaração sem `{}` provavelmente indica um erro de digitação.

### PP0005 / PP0006 — Não utilizados
- **PP0005** cobre variáveis globais (`new variavel;`).
- **PP0006** cobre funções `stock` e `static`.
- Ambos usam a flag `unnecessary`, que faz o VS Code exibir o símbolo com texto desbotado/riscado além do sublinhado de aviso.
- Stocks em `.inc` são **silenciados** por padrão (são funções de biblioteca). Use `analysis.warnUnusedInInc: true` para habilitá-los.

### PP0007 / PP0008 — Depreciação
Aplique `// @DEPRECATED` (ou `/* @DEPRECATED */`, case-insensitive) na linha anterior à declaração ou inline na mesma linha.

- **PP0007** é emitido na própria declaração depreciada (hint visual) e em cada uso posterior.
- Cobre: `native`, `stock`, `public`, `forward`, `static`, `#define` e variáveis globais.
- Depreciar um `forward` marca automaticamente o `public` par (e vice-versa).
- **PP0008** é emitido na linha do `#include` depreciado; todos os símbolos daquele arquivo passam a emitir PP0007 com a mensagem *"pertence a um include depreciado"*.

### PP0009 — Parâmetro não utilizado
Emitido como Hint (não Aviso) para evitar ruído em callbacks com assinatura fixa definida pelo SA-MP/OMP SDK.

### PP0010 — Função não declarada
O motor verifica que cada chamada de função possui uma declaração correspondente em algum dos includes transitivos. Não é emitido em arquivos `.inc`.

### PP0011 — `#define` não utilizado
Emitido como Hint. Defines de biblioteca em `.inc` que não têm uso local são comuns e esperados — o nível Hint evita falsos alertas.

### PP0012 — `#include` sem uso
Emitido como Hint quando nenhum símbolo (função, macro ou variável) do include é referenciado no arquivo atual. Não emitido se o include não foi resolvido (PP0001 já cobre esse caso). Não emitido em `.inc`.

### PP0013 — `#tryinclude` não resolvido
`#tryinclude` é uma diretiva opcional por definição — a ausência do arquivo não é erro. O motor emite Hint apenas para informar que o arquivo não foi encontrado, sem impedir a análise.
