// SPDX-License-Identifier: AGPL-3.0-or-later
//! OCR das páginas: texto reconhecido e camada pesquisável, página a página,
//! com progresso e cancelamento.

use crate::app::erros::ToolError;
use crate::app::portas::{LayoutSource, OcrEngine, Renderer};
use crate::dominio::geometria::LayoutRect;
use crate::dominio::layout::Page;
use crate::dominio::ocr::valid_language;
use crate::dominio::paginas::PageRange;
use std::fmt;
use std::path::Path;

/// Resolução das páginas enviadas ao OCR.
pub const OCR_DPI: u32 = 300;

/// Portas usadas pelo OCR.
#[derive(Clone, Copy)]
pub struct OcrSources<'a> {
    pub layout: &'a dyn LayoutSource,
    pub renderer: &'a dyn Renderer,
    pub engine: &'a dyn OcrEngine,
}

/// Chamado antes de cada página com `(feitas, total)`; `false` cancela.
pub type Progress<'a> = &'a dyn Fn(usize, usize) -> bool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrError {
    /// Idioma fora do formato do Tesseract (`por`, `por+eng`).
    InvalidLanguage(String),
    /// Idioma que o motor não tem instalado.
    MissingLanguage {
        language: String,
        installed: Vec<String>,
    },
    Cancelled,
    Tool(ToolError),
}

