// SPDX-License-Identifier: AGPL-3.0-or-later
//! Estrutura de uma página a partir do layout: parágrafos (com junção de
//! hifenização), títulos (pela altura da linha) e listas (pelo marcador).

use super::modelo::{Block, PageImage};
use super::tabelas::{find_tables_with_rulings, WordId};
use crate::dominio::geometria::Segment;
use crate::dominio::layout::{Line, Page};
use std::collections::{BTreeMap, HashSet};

/// Uma linha a partir desta proporção da altura do corpo é candidata a título.
const HEADING_RATIO: f64 = 1.2;
/// Marcadores gráficos de lista.
const BULLETS: &[&str] = &["•", "◦", "▪", "▫", "‣", "●", "○", "■", "-", "–", "—", "*"];

/// Blocos de conteúdo da página, na ordem de leitura do layout.
pub fn structure(page: &Page) -> Vec<Block> {
    structure_with(page, Vec::new(), &[])
}

/// Como [`structure`], com as imagens da página entre os blocos (na ordem
/// da posição vertical) e as grades desenhadas (`segments`, origem no topo)
/// delimitando tabelas.
pub fn structure_with(page: &Page, mut images: Vec<PageImage>, segments: &[Segment]) -> Vec<Block> {
    images.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
    let mut images = images.into_iter().peekable();
    let mut blocks = Vec::new();
    let mut emit_images_above = |blocks: &mut Vec<Block>, top: f64| {
        while let Some(image) = images.next_if(|image| image.rect.top() < top) {
            blocks.push(Block::Image {
                size: (image.rect.width(), image.rect.height()),
                png: image.png,
            });
        }
    };
    if let Some(body) = body_height(page) {
        for (top, group) in groups(page, body, segments) {
            emit_images_above(&mut blocks, top);
            match group {
                Group::Table(rows) => blocks.push(Block::Table { rows }),
                Group::Lines(lines) => {
                    let lines = merge_loose_markers(lines);
                    match heading(&lines, body) {
                        Some(heading) => blocks.push(heading),
                        None => blocks.extend(paragraphs_and_lists(&lines, body)),
                    }
                }
            }
        }
    }
    emit_images_above(&mut blocks, f64::INFINITY);
    blocks
}

/// Blocos do layout como grupos de linhas ou tabelas, com o topo de cada um.
fn groups(page: &Page, body: f64, segments: &[Segment]) -> Vec<(f64, Group)> {
    let tables = find_tables_with_rulings(page, segments);
    let in_table: HashSet<WordId> = tables
        .iter()
        .flat_map(|table| table.words.iter().copied())
        .collect();
    let mut emitted = vec![false; tables.len()];
    // Grupos na ordem do layout; cada tabela entra antes do primeiro bloco
    // que tem palavras dela, e as palavras dela saem das linhas.
    let mut groups: Vec<(f64, Group)> = Vec::new();
    for (b, layout_block) in page.blocks.iter().enumerate() {
        for (index, table) in tables.iter().enumerate() {
            if !emitted[index] && table.words.iter().any(|&(block, _, _)| block == b) {
                emitted[index] = true;
                groups.push((table.top, Group::Table(table.rows.clone())));
            }
        }
        let lines: Vec<TextLine> = layout_block
            .lines
            .iter()
            .enumerate()
            .filter_map(|(l, line)| {
                let words = line
                    .words
                    .iter()
                    .enumerate()
                    .filter(|(w, _)| !in_table.contains(&(b, l, *w)))
                    .map(|(_, word)| word.clone())
                    .collect();
                TextLine::new(&Line { words })
            })
            .collect();
        if lines.is_empty() || is_margin_number(&lines, page.height) {
            continue;
        }
        if let Some((_, Group::Lines(previous))) = groups.last_mut() {
            if continues_heading_line(previous, &lines, body) {
                previous[0].words.extend(lines[0].words.iter().cloned());
                continue;
            }
        }
        groups.push((lines[0].top, Group::Lines(lines)));
    }
    groups
}

