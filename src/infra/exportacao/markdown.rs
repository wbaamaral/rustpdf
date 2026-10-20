// SPDX-License-Identifier: AGPL-3.0-or-later
//! Documento exportado → Markdown (CommonMark).

use crate::dominio::exportacao::modelo::{Block, Document};

/// Markdown e as imagens que ele referencia (caminho relativo, PNG).
#[derive(Debug, Clone, PartialEq)]
pub struct Markdown {
    pub text: String,
    pub images: Vec<(String, Vec<u8>)>,
}

/// Opções de renderização do Markdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MarkdownOptions {
    /// Modo otimizado para consumo por agentes de IA: evita escape excessivo de caracteres comuns.
    pub ai_mode: bool,
}

/// Markdown do documento: blocos separados por linha em branco, itens de
/// lista consecutivos juntos; termina com uma quebra de linha. As imagens
/// ficam em `images_folder` (`p<página>-i<n>.png`), ao lado do arquivo.
pub fn render(document: &Document, images_folder: &str) -> Markdown {
    render_with_options(document, images_folder, &MarkdownOptions::default())
}

/// Como [`render`], aplicando as opções especificadas.
pub fn render_with_options(
    document: &Document,
    images_folder: &str,
    options: &MarkdownOptions,
) -> Markdown {
    let mut text = String::new();
    let mut images = Vec::new();
    let mut previous_was_item = false;
    for page in &document.pages {
        let mut count = 0;
        for block in &page.blocks {
            let is_item = matches!(block, Block::ListItem { .. });
            if !text.is_empty() {
                text.push_str(if is_item && previous_was_item {
                    "\n"
                } else {
                    "\n\n"
                });
            }
            match block {
                Block::Image { png, .. } => {
                    count += 1;
                    let path = format!("{images_folder}/p{}-i{count}.png", page.number);
                    let target = if path.contains(' ') {
                        format!("<{path}>")
                    } else {
                        path.clone()
                    };
                    text.push_str(&format!("![Imagem da página {}]({target})", page.number));
                    images.push((path, png.clone()));
                }
                _ => text.push_str(&block_markdown(block, options)),
            }
            previous_was_item = is_item;
        }
    }
    if !text.is_empty() {
        text.push('\n');
    }
    Markdown { text, images }
}

fn block_markdown(block: &Block, options: &MarkdownOptions) -> String {
    match block {
        Block::Table { rows } => table_markdown(rows, options),
        Block::Image { .. } => String::new(), // tratada em `render`
        Block::Heading { level, text } => {
            let rendered_text = if options.ai_mode {
                text.trim().to_string()
            } else {
                escape_inline(text)
            };
            format!(
                "{} {}",
                "#".repeat(usize::from(*level).clamp(1, 6)),
                rendered_text
            )
        }
        Block::Paragraph { text } => {
            if options.ai_mode {
                text.to_string()
            } else {
                escape(text)
            }
        }
        Block::ListItem {
            marker,
            level,
            text,
        } => {
            let indent = "    ".repeat(usize::from(*level));
            let text = if options.ai_mode {
                text.to_string()
            } else {
                escape(text)
            };
            match numbered(marker) {
                Some(number) => format!("{indent}{number}. {text}"),
                None if marker.is_empty() => format!("{indent}- {text}"),
                None => {
                    let m = if options.ai_mode {
                        marker.to_string()
                    } else {
                        escape_inline(marker)
                    };
                    format!("{indent}- {m} {text}")
                }
            }
        }
    }
}

/// Tabela GFM com as colunas alinhadas; a primeira linha é o cabeçalho.
fn table_markdown(rows: &[Vec<String>], options: &MarkdownOptions) -> String {
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    if options.ai_mode {
                        cell.replace('|', "\\|")
                    } else {
                        escape_inline(cell).replace('|', "\\|")
                    }
                })
                .collect()
        })
        .collect();
    let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            cells
                .iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
                .max(3)
        })
        .collect();
    let line = |row: &[String]| {
        let padded: Vec<String> = widths
            .iter()
            .enumerate()
            .map(|(column, width)| {
                let cell = row.get(column).map(String::as_str).unwrap_or("");
                format!("{cell}{}", " ".repeat(width - cell.chars().count()))
            })
            .collect();
        format!("| {} |", padded.join(" | "))
    };
    let separator: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
    let mut out = Vec::with_capacity(cells.len() + 1);
    if let Some((header, body)) = cells.split_first() {
        out.push(line(header));
        out.push(format!("| {} |", separator.join(" | ")));
        out.extend(body.iter().map(|row| line(row)));
    }
    out.join("\n")
}

/// Número de um rótulo `12.` ou `12)`.
fn numbered(marker: &str) -> Option<&str> {
    let number = marker
        .strip_suffix('.')
        .or_else(|| marker.strip_suffix(')'))?;
    (!number.is_empty() && number.chars().all(|c| c.is_ascii_digit())).then_some(number)
}

/// Escapa a sintaxe em linha (ênfase, código, links, HTML).
fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '\\' | '*' | '_' | '`' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

