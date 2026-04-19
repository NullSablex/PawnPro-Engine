# Política de Segurança — PawnPro Engine

## Reportar uma vulnerabilidade

Encontrou uma vulnerabilidade de segurança? Não abra uma issue pública.

**Contato:** abra um [Security Advisory](https://github.com/NullSablex/PawnPro-Engine/security/advisories/new) privado no GitHub ou envie um e-mail diretamente ao mantenedor.

Resposta esperada em até **7 dias úteis**.

---

## Escopo

Esta política cobre o código-fonte do motor LSP em Rust (repositório `NullSablex/PawnPro-Engine`). Para a extensão VS Code, consulte o [SECURITY.md do PawnPro](https://github.com/NullSablex/PawnPro/blob/master/SECURITY.md).

---

## Dependências

O motor não possui dependências em runtime além da libc do sistema. O binário é compilado como executável estático (musl no Linux) sem bibliotecas dinâmicas externas.

Dependências de build (listadas em `Cargo.toml`) são verificadas via `cargo audit` no CI a cada push.

---

## Versões suportadas

Somente a versão mais recente disponível nas [Releases](https://github.com/NullSablex/PawnPro-Engine/releases) recebe correções de segurança.
