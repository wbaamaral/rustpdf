// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lógica de interface e da CLI, testável sem GTK.
//! Não fala com a infraestrutura; recebe os casos de uso já montados.
#![forbid(unsafe_code)]
pub mod cli;
pub mod exportacao;
pub mod miniaturas;
