// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lógica de interface e da CLI, testável sem GTK.
//! Não fala com a infraestrutura; recebe os casos de uso já montados.
#![forbid(unsafe_code)]
pub mod arraste;
pub mod cli;
pub mod dialogos;
pub mod edicao_texto;
pub mod exportacao;
pub mod formularios;
pub mod menu_contexto;
pub mod miniaturas;
pub mod selecao;