/// Escapa também o que, no início, viraria título, citação ou lista.
fn escape(text: &str) -> String {
    let out = escape_inline(text);
    if out.starts_with(['#', '>', '-', '+']) {
        return format!("\\{out}");
    }
    let digits = out.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && out[digits..].starts_with(['.', ')']) {
        return format!("{}\\{}", &out[..digits], &out[digits..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::exportacao::modelo::{Block, Document, PageContent, A4};

    fn document(pages: Vec<Vec<Block>>) -> Document {
        Document {
            pages: pages
                .into_iter()
                .enumerate()
                .map(|(index, blocks)| PageContent {
                    number: index + 1,
                    size: A4,
                    blocks,
                })
                .collect(),
        }
    }

    fn item(marker: &str, level: u8, text: &str) -> Block {
        Block::ListItem {
            marker: marker.into(),
            level,
            text: text.into(),
        }
    }

    #[test]
    fn headings_paragraphs_and_lists() {
        let doc = document(vec![
            vec![
                Block::Heading {
                    level: 1,
                    text: "Relatório".into(),
                },
                Block::Paragraph {
                    text: "Primeiro parágrafo.".into(),
                },
                item("", 0, "Um"),
                item("", 1, "Um ponto um"),
                item("2.", 0, "Dois"),
                item("b)", 0, "Letra"),
                Block::Heading {
                    level: 3,
                    text: "Fim".into(),
                },
            ],
            vec![Block::Paragraph {
                text: "Página dois.".into(),
            }],
        ]);
        assert_eq!(
            render(&doc, "img").text,
            "# Relatório\n\nPrimeiro parágrafo.\n\n- Um\n    - Um ponto um\n2. Dois\n- b) Letra\n\n### Fim\n\nPágina dois.\n"
        );
    }

    #[test]
    fn markdown_syntax_in_text_is_escaped() {
        let doc = document(vec![vec![
            Block::Paragraph {
                text: "# não é título".into(),
            },
            Block::Paragraph {
                text: "1. não é lista; *nem* _isto_ [nem] `isto` <b> \\".into(),
            },
            Block::Paragraph {
                text: "- traço inicial".into(),
            },
        ]]);
        assert_eq!(
            render(&doc, "img").text,
            "\\# não é título\n\n1\\. não é lista; \\*nem\\* \\_isto\\_ \\[nem\\] \\`isto\\` \\<b> \\\\\n\n\\- traço inicial\n"
        );
    }

    #[test]
    fn empty_pages_add_nothing() {
        let doc = document(vec![
            vec![],
            vec![Block::Paragraph {
                text: "Só.".into()
            }],
            vec![],
        ]);
        assert_eq!(render(&doc, "img").text, "Só.\n");
        assert_eq!(render(&Document::default(), "img").text, "");
    }

    #[test]
    fn tables_become_aligned_gfm_tables() {
        let doc = document(vec![vec![
            Block::Paragraph {
                text: "Preços:".into(),
            },
            Block::Table {
                rows: vec![
                    vec!["Item".into(), "Preço".into()],
                    vec!["Lápis | grafite".into(), "1,00".into()],
                    vec!["Borracha".into(), "".into()],
                ],
            },
        ]]);
        assert_eq!(
            render(&doc, "img").text,
            "Preços:\n\n\
| Item             | Preço |\n\
| ---------------- | ----- |\n\
| Lápis \\| grafite | 1,00  |\n\
| Borracha         |       |\n"
        );
    }

    #[test]
    fn images_are_linked_and_returned_as_files() {
        let doc = document(vec![
            vec![Block::Paragraph {
                text: "Figura:".into(),
            }],
            vec![
                Block::Image {
                    png: vec![1, 2],
                    size: (200.0, 100.0),
                },
                Block::Image {
                    png: vec![3],
                    size: (10.0, 10.0),
                },
            ],
        ]);
        let markdown = render(&doc, "relatorio_arquivos");
        assert_eq!(
            markdown.text,
            "Figura:\n\n![Imagem da página 2](relatorio_arquivos/p2-i1.png)\n\n![Imagem da página 2](relatorio_arquivos/p2-i2.png)\n"
        );
        assert_eq!(
            markdown.images,
            [
                ("relatorio_arquivos/p2-i1.png".to_string(), vec![1, 2]),
                ("relatorio_arquivos/p2-i2.png".to_string(), vec![3]),
            ]
        );
        let spaced = render(&doc, "meu relatório_arquivos");
        assert!(
            spaced
                .text
                .contains("](<meu relatório_arquivos/p2-i1.png>)"),
            "{}",
            spaced.text
        );
    }

    #[test]
    fn ai_mode_avoids_over_escaping() {
        let doc = document(vec![vec![
            Block::Heading {
                level: 2,
                text: "Seção [1]: Variáveis".into(),
            },
            Block::Paragraph {
                text: "Referência [1] descreve _variáveis_ com *ênfase* e `código` <sem-escape>."
                    .into(),
            },
            Block::Table {
                rows: vec![
                    vec!["Item [A]".into(), "Nota".into()],
                    vec!["Teste | pipe".into(), "100%".into()],
                ],
            },
        ]]);

        let ai_markdown = render_with_options(&doc, "img", &MarkdownOptions { ai_mode: true });
        assert_eq!(
            ai_markdown.text,
            "## Seção [1]: Variáveis\n\n\
Referência [1] descreve _variáveis_ com *ênfase* e `código` <sem-escape>.\n\n\
| Item [A]      | Nota |\n\
| ------------- | ---- |\n\
| Teste \\| pipe | 100% |\n"
        );
    }
}
