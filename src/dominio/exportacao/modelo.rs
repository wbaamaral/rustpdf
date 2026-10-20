// SPDX-License-Identifier: AGPL-3.0-or-later
//! Modelo do documento exportado: o que os escritores (Markdown, CSV, Office)
//! recebem. Não guarda posições finas, só a estrutura inferida.
use crate::dominio::geometria::LayoutRect;

/// Elemento de conteúdo de uma página, na ordem de leitura.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Título de nível 1 (maior) a 3.
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    /// Item de lista; `marker` vazio = marcador gráfico (•), senão o rótulo
    /// original (`1.`, `a)`, …); `level` 0 = primeiro nível.
    ListItem {
        marker: String,
        level: u8,
        text: String,
    },
    /// Tabela em linhas × colunas (mesma quantidade de células por linha);
    /// a primeira linha é tratada como cabeçalho.
    Table {
        rows: Vec<Vec<String>>,
    },
    /// Imagem (PNG) com o tamanho em que aparece na página, em pontos.
    Image {
        png: Vec<u8>,
        size: (f64, f64),
    },
}

/// Imagem da página já renderizada, com a caixa onde aparece.
#[derive(Debug, Clone, PartialEq)]
pub struct PageImage {
    pub rect: LayoutRect,
    pub png: Vec<u8>,
}

/// Página exportada, numerada como no PDF (1-based).
#[derive(Debug, Clone, PartialEq)]
pub struct PageContent {
    pub number: usize,
    /// Largura e altura da página no PDF, em pontos.
    pub size: (f64, f64),
    pub blocks: Vec<Block>,
}

/// A4 em pontos (595 × 842), para páginas montadas em testes.
pub const A4: (f64, f64) = (595.0, 842.0);

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub pages: Vec<PageContent>,
}

/// Tabela do documento com a página e a posição dela na página (1-based).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableRef<'a> {
    pub page: usize,
    pub index: usize,
    pub rows: &'a [Vec<String>],
}

impl Document {
    /// Todas as tabelas, na ordem do documento.
    pub fn tables(&self) -> Vec<TableRef<'_>> {
        let mut tables = Vec::new();
        for page in &self.pages {
            let mut index = 0;
            for block in &page.blocks {
                if let Block::Table { rows } = block {
                    index += 1;
                    tables.push(TableRef {
                        page: page.number,
                        index,
                        rows,
                    });
                }
            }
        }
        tables
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(cells: &[&str]) -> Block {
        Block::Table {
            rows: vec![cells.iter().map(|c| c.to_string()).collect()],
        }
    }

    #[test]
    fn tables_are_listed_with_page_and_position() {
        let document = Document {
            pages: vec![
                PageContent {
                    number: 2,
                    size: A4,
                    blocks: vec![
                        table(&["a"]),
                        Block::Paragraph { text: "x".into() },
                        table(&["b"]),
                    ],
                },
                PageContent {
                    number: 5,
                    size: A4,
                    blocks: vec![table(&["c"])],
                },
            ],
        };
        let found: Vec<(usize, usize, &str)> = document
            .tables()
            .iter()
            .map(|t| (t.page, t.index, t.rows[0][0].as_str()))
            .collect();
        assert_eq!(found, [(2, 1, "a"), (2, 2, "b"), (5, 1, "c")]);
        assert!(Document::default().tables().is_empty());
    }
}
