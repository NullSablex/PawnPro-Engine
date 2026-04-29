# Changelog
Todas as mudanças notáveis neste projeto serão documentadas aqui.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.0.0/),
e este projeto adere ao [Semantic Versioning](https://semver.org/lang/pt-BR/).

## Versões anteriores

- [Versões 0.x](changelogs/CHANGELOG_v0.md)

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
