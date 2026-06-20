//! Tabelas de tradução, um módulo por idioma. Cada um expõe `get(MsgKey)`;
//! o roteamento por `Locale` fica no pai (`messages::msg`).

pub mod en;
pub mod es;
pub mod pt_br;
pub mod ro;
pub mod ru;
