// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tabelas sem bordas: palavras alinhadas em linhas e colunas.
//!
//! O poppler costuma separar cada coluna num bloco; por isso a detecção olha
//! a página inteira: agrupa palavras por linha (centro vertical) e quebra
//! cada linha em células nos espaços largos (≥ 0,35 altura); linhas
//! consecutivas com ≥ 2 células formam uma região candidata; as colunas são
//! a união das faixas das células (projeção). No texto corrido as células
//! são longas e não se alinham. Aceita ≥ 2 linhas e ≥ 2 colunas de células
//! curtas; listas com marcador não são tabela.

use super::estrutura::is_list_marker;
use crate::dominio::geometria::{LayoutRect, Segment};
use crate::dominio::layout::Page;
use std::collections::HashSet;

/// Palavra no layout: (bloco, linha, palavra).
pub type WordId = (usize, usize, usize);

/// Traços mais próximos que isto (pontos) são a mesma linha da grade.
const RULE_MERGE: f64 = 1.5;
/// Comprimento mínimo de um traço da grade, em pontos.
const RULE_MIN_LENGTH: f64 = 3.0;

/// Espaço entre palavras, em alturas de linha, que separa células. Medidas
/// reais: espaço comum ≈ 0,23; menor vão entre células numa tabela compacta
/// do LibreOffice ≈ 0,39 (números alinhados à direita). Linhas de prosa que
/// se partam por acaso não viram tabela: as células se sobrepõem entre as
/// linhas e colapsam numa coluna só.
const WIDE_GAP: f64 = 0.35;
/// Distância entre os topos de linhas consecutivas de uma mesma tabela, em alturas.
const ROW_SPACING: f64 = 2.5;
/// Fração das células de uma coluna que precisa estar alinhada.
const MIN_ALIGNED: f64 = 0.6;
/// Diferença máxima entre bordas alinhadas de uma coluna, em alturas.
const ALIGN_TOLERANCE: f64 = 0.5;
/// Espaço claramente largo, em alturas, exigido nas tabelas de duas linhas.
const CLEAR_GAP: f64 = 1.0;
/// Fração mínima das colunas que cada linha da tabela preenche.
const MIN_ROW_FILL: f64 = 0.4;
/// Fração de células com pontuação de código acima da qual é código-fonte.
const MAX_CODE_CELLS: f64 = 0.3;
/// Pontuação típica de código-fonte (rara em tabelas de dados).
const CODE_PUNCTUATION: [char; 14] = [
    ';', '{', '}', '=', '(', ')', '*', '[', ']', '<', '>', '#', '&', '|',
];
/// Mediana de palavras por célula de uma coluna acima da qual é texto
/// corrido, não tabela.
const MAX_WORDS_PER_CELL: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct FoundTable {
    /// Linhas × colunas; a primeira linha costuma ser o cabeçalho.
    pub rows: Vec<Vec<String>>,
    /// Topo da primeira linha (para a ordem de leitura).
    pub top: f64,
    /// Palavras usadas pela tabela (saem dos parágrafos).
    pub words: Vec<WordId>,
}

#[derive(Debug, Clone)]
struct PlacedWord {
    id: WordId,
    text: String,
    rect: LayoutRect,
}

impl PlacedWord {
    /// Ponto-guia de sumário (". . . ."): liga título e página, não é conteúdo.
    fn is_leader(&self) -> bool {
        !self.text.is_empty() && self.text.chars().all(|c| matches!(c, '.' | '·' | '…'))
    }
}

/// Linha visual: palavras da mesma altura, da esquerda para a direita,
/// já quebradas em células nos espaços largos.
#[derive(Debug, Clone)]
struct Row {
    top: f64,
    height: f64,
    cells: Vec<Cell>,
    /// Pontos-guia da linha (consumidos, sem virar conteúdo).
    leaders: Vec<WordId>,
}

#[derive(Debug, Clone)]
struct Cell {
    words: Vec<PlacedWord>,
    left: f64,
    right: f64,
}

/// Tabelas sem bordas da página, de cima para baixo.
pub fn find_tables(page: &Page) -> Vec<FoundTable> {
    find_tables_with_rulings(page, &[])
}

/// Tabelas da página, de cima para baixo: primeiro as desenhadas com grade
/// de traços (`segments`, pontos com origem no topo), que aceitam células
/// longas; depois, nas palavras restantes, as alinhadas sem bordas.
pub fn find_tables_with_rulings(page: &Page, segments: &[Segment]) -> Vec<FoundTable> {
    let words = placed_words(page);
    let mut tables = grid_tables(&words, segments);
    let used: HashSet<WordId> = tables
        .iter()
        .flat_map(|table| table.words.iter().copied())
        .collect();
    let remaining = words
        .into_iter()
        .filter(|word| !used.contains(&word.id))
        .collect();
    tables.extend(projection_tables(remaining));
    tables.sort_by(|a, b| a.top.total_cmp(&b.top));
    tables
}

