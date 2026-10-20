// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tabelas exportadas → CSV (RFC 4180).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvOptions {
    pub separator: char,
    /// Marca UTF-8 no início: o Excel precisa dela para ler os acentos.
    pub bom: bool,
}

impl Default for CsvOptions {
    /// `;` e BOM: abre direto no Excel em português.
    fn default() -> Self {
        Self {
            separator: ';',
            bom: true,
        }
    }
}

/// CSV da tabela: campos entre aspas só quando necessário, linhas com CRLF.
pub fn render(rows: &[Vec<String>], options: &CsvOptions) -> String {
    let mut out = String::new();
    if options.bom {
        out.push('\u{feff}');
    }
    for row in rows {
        let fields: Vec<String> = row
            .iter()
            .map(|cell| field(cell, options.separator))
            .collect();
        out.push_str(&fields.join(&options.separator.to_string()));
        out.push_str("\r\n");
    }
    out
}

fn field(cell: &str, separator: char) -> String {
    let needs_quotes =
        cell.contains([separator, '"', '\n', '\r']) || cell.starts_with(' ') || cell.ends_with(' ');
    if needs_quotes {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// Arquivo de cada tabela `(página, posição)`: uma só usa `output`; várias
/// ganham o sufixo `-p<página>-t<posição>` antes da extensão.
pub fn output_paths(output: &Path, tables: &[(usize, usize)]) -> Vec<PathBuf> {
    if tables.len() == 1 {
        return vec![output.to_path_buf()];
    }
    let stem = output
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = output
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    tables
        .iter()
        .map(|(page, index)| output.with_file_name(format!("{stem}-p{page}-t{index}{extension}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn rows(cells: &[&[&str]]) -> Vec<Vec<String>> {
        cells
            .iter()
            .map(|row| row.iter().map(|c| c.to_string()).collect())
            .collect()
    }

    #[test]
    fn fields_are_quoted_only_when_needed() {
        let options = CsvOptions {
            separator: ';',
            bom: false,
        };
        let table = rows(&[
            &["Item", "Preço"],
            &["Lápis; grafite", "1,00"],
            &["Diz \"oi\"", " espaço "],
            &["Duas\nlinhas", ""],
        ]);
        assert_eq!(
            render(&table, &options),
            "Item;Preço\r\n\"Lápis; grafite\";1,00\r\n\"Diz \"\"oi\"\"\";\" espaço \"\r\n\"Duas\nlinhas\";\r\n"
        );
    }

    #[test]
    fn bom_and_separator_follow_the_options() {
        let table = rows(&[&["a", "b,c"]]);
        let comma = CsvOptions {
            separator: ',',
            bom: true,
        };
        assert_eq!(render(&table, &comma), "\u{feff}a,\"b,c\"\r\n");
        assert_eq!(
            CsvOptions::default(),
            CsvOptions {
                separator: ';',
                bom: true
            }
        );
    }

    #[test]
    fn one_table_uses_the_output_name_and_more_get_suffixes() {
        let output = Path::new("/tmp/saida.csv");
        assert_eq!(
            output_paths(output, &[(3, 1)]),
            [PathBuf::from("/tmp/saida.csv")]
        );
        assert_eq!(
            output_paths(output, &[(3, 1), (3, 2), (7, 1)]),
            [
                PathBuf::from("/tmp/saida-p3-t1.csv"),
                PathBuf::from("/tmp/saida-p3-t2.csv"),
                PathBuf::from("/tmp/saida-p7-t1.csv"),
            ]
        );
    }
}
