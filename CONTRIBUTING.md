# Contribuindo com o PawnPro Engine

Obrigado pelo interesse em contribuir! Leia este guia antes de abrir uma issue ou pull request.

## Antes de começar

- Verifique se já existe uma [issue](https://github.com/NullSablex/PawnPro-Engine/issues) aberta para o problema ou feature.
- Para mudanças significativas, abra uma issue primeiro para discutir a abordagem antes de implementar.
- Ao contribuir, você concorda que seu código será licenciado sob os mesmos termos da [licença do projeto](LICENSE.md).

## Configurando o ambiente

**Pré-requisitos:**
- Rust stable (`rustup install stable`)

```bash
git clone https://github.com/NullSablex/PawnPro-Engine
cd pawnpro-engine
cargo build
```

**Executar testes:**
```bash
cargo test
```

**Lint (obrigatório antes de PR):**
```bash
cargo clippy -- -D warnings
```

**Build release:**
```bash
cargo build --release
```

O binário de debug é detectado automaticamente pela extensão PawnPro se estiver em `../pawnpro-engine/target/debug/`.

## Estrutura do projeto

```
src/
  parser/       ← lexer, parser de símbolos e tipos
  analyzer/     ← diagnósticos PP0001–PP0013
  intellisense/ ← completions, hover, signature, codelens, references, semantic tokens
  workspace.rs  ← orquestra análise de cada arquivo
  server.rs     ← handlers LSP
  config.rs     ← configuração recebida via initializationOptions
docs/           ← documentação detalhada (não incluída nos releases)
```

## Regras de código

- **Zero `unwrap()` em código de produção** fora de `Lazy::new`. Usar `?`, `if let`, `unwrap_or`.
- **Nunca `panic!` em caminhos de análise** — o motor precisa sobreviver a qualquer entrada malformada.
- **Regexes sempre como `static Lazy<Regex>`** — nunca compilar dentro de loops.
- **Nunca interpolar input não escapado em `Regex::new`**.
- **`cargo clippy -- -D warnings` deve passar sem erros** — obrigatório.
- **Novos diagnósticos sempre com constante em `analyzer/codes.rs`**.
- **Funções utilitárias de texto em `parser/lexer.rs`** — não duplicar `decode_bytes`, `strip_line_comments`, `update_brace_depth` ou `has_inline_deprecated` em outros módulos.

## Adicionando um novo diagnóstico

1. Adicionar constante em `analyzer/codes.rs`: `pub const PP00XX: &str = "PP00XX";`
2. Implementar a lógica no analyzer correspondente (ou criar novo em `analyzer/`).
3. Chamar o novo analyzer em `analyzer/mod.rs` dentro de `analyze()`.
4. Adicionar testes unitários.
5. Documentar em `docs/diagnostics.md`.

## Abrindo uma Pull Request

1. Crie um branch a partir de `main`: `git checkout -b feat/minha-feature`
2. Faça as alterações seguindo as regras acima.
3. Certifique-se que `cargo clippy -- -D warnings` e `cargo test` passam.
4. Abra a PR com uma descrição clara do que foi alterado e por quê.

## Reportando bugs

Inclua na issue:
- Versão do motor (tag do release ou hash do commit)
- Sistema operacional e arquitetura
- Exemplo mínimo de arquivo `.pwn` que reproduz o problema
- Diagnóstico incorreto ou comportamento inesperado observado

## Sugestões de features

Abra uma issue com o label `enhancement` descrevendo:
- O problema que a feature resolveria
- Como você imagina que funcionaria no contexto do LSP
- Alternativas que você considerou