/// Linhas de um bloco do layout, ou uma tabela já montada.
enum Group {
    Lines(Vec<TextLine>),
    Table(Vec<Vec<String>>),
}

/// Linha com o texto, a margem esquerda, o topo e a altura.
#[derive(Debug, Clone)]
struct TextLine {
    words: Vec<String>,
    left: f64,
    top: f64,
    height: f64,
}

impl TextLine {
    fn new(line: &Line) -> Option<Self> {
        let rect = line.rect()?;
        Some(Self {
            words: line.words.iter().map(|word| word.text.clone()).collect(),
            left: rect.left(),
            top: rect.top(),
            height: rect.height(),
        })
    }

    fn text(&self) -> String {
        self.words.join(" ")
    }

    /// Marcador de lista no início: `Some("")` para marcador gráfico,
    /// `Some("1.")` etc. para rótulos.
    fn marker(&self) -> Option<String> {
        let first = self.words.first()?;
        if BULLETS.contains(&first.as_str()) {
            Some(String::new())
        } else if is_label(first) {
            Some(first.clone())
        } else {
            None
        }
    }
}

/// Marcador de lista: gráfico (•, –, *) ou rótulo (`1.`, `a)`, `iv.`).
pub(super) fn is_list_marker(word: &str) -> bool {
    BULLETS.contains(&word) || is_label(word)
}

/// `1.`, `12)`, `a.`, `B)`, `iv.` — números de até 3 dígitos, uma letra ou
/// algarismos romanos curtos, seguidos de `.` ou `)`.
fn is_label(word: &str) -> bool {
    let Some(stem) = word.strip_suffix('.').or_else(|| word.strip_suffix(')')) else {
        return false;
    };
    let digits = !stem.is_empty() && stem.len() <= 3 && stem.chars().all(|c| c.is_ascii_digit());
    let letter = stem.chars().count() == 1 && stem.chars().all(|c| c.is_alphabetic());
    let roman = (2..=4).contains(&stem.len())
        && stem
            .chars()
            .all(|c| matches!(c, 'i' | 'v' | 'x' | 'I' | 'V' | 'X'));
    digits || letter || roman
}

/// Altura de linha do corpo do texto: a que cobre mais caracteres na página
/// (em meio ponto; empate fica com a menor).
fn body_height(page: &Page) -> Option<f64> {
    let mut weights: BTreeMap<i64, usize> = BTreeMap::new();
    for line in page.lines() {
        let Some(rect) = line.rect() else {
            continue;
        };
        let characters: usize = line
            .words
            .iter()
            .map(|word| word.text.chars().count())
            .sum();
        *weights
            .entry((rect.height() * 2.0).round() as i64)
            .or_default() += characters;
    }
    let (key, _) = weights
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))?;
    Some((*key as f64 / 2.0).max(1.0))
}

/// Número de página: bloco de uma palavra numérica na faixa de 8% do topo ou
/// da base da página.
fn is_margin_number(lines: &[TextLine], page_height: f64) -> bool {
    let [line] = lines else {
        return false;
    };
    let numeric =
        matches!(line.words.as_slice(), [word] if word.chars().all(|c| c.is_ascii_digit()));
    let band = 0.08 * page_height;
    numeric && (line.top < band || line.top + line.height > page_height - band)
}

/// O bloco `next` continua, na mesma linha, o título de uma linha em
/// `previous` (o poppler separa "1" e "Introdução" quando há espaço largo).
fn continues_heading_line(previous: &[TextLine], next: &[TextLine], body: f64) -> bool {
    let ([before], [after]) = (previous, next) else {
        return false;
    };
    let heading_sized = |line: &TextLine| line.height >= HEADING_RATIO * body;
    heading_sized(before)
        && heading_sized(after)
        && (before.top - after.top).abs() < 0.2 * before.height
        && (before.height / after.height - 1.0).abs() < 0.1
        && after.left > before.left
}

