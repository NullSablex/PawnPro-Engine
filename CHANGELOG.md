# Changelog
Todas as mudanças notáveis neste projeto serão documentadas aqui.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.0.0/),
e este projeto adere ao [Semantic Versioning](https://semver.org/lang/pt-BR/).

Podem existir falhas ou itens não declarados, causados por falha humana ou por IA,
caso encontre por favor relate para ajudar a manter a consistência dos dados.

## Versões anteriores

- [Versões 0.x](changelogs/CHANGELOG_v0.md)

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
