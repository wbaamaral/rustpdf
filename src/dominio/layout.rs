// SPDX-License-Identifier: AGPL-3.0-or-later
//! Layout de uma página: blocos › linhas › palavras com caixas em pontos,
//! origem no topo (como o poppler e o OCR entregam).

use crate::dominio::geometria::LayoutRect;

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub rect: LayoutRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub words: Vec<Word>,
}

impl Line {
    /// Palavras separadas por espaço.
    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Caixa que cobre todas as palavras (`None` sem palavras).
    pub fn rect(&self) -> Option<LayoutRect> {
        self.words
            .iter()
            .map(|word| word.rect)
            .reduce(|joined, rect| joined.union(&rect))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub blocks: Vec<Block>,
}

/// Linha sob um ponto: a linha, a palavra mais próxima dele e a distância
/// do ponto à caixa da linha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineHit<'a> {
    pub line: &'a Line,
    pub word: &'a Word,
    pub distance: f64,
}

impl Page {
    /// O mesmo layout com todas as medidas multiplicadas por `factor`
    /// (ex.: pixels do OCR a 300 dpi → pontos com `72 / 300`).
    pub fn scaled(&self, factor: f64) -> Page {
        let scale = |rect: LayoutRect| {
            LayoutRect::new(
                rect.left() * factor,
                rect.top() * factor,
                rect.right() * factor,
                rect.bottom() * factor,
            )
        };
        Page {
            width: self.width * factor,
            height: self.height * factor,
            blocks: self
                .blocks
                .iter()
                .map(|block| Block {
                    lines: block
                        .lines
                        .iter()
                        .map(|line| Line {
                            words: line
                                .words
                                .iter()
                                .map(|word| Word {
                                    text: word.text.clone(),
                                    rect: scale(word.rect),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.blocks.iter().flat_map(|block| block.lines.iter())
    }

    pub fn words(&self) -> impl Iterator<Item = &Word> {
        self.lines().flat_map(|line| line.words.iter())
    }

    /// Linha cuja caixa está a até `tolerance` pontos de `point` (`(x, topo)`),
    /// inclusive entre as palavras; a mais próxima vence (empate: a primeira).
    pub fn line_at(&self, point: (f64, f64), tolerance: f64) -> Option<LineHit<'_>> {
        let mut best: Option<LineHit<'_>> = None;
        for line in self.lines() {
            let Some(rect) = line.rect() else {
                continue;
            };
            let distance = rect.distance_to(point);
            if distance > tolerance || best.is_some_and(|hit| hit.distance <= distance) {
                continue;
            }
            let word = line
                .words
                .iter()
                .min_by(|a, b| {
                    a.rect
                        .distance_to(point)
                        .total_cmp(&b.rect.distance_to(point))
                })
                .expect("linha com caixa tem palavras");
            best = Some(LineHit {
                line,
                word,
                distance,
            });
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, left: f64, top: f64, right: f64, bottom: f64) -> Word {
        Word {
            text: text.into(),
            rect: LayoutRect::new(left, top, right, bottom),
        }
    }

    /// Duas linhas: "Olá mundo" (topo 100) e "Fim" (topo 200), em blocos distintos.
    fn page() -> Page {
        Page {
            width: 595.0,
            height: 842.0,
            blocks: vec![
                Block {
                    lines: vec![Line {
                        words: vec![
                            word("Olá", 100.0, 100.0, 125.0, 112.0),
                            word("mundo", 140.0, 100.0, 180.0, 112.0),
                        ],
                    }],
                },
                Block {
                    lines: vec![Line {
                        words: vec![word("Fim", 100.0, 200.0, 120.0, 212.0)],
                    }],
                },
            ],
        }
    }

    #[test]
    fn line_text_and_box_cover_its_words() {
        let page = page();
        let line = &page.blocks[0].lines[0];
        assert_eq!(line.text(), "Olá mundo");
        assert_eq!(
            line.rect(),
            Some(LayoutRect::new(100.0, 100.0, 180.0, 112.0))
        );
        assert_eq!(Line { words: vec![] }.rect(), None);
    }

    #[test]
    fn words_and_lines_follow_reading_order() {
        let page = page();
        let words: Vec<&str> = page.words().map(|word| word.text.as_str()).collect();
        assert_eq!(words, ["Olá", "mundo", "Fim"]);
        assert_eq!(page.lines().count(), 2);
    }

    #[test]
    fn line_at_accepts_gaps_between_words_and_picks_the_nearest_word() {
        let page = page();
        let hit = page.line_at((150.0, 106.0), 2.0).unwrap();
        assert_eq!(hit.line.text(), "Olá mundo");
        assert_eq!(hit.word.text, "mundo");
        assert_eq!(hit.distance, 0.0);
        // Entre as palavras (x = 132): ainda é a linha; "Olá" está a 7 pt,
        // "mundo" a 8 pt.
        let gap = page.line_at((132.0, 106.0), 2.0).unwrap();
        assert_eq!(gap.word.text, "Olá");
        assert_eq!(gap.distance, 0.0);
    }

    #[test]
    fn line_at_respects_tolerance_and_prefers_the_closest_line() {
        let page = page();
        assert!(page.line_at((150.0, 150.0), 2.0).is_none());
        let near = page.line_at((110.0, 113.5), 2.0).unwrap();
        assert_eq!(near.line.text(), "Olá mundo");
        assert_eq!(near.distance, 1.5);
        assert_eq!(page.line_at((110.0, 205.0), 2.0).unwrap().word.text, "Fim");
    }

    #[test]
    fn scaling_converts_pixels_to_points() {
        // OCR a 300 dpi: 2480 × 3508 px → A4 em pontos.
        let pixels = Page {
            width: 2480.0,
            height: 3508.0,
            blocks: vec![Block {
                lines: vec![Line {
                    words: vec![word("OCR", 300.0, 600.0, 600.0, 700.0)],
                }],
            }],
        };
        let points = pixels.scaled(72.0 / 300.0);
        assert!((points.width - 595.2).abs() < 1e-9 && (points.height - 841.92).abs() < 1e-9);
        let rect = points.words().next().unwrap().rect;
        assert_eq!(rect, LayoutRect::new(72.0, 144.0, 144.0, 168.0));
    }
}