/// Junta um rótulo sozinho na linha ("3.") ao texto da linha seguinte.
fn merge_loose_markers(lines: Vec<TextLine>) -> Vec<TextLine> {
    let mut merged: Vec<TextLine> = Vec::new();
    let mut pending: Option<TextLine> = None;
    for line in lines {
        if let Some(mut marker) = pending.take() {
            marker.words.extend(line.words);
            marker.height = marker.height.max(line.height);
            merged.push(marker);
            continue;
        }
        if line.words.len() == 1 && line.marker().is_some_and(|m| !m.is_empty()) {
            pending = Some(line);
        } else {
            merged.push(line);
        }
    }
    merged.extend(pending);
    merged
}

/// Bloco curto (até 2 linhas), sem lista, com linhas maiores que o corpo.
fn heading(lines: &[TextLine], body: f64) -> Option<Block> {
    let smallest = lines
        .iter()
        .map(|line| line.height)
        .fold(f64::MAX, f64::min);
    let largest = lines.iter().map(|line| line.height).fold(0.0, f64::max);
    if lines.len() > 2
        || smallest < HEADING_RATIO * body
        || lines.iter().any(|line| line.marker().is_some())
    {
        return None;
    }
    let ratio = largest / body;
    let level = if ratio >= 2.0 {
        1
    } else if ratio >= 1.5 {
        2
    } else {
        3
    };
    let text: Vec<String> = lines.iter().map(TextLine::text).collect();
    Some(Block::Heading {
        level,
        text: text.join(" "),
    })
}

/// Parágrafos e itens de lista de um bloco do layout.
fn paragraphs_and_lists(lines: &[TextLine], body: f64) -> Vec<Block> {
    let margin = lines.iter().map(|line| line.left).fold(f64::MAX, f64::min);
    let mut blocks = Vec::new();
    // Item ou parágrafo em montagem, com a margem do marcador (itens).
    let mut current: Option<(Block, f64)> = None;
    for line in lines {
        if let Some(marker) = line.marker() {
            blocks.extend(current.take().map(|(block, _)| block));
            let level = ((line.left - margin) / body).floor().clamp(0.0, 5.0) as u8;
            current = Some((
                Block::ListItem {
                    marker,
                    level,
                    text: line.words[1..].join(" "),
                },
                line.left,
            ));
            continue;
        }
        match &mut current {
            // Continuação de item: linha recuada em relação ao marcador.
            Some((Block::ListItem { text, .. }, left)) if line.left > *left + 0.5 => {
                append_line(text, &line.text())
            }
            Some((Block::Paragraph { text }, _)) => append_line(text, &line.text()),
            _ => {
                blocks.extend(current.take().map(|(block, _)| block));
                current = Some((Block::Paragraph { text: line.text() }, line.left));
            }
        }
    }
    blocks.extend(current.map(|(block, _)| block));
    blocks
}

