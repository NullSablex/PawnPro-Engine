//! Conversões numéricas seguras para uso em todo o crate.
//!
//! O protocolo LSP usa `u32` para linhas e colunas, mas internamente trabalhamos
//! com índices `usize`. Em vez de `as u32` (que trunca silenciosamente e dispara
//! `clippy::cast_possible_truncation`), centralizamos a conversão aqui com
//! **saturação**: um documento jamais terá mais de `u32::MAX` (~4,3 bilhões) de
//! linhas ou colunas, então saturar é seguro e nunca causa panic nem corrupção
//! de posição como o truncamento faria.

/// Converte um índice `usize` (linha/coluna) para `u32` saturando em `u32::MAX`.
#[inline]
#[must_use]
pub fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