fn placed_words(page: &Page) -> Vec<PlacedWord> {
    let mut words = Vec::new();
    for (b, block) in page.blocks.iter().enumerate() {
        for (l, line) in block.lines.iter().enumerate() {
            for (w, word) in line.words.iter().enumerate() {
                words.push(PlacedWord {
                    id: (b, l, w),
                    text: word.text.clone(),
                    rect: word.rect,
                });
            }
        }
    }
    words
}

/// Traço horizontal `(y, x0, x1)` ou vertical `(x, y0, y1)`.
type Rule = (f64, f64, f64);

/// Tabelas com grade: componentes de traços horizontais e verticais que se
/// cruzam; as posições (agrupadas) definem as faixas de linhas e colunas.
fn grid_tables(words: &[PlacedWord], segments: &[Segment]) -> Vec<FoundTable> {
    let mut horizontals: Vec<Rule> = Vec::new();
    let mut verticals: Vec<Rule> = Vec::new();
    for &((x0, y0), (x1, y1)) in segments {
        if (y0 - y1).abs() <= 0.5 && (x1 - x0).abs() >= RULE_MIN_LENGTH {
            horizontals.push(((y0 + y1) / 2.0, x0.min(x1), x0.max(x1)));
        } else if (x0 - x1).abs() <= 0.5 && (y1 - y0).abs() >= RULE_MIN_LENGTH {
            verticals.push(((x0 + x1) / 2.0, y0.min(y1), y0.max(y1)));
        }
    }
    // Componentes por união: um horizontal e um vertical se ligam quando se cruzam.
    let total = horizontals.len() + verticals.len();
    let mut parent: Vec<usize> = (0..total).collect();
    fn root(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    for (h, &(y, x0, x1)) in horizontals.iter().enumerate() {
        for (v, &(x, y0, y1)) in verticals.iter().enumerate() {
            let crosses = (x0 - RULE_MERGE..=x1 + RULE_MERGE).contains(&x)
                && (y0 - RULE_MERGE..=y1 + RULE_MERGE).contains(&y);
            if crosses {
                let (a, b) = (
                    root(&mut parent, h),
                    root(&mut parent, horizontals.len() + v),
                );
                parent[a] = b;
            }
        }
    }
    let mut components: Vec<(usize, Vec<f64>, Vec<f64>)> = Vec::new();
    for index in 0..total {
        let component = root(&mut parent, index);
        let entry = match components.iter().position(|(id, _, _)| *id == component) {
            Some(position) => &mut components[position],
            None => {
                components.push((component, Vec::new(), Vec::new()));
                components.last_mut().expect("acabou de entrar")
            }
        };
        if index < horizontals.len() {
            entry.1.push(horizontals[index].0);
        } else {
            entry.2.push(verticals[index - horizontals.len()].0);
        }
    }
    components
        .into_iter()
        .filter_map(|(_, ys, xs)| grid_table(words, &cluster(ys), &cluster(xs)))
        .collect()
}

/// Posições ordenadas, com as próximas (bordas duplas, retângulos finos)
/// fundidas na média.
fn cluster(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    let mut groups: Vec<Vec<f64>> = Vec::new();
    for value in values {
        match groups.last_mut() {
            Some(group) if value - group[group.len() - 1] <= RULE_MERGE => group.push(value),
            _ => groups.push(vec![value]),
        }
    }
    groups
        .iter()
        .map(|group| group.iter().sum::<f64>() / group.len() as f64)
        .collect()
}

/// Tabela da grade com linhas em `ys` e colunas em `xs` (≥ 2 × 2 com texto).
fn grid_table(words: &[PlacedWord], ys: &[f64], xs: &[f64]) -> Option<FoundTable> {
    if ys.len() < 3 || xs.len() < 3 {
        return None;
    }
    let band = |edges: &[f64], value: f64| {
        edges
            .windows(2)
            .position(|pair| (pair[0]..pair[1]).contains(&value))
    };
    let mut cells: Vec<Vec<Vec<&PlacedWord>>> = vec![vec![Vec::new(); xs.len() - 1]; ys.len() - 1];
    let mut used = Vec::new();
    for word in words {
        let center_x = (word.rect.left() + word.rect.right()) / 2.0;
        let center_y = (word.rect.top() + word.rect.bottom()) / 2.0;
        if let (Some(row), Some(column)) = (band(ys, center_y), band(xs, center_x)) {
            cells[row][column].push(word);
            used.push(word.id);
        }
    }
    let rows: Vec<Vec<String>> = cells
        .into_iter()
        .filter(|row| row.iter().any(|cell| !cell.is_empty()))
        .map(|row| {
            row.into_iter()
                .map(|mut cell| {
                    cell.sort_by(|a, b| {
                        a.rect
                            .top()
                            .total_cmp(&b.rect.top())
                            .then(a.rect.left().total_cmp(&b.rect.left()))
                    });
                    cell.iter()
                        .map(|word| word.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect()
        })
        .collect();
    (rows.len() >= 2).then(|| FoundTable {
        rows,
        top: ys[0],
        words: used,
    })
}

/// Tabelas sem bordas entre as palavras dadas.
fn projection_tables(words: Vec<PlacedWord>) -> Vec<FoundTable> {
    let rows = rows(words);
    let mut tables = Vec::new();
    let mut run: Vec<&Row> = Vec::new();
    for row in &rows {
        let continues = run.last().is_some_and(|previous| {
            row.top - previous.top <= ROW_SPACING * previous.height.max(row.height)
        });
        let candidate = row.cells.len() >= 2;
        if candidate && (run.is_empty() || continues) {
            run.push(row);
            continue;
        }
        tables.extend(table(&run));
        run.clear();
        if candidate {
            run.push(row);
        }
    }
    tables.extend(table(&run));
    tables
}

/// Linhas visuais: palavras agrupadas pelo centro vertical.
fn rows(mut words: Vec<PlacedWord>) -> Vec<Row> {
    let center = |word: &PlacedWord| (word.rect.top() + word.rect.bottom()) / 2.0;
    words.sort_by(|a, b| center(a).total_cmp(&center(b)));
    let mut groups: Vec<Vec<PlacedWord>> = Vec::new();
    for word in words {
        let joins = groups.last().is_some_and(|group| {
            let last = &group[group.len() - 1];
            (center(&word) - center(last)).abs() < 0.5 * word.rect.height().min(last.rect.height())
        });
        if joins {
            groups.last_mut().expect("grupo existe").push(word);
        } else {
            groups.push(vec![word]);
        }
    }
    groups.into_iter().map(row).collect()
}

/// Quebra a linha em células nos espaços de pelo menos [`WIDE_GAP`].
fn row(words: Vec<PlacedWord>) -> Row {
    let top = words.iter().map(|w| w.rect.top()).fold(f64::MAX, f64::min);
    let height = words.iter().map(|w| w.rect.height()).fold(0.0, f64::max);
    let (leaders, mut content): (Vec<PlacedWord>, Vec<PlacedWord>) =
        words.into_iter().partition(PlacedWord::is_leader);
    content.sort_by(|a, b| a.rect.left().total_cmp(&b.rect.left()));
    let mut cells: Vec<Cell> = Vec::new();
    for word in content {
        match cells.last_mut() {
            Some(cell) if word.rect.left() - cell.right < WIDE_GAP * height => {
                cell.right = cell.right.max(word.rect.right());
                cell.words.push(word);
            }
            _ => cells.push(Cell {
                left: word.rect.left(),
                right: word.rect.right(),
                words: vec![word],
            }),
        }
    }
    Row {
        top,
        height,
        cells,
        leaders: leaders.into_iter().map(|word| word.id).collect(),
    }
}

/// Tabela das linhas `run`, se as células formarem ≥ 2 colunas de texto
/// curto e a primeira coluna não for de marcadores de lista.
fn table(run: &[&Row]) -> Option<FoundTable> {
    if run.len() < 2 {
        return None;
    }
    let list = run
        .iter()
        .all(|row| row.cells[0].words.len() == 1 && is_list_marker(&row.cells[0].words[0].text));
    if list {
        return None;
    }
    let columns = columns(run);
    if columns.len() < 2 {
        return None;
    }
    let mut rows = Vec::new();
    let mut words = Vec::new();
    // Palavras por célula preenchida e extensão das células, por coluna.
    let mut word_counts: Vec<Vec<usize>> = vec![Vec::new(); columns.len()];
    let mut extents: Vec<Vec<(f64, f64)>> = vec![Vec::new(); columns.len()];
    for row in run {
        let mut texts: Vec<Vec<&str>> = vec![Vec::new(); columns.len()];
        let mut spans: Vec<Option<(f64, f64)>> = vec![None; columns.len()];
        for cell in &row.cells {
            let column = columns
                .iter()
                .position(|&(left, right)| cell.left >= left && cell.right <= right)?;
            texts[column].extend(cell.words.iter().map(|word| word.text.as_str()));
            words.extend(cell.words.iter().map(|word| word.id));
            let span = spans[column].get_or_insert((cell.left, cell.right));
            *span = (span.0.min(cell.left), span.1.max(cell.right));
        }
        for (column, span) in spans.into_iter().enumerate() {
            extents[column].extend(span);
        }
        words.extend(row.leaders.iter().copied());
        for (column, text) in texts.iter().enumerate() {
            if !text.is_empty() {
                word_counts[column].push(text.len());
            }
        }
        rows.push(texts.iter().map(|cell| cell.join(" ")).collect());
    }
    // Toda linha preenche boa parte das colunas (título sobre linha
    // justificada não é cabeçalho de tabela).
    let sparse = rows.iter().any(|row: &Vec<String>| {
        let filled = row.iter().filter(|cell| !cell.is_empty()).count();
        (filled as f64) < MIN_ROW_FILL * columns.len() as f64
    });
    // Código-fonte alinhado: muitas células com pontuação de código.
    let cells: Vec<&String> = rows
        .iter()
        .flatten()
        .filter(|cell| !cell.is_empty())
        .collect();
    let code = cells
        .iter()
        .filter(|cell| cell.contains(CODE_PUNCTUATION))
        .count();
    if sparse || code as f64 > MAX_CODE_CELLS * cells.len() as f64 {
        return None;
    }
    let height = run.iter().map(|row| row.height).fold(0.0, f64::max);
    // Cada coluna aparece em boa parte das linhas e tem as células alinhadas
    // por alguma borda (em prosa, as "colunas" caem em pontos aleatórios).
    let consistent = extents
        .iter()
        .all(|cells| 2 * cells.len() >= run.len() && aligned(cells, ALIGN_TOLERANCE * height));
    // Com só duas linhas, coincidências são fáceis: cada uma precisa de um
    // espaço claramente largo entre células.
    let clear = run.len() > 2
        || run.iter().all(|row| {
            row.cells
                .windows(2)
                .any(|pair| pair[1].left - pair[0].right >= CLEAR_GAP * row.height)
        });
    if !consistent || !clear {
        return None;
    }
    // Uma coluna de células longas é texto corrido (mesmo que outra seja curta).
    let prose = word_counts.iter_mut().any(|counts| {
        counts.sort_unstable();
        counts
            .get(counts.len() / 2)
            .is_some_and(|&median| median > MAX_WORDS_PER_CELL)
    });
    if prose {
        return None;
    }
    Some(FoundTable {
        rows,
        top: run[0].top,
        words,
    })
}

/// Borda de uma célula `(esquerda, direita)` usada no alinhamento.
type Edge = fn(&(f64, f64)) -> f64;

/// A maioria ([`MIN_ALIGNED`]) das células de uma coluna está alinhada à
/// esquerda, à direita ou ao centro (sumários com subseções recuadas e
/// cabeçalhos centralizados continuam valendo).
fn aligned(cells: &[(f64, f64)], tolerance: f64) -> bool {
    let needed = (MIN_ALIGNED * cells.len() as f64).ceil() as usize;
    let edges: [Edge; 3] = [|cell| cell.0, |cell| cell.1, |cell| (cell.0 + cell.1) / 2.0];
    edges.iter().any(|edge| {
        let values: Vec<f64> = cells.iter().map(edge).collect();
        values.iter().any(|start| {
            let group = values
                .iter()
                .filter(|value| (*start..=start + tolerance).contains(value))
                .count();
            group >= needed
        })
    })
}

/// Colunas: união das faixas horizontais das células que se sobrepõem.
fn columns(run: &[&Row]) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> = run
        .iter()
        .flat_map(|row| row.cells.iter().map(|cell| (cell.left, cell.right)))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (left, right) in spans {
        match merged.last_mut() {
            Some(last) if left <= last.1 => last.1 = last.1.max(right),
            _ => merged.push((left, right)),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::geometria::LayoutRect;
    use crate::dominio::layout::{Block as LayoutBlock, Line, Page, Word};

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

    /// Bloco do layout com uma linha por item `(texto, topo)`, numa coluna `left`.
    fn column(left: f64, cells: &[(&str, f64)]) -> LayoutBlock {
        LayoutBlock {
            lines: cells
                .iter()
                .map(|(text, top)| line(text, left, *top, 12.0))
                .collect(),
        }
    }

    fn page(blocks: Vec<LayoutBlock>) -> Page {
        Page {
            width: 595.0,
            height: 842.0,
            blocks,
        }
    }

    fn texts(rows: &[Vec<&str>]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|row| row.iter().map(|cell| cell.to_string()).collect())
            .collect()
    }

    #[test]
    fn columns_in_separate_blocks_form_a_table() {
        let layout = page(vec![
            column(
                72.0,
                &[("Produto", 100.0), ("Caneta azul", 116.0), ("Lápis", 132.0)],
            ),
            column(250.0, &[("Qtd", 100.0), ("10", 116.0), ("3", 132.0)]),
            column(
                350.0,
                &[("Preço", 100.0), ("R$ 2,50", 116.0), ("R$ 1,00", 132.0)],
            ),
        ]);
        let tables = find_tables(&layout);
        assert_eq!(tables.len(), 1);
        assert_eq!(
            tables[0].rows,
            texts(&[
                vec!["Produto", "Qtd", "Preço"],
                vec!["Caneta azul", "10", "R$ 2,50"],
                vec!["Lápis", "3", "R$ 1,00"],
            ])
        );
        assert_eq!(tables[0].top, 100.0);
        // Produto, Caneta, azul, Lápis, Qtd, 10, 3, Preço, R$, 2,50, R$, 1,00.
        assert_eq!(tables[0].words.len(), 12, "palavras usadas pela tabela");
    }

    #[test]
    fn missing_cells_stay_empty() {
        let layout = page(vec![
            column(72.0, &[("Nome", 100.0), ("Ana", 116.0), ("Bia", 132.0)]),
            column(250.0, &[("Cidade", 100.0), ("Recife", 116.0)]),
            column(400.0, &[("Idade", 100.0), ("30", 116.0), ("41", 132.0)]),
        ]);
        let tables = find_tables(&layout);
        assert_eq!(
            tables[0].rows,
            texts(&[
                vec!["Nome", "Cidade", "Idade"],
                vec!["Ana", "Recife", "30"],
                vec!["Bia", "", "41"],
            ])
        );
    }

    #[test]
    fn prose_is_not_a_table() {
        // Parágrafo comum: uma célula por linha.
        let paragraph = page(vec![LayoutBlock {
            lines: vec![
                line("O texto corre normalmente pela linha", 72.0, 100.0, 12.0),
                line("e continua na seguinte sem colunas.", 72.0, 114.0, 12.0),
            ],
        }]);
        assert!(find_tables(&paragraph).is_empty());
        // Duas colunas de texto corrido: células longas.
        let columns = page(vec![
            column(
                72.0,
                &[
                    ("Esta coluna tem frases longas de texto", 100.0),
                    ("que seguem por várias linhas de prosa", 114.0),
                ],
            ),
            column(
                320.0,
                &[
                    ("E esta outra coluna também segue com", 100.0),
                    ("frases longas que não são uma tabela", 114.0),
                ],
            ),
        ]);
        assert!(find_tables(&columns).is_empty());
    }

    #[test]
    fn a_single_aligned_row_is_not_a_table() {
        let layout = page(vec![
            column(72.0, &[("Página", 100.0)]),
            column(400.0, &[("12", 100.0)]),
        ]);
        assert!(find_tables(&layout).is_empty());
    }

    #[test]
    fn rows_far_apart_split_into_separate_tables() {
        let layout = page(vec![
            column(
                72.0,
                &[("A", 100.0), ("B", 116.0), ("C", 400.0), ("D", 416.0)],
            ),
            column(
                250.0,
                &[("1", 100.0), ("2", 116.0), ("3", 400.0), ("4", 416.0)],
            ),
        ]);
        let tables = find_tables(&layout);
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[1].rows, texts(&[vec!["C", "3"], vec!["D", "4"]]));
    }

    /// Palavra com caixa exata `(texto, esquerda, topo, direita)`, altura 9,8.
    fn word(text: &str, left: f64, top: f64, right: f64) -> Word {
        Word {
            text: text.into(),
            rect: LayoutRect::new(left, top, right, top + 9.8),
        }
    }

    #[test]
    fn table_of_contents_with_close_numbers_is_detected() {
        // Posições reais de um sumário: "10" fica a só 5,5 pt do título.
        let entries = [
            ("1", 68.3, "Introduction", 138.4, "3", 158.1),
            ("9", 68.3, "Subgraphs", 125.0, "10", 182.5),
            ("10", 73.7, "Utility", 108.0, "11", 206.9),
            ("11", 73.7, "Error", 106.0, "12", 231.4),
        ];
        let blocks = entries
            .iter()
            .map(
                |(number, number_end, title, title_end, page, top)| LayoutBlock {
                    lines: vec![Line {
                        words: vec![
                            word(number, 62.8, *top, *number_end),
                            word(title, 79.2, *top, *title_end),
                            word(page, 536.5, *top, 547.5),
                        ],
                    }],
                },
            )
            .collect();
        let tables = find_tables(&page(blocks));
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows[2], ["10", "Utility", "11"]);
    }

    #[test]
    fn leader_dots_are_ignored_and_dropped() {
        // "1 Introdução . . . . 3": os pontos-guia não viram colunas nem texto.
        let rows = [
            (158.1, &[200.0, 210.0, 220.0][..]),
            (182.5, &[300.0, 310.0][..]),
        ];
        let blocks = rows
            .iter()
            .map(|(top, dots)| {
                let mut words = vec![
                    word("1", 62.8, *top, 68.3),
                    word("Título", 79.2, *top, 110.0),
                ];
                words.extend(dots.iter().map(|x| word(".", *x, *top, x + 2.0)));
                words.push(word("3", 536.5, *top, 547.5));
                LayoutBlock {
                    lines: vec![Line { words }],
                }
            })
            .collect();
        let tables = find_tables(&page(blocks));
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows[0], ["1", "Título", "3"]);
        // Linha 1: 1, Título, 3 pontos, 3; linha 2: 1, Título, 2 pontos, 3.
        assert_eq!(tables[0].words.len(), 11, "os pontos também são consumidos");
    }

    #[test]
    fn compact_table_from_an_office_suite_is_detected() {
        // Posições reais de uma tabela gerada pelo LibreOffice (altura 13,3).
        let at = |text: &str, left: f64, top: f64, right: f64| Word {
            text: text.into(),
            rect: LayoutRect::new(left, top, right, top + 13.3),
        };
        let line = |words: Vec<Word>| LayoutBlock {
            lines: vec![Line { words }],
        };
        let layout = page(vec![
            line(vec![
                at("Resumo", 56.8, 82.6, 96.1),
                at("das", 99.1, 82.6, 115.1),
                at("vendas", 118.1, 82.6, 151.4),
            ]),
            line(vec![
                at("Região", 63.9, 112.7, 99.1),
                at("Janeiro", 110.5, 112.7, 149.1),
                at("Fevereiro", 156.1, 112.7, 205.3),
                at("Março", 215.8, 112.7, 249.7),
            ]),
            line(vec![
                at("Norte", 58.2, 143.6, 85.5),
                at("1.200,00", 107.5, 143.6, 149.6),
                at("1.350,50", 154.8, 143.6, 196.8),
                at("980,00", 209.4, 143.6, 242.4),
            ]),
            line(vec![
                at("Sul", 58.2, 174.5, 74.2),
                at("3.100,00", 107.5, 174.5, 149.6),
                at("3.300,00", 209.4, 174.5, 251.4),
            ]),
        ]);
        let tables = find_tables(&layout);
        assert_eq!(tables.len(), 1, "{tables:?}");
        assert_eq!(
            tables[0].rows,
            texts(&[
                vec!["Região", "Janeiro", "Fevereiro", "Março"],
                vec!["Norte", "1.200,00", "1.350,50", "980,00"],
                vec!["Sul", "3.100,00", "", "3.300,00"],
            ])
        );
    }

    #[test]
    fn bulleted_list_is_not_a_table() {
        let layout = page(vec![
            column(72.0, &[("•", 100.0), ("•", 116.0)]),
            column(
                90.0,
                &[("Primeiro item da lista.", 100.0), ("Segundo.", 116.0)],
            ),
        ]);
        assert!(find_tables(&layout).is_empty());
    }

    #[test]
    fn justified_prose_with_coinciding_gaps_is_not_a_table() {
        // Título curto e duas linhas justificadas cujos espaços largos caem
        // na mesma posição: as colunas de prosa têm células longas.
        let spaced = |parts: &[(&str, f64)], top: f64| LayoutBlock {
            lines: vec![Line {
                words: parts
                    .iter()
                    .flat_map(|(text, left)| line(text, *left, top, 10.0).words)
                    .collect(),
            }],
        };
        let layout = page(vec![
            spaced(&[("1", 72.0), ("Introdução", 90.0)], 80.0),
            spaced(
                &[
                    ("Cgraph é uma biblioteca de grafos e", 72.0),
                    ("define", 260.0),
                    ("tipos", 300.0),
                ],
                100.0,
            ),
            spaced(
                &[
                    ("nós arestas e subgrafos com atributos", 72.0),
                    ("que", 260.0),
                    ("podem", 300.0),
                ],
                112.0,
            ),
        ]);
        assert!(
            find_tables(&layout).is_empty(),
            "{:?}",
            find_tables(&layout)
        );
    }

    /// Linha com cada palavra numa posição exata (espaços largos à vontade).
    fn spread(words: &[(&str, f64)], top: f64) -> LayoutBlock {
        LayoutBlock {
            lines: vec![Line {
                words: words
                    .iter()
                    .map(|(text, left)| word(text, *left, top, left + 5.0 * text.len() as f64))
                    .collect(),
            }],
        }
    }

    #[test]
    fn short_heading_over_a_fully_justified_line_is_not_a_table() {
        let layout = page(vec![
            spread(&[("4", 62.0), ("Nós", 80.0)], 100.0),
            spread(
                &[
                    ("Em", 62.0),
                    ("Cgraph,", 90.0),
                    ("um", 150.0),
                    ("nó", 180.0),
                    ("é", 210.0),
                    ("dado", 230.0),
                    ("pelo", 270.0),
                    ("nome", 310.0),
                ],
                120.0,
            ),
        ]);
        assert!(
            find_tables(&layout).is_empty(),
            "{:?}",
            find_tables(&layout)
        );
    }

    #[test]
    fn aligned_source_code_is_not_a_table() {
        let layout = page(vec![
            spread(
                &[
                    ("for", 62.0),
                    ("(n", 90.0),
                    ("=", 120.0),
                    ("first(g);", 140.0),
                ],
                100.0,
            ),
            spread(
                &[
                    ("for", 62.0),
                    ("(e", 90.0),
                    ("=", 120.0),
                    ("out(g,n);", 140.0),
                ],
                112.0,
            ),
            spread(
                &[("x", 62.0), ("+=", 90.0), ("1;", 120.0), ("}", 140.0)],
                124.0,
            ),
        ]);
        assert!(
            find_tables(&layout).is_empty(),
            "{:?}",
            find_tables(&layout)
        );
    }

    #[test]
    fn two_justified_lines_are_not_a_table() {
        // Posições reais (altura 8,9): espaços justificados de ~0,42 altura.
        let at = |text: &str, left: f64, top: f64, right: f64| Word {
            text: text.into(),
            rect: LayoutRect::new(left, top, right, top + 8.9),
        };
        let row = |words: Vec<Word>| LayoutBlock {
            lines: vec![Line { words }],
        };
        let layout = page(vec![
            row(vec![
                at("dot", 153.1, 237.7, 165.8),
                at("draws", 169.5, 237.7, 193.2),
                at("directed", 196.9, 237.7, 229.0),
                at("graphs", 232.7, 237.7, 259.3),
                at("as", 263.0, 237.7, 271.3),
                at("hierarchies.", 275.0, 237.7, 321.2),
                at("It", 328.0, 237.7, 334.1),
            ]),
            row(vec![
                at("gram,", 153.1, 249.7, 176.0),
                at("web", 180.0, 249.7, 196.6),
                at("visualization", 200.3, 249.7, 251.3),
                at("service,", 255.0, 249.7, 285.0),
                at("or", 289.0, 249.7, 297.0),
                at("with", 300.9, 249.7, 318.0),
                at("a", 330.0, 249.7, 334.5),
            ]),
        ]);
        assert!(
            find_tables(&layout).is_empty(),
            "{:?}",
            find_tables(&layout)
        );
    }

    #[test]
    fn misaligned_cells_are_not_columns() {
        // Três linhas cujas "colunas" começam e terminam em pontos diversos.
        let layout = page(vec![
            spread(&[("Alfa", 62.0), ("beta", 140.0), ("gama", 260.0)], 100.0),
            spread(&[("Delta", 62.0), ("eps", 170.0), ("zeta", 300.0)], 112.0),
            spread(&[("Teta", 62.0), ("iota", 115.0), ("capa", 230.0)], 124.0),
        ]);
        assert!(
            find_tables(&layout).is_empty(),
            "{:?}",
            find_tables(&layout)
        );
    }

    #[test]
    fn table_of_contents_with_indented_subsections_is_kept() {
        // Seções em 79 pt e uma subseção recuada em 104 pt (maioria alinhada).
        let entry = |number: &str, title: &str, left: f64, page: &str, top: f64| LayoutBlock {
            lines: vec![Line {
                words: vec![
                    word(number, 62.8, top, 68.3),
                    word(title, left, top, left + 40.0),
                    word(page, 536.5, top, 547.5),
                ],
            }],
        };
        let layout = page(vec![
            entry("7", "Atributos", 79.2, "7", 158.1),
            entry("", "Registros", 104.3, "8", 182.5),
            entry("8", "Subgrafos", 79.2, "9", 206.9),
            entry("9", "Arestas", 79.2, "10", 231.4),
        ]);
        let tables = find_tables(&layout);
        assert_eq!(tables.len(), 1, "{tables:?}");
        assert_eq!(tables[0].rows[1], ["", "Registros", "8"]);
    }

    /// Grade com linhas em `ys` e colunas em `xs` (origem no topo).
    fn grid(xs: &[f64], ys: &[f64]) -> Vec<((f64, f64), (f64, f64))> {
        let (left, right) = (xs[0], xs[xs.len() - 1]);
        let (top, bottom) = (ys[0], ys[ys.len() - 1]);
        let mut segments: Vec<_> = ys.iter().map(|&y| ((left, y), (right, y))).collect();
        segments.extend(xs.iter().map(|&x| ((x, top), (x, bottom))));
        segments
    }

    /// Texto numa célula: uma linha começando em `left`, no `top`.
    fn cell(text: &str, left: f64, top: f64) -> LayoutBlock {
        LayoutBlock {
            lines: vec![line(text, left, top, 10.0)],
        }
    }

    #[test]
    fn ruled_grid_accepts_long_text_cells() {
        // Coluna de descrição com muitas palavras: sem a grade, seria prosa.
        let layout = page(vec![
            cell("Item", 75.0, 103.0),
            cell("Descrição", 155.0, 103.0),
            cell("Caneta", 75.0, 123.0),
            cell("Caneta azul de ponta fina para escrita", 155.0, 123.0),
            cell("Lápis", 75.0, 143.0),
            cell("Lápis preto número dois com borracha", 155.0, 143.0),
        ]);
        let segments = grid(&[72.0, 150.0, 400.0], &[100.0, 120.0, 140.0, 160.0]);
        assert!(find_tables(&layout).is_empty(), "sem traços: prosa");
        let tables = find_tables_with_rulings(&layout, &segments);
        assert_eq!(tables.len(), 1);
        assert_eq!(
            tables[0].rows[1],
            ["Caneta", "Caneta azul de ponta fina para escrita"]
        );
        assert_eq!(tables[0].top, 100.0);
        assert_eq!(tables[0].words.len(), 17);
    }

    #[test]
    fn double_borders_and_thin_rectangles_do_not_create_rows() {
        let layout = page(vec![
            cell("A", 75.0, 103.0),
            cell("B", 155.0, 103.0),
            cell("1", 75.0, 123.0),
            cell("2", 155.0, 123.0),
        ]);
        // Borda superior dupla (100 e 100,8) e colunas desenhadas duas vezes.
        let mut segments = grid(&[72.0, 150.0, 300.0], &[100.0, 100.8, 120.0, 140.0]);
        segments.extend(grid(&[72.3, 150.2, 300.0], &[100.0, 140.0]));
        let tables = find_tables_with_rulings(&layout, &segments);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows, texts(&[vec!["A", "B"], vec!["1", "2"]]));
    }

    #[test]
    fn lines_that_are_not_a_grid_change_nothing() {
        // Só um sublinhado e uma régua horizontal: sem grade.
        let layout = page(vec![
            column(
                72.0,
                &[("Produto", 100.0), ("Caneta", 116.0), ("Lápis", 132.0)],
            ),
            column(250.0, &[("Qtd", 100.0), ("10", 116.0), ("3", 132.0)]),
        ]);
        let segments = vec![
            ((72.0, 112.5), (110.0, 112.5)),
            ((72.0, 150.0), (300.0, 150.0)),
        ];
        let with = find_tables_with_rulings(&layout, &segments);
        assert_eq!(with, find_tables(&layout), "a projeção continua valendo");
        assert!(find_tables_with_rulings(
            &page(vec![]),
            &grid(&[0.0, 10.0, 20.0], &[0.0, 10.0, 20.0])
        )
        .is_empty());
    }

    #[test]
    fn grid_and_projection_tables_coexist_without_sharing_words() {
        let mut blocks = vec![
            cell("X", 75.0, 103.0),
            cell("Y", 155.0, 103.0),
            cell("1", 75.0, 123.0),
            cell("2", 155.0, 123.0),
        ];
        blocks.push(column(72.0, &[("Nome", 400.0), ("Ana", 416.0)]));
        blocks.push(column(250.0, &[("Idade", 400.0), ("30", 416.0)]));
        let segments = grid(&[72.0, 150.0, 300.0], &[100.0, 120.0, 140.0]);
        let tables = find_tables_with_rulings(&page(blocks), &segments);
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].rows[0], ["X", "Y"]);
        assert_eq!(tables[1].rows[0], ["Nome", "Idade"]);
    }
}
