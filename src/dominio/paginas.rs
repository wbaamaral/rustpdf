// SPDX-License-Identifier: AGPL-3.0-or-later
//! Números e intervalos de páginas (1-based), validados contra o total.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageError {
    /// Texto que não é número de página.
    Invalid(String),
    /// Página fora de `1..=total`.
    OutOfRange { page: usize, total: usize },
    /// Intervalo `a-b` com `a > b`.
    Reversed { first: usize, last: usize },
}

impl fmt::Display for PageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(text) => write!(f, "Número de página inválido: {text}"),
            Self::OutOfRange { total, .. } => write!(f, "Página fora de 1..={total}"),
            Self::Reversed { first, last } => {
                write!(f, "Intervalo de páginas invertido: {first}-{last}")
            }
        }
    }
}

impl std::error::Error for PageError {}

impl From<PageError> for String {
    fn from(error: PageError) -> Self {
        error.to_string()
    }
}

/// Página de um documento com `total` páginas, contada a partir de 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageNumber(usize);

impl PageNumber {
    pub fn new(page: usize, total: usize) -> Result<Self, PageError> {
        if (1..=total).contains(&page) {
            Ok(Self(page))
        } else {
            Err(PageError::OutOfRange { page, total })
        }
    }

    pub fn parse(text: &str, total: usize) -> Result<Self, PageError> {
        let text = text.trim();
        let page = text
            .parse()
            .map_err(|_| PageError::Invalid(text.to_string()))?;
        Self::new(page, total)
    }

    /// Número (1-based).
    pub fn get(self) -> usize {
        self.0
    }

    /// Índice (0-based).
    pub fn index(self) -> usize {
        self.0 - 1
    }
}

/// Conjunto ordenado de páginas, escrito como `1-3,7`, `9-` ou `-2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRange(Vec<PageNumber>);

impl PageRange {
    pub fn all(total: usize) -> Self {
        Self((1..=total).map(PageNumber).collect())
    }

    /// Texto vazio = todas as páginas.
    pub fn parse(text: &str, total: usize) -> Result<Self, PageError> {
        if text.trim().is_empty() {
            return Ok(Self::all(total));
        }
        let mut pages = Vec::new();
        for part in text.split(',').map(str::trim) {
            let (first, last) = match part.split_once('-') {
                Some((first, last)) => (bound(first, 1, total)?, bound(last, total, total)?),
                None => {
                    let page = PageNumber::parse(part, total)?.get();
                    (page, page)
                }
            };
            if first > last {
                return Err(PageError::Reversed { first, last });
            }
            for page in first..=last {
                pages.push(PageNumber::new(page, total)?);
            }
        }
        pages.sort();
        pages.dedup();
        Ok(Self(pages))
    }

    pub fn pages(&self) -> &[PageNumber] {
        &self.0
    }

    pub fn contains(&self, page: PageNumber) -> bool {
        self.0.binary_search(&page).is_ok()
    }
}

/// Extremo de um intervalo: vazio vale `default`; senão, página válida.
fn bound(text: &str, default: usize, total: usize) -> Result<usize, PageError> {
    let text = text.trim();
    if text.is_empty() {
        return PageNumber::new(default, total).map(PageNumber::get);
    }
    PageNumber::parse(text, total).map(PageNumber::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_number_is_one_based_and_bounded() {
        assert_eq!(PageNumber::parse("3", 5).unwrap().get(), 3);
        assert_eq!(PageNumber::parse(" 1 ", 5).unwrap().index(), 0);
        assert_eq!(
            PageNumber::parse("0", 5),
            Err(PageError::OutOfRange { page: 0, total: 5 })
        );
        assert_eq!(
            PageNumber::parse("6", 5),
            Err(PageError::OutOfRange { page: 6, total: 5 })
        );
        assert_eq!(
            PageNumber::parse("x", 5),
            Err(PageError::Invalid("x".into()))
        );
        assert_eq!(
            PageNumber::new(2, 1),
            Err(PageError::OutOfRange { page: 2, total: 1 })
        );
    }

    #[test]
    fn range_accepts_lists_intervals_and_all() {
        let pages = |text: &str| -> Vec<usize> {
            PageRange::parse(text, 10)
                .unwrap()
                .pages()
                .iter()
                .map(|page| page.get())
                .collect()
        };
        assert_eq!(pages("1-3,7"), [1, 2, 3, 7]);
        assert_eq!(pages("7, 2-3 ,2"), [2, 3, 7], "ordenado e sem repetição");
        assert_eq!(pages("9-"), [9, 10], "até o fim");
        assert_eq!(pages("-2"), [1, 2], "do início");
        assert_eq!(pages(""), (1..=10).collect::<Vec<_>>(), "vazio = todas");
        assert_eq!(PageRange::all(3).pages().len(), 3);
        assert!(PageRange::parse("1-3", 10)
            .unwrap()
            .contains(PageNumber::new(2, 10).unwrap()));
    }

    #[test]
    fn range_rejects_bad_input() {
        assert_eq!(
            PageRange::parse("3-1", 10),
            Err(PageError::Reversed { first: 3, last: 1 })
        );
        assert_eq!(
            PageRange::parse("2-12", 10),
            Err(PageError::OutOfRange {
                page: 12,
                total: 10
            })
        );
        assert_eq!(
            PageRange::parse("1,,2", 10),
            Err(PageError::Invalid("".into()))
        );
        assert_eq!(
            PageRange::parse("a-b", 10),
            Err(PageError::Invalid("a".into()))
        );
        assert_eq!(
            PageRange::parse("1", 0),
            Err(PageError::OutOfRange { page: 1, total: 0 })
        );
    }

    #[test]
    fn errors_read_in_portuguese() {
        let message: String = PageError::OutOfRange { page: 2, total: 1 }.into();
        assert_eq!(message, "Página fora de 1..=1");
        assert_eq!(
            PageError::Invalid("x".into()).to_string(),
            "Número de página inválido: x"
        );
        assert_eq!(
            PageError::OutOfRange { page: 9, total: 5 }.to_string(),
            "Página fora de 1..=5"
        );
        assert_eq!(
            PageError::Reversed { first: 3, last: 1 }.to_string(),
            "Intervalo de páginas invertido: 3-1"
        );
    }
}
