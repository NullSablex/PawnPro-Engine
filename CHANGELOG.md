# Changelog
Todas as mudanças notáveis neste projeto serão documentadas aqui.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.0.0/),
e este projeto adere ao [Semantic Versioning](https://semver.org/lang/pt-BR/).

Podem existir falhas ou itens não declarados, causados por falha humana ou por IA,
caso encontre por favor relate para ajudar a manter a consistência dos dados.

## Versões anteriores

- [Versões 0.x](changelogs/CHANGELOG_v0.md)

---

## [1.4.0] - 01/09/2026

### Adicionado
- **Comentários de documentação nos hovers, signature help e autocomplete** — o
  comentário imediatamente acima de uma declaração passa a ser interpretado, não
  apenas repassado como texto cru. Duas convenções são reconhecidas, detectadas
  pelo próprio conteúdo:
  - **Javadoc** (`@param`, `@return`, `@remarks`/`@note`) — o primeiro parágrafo
    vira o resumo, e linhas seguintes a uma tag continuam o texto dela
  - **XMLdoc** (`<summary>`, `<param name="">`, `<returns>`, `<remarks>`) — a
    convenção herdada do C# que o `omp-stdlib` usa. O HTML inline vira Markdown
    (`<b>` → negrito, `<c>` → código, `<br />` → quebra de linha); `<library>`,
    `<seealso>` e as âncoras `<a href="#Func">` são metadados do gerador da wiki
    do open.mp e não aparecem no hover, onde só ocupariam espaço

  Em ambos, cada parâmetro é casado **pelo nome** (o sufixo `[]` é ignorado) e
  não pela posição — um comentário pode omitir parâmetros ou listá-los fora de
  ordem. O **signature help** passa a mostrar a descrição do parâmetro sob o
  cursor; o **autocomplete** mostra só o resumo, e o **hover**, o bloco inteiro
  formatado, com as seções traduzidas para o idioma resolvido. Um comentário sem
  marcação continua virando a descrição, como antes
- **Tags de documentação no autocomplete** — o trigger `@`, dentro de um
  comentário, passa a oferecer `@param`, `@return` e `@remarks` com snippets
- **Diagnósticos e hovers traduzidos para Espanhol, Romeno e Russo** — as tabelas
  de mensagens `messages/langs/{es,ro,ru}.rs` existiam como esqueleto (texto ainda
  em inglês, copiado de `en.rs`) e agora estão de fato traduzidas: as 75 mensagens
  (diagnósticos PP*, contadores de referências do CodeLens e descrições de hover
  de palavras-chave) saem no idioma resolvido por `Locale::from_str`. Marcadores
  `{}` / `{n}` / `{style}` preservados; termos técnicos do Pawn (`native`,
  `forward`, `stock`, `public`, `static`, `enum`) mantidos em inglês

### Alterado
- **`#pragma deprecated` substitui o marcador `@DEPRECATED`** — a depreciação
  passa a usar a diretiva do próprio compilador Pawn, em vez do marcador em
  comentário que a engine reconhecia antes. A semântica acompanha a do
  compilador: a diretiva marca o **próximo símbolo declarado** e não tem forma
  inline. O texto que a segue é opcional e, quando presente, é **anexado ao
  aviso** PP0007 — é a parte acionável, já que normalmente diz o que usar no
  lugar:

  ```pawn
  #pragma deprecated Use BanPlayerFor em vez desta
  stock BanTemporario(playerid, seconds) { }
  ```

  Cobertura inalterada: `native`, `stock`, `public`, `forward`, `static`,
  `#define`, variáveis globais e `#include` (PP0008), incluindo o pareamento
  automático entre `forward` e `public`
- **`@DEPRECATED` deixa de ser reconhecido** — comentários com o marcador antigo
  não marcam mais nada. Quem o usava precisa trocar por `#pragma deprecated`
  (ver acima). O completion do `@` que existia só para inseri-lo deu lugar às
  tags de documentação
- **Dependências (Rust)** — `tokio` 1.52.3 → 1.53.1, `regex` 1.12.4 → 1.13.1,
  `serde` 1.0.228 → 1.0.229, `serde_json` 1.0.150 → 1.0.151 e `futures` 0.3.32 →
  0.3.34, além das transitivas do `Cargo.lock`
- **CI — GitHub Actions atualizadas** (pinadas por SHA): `github/codeql-action`
  4.36.2 → 4.37.9, `actions/checkout` 4.2.2 → 7.0.1, `actions/upload-artifact`
  6.0.0 → 7.0.1, `actions/download-artifact` 7.0.0 → 8.0.1,
  `actions/upload-pages-artifact` 3.0.1 → 5.0.0, `Swatinem/rust-cache` 2.9.1 →
  2.9.2, `ossf/scorecard-action` 2.4.3 → 2.4.4 e `softprops/action-gh-release`
  3.0.1 → 3.0.3
- **Docs (CI)** — `mkdocs-material` para 9.7.7 e `pymdown-extensions` 10.21.3 →
  11.0.2 (pinados por hash)
- **Dependabot — um PR por ecossistema** — as atualizações passam a ser agrupadas
  por ecossistema (cargo, GitHub Actions, pip) em vez de abrir um PR por
  dependência, reduzindo o ruído de manutenção

- **Autocomplete ordenado pela proximidade do cursor** — a lista vinha ordenada
  de um jeito que punha as variáveis locais e os parâmetros **abaixo** de
  milhares de nativas dos includes; o que está mais perto de quem escreve
  aparecia por último. A ordem passa a ser: locais e parâmetros, símbolos do
  próprio arquivo, símbolos dos includes, palavras-chave e, por fim, os
  marcados com `#pragma deprecated` — que continuam aparecendo (às vezes é
  mesmo o que se quer), mas nunca à frente de uma alternativa viva. Dentro de
  cada grupo a ordem é alfabética sem diferenciar maiúsculas
- **Listas grandes de completion não travam mais a digitação** — um projeto com
  muitos includes mandava todos os símbolos ao editor a cada tecla. A resposta
  passa a ser cortada em 1000 itens e marcada como `isIncomplete`, fazendo o
  editor pedir de novo conforme o prefixo cresce. O corte vem depois da
  ordenação, então o que se perde são os itens mais distantes do cursor
- **Correções rápidas para mais nove diagnósticos** — passam a ter *quick fix*:
  remover o corpo `{ }` ilegal de um `native`/`forward` (`PP0002`/`PP0003`);
  dar corpo vazio ou converter em `forward` quando falta o corpo (`PP0004`);
  trocar por um símbolo de nome parecido quando a função chamada não existe
  (`PP0010`, o caso comum de erro de digitação); remover o `#define` e o
  `#include` não utilizados (`PP0011`/`PP0012`); e reindentar a linha
  (`PP0017`), usando o estilo de formatação configurado no projeto, não uma
  indentação fixa. Somados aos que já existiam, catorze dos dezenove
  diagnósticos agora oferecem correção
- **`PP0019` — `#pragma` desconhecido ou malformado** — o compilador rejeita uma
  diretiva que não conhece (erro 207), mas só na compilação; agora o aviso
  aparece enquanto se escreve, com *quick fix*. Cobre o nome errado
  (`#pragma deprected` → sugere `deprecated`, comparando com a lista do
  compilador) e a mensagem de `deprecated` escrita entre aspas — a diretiva toma
  o resto da linha como texto livre, então as aspas entrariam na mensagem em vez
  de delimitá-la. Aspas no meio do texto continuam sendo texto legítimo

### Corrigido
- **Réguas de separação viravam documentação** — um cabeçalho de seção
  (`// ------------`) acima de uma função era lido como doc comment dela, e o
  hover exibia a régua e o texto da seção no lugar da documentação. Uma linha
  de comentário composta só de `-`, `=`, `*`, `_`, `#` ou `~` passa a encerrar
  a varredura: é ornamento, e o que está acima dela pertence a outra seção.
  Comentários `//` com texto continuam sendo documentação, como antes
- **Doc comment de um símbolo aparecendo no hover de outro** — quando o
  comentário era um bloco de uma linha só (`/** … */`), a varredura empurrava
  essa linha e ia procurar o `/*` de abertura **a partir da linha anterior**,
  atravessando o código acima até casar com o `/**` de outro comentário. O
  hover de um `#define`, por exemplo, mostrava a documentação da função
  anterior com o código do meio junto. Um `*/` sem abertura também deixa de
  virar doc, em vez de arrastar o arquivo até o topo
- **Aviso de depreciado desalinhando o hover** — era escrito como *blockquote*
  (`>`), o que fazia o editor recuar o bloco e ler a linha `---` seguinte como
  continuação dele, desalinhando a documentação inteira. Passa a ser texto
  normal — e agora traz junto a mensagem do `#pragma deprecated`, que antes só
  aparecia no aviso de uso
- **`native`/`forward` com assinatura quebrada em várias linhas eram ignorados** —
  ao fechar o `)`, o parser só criava o símbolo se a linha trouxesse `{`; sem
  isso, ficava esperando um corpo que nunca chega, já que essas duas formas
  declaram sem corpo e terminam em `;`. O símbolo se perdia: sem hover, sem
  autocomplete e sem signature help. Atinge em cheio os includes do open.mp,
  onde assinaturas longas em várias linhas são comuns — um
  `ApplyActorAnimation`, com nove parâmetros em nove linhas, era invisível para
  a engine
- **Comentário de documentação perdido quando `#pragma deprecated` ficava entre
  ele e a declaração** — a varredura do doc caminha para cima e parava na
  primeira linha que não fosse comentário, e a diretiva cortava o caminho. Passa
  a pular a diretiva
- **CodeQL: análise ausente em PRs de docs e dependências** — o repositório usava
  o *default setup* do CodeQL, que só analisa um pull request quando ele toca
  arquivos das linguagens configuradas. Um PR que mexia apenas em documentação ou
  em manifestos de dependência não gerava análise alguma, enquanto `master`
  continuava com uma por linguagem — a regra de proteção `code_scanning` não
  conseguia comparar os dois lados e reportava *"configurations not found"*,
  travando o merge. Substituído pelo *advanced setup* (`.github/workflows/codeql.yml`),
  sem filtro de caminho: as duas configurações (`/language:actions` e
  `/language:rust`) passam a existir em todo pull request

## [1.3.0] - 04/07/2026

### Adicionado
- **Preservar alinhamento de arrays no formatador** — nova opção
  `formatPreserveArrayAlignment` (via `initializationOptions`): quando ligada, o
  formatador mantém intacto o alinhamento manual em colunas de inicializadores de
  array `{ }` quebrados em várias linhas — as linhas internas saem sem colapsar os
  espaços de alinhamento nem re-indentar. Opt-in; o padrão continua re-indentando.

### Corrigido
- **Quick fix de nomes (`PP0018`) atuava em comentários e keywords** ([#5]) — a code action de nomenclatura passou a ancorar no **diagnóstico** já emitido (que aponta o identificador real do símbolo), em vez da palavra crua sob o cursor. Com isso, deixa de oferecer renomeação para tokens dentro de comentários (`//`, `/* */`) e para a keyword da declaração (`stock`/`public`/`new`/`#define`...), sugerindo apenas o nome do símbolo.
- **Formatador achatava declaração `new` multilinha** — uma lista de variáveis num único `new`/`static`/`const`/`decl` quebrada em várias linhas por vírgulas era realinhada indevidamente; agora as linhas de continuação são reconhecidas como parte da mesma declaração e mantêm a indentação.
- **Aviso de deprecação do Node 20 no CI** — `actions/checkout`, `actions/deploy-pages` e `softprops/action-gh-release` subiram para versões baseadas em Node 24, eliminando o alerta dos runners do GitHub na geração de artefatos da engine.

[#5]: https://github.com/NullSablex/PawnPro-Engine/issues/5

---

## [1.2.0] - 21/06/2026

### Adicionado

#### Assistente de nomes (`PP0018`)
- **Diagnóstico de nomes pobres**, offline e determinístico (sem IA, sem rede). Avalia funções, parâmetros e variáveis locais (`new`/`decl`/`static`), além de globais, constantes (`const`/enum) e macros (`#define`). Desligado por padrão (`analysis.naming.enabled`).
- **Categorias distintas** — constantes (`const`/enum) e macros (`#define`) são tratadas como categorias separadas, cada uma com seu estilo (um `#define` não é uma constante tipada).
- **Regras**: nomes curtos (`minLength`, com tolerância a índices de loop), placeholders (`tmp`/`foo`/… — lista configurável) e **estilo de caixa por categoria**.
- **Multi-estilo por categoria** — cada categoria aceita uma lista de estilos (`camelCase`/`snake_case`/`PascalCase`/`UPPER_CASE`/`Capitalized_Snake`); um nome é aceito se casar com qualquer um deles. Lista vazia = sem checagem. O `Capitalized_Snake` reconhece cada trecho separado por `_` começando com maiúscula e contendo ao menos uma minúscula (ex.: `Carregar_Lixeiras`); o `_` é opcional, então `Capitalized_Snake` é um superconjunto de `PascalCase` — nomes como `Palavrao` e `CarregarLixeiras` também casam. A detecção rejeita segmentos todo-maiúsculos (`Carregar_LIXEIRAS`), que são `UPPER_CASE`.
- **Listas externas** — `blocklist` e índices de loop podem vir de arquivos `.ban`/`.allow` (um termo por linha, `#` comenta), com prioridade sobre o inline e _fallback_ automático. Limite de processamento configurável (`maxListFileBytes`, padrão 32 MB).
- **Reação em tempo real** — mudanças na configuração de nomes (via `workspace/didChangeConfiguration`) republicam os diagnósticos sem reiniciar o servidor.

#### Renomeação e quick fixes (`textDocument/codeAction`)
- **`textDocument/rename`** com `prepareProvider` — reusa a busca de referências para renomear todas as ocorrências.
- **Quick fix de estilo** — sobre `PP0018`, oferece converter o nome para o estilo configurado (incluindo `Capitalized_Snake`, ex.: `carregar_lixeiras` → `Carregar_Lixeiras`).
- **Quick fix de remoção de código não usado** — sobre `PP0005` (variável), `PP0006`/`PP0016` (função) e `PP0009` (parâmetro): remove a declaração inteira (variável/função pelo balanço de chaves, ignorando `{}` em strings; parâmetro junto da vírgula adjacente). É oferta, nunca automático; **não disponível em arquivos `.inc`** — onde "não usado" costuma ser falso positivo para quem desenvolve a biblioteca.

#### Internacionalização
- **Mensagens por idioma** reorganizadas em `src/messages/langs/`. Além de PT-BR e EN, esqueletos para **Espanhol, Russo e Romeno** (placeholder, a traduzir). O `Locale` resolve pelo prefixo da tag; desconhecido cai em inglês.

#### Motor de formatação guiado por estrutura
- Reescrita do formatador sobre a `StmtTree` (indentação estrutural real) com **presets** Allman/K&R/Compacto/Custom e ajustes finos, validados contra o `pawncc` como oráculo.
- **Strings de continuação de linha preservadas** — literais quebrados com `\` no fim da linha (ex.: lista de itens de `ShowPlayerDialog`) mantêm seu conteúdo intacto; os `{...}` de cores embutidas não são mais interpretados como blocos nem reindentados.

### Corrigido

#### Formatador (`textDocument/formatting` e `rangeFormatting`)
- **Operadores corrompidos** — o reconhecimento inseria espaços que quebravam a compilação. Reescrito com _longest-match_ baseado na tabela `sc_tokens[]` do compilador open.mp:
  - `i++` / `i--` não são mais fatiados em `i + +` / `i - -`
  - `>>>=`, `>>>`, `<<=`, `>>=`, `&=`, `|=`, `^=` preservados (antes viravam `>> >=` etc.)
  - `sizeof(x)` / `tagof(x)` não recebem mais espaço espúrio (são operadores, não keywords)
  - `for(a;b;c)` agora espaça os `;` corretamente, sem produzir `for (;; )`
  - `...`, `..`, `::` preservados intactos
- **`warning 217: loose indentation`** — a indentação produzida divergia do que o compilador espera. Reescrita a lógica de profundidade com pilha de blocos:
  - corpos de controle sem chaves (`if`/`for`/`while`/`else`) agora indentam +1 nível
  - chave de abertura com statement na mesma linha (`{ stmt;`) é normalizada para linha própria (estilo Allman), eliminando a divergência `stmt_sameline` do compilador
  - `}` de fechamento alinha com o `{` correspondente
  - validado com o compilador `pawncc` real como oráculo
- **Linha em branco espúria ao formatar** — _off-by-one_ no range do `TextEdit`: tanto `format_document` quanto `format_range` adicionavam uma linha após o conteúdo. `format_range` também emitia edit mesmo sem mudanças e reformatava a linha-limite quando a seleção terminava em `character 0`. Todos corrigidos.
- **Vírgula dupla em assinatura multi-linha** — `MyFunc(a,\n b)` produzia `MyFunc(a,, b)`; o acumulador de parâmetros agora normaliza o separador.

### Segurança
- **`cargo audit` no CI** — novo job que falha em vulnerabilidades conhecidas das dependências (RustSec).
- **OpenSSF Scorecard** — workflow `scorecard.yml` avaliando boas práticas de segurança do repositório.
- **Teto de tamanho do `config.json`** — 32 MB, recusado antes do parse (barreira contra estouro de memória); arquivos `.ban`/`.allow` têm limite de processamento próprio e configurável.

### Qualidade de código
- **CI mais rigoroso** — `cargo clippy -W clippy::pedantic -D warnings` em todos os targets, mais `cargo fmt --check`. O crate passa limpo.
- **Conversões numéricas centralizadas** — `src/util.rs` com `to_u32` (saturante) substitui casts `as` espalhados, eliminando truncamentos silenciosos.
- **Utilitário de texto compartilhado** — `src/text.rs` unifica a extração de identificador sob o cursor (antes duplicada entre referências, rename e provedores).
- **Parser de símbolos refatorado** — migrou para um `ParserState` com handlers por forma sintática.
- **Dependência morta removida** — `once_cell` (substituída por `std::sync::LazyLock`).
- **Cobertura de testes ampliada** — formatador, indentação, naming (regras/locais/estilo/sugestão), rename, quick fix de remoção e utilitários de texto.
- **`code_action` modularizado** — separado em `naming_actions` / `removal_actions` com helpers de edição reutilizáveis, em vez de uma função única.
- **Deploy da documentação por GitHub Actions** — `docs.yml` deixou de publicar pela branch `gh-pages` (`mkdocs gh-deploy`) e passou ao pipeline oficial de Pages (`actions/upload-pages-artifact` + `actions/deploy-pages`), sem `contents: write`; build com `--strict`. Dispara apenas quando `docs/**` ou `mkdocs.yml` mudam.
- **Dependências de CI pinadas** — todas as GitHub Actions de todos os workflows (`ci`, `docs`, `release`, `scorecard`) são referenciadas por commit SHA (com a versão em comentário); o deploy de docs usa `pip install --require-hashes` sobre um `docs/requirements.txt` com hash de cada dependência. Atende à boa prática de dependências pinadas do OpenSSF Scorecard.
- **Descrição da release preenchida automaticamente** — o `release.yml` monta o corpo da release a partir da seção correspondente do `CHANGELOG.md` (mais o bloco de novos contribuidores e o link de comparação do GitHub), em vez das notas automáticas genéricas.

---

## [1.1.0] - 29/04/2026

### Adicionado

#### Novos diagnósticos
- **PP0014** — `native` declarada mas nunca chamada em nenhum arquivo do workspace (Hint)
- **PP0015** — `forward` declarado mas nunca chamado (Hint)
- **PP0016** — função sem keyword declarada mas nunca chamada (Warning desbotado)
- **PP0017** — indentação inconsistente dentro de um bloco

#### Novos SymbolKinds
- **`SymbolKind::Plain`** — funções sem keyword (`Func(params){}`); evita falsos positivos de PP0006 em callbacks como `OnPlayerConnect` declarados sem `public`
- **`SymbolKind::Enum`** — nome de enum (`enum NomeDoEnum { ... }`); hover exibe `enum` em vez de `const`
- **`SymbolKind::Const`** — constante declarada com `const`; hover exibe `const`

#### Parser
- Nomes de enum registrados como símbolo com kind dedicado, incluindo enums com tag (`enum E_ZONES: { ... }`)

#### IntelliSense
- **Formatador de documentos** (`textDocument/formatting` e `textDocument/rangeFormatting`) — indentação, espaçamento de operadores e keywords, colapso de linhas em branco consecutivas
- **Completion contextual** — snippets de keywords separados por contexto: `KW_IN_BODY` (if/for/while/new/return…) e `KW_TOP_LEVEL` (stock/public/forward/#define/#include…)
- **Completion de variáveis locais** — parâmetros e variáveis declaradas com `new`/`static` visíveis na posição do cursor

#### Internacionalização (i18n)
- Novo módulo `src/messages/` com `Locale` (`En` / `PtBr`) e `MsgKey`; todas as mensagens de diagnóstico, hover, codelens e snippets de completion são internacionalizadas
- Nova opção de configuração `locale` — lida de `initializationOptions` e `workspace/didChangeConfiguration`

#### Configuração
- Nova opção `suppressDiagnosticsInInc` — suprime todos os diagnósticos em arquivos `.inc`/`.p`/`.pawn` quando habilitada

#### Handlers LSP
- **`textDocument/didSave`** — republica diagnósticos para todos os arquivos abertos que dependem do arquivo salvo
- **`workspace/didChangeWatchedFiles`** — evita cache e republica dependentes quando includes externos ao editor mudam

#### Infraestrutura interna
- `dep_graph` (`DashMap<PathBuf, HashSet<PathBuf>>`) — grafo reverso de dependências para invalidação granular de cache
- `tabsize_cache` — cache workspace-wide de `#pragma tabsize` para evitar releitura a cada análise
- `open_dependents(uri)` — percorre `dep_graph` via BFS e retorna as URIs abertas que dependem transitivamente de um arquivo
- `evict_path_from_cache(path)` — evita o arquivo e todos os dependentes transitivos em uma única BFS
- `ConfigUpdate` struct — elimina duplicação de parsing de configuração entre `initialize` e `did_change_configuration`
- Novos módulos `parser/stmt_parser.rs` e `parser/token_lexer.rs`

---

### Aprimorado

- **PP0006/PP0012** — varredura do workspace inteiro (todos os `.pwn`/`.inc`/`.p`/`.pawn`) para determinar se uma stock ou include é usada; elimina falsos positivos em arquivos `.inc` incluídos por múltiplos `.pwn`
- **PP0012** — `collect_transitive_exports` faz BFS nos includes do include e coleta todos os símbolos re-exportados transitivamente; elimina falsos positivos quando um include encadeia outros includes
- **PP0010** — suprime também arquivos `.p` e `.pawn` (além de `.inc`), pois nenhuma dessas extensões é compilada diretamente
- **PP0011** — macros com parâmetros agora verificadas em `local_calls` além da varredura de identificadores
- **Resolução de includes** — testa extensões em ordem: sem extensão → `.inc` → `.p` → `.pawn` → `.pwn`, espelhando o compilador real (`sc2.c plungequalifiedfile`); busca case-insensitive em Linux/macOS
- **Resolução de includes** — limites aumentados: profundidade 16 / 1000 arquivos (antes 8 / 500)
- **Símbolos com prefixo `_`** — suprimidos de PP0005, PP0006 e PP0011 (convenção de símbolo intencionalmente não usado)
- **PP0001** — falso positivo removido para includes de sistema (qawno/include, pawno/include) que ficam fora do workspace por design
- **`did_change`** — republica diagnósticos para os dependentes do arquivo alterado, não apenas o próprio arquivo
- **`text_document_sync`** — expandido para `Options` com `save: SaveOptions`, habilitando notificações de `textDocument/didSave`
- **`undefined` (PP0010)** — truncagem `sNAMEMAX=31` aplicada ao comparar nomes, alinhando com o limite real do compilador
- **`collect_workspace_all`** — uma única passagem por arquivo do workspace acumula calls, idents e idents sem diretivas (antes eram três passagens separadas)
- **`evict_dependents`** — propaga a evicção por toda a cadeia de dependência via BFS transitivo (antes evitava apenas um nível)
- **`republish_all_open_docs`** — usa `join_all` para republicar diagnósticos de todos os documentos em paralelo

---

### Corrigido

- **`deprecated_decl`** — estava com `deprecated: false`; `DiagnosticTag::DEPRECATED` (tachado no editor) não era ativado na própria linha de declaração
- **`record_dependencies`** — simplificado para usar diretamente os `parents` do `reverse_deps`, eliminando inconsistências de URI vs PathBuf

---

### Alterado

- **`dep_graph`** — migrado de `DashMap<PathBuf, HashSet<String>>` para `DashMap<PathBuf, HashSet<PathBuf>>`; grafo inteiramente em `PathBuf`
- **`parsed_cache`** — migrado de `DashMap<String, ParsedFile>` para `DashMap<PathBuf, Arc<ParsedFile>>`; chave canônica e sem clone desnecessário
- Restrições de versão de `tokio` e `futures` relaxadas (sem lock em patch)

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
- **`Symbol`** — campos `min_args` e `max_args` removidos; `parse_params` simplificada para retornar `Vec<Param>` diretamente
- **`Document`** — campo `uri` removido de `workspace.rs` (redundante com a chave do `DashMap`)
- **`Severity`** — variante `Info` removida de `diagnostic.rs`; branch correspondente removida do match em `server.rs`
- **`collect_recursive`** — refatorada com struct `CollectCtx` para consolidar os parâmetros; elimina o aviso `too_many_arguments` do clippy
- **`release.yml`** — runner `macos-13` migrado para `macos-latest`; `actions/upload-artifact` atualizado para `v6`; `actions/download-artifact` atualizado para `v7`