/// Acrescenta a linha seguinte ao texto. Palavra hifenizada no fim da linha
/// continua na próxima sem espaço: antes de minúscula o hífen é só de
/// quebra (sai); antes de maiúscula é de palavra composta (fica). Nos demais
/// casos, as linhas se separam por espaço.
fn append_line(text: &mut String, next: &str) {
    let hyphenated = text
        .strip_suffix('-')
        .and_then(|stem| stem.chars().last())
        .is_some_and(char::is_alphabetic);
    if hyphenated {
        if next.chars().next().is_some_and(char::is_lowercase) {
            text.pop();
        }
    } else if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::exportacao::modelo::PageImage;
    use crate::dominio::geometria::LayoutRect;
    use crate::dominio::layout::{Block as LayoutBlock, Line, Page, Word};

    /// Linha com palavras de largura proporcional ao texto, a partir de `left`.
    fn line(text: &str, left: f64, top: f64, size: f64) -> Line {
        let mut x = left;
        let words = text
            .split_whitespace()
            .map(|word| {
                let right = x + 0.5 * size * word.chars().count() as f64;
                let rect = LayoutRect::new(x, top, right, top + size);
                x = right + 0.3 * size;
                Word {
                    text: word.into(),
                    rect,
                }
            })
            .collect();
        Line { words }
    }

    fn page(blocks: Vec<Vec<Line>>) -> Page {
        Page {
            width: 595.0,
            height: 842.0,
            blocks: blocks
                .into_iter()
                .map(|lines| LayoutBlock { lines })
                .collect(),
        }
    }

    fn paragraph(text: &str) -> Block {
        Block::Paragraph { text: text.into() }
    }

    #[test]
    fn lines_of_a_block_form_one_paragraph() {
        let layout = page(vec![
            vec![
                line("O editor abre o", 72.0, 100.0, 12.0),
                line("documento em segundos.", 72.0, 114.0, 12.0),
            ],
            vec![line("Outro parágrafo.", 72.0, 140.0, 12.0)],
        ]);
        assert_eq!(
            structure(&layout),
            [
                paragraph("O editor abre o documento em segundos."),
                paragraph("Outro parágrafo.")
            ]
        );
    }

    #[test]
    fn hyphenated_line_breaks_are_joined() {
        let layout = page(vec![vec![
            line("A exporta-", 72.0, 100.0, 12.0),
            line("ção funciona; Rio-", 72.0, 114.0, 12.0),
            line("Grande fica.", 72.0, 128.0, 12.0),
        ]]);
        assert_eq!(
            structure(&layout),
            [paragraph("A exportação funciona; Rio-Grande fica.")],
            "minúscula depois do hífen junta; maiúscula mantém o hífen"
        );
    }

    #[test]
    fn taller_short_blocks_become_headings_by_size() {
        let layout = page(vec![
            vec![line("Relatório anual", 72.0, 60.0, 28.0)],
            vec![line("Resultados", 72.0, 100.0, 20.0)],
            vec![line("Detalhes", 72.0, 130.0, 15.0)],
            vec![
                line("Texto do corpo com várias", 72.0, 150.0, 12.0),
                line("palavras em duas linhas.", 72.0, 164.0, 12.0),
            ],
            vec![line("Mais corpo.", 72.0, 190.0, 12.0)],
        ]);
        let blocks = structure(&layout);
        assert_eq!(
            blocks[..3],
            [
                Block::Heading {
                    level: 1,
                    text: "Relatório anual".into()
                },
                Block::Heading {
                    level: 2,
                    text: "Resultados".into()
                },
                Block::Heading {
                    level: 3,
                    text: "Detalhes".into()
                },
            ]
        );
        assert_eq!(
            blocks[3],
            paragraph("Texto do corpo com várias palavras em duas linhas.")
        );
    }

    #[test]
    fn bullets_and_numbers_become_list_items_with_levels() {
        let layout = page(vec![
            vec![
                line("• Primeiro item", 72.0, 100.0, 12.0),
                line("que continua", 84.0, 114.0, 12.0),
                line("• Segundo", 72.0, 128.0, 12.0),
                line("– Sub-item", 92.0, 142.0, 12.0),
            ],
            vec![
                line("1. Passo um", 72.0, 170.0, 12.0),
                line("2) Passo dois", 72.0, 184.0, 12.0),
            ],
        ]);
        let item = |marker: &str, level, text: &str| Block::ListItem {
            marker: marker.into(),
            level,
            text: text.into(),
        };
        assert_eq!(
            structure(&layout),
            [
                item("", 0, "Primeiro item que continua"),
                item("", 0, "Segundo"),
                item("", 1, "Sub-item"),
                item("1.", 0, "Passo um"),
                item("2)", 0, "Passo dois"),
            ]
        );
    }

    #[test]
    fn loose_number_joins_the_following_line() {
        let layout = page(vec![vec![
            line("3.", 72.0, 100.0, 12.0),
            line("Item com número solto", 90.0, 100.5, 12.0),
        ]]);
        assert_eq!(
            structure(&layout),
            [Block::ListItem {
                marker: "3.".into(),
                level: 0,
                text: "Item com número solto".into()
            }]
        );
    }

    #[test]
    fn numbers_inside_sentences_are_not_lists() {
        let layout = page(vec![vec![line(
            "2024. Ano de crescimento",
            72.0,
            100.0,
            12.0,
        )]]);
        assert_eq!(structure(&layout), [paragraph("2024. Ano de crescimento")]);
        assert!(structure(&page(vec![])).is_empty());
    }

    #[test]
    fn heading_split_in_blocks_on_the_same_line_is_joined() {
        // O poppler separa "1" e "Introdução" em blocos na mesma linha de base.
        let layout = page(vec![
            vec![line("1", 72.0, 100.0, 20.0)],
            vec![line("Introdução", 100.0, 100.0, 20.0)],
            vec![line("Corpo do texto em tamanho normal.", 72.0, 130.0, 12.0)],
        ]);
        assert_eq!(
            structure(&layout)[0],
            Block::Heading {
                level: 2,
                text: "1 Introdução".into()
            }
        );
    }

    #[test]
    fn page_numbers_in_the_margins_are_dropped() {
        let layout = page(vec![
            vec![line("7", 290.0, 30.0, 10.0)],
            vec![line("Texto da página.", 72.0, 100.0, 12.0)],
            vec![line("12", 290.0, 800.0, 10.0)],
            vec![line("Nota 3", 72.0, 790.0, 10.0)],
        ]);
        assert_eq!(
            structure(&layout),
            [paragraph("Texto da página."), paragraph("Nota 3")],
            "só o número sozinho na margem sai"
        );
    }

    #[test]
    fn tables_appear_in_reading_order_without_repeating_their_words() {
        let column = |left: f64, cells: &[(&str, f64)]| LayoutBlock {
            lines: cells
                .iter()
                .map(|(text, top)| line(text, left, *top, 12.0))
                .collect(),
        };
        let layout = Page {
            width: 595.0,
            height: 842.0,
            blocks: vec![
                LayoutBlock {
                    lines: vec![line("Antes da tabela.", 72.0, 80.0, 12.0)],
                },
                column(72.0, &[("Item", 110.0), ("Lápis", 126.0)]),
                column(250.0, &[("Preço", 110.0), ("1,00", 126.0)]),
                LayoutBlock {
                    lines: vec![line("Depois da tabela.", 72.0, 160.0, 12.0)],
                },
            ],
        };
        assert_eq!(
            structure(&layout),
            [
                paragraph("Antes da tabela."),
                Block::Table {
                    rows: vec![
                        vec!["Item".into(), "Preço".into()],
                        vec!["Lápis".into(), "1,00".into()],
                    ]
                },
                paragraph("Depois da tabela."),
            ]
        );
    }

    #[test]
    fn images_are_placed_by_their_vertical_position() {
        let layout = page(vec![
            vec![line("Antes.", 72.0, 100.0, 12.0)],
            vec![line("Depois.", 72.0, 300.0, 12.0)],
        ]);
        let image = |top: f64, png: u8| PageImage {
            rect: LayoutRect::new(72.0, top, 272.0, top + 100.0),
            png: vec![png],
        };
        let blocks = structure_with(&layout, vec![image(400.0, 2), image(150.0, 1)], &[]);
        let picture = |png: u8| Block::Image {
            png: vec![png],
            size: (200.0, 100.0),
        };
        assert_eq!(
            blocks,
            [
                paragraph("Antes."),
                picture(1),
                paragraph("Depois."),
                picture(2)
            ]
        );
        assert_eq!(
            structure_with(&page(vec![]), vec![image(10.0, 3)], &[]),
            [picture(3)]
        );
    }
}
