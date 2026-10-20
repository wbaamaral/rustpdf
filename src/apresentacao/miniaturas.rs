// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ordem de geração das miniaturas do painel lateral.

/// Páginas por lote: um `pdftoppm` por lote equilibra resposta e custo.
const BATCH: usize = 8;

/// Lotes `(primeira, última)` para gerar as miniaturas de `pages`: a página
/// `current` sozinha primeiro (aparece logo), depois as demais em blocos de
/// até [`BATCH`] páginas consecutivas.
pub fn batches(current: usize, pages: &[usize]) -> Vec<(usize, usize)> {
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut batches: Vec<(usize, usize)> = Vec::new();
    if sorted.contains(&current) {
        batches.push((current, current));
    }
    for number in sorted.into_iter().filter(|number| *number != current) {
        match batches.last_mut() {
            Some((first, last))
                if *last + 1 == number && *last - *first < BATCH - 1 && *first != current =>
            {
                *last = number
            }
            _ => batches.push((number, number)),
        }
    }
    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_page_comes_first_alone() {
        assert_eq!(batches(3, &[1, 2, 3, 4, 5]), [(3, 3), (1, 2), (4, 5)]);
    }

    #[test]
    fn consecutive_pages_group_in_batches_of_eight() {
        let pages: Vec<usize> = (1..=20).collect();
        assert_eq!(batches(1, &pages), [(1, 1), (2, 9), (10, 17), (18, 20)]);
    }

    #[test]
    fn gaps_and_order_are_handled() {
        assert_eq!(batches(9, &[7, 2, 3, 9]), [(9, 9), (2, 3), (7, 7)]);
        assert_eq!(batches(5, &[1, 2]), [(1, 2)], "atual fora da lista");
        assert!(batches(1, &[]).is_empty());
    }
}
