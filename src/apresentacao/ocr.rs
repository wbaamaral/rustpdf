// SPDX-License-Identifier: AGPL-3.0-or-later
//! Textos e escolhas do OCR na interface e na CLI.

use crate::app::ocr::OcrSummary;
use crate::dominio::paginas::PageRange;

/// Idioma sugerido no diálogo e usado pela CLI sem idioma.
pub const DEFAULT_LANGUAGE: &str = "por+eng";

/// Páginas pedidas: vazio ou `todas` = documento inteiro; senão `1-3,7`.
pub fn pages(text: &str, total: usize) -> Result<PageRange, String> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("todas") {
        return Ok(PageRange::all(total));
    }
    Ok(PageRange::parse(text, total)?)
}

/// Andamento com `done` páginas já lidas de `total`.
pub fn progress_message(done: usize, total: usize) -> String {
    if done >= total {
        "OCR: aplicando o resultado…".into()
    } else {
        format!("OCR: página {} de {total}", done + 1)
    }
}

/// Fração concluída (0 a 1) para a barra de progresso.
pub fn fraction(done: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// Resumo do OCR pesquisável.
pub fn summary_message(summary: &OcrSummary) -> String {
    let mut message = if summary.recognized == 0 {
        "OCR pesquisável: nenhuma página reconhecida".to_string()
    } else {
        format!(
            "OCR pesquisável: {} em {}",
            plural(summary.words, "palavra", "palavras"),
            plural(summary.recognized, "página", "páginas")
        )
    };
    if summary.had_text > 0 {
        message.push_str(&format!(
            "; {} já {} texto",
            plural(summary.had_text, "página", "páginas"),
            if summary.had_text == 1 {
                "tinha"
            } else {
                "tinham"
            }
        ));
    }
    if summary.no_words > 0 {
        message.push_str(&format!(
            "; {} sem palavras",
            plural(summary.no_words, "página", "páginas")
        ));
    }
    message
}

/// Barra de status depois do OCR de texto.
pub fn text_done_message(pages: usize) -> String {
    format!(
        "OCR: {} {}",
        plural(pages, "página", "páginas"),
        if pages == 1 { "lida" } else { "lidas" }
    )
}

/// Texto reconhecido para a janela de leitura: com mais de uma página, cada
/// uma sob o título "— Página N —".
pub fn text_report(pages: &PageRange, texts: &[String]) -> String {
    if let [text] = texts {
        return text.clone();
    }
    pages
        .pages()
        .iter()
        .zip(texts)
        .map(|(page, text)| format!("— Página {} —\n{}", page.get(), text.trim_end()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_accept_all_or_a_range() {
        assert_eq!(pages("", 3).unwrap(), PageRange::all(3));
        assert_eq!(pages(" Todas ", 3).unwrap(), PageRange::all(3));
        assert_eq!(pages("2-3", 3).unwrap().pages().len(), 2);
        assert_eq!(pages("5", 3).unwrap_err(), "Página fora de 1..=3");
    }

    #[test]
    fn progress_counts_the_page_being_read() {
        assert_eq!(progress_message(0, 4), "OCR: página 1 de 4");
        assert_eq!(progress_message(3, 4), "OCR: página 4 de 4");
        assert_eq!(progress_message(4, 4), "OCR: aplicando o resultado…");
        assert_eq!(fraction(1, 4), 0.25);
        assert_eq!(fraction(0, 0), 1.0);
        assert_eq!(fraction(9, 4), 1.0);
    }

    #[test]
    fn summary_names_only_what_happened() {
        let summary = |recognized, had_text, no_words, words| OcrSummary {
            recognized,
            had_text,
            no_words,
            words,
        };
        assert_eq!(
            summary_message(&summary(1, 0, 0, 1)),
            "OCR pesquisável: 1 palavra em 1 página"
        );
        assert_eq!(
            summary_message(&summary(40, 2, 1, 18312)),
            "OCR pesquisável: 18312 palavras em 40 páginas; 2 páginas já tinham texto; 1 página sem palavras"
        );
        assert_eq!(
            summary_message(&summary(0, 1, 0, 0)),
            "OCR pesquisável: nenhuma página reconhecida; 1 página já tinha texto"
        );
    }

    #[test]
    fn text_done_counts_pages() {
        assert_eq!(text_done_message(1), "OCR: 1 página lida");
        assert_eq!(text_done_message(3), "OCR: 3 páginas lidas");
    }

    #[test]
    fn report_titles_each_page_when_there_are_several() {
        let range = PageRange::parse("2-3", 3).unwrap();
        let texts = ["um\n".to_string(), "dois\n\n".to_string()];
        assert_eq!(
            text_report(&range, &texts),
            "— Página 2 —\num\n\n— Página 3 —\ndois"
        );
        let single = PageRange::parse("1", 3).unwrap();
        assert_eq!(text_report(&single, &["só\n".into()]), "só\n");
    }
}
