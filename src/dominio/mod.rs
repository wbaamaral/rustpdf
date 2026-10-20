// SPDX-License-Identifier: AGPL-3.0-or-later
//! Regras puras do editor: geometria, conteúdo, layout e exportação.
//! Sem I/O, FFI, processos ou threads (ver `tests/arquitetura.rs`).
#![forbid(unsafe_code)]

pub mod exportacao;
pub mod geometria;
pub mod layout;
pub mod paginas;
