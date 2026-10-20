// SPDX-License-Identifier: AGPL-3.0-or-later
//! Páginas como slides: caixas de texto e imagens nas posições do PDF
//! (PowerPoint editável) ou a página inteira como imagem (fiel).

pub use super::modelo::PageImage;
use crate::dominio::geometria::LayoutRect;
use crate::dominio::layout::Page;

/// Fonte ≈ 80% da altura da linha (a caixa do poppler inclui ascendentes e
/// descendentes).
const FONT_RATIO: f64 = 0.8;

/// Elemento posicionado no slide (pontos, origem no topo).
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Text {
        rect: LayoutRect,
        lines: Vec<String>,
        /// Tamanho da fonte em pontos (múltiplo de 0,5).
        font_size: f64,
    },
    Picture {
        rect: LayoutRect,
        png: Vec<u8>,
    },
    Line {
        start: (f64, f64),
        end: (f64, f64),
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slide {
    /// Largura e altura da página, em pontos.
    pub size: (f64, f64),
    pub shapes: Vec<Shape>,
}

/// Slide editável: uma caixa de texto por bloco do layout e as imagens.
pub fn slide(page: &Page, images: Vec<PageImage>) -> Slide {
    slide_with_rulings(page, images, Vec::new())
}

/// Slide editável incluindo traços/linhas vetoriais (para formatos gráficos vetoriais como ODG).
pub fn slide_with_rulings(
    page: &Page,
    images: Vec<PageImage>,
    rulings: Vec<((f64, f64), (f64, f64))>,
) -> Slide {
    let mut shapes: Vec<Shape> = page
        .blocks
        .iter()
        .filter_map(|block| {
            let rect = block
                .lines
                .iter()
                .filter_map(|line| line.rect())
                .reduce(|joined, rect| joined.union(&rect))?;
            let height = block
                .lines
                .iter()
                .filter_map(|line| line.rect())
                .map(|rect| rect.height())
                .fold(0.0, f64::max);
            Some(Shape::Text {
                rect,
                lines: block.lines.iter().map(|line| line.text()).collect(),
                font_size: ((height * FONT_RATIO) * 2.0).round() / 2.0,
            })
        })
        .collect();
    shapes.extend(images.into_iter().map(|image| Shape::Picture {
        rect: image.rect,
        png: image.png,
    }));
    shapes.extend(
        rulings
            .into_iter()
            .map(|(start, end)| Shape::Line { start, end }),
    );
    Slide {
        size: (page.width, page.height),
        shapes,
    }
}

/// Slide fiel: a página renderizada ocupando o slide inteiro.
pub fn faithful_slide(size: (f64, f64), png: Vec<u8>) -> Slide {
    Slide {
        size,
        shapes: vec![Shape::Picture {
            rect: LayoutRect::new(0.0, 0.0, size.0, size.1),
            png,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::layout::{Block, Line, Page, Word};

    fn line(text: &str, left: f64, top: f64, size: f64) -> Line {
        let mut x = left;
        Line {
            words: text
                .split_whitespace()
                .map(|word| {
                    let right = x + 0.5 * size * word.len() as f64;
                    let rect = LayoutRect::new(x, top, right, top + size);
                    x = right + 0.3 * size;
                    Word {
                        text: word.into(),
                        rect,
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn each_layout_block_becomes_a_text_box_where_it_was() {
        let page = Page {
            width: 595.0,
            height: 842.0,
            blocks: vec![
                Block {
                    lines: vec![line("Título", 72.0, 60.0, 24.0)],
                },
                Block {
                    lines: vec![
                        line("Primeira linha", 72.0, 120.0, 12.0),
                        line("segunda", 72.0, 134.0, 12.0),
                    ],
                },
                Block { lines: vec![] },
            ],
        };
        let slide = slide(&page, Vec::new());
        assert_eq!(slide.size, (595.0, 842.0));
        assert_eq!(slide.shapes.len(), 2, "bloco vazio não vira caixa");
        let Shape::Text {
            rect,
            lines,
            font_size,
        } = &slide.shapes[1]
        else {
            panic!("esperava texto");
        };
        assert_eq!(lines, &["Primeira linha", "segunda"]);
        assert_eq!(rect.left(), 72.0);
        assert_eq!(rect.top(), 120.0);
        assert_eq!(*font_size, 9.5, "altura da linha × 0,8, em meio ponto");
        let Shape::Text { font_size, .. } = &slide.shapes[0] else {
            panic!()
        };
        assert_eq!(*font_size, 19.0);
    }

    #[test]
    fn images_keep_their_boxes() {
        let page = Page {
            width: 300.0,
            height: 400.0,
            blocks: vec![],
        };
        let rect = LayoutRect::new(10.0, 20.0, 110.0, 70.0);
        let slide = slide(&page, vec![PageImage { rect, png: vec![7] }]);
        assert_eq!(slide.shapes, [Shape::Picture { rect, png: vec![7] }]);
    }

    #[test]
    fn faithful_slide_is_one_full_page_picture() {
        let slide = faithful_slide((300.0, 400.0), vec![1, 2]);
        assert_eq!(
            slide.shapes,
            [Shape::Picture {
                rect: LayoutRect::new(0.0, 0.0, 300.0, 400.0),
                png: vec![1, 2]
            }]
        );
    }

    #[test]
    fn slide_with_rulings_keeps_vector_lines() {
        let page = Page {
            width: 300.0,
            height: 400.0,
            blocks: vec![],
        };
        let rulings = vec![((10.0, 20.0), (100.0, 20.0))];
        let slide = slide_with_rulings(&page, Vec::new(), rulings);
        assert_eq!(
            slide.shapes,
            [Shape::Line {
                start: (10.0, 20.0),
                end: (100.0, 20.0)
            }]
        );
    }
}