impl fmt::Display for OcrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLanguage(language) => write!(
                f,
                "Código de idioma OCR inválido: {language} (ex.: por, eng, por+eng)"
            ),
            Self::MissingLanguage {
                language,
                installed,
            } => write!(
                f,
                "Idioma OCR não instalado: {language} (instalados: {})",
                installed.join(", ")
            ),
            Self::Cancelled => write!(f, "OCR cancelado"),
            Self::Tool(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for OcrError {}

impl From<ToolError> for OcrError {
    fn from(error: ToolError) -> Self {
        Self::Tool(error)
    }
}

impl From<OcrError> for String {
    fn from(error: OcrError) -> Self {
        error.to_string()
    }
}

/// O que o OCR pesquisável fez numa página.
#[derive(Debug, Clone, PartialEq)]
pub enum PageOutcome {
    /// A página já tinha texto e não foi reconhecida.
    HadText,
    /// O OCR não encontrou palavras.
    NoWords,
    /// Palavras reconhecidas, em **pontos** da página (origem no topo).
    Recognized(Page),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageOcr {
    /// Número da página (1-based).
    pub page: usize,
    pub outcome: PageOutcome,
}

/// Contagens de um OCR pesquisável.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OcrSummary {
    pub recognized: usize,
    pub had_text: usize,
    pub no_words: usize,
    pub words: usize,
}

impl OcrSummary {
    pub fn of(pages: &[PageOcr]) -> Self {
        let mut summary = Self::default();
        for page in pages {
            match &page.outcome {
                PageOutcome::HadText => summary.had_text += 1,
                PageOutcome::NoWords => summary.no_words += 1,
                PageOutcome::Recognized(layout) => {
                    summary.recognized += 1;
                    summary.words += layout.words().count();
                }
            }
        }
        summary
    }
}

/// Recusa idioma malformado ou não instalado antes de renderizar qualquer
/// página. Sem a lista de instalados, deixa o motor decidir.
pub fn check_language(engine: &dyn OcrEngine, language: &str) -> Result<(), OcrError> {
    if !valid_language(language) {
        return Err(OcrError::InvalidLanguage(language.into()));
    }
    let installed = engine.languages()?;
    if installed.is_empty() {
        return Ok(());
    }
    match language
        .split('+')
        .find(|part| !installed.iter().any(|name| name == part))
    {
        Some(missing) => Err(OcrError::MissingLanguage {
            language: missing.into(),
            installed,
        }),
        None => Ok(()),
    }
}

/// Página renderizada a [`OCR_DPI`] e reconhecida, com as palavras levadas
/// para pontos de `size` (largura × altura da página).
pub fn recognize_page(
    sources: &OcrSources,
    pdf: &Path,
    page: usize,
    size: (f64, f64),
    language: &str,
) -> Result<Page, ToolError> {
    let (width, height) = size;
    let whole = LayoutRect::new(0.0, 0.0, width, height);
    let png = sources.renderer.render_region(pdf, page, whole, OCR_DPI)?;
    let recognized = sources.engine.words(&png, language)?;
    // Pixels → pontos pela largura real da imagem (arredondada pelo renderizador).
    let mut scaled = recognized.scaled(width / recognized.width);
    scaled.width = width;
    scaled.height = height;
    Ok(scaled)
}

fn advance(progress: Progress, done: usize, total: usize) -> Result<(), OcrError> {
    if progress(done, total) {
        Ok(())
    } else {
        Err(OcrError::Cancelled)
    }
}

/// OCR pesquisável das `pages` de `pdf`: reconhece cada página (as que já
/// têm texto só com `redo`) e devolve o resultado de cada uma, na ordem.
pub fn recognize(
    sources: &OcrSources,
    pdf: &Path,
    pages: &PageRange,
    language: &str,
    redo: bool,
    progress: Progress,
) -> Result<Vec<PageOcr>, OcrError> {
    check_language(sources.engine, language)?;
    let total = pages.pages().len();
    let mut results = Vec::with_capacity(total);
    for (done, page) in pages.pages().iter().enumerate() {
        advance(progress, done, total)?;
        let page = page.get();
        let layout = sources.layout.layout(pdf, page)?;
        let outcome = if !redo && layout.words().next().is_some() {
            PageOutcome::HadText
        } else {
            let size = (layout.width, layout.height);
            let recognized = recognize_page(sources, pdf, page, size, language)?;
            if recognized.words().next().is_some() {
                PageOutcome::Recognized(recognized)
            } else {
                PageOutcome::NoWords
            }
        };
        results.push(PageOcr { page, outcome });
    }
    advance(progress, total, total)?;
    Ok(results)
}

/// Texto reconhecido de cada página de `pages`, na ordem; `language`
/// `None` usa o idioma padrão do motor.
pub fn read_text(
    sources: &OcrSources,
    pdf: &Path,
    pages: &PageRange,
    language: Option<&str>,
    progress: Progress,
) -> Result<Vec<String>, OcrError> {
    if let Some(language) = language {
        check_language(sources.engine, language)?;
    }
    let total = pages.pages().len();
    let mut texts = Vec::with_capacity(total);
    for (done, page) in pages.pages().iter().enumerate() {
        advance(progress, done, total)?;
        let layout = sources.layout.layout(pdf, page.get())?;
        let whole = LayoutRect::new(0.0, 0.0, layout.width, layout.height);
        let png = sources
            .renderer
            .render_region(pdf, page.get(), whole, OCR_DPI)?;
        texts.push(sources.engine.text(&png, language)?);
    }
    advance(progress, total, total)?;
    Ok(texts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{FakeOcr, FakePoppler};
    use std::cell::RefCell;

    const PDF: &str = "documento.pdf";

    /// Página 1 com texto, 2 e 3 digitalizadas (sem texto).
    fn poppler() -> FakePoppler {
        FakePoppler::new(vec![
            vec![(72.0, 700.0, 12.0, "Texto digital")],
            vec![],
            vec![],
        ])
    }

    fn all(total: usize) -> PageRange {
        PageRange::all(total)
    }

    fn keep_going(_: usize, _: usize) -> bool {
        true
    }

    #[test]
    fn recognizes_pages_without_text_and_skips_the_others() {
        let poppler = poppler();
        let ocr = FakeOcr::new(&[&["Texto", "digitalizado"]]);
        let sources = OcrSources {
            layout: &poppler,
            renderer: &poppler,
            engine: &ocr,
        };
        let results =
            recognize(&sources, Path::new(PDF), &all(3), "por", false, &keep_going).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].outcome, PageOutcome::HadText);
        let PageOutcome::Recognized(layout) = &results[1].outcome else {
            panic!("página 2 reconhecida: {:?}", results[1].outcome);
        };
        assert_eq!(results[1].page, 2);
        assert_eq!((layout.width, layout.height), (595.0, 842.0));
        let words: Vec<&str> = layout.words().map(|word| word.text.as_str()).collect();
        assert_eq!(words, ["Texto", "digitalizado"]);
        // Caixas em pontos da página, dentro dela.
        for word in layout.words() {
            assert!(word.rect.right() <= 595.0 && word.rect.bottom() <= 842.0);
            assert!(word.rect.left() >= 0.0 && word.rect.top() >= 0.0);
        }
        assert_eq!(
            OcrSummary::of(&results),
            OcrSummary {
                recognized: 2,
                had_text: 1,
                no_words: 0,
                words: 4
            }
        );
    }

    #[test]
    fn redo_recognizes_pages_that_already_have_text() {
        let poppler = poppler();
        let ocr = FakeOcr::new(&[&["Outra"]]);
        let sources = OcrSources {
            layout: &poppler,
            renderer: &poppler,
            engine: &ocr,
        };
        let pages = PageRange::parse("1", 3).unwrap();
        let results =
            recognize(&sources, Path::new(PDF), &pages, "eng", true, &keep_going).unwrap();
        assert!(matches!(results[0].outcome, PageOutcome::Recognized(_)));
    }

    #[test]
    fn pages_without_words_do_not_stop_the_document() {
        let poppler = poppler();
        let ocr = FakeOcr::new(&[]);
        let sources = OcrSources {
            layout: &poppler,
            renderer: &poppler,
            engine: &ocr,
        };
        let results =
            recognize(&sources, Path::new(PDF), &all(3), "por", false, &keep_going).unwrap();
        let summary = OcrSummary::of(&results);
        assert_eq!((summary.no_words, summary.had_text), (2, 1));
    }

    #[test]
    fn reports_progress_before_each_page_and_at_the_end() {
        let poppler = poppler();
        let ocr = FakeOcr::new(&[&["x"]]);
        let sources = OcrSources {
            layout: &poppler,
            renderer: &poppler,
            engine: &ocr,
        };
        let seen = RefCell::new(Vec::new());
        let progress = |done: usize, total: usize| {
            seen.borrow_mut().push((done, total));
            true
        };
        let pages = PageRange::parse("2-3", 3).unwrap();
        recognize(&sources, Path::new(PDF), &pages, "por", false, &progress).unwrap();
        assert_eq!(seen.into_inner(), [(0, 2), (1, 2), (2, 2)]);
    }

    #[test]
    fn cancelling_stops_before_the_next_page() {
        let poppler = poppler();
        let ocr = FakeOcr::new(&[&["x"]]);
        let sources = OcrSources {
            layout: &poppler,
            renderer: &poppler,
            engine: &ocr,
        };
        let stop_at_second = |done: usize, _: usize| done < 1;
        assert_eq!(
            recognize(
                &sources,
                Path::new(PDF),
                &all(3),
                "por",
                false,
                &stop_at_second
            ),
            Err(OcrError::Cancelled)
        );
        assert_eq!(
            read_text(&sources, Path::new(PDF), &all(3), None, &stop_at_second),
            Err(OcrError::Cancelled)
        );
    }

    #[test]
    fn language_is_checked_before_any_page() {
        let poppler = poppler();
        let ocr = FakeOcr::new(&[&["x"]]);
        let sources = OcrSources {
            layout: &poppler,
            renderer: &poppler,
            engine: &ocr,
        };
        let never = |_: usize, _: usize| -> bool { panic!("nenhuma página antes do idioma") };
        assert_eq!(
            recognize(&sources, Path::new(PDF), &all(3), "p;rm", false, &never),
            Err(OcrError::InvalidLanguage("p;rm".into()))
        );
        let missing = recognize(&sources, Path::new(PDF), &all(3), "por+deu", false, &never);
        assert_eq!(
            missing,
            Err(OcrError::MissingLanguage {
                language: "deu".into(),
                installed: vec!["eng".into(), "por".into()]
            })
        );
        assert_eq!(
            missing.unwrap_err().to_string(),
            "Idioma OCR não instalado: deu (instalados: eng, por)"
        );
        assert_eq!(
            read_text(&sources, Path::new(PDF), &all(3), Some("xx-"), &never)
                .unwrap_err()
                .to_string(),
            "Idioma OCR não instalado: xx- (instalados: eng, por)"
        );
    }

    /// Motor que não sabe dizer os idiomas instalados.
    struct Unlisted;

    impl OcrEngine for Unlisted {
        fn text(&self, _: &[u8], language: Option<&str>) -> Result<String, ToolError> {
            Ok(format!("idioma {}", language.unwrap_or("padrão")))
        }
        fn words(&self, png: &[u8], language: &str) -> Result<Page, ToolError> {
            FakeOcr::new(&[]).words(png, language)
        }
        fn languages(&self) -> Result<Vec<String>, ToolError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn without_the_installed_list_the_engine_decides() {
        assert_eq!(check_language(&Unlisted, "klingon"), Ok(()));
        assert_eq!(
            check_language(&Unlisted, ""),
            Err(OcrError::InvalidLanguage(String::new()))
        );
    }

    #[test]
    fn reads_the_text_of_every_page_in_order() {
        let poppler = poppler();
        let sources = OcrSources {
            layout: &poppler,
            renderer: &poppler,
            engine: &Unlisted,
        };
        let texts = read_text(&sources, Path::new(PDF), &all(2), None, &keep_going).unwrap();
        assert_eq!(texts, ["idioma padrão", "idioma padrão"]);
        let texts = read_text(&sources, Path::new(PDF), &all(1), Some("por"), &keep_going);
        assert_eq!(texts.unwrap(), ["idioma por"]);
    }

    #[test]
    fn tool_failures_keep_their_message() {
        let poppler = poppler();
        let ocr = FakeOcr::new(&[&["x"]]);
        let sources = OcrSources {
            layout: &poppler,
            renderer: &poppler,
            engine: &ocr,
        };
        // A página 4 não existe no dublê.
        let pages = PageRange::parse("1-4", 4).unwrap();
        let error =
            recognize(&sources, Path::new(PDF), &pages, "por", false, &keep_going).unwrap_err();
        assert!(matches!(error, OcrError::Tool(_)));
        assert_eq!(String::from(error), "dublê falhou: página 4 inexistente");
        assert_eq!(OcrError::Cancelled.to_string(), "OCR cancelado");
    }
}
