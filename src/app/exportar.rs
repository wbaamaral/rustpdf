// SPDX-License-Identifier: AGPL-3.0-or-later
//! Caso de uso: extrair o documento estruturado das páginas escolhidas.

use crate::app::erros::ToolError;
use crate::app::ocr::{recognize_page, OcrSources};
use crate::app::portas::{ImageSource, LayoutSource, OcrEngine, Renderer, RulingSource};
use crate::dominio::exportacao::estrutura::structure_with;
use crate::dominio::exportacao::modelo::{Document, PageContent, PageImage, TableRef};
use crate::dominio::exportacao::slides::{faithful_slide, slide_with_rulings, Slide};
use crate::dominio::geometria::LayoutRect;
use crate::dominio::layout::Page;
pub use crate::dominio::paginas::PageRange;
use std::fmt;
use std::path::{Path, PathBuf};

/// Formato de saída da exportação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Csv,
    Xlsx,
    Docx,
    Pptx,
    Odt,
    Odg,
}

impl ExportFormat {
    pub const ALL: [Self; 7] = [
        Self::Docx,
        Self::Pptx,
        Self::Odt,
        Self::Odg,
        Self::Xlsx,
        Self::Markdown,
        Self::Csv,
    ];

    /// Formato pela extensão do arquivo de saída (sem diferenciar maiúsculas).
    pub fn from_output(output: &str) -> Result<Self, String> {
        let extension = output
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        Self::ALL
            .into_iter()
            .find(|format| extension.as_deref() == Some(format.extension()))
            .ok_or_else(|| {
                format!(
                    "Formato de exportação não suportado: {output} (disponíveis: .md, .csv, .xlsx, .docx, .pptx, .odt, .odg)"
                )
            })
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
            Self::Docx => "docx",
            Self::Pptx => "pptx",
            Self::Odt => "odt",
            Self::Odg => "odg",
        }
    }

    /// Nome para a interface.
    pub fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Csv => "CSV",
            Self::Xlsx => "Excel",
            Self::Docx => "Word",
            Self::Pptx => "PowerPoint",
            Self::Odt => "ODT (Writer)",
            Self::Odg => "ODG (Draw)",
        }
    }

    /// Só tabelas (CSV, Excel): exige ao menos uma.
    pub fn tables_only(self) -> bool {
        matches!(self, Self::Csv | Self::Xlsx)
    }
}

/// Pedido de exportação: destino, formato, páginas e opções.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRequest {
    pub output: PathBuf,
    pub format: ExportFormat,
    /// Intervalo como digitado (validado contra o total ao exportar); vazio = todas.
    pub pages: Option<String>,
    /// Markdown e Word: incluir as imagens da página.
    pub images: bool,
    /// Word: quebra de página entre as páginas do PDF.
    pub page_breaks: bool,
    /// PowerPoint: a página como imagem, em vez de caixas editáveis.
    pub faithful: bool,
    /// CSV: separador de campos e marca UTF-8 (BOM).
    pub separator: char,
    pub bom: bool,
    /// Idioma do OCR para páginas sem texto (`None` = sem OCR).
    pub ocr: Option<String>,
    /// Otimizar a exportação para consumo por agentes de IA (remove ruídos, cabeçalhos repetidos e formatação excessiva).
    pub ai_optimized: bool,
}

impl ExportRequest {
    /// Pedido com as opções padrão (imagens, `;`, BOM).
    pub fn new(output: PathBuf, format: ExportFormat) -> Self {
        Self {
            output,
            format,
            pages: None,
            images: true,
            page_breaks: false,
            faithful: false,
            separator: ';',
            bom: true,
            ocr: None,
            ai_optimized: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExportError {
    Tool(ToolError),
    /// Nenhuma das páginas escolhidas tem texto.
    NoText,
    /// Exportação de tabelas (CSV) sem nenhuma tabela.
    NoTables,
    /// Interrompida a pedido (o progresso devolveu `false`).
    Cancelled,
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tool(error) => error.fmt(f),
            Self::NoText => f.write_str(
                "As páginas escolhidas não têm texto extraível (páginas digitalizadas precisam de OCR)",
            ),
            Self::NoTables => f.write_str("Nenhuma tabela encontrada nas páginas escolhidas"),
            Self::Cancelled => f.write_str("Exportação cancelada"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<ToolError> for ExportError {
    fn from(error: ToolError) -> Self {
        Self::Tool(error)
    }
}

impl From<ExportError> for String {
    fn from(error: ExportError) -> Self {
        error.to_string()
    }
}

/// Portas usadas pela exportação e opções de leitura das páginas.
#[derive(Clone, Copy)]
pub struct Sources<'a> {
    pub layout: &'a dyn LayoutSource,
    pub renderer: &'a dyn Renderer,
    /// Onde estão as imagens; `None` = exportar sem imagens.
    pub images: Option<&'a dyn ImageSource>,
    /// OCR das páginas sem texto: motor e idioma (formato do Tesseract).
    pub ocr: Option<(&'a dyn OcrEngine, &'a str)>,
    /// Traços da página (grades de tabelas); `None` = só tabelas sem bordas.
    pub rulings: Option<&'a dyn RulingSource>,
    /// Chamado antes de cada página com `(feitas, total)`; `false` cancela.
    pub progress: Option<&'a dyn Fn(usize, usize) -> bool>,
    /// Resolução das imagens e das páginas renderizadas.
    pub dpi: u32,
}

impl<'a> Sources<'a> {
    /// Só texto, a 150 dpi.
    pub fn new(layout: &'a dyn LayoutSource, renderer: &'a dyn Renderer) -> Self {
        Self {
            layout,
            renderer,
            images: None,
            ocr: None,
            rulings: None,
            progress: None,
            dpi: 150,
        }
    }

    pub fn with_images(self, images: &'a dyn ImageSource) -> Self {
        Self {
            images: Some(images),
            ..self
        }
    }

    pub fn with_ocr(self, engine: &'a dyn OcrEngine, language: &'a str) -> Self {
        Self {
            ocr: Some((engine, language)),
            ..self
        }
    }

    pub fn with_rulings(self, rulings: &'a dyn RulingSource) -> Self {
        Self {
            rulings: Some(rulings),
            ..self
        }
    }

    pub fn with_progress(self, progress: &'a dyn Fn(usize, usize) -> bool) -> Self {
        Self {
            progress: Some(progress),
            ..self
        }
    }

    /// Avisa o progresso; `Err(Cancelled)` se pediram para parar.
    fn advance(&self, done: usize, total: usize) -> Result<(), ExportError> {
        match self.progress {
            Some(progress) if !progress(done, total) => Err(ExportError::Cancelled),
            _ => Ok(()),
        }
    }

    pub fn with_dpi(self, dpi: u32) -> Self {
        Self { dpi, ..self }
    }
}

/// Estrutura (títulos, parágrafos, listas, tabelas e, se pedido, imagens)
/// das páginas `pages` de `pdf`. Páginas vazias ficam vazias; se todas
/// estiverem, é erro.
pub fn extract(sources: &Sources, pdf: &Path, pages: &PageRange) -> Result<Document, ExportError> {
    let mut document = Document::default();
    let total = pages.pages().len();
    for (done, page) in pages.pages().iter().enumerate() {
        sources.advance(done, total)?;
        let layout = page_layout(sources, pdf, page.get())?;
        let images = render_images(sources, pdf, page.get(), &layout)?;
        let segments = match sources.rulings {
            Some(rulings) => rulings
                .segments(pdf, page.get())?
                .into_iter()
                .map(|((x0, y0), (x1, y1))| ((x0, layout.height - y0), (x1, layout.height - y1)))
                .collect(),
            None => Vec::new(),
        };
        document.pages.push(PageContent {
            number: page.get(),
            size: (layout.width, layout.height),
            blocks: structure_with(&layout, images, &segments),
        });
    }
    if document.pages.iter().all(|page| page.blocks.is_empty()) {
        return Err(ExportError::NoText);
    }
    Ok(document)
}

/// Slides das páginas: editáveis (caixas de texto e imagens nas posições)
/// ou fiéis (a página renderizada como imagem).
pub fn extract_slides(
    sources: &Sources,
    pdf: &Path,
    pages: &PageRange,
    faithful: bool,
) -> Result<Vec<Slide>, ExportError> {
    let mut slides = Vec::new();
    let total = pages.pages().len();
    for (done, page) in pages.pages().iter().enumerate() {
        sources.advance(done, total)?;
        let layout = page_layout(sources, pdf, page.get())?;
        slides.push(if faithful {
            let whole = LayoutRect::new(0.0, 0.0, layout.width, layout.height);
            let png = sources
                .renderer
                .render_region(pdf, page.get(), whole, sources.dpi)?;
            faithful_slide((layout.width, layout.height), png)
        } else {
            let pictures = render_images(sources, pdf, page.get(), &layout)?;
            let segments = match sources.rulings {
                Some(rulings) => rulings
                    .segments(pdf, page.get())?
                    .into_iter()
                    .map(|((x0, y0), (x1, y1))| {
                        ((x0, layout.height - y0), (x1, layout.height - y1))
                    })
                    .collect(),
                None => Vec::new(),
            };
            slide_with_rulings(&layout, pictures, segments)
        });
    }
    Ok(slides)
}

/// Layout da página; sem nenhuma palavra e com OCR pedido, o da página
/// renderizada e reconhecida, em pontos.
fn page_layout(sources: &Sources, pdf: &Path, page: usize) -> Result<Page, ExportError> {
    let layout = sources.layout.layout(pdf, page)?;
    let Some((engine, language)) = sources.ocr else {
        return Ok(layout);
    };
    if layout.words().next().is_some() {
        return Ok(layout);
    }
    let ocr = OcrSources {
        layout: sources.layout,
        renderer: sources.renderer,
        engine,
    };
    Ok(recognize_page(
        &ocr,
        pdf,
        page,
        (layout.width, layout.height),
        language,
    )?)
}

/// Imagens da página renderizadas, com as caixas em origem no topo
/// (nenhuma se a exportação não pediu imagens).
fn render_images(
    sources: &Sources,
    pdf: &Path,
    page: usize,
    layout: &Page,
) -> Result<Vec<PageImage>, ExportError> {
    let Some(images) = sources.images else {
        return Ok(Vec::new());
    };
    let mut rendered = Vec::new();
    for rect in images.image_boxes(pdf, page)? {
        let rect = rect.to_layout(layout.height);
        rendered.push(PageImage {
            png: sources
                .renderer
                .render_region(pdf, page, rect, sources.dpi)?,
            rect,
        });
    }
    Ok(rendered)
}

/// Tabelas do documento; nenhuma é erro (formatos só de tabelas, como CSV).
pub fn require_tables(document: &Document) -> Result<Vec<TableRef<'_>>, ExportError> {
    let tables = document.tables();
    if tables.is_empty() {
        return Err(ExportError::NoTables);
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::exportacao::modelo::Block;
    use crate::testkit::{FakePoppler, TextLine};
    use std::path::Path;

    const CAPA: &[TextLine] = &[
        (72.0, 760.0, 28.0, "Relatório"),
        (72.0, 700.0, 12.0, "Texto da capa."),
    ];
    const LISTA: &[TextLine] = &[(72.0, 760.0, 12.0, "• Um"), (72.0, 740.0, 12.0, "• Dois")];

    fn source() -> FakePoppler {
        FakePoppler::new(vec![CAPA.to_vec(), LISTA.to_vec(), vec![]])
    }

    #[test]
    fn extracts_the_chosen_pages_in_order() {
        let pages = PageRange::parse("1-2", 3).unwrap();
        let document = extract(
            &Sources::new(&source(), &source()),
            Path::new("x.pdf"),
            &pages,
        )
        .unwrap();
        assert_eq!(document.pages.len(), 2);
        assert_eq!(document.pages[0].number, 1);
        assert_eq!(
            document.pages[0].size,
            (595.0, 842.0),
            "tamanho da página do PDF"
        );
        assert_eq!(
            document.pages[0].blocks,
            [
                Block::Heading {
                    level: 1,
                    text: "Relatório".into()
                },
                Block::Paragraph {
                    text: "Texto da capa.".into()
                },
            ]
        );
        assert_eq!(document.pages[1].number, 2);
        assert_eq!(document.pages[1].blocks.len(), 2);
    }

    #[test]
    fn pages_without_text_are_kept_but_all_empty_is_an_error() {
        let all = PageRange::parse("", 3).unwrap();
        let document = extract(
            &Sources::new(&source(), &source()),
            Path::new("x.pdf"),
            &all,
        )
        .unwrap();
        assert!(document.pages[2].blocks.is_empty());
        let blank = PageRange::parse("3", 3).unwrap();
        assert_eq!(
            extract(
                &Sources::new(&source(), &source()),
                Path::new("x.pdf"),
                &blank
            ),
            Err(ExportError::NoText)
        );
    }

    #[test]
    fn tool_failures_are_reported() {
        let missing = PageRange::parse("1", 1).unwrap();
        let empty = FakePoppler::new(vec![]);
        assert!(matches!(
            extract(&Sources::new(&empty, &empty), Path::new("x.pdf"), &missing),
            Err(ExportError::Tool(_))
        ));
        assert_eq!(
            ExportError::NoText.to_string(),
            "As páginas escolhidas não têm texto extraível (páginas digitalizadas precisam de OCR)"
        );
    }

    #[test]
    fn csv_needs_at_least_one_table() {
        let pages = PageRange::parse("1", 3).unwrap();
        let document = extract(
            &Sources::new(&source(), &source()),
            Path::new("x.pdf"),
            &pages,
        )
        .unwrap();
        assert_eq!(
            require_tables(&document).unwrap_err(),
            ExportError::NoTables
        );
        assert_eq!(
            ExportError::NoTables.to_string(),
            "Nenhuma tabela encontrada nas páginas escolhidas"
        );
    }

    #[test]
    fn images_are_rendered_and_placed_among_the_text() {
        use crate::testkit::{FakeImages, PNG_2X2};
        use crate::PdfRect;
        // Imagem de 100 × 50 pt entre o título (topo ≈ 61) e o texto (topo ≈ 133).
        let images = FakeImages::new(vec![
            vec![PdfRect::new(72.0, 720.0, 172.0, 770.0)],
            vec![],
            vec![],
        ]);
        let pages = PageRange::parse("1", 3).unwrap();
        let document = extract(
            &Sources::new(&source(), &source())
                .with_images(&images)
                .with_dpi(150),
            Path::new("x.pdf"),
            &pages,
        )
        .unwrap();
        assert_eq!(
            document.pages[0].blocks[1],
            Block::Image {
                png: PNG_2X2.to_vec(),
                size: (100.0, 50.0)
            }
        );
        assert_eq!(document.pages[0].blocks.len(), 3);
    }

    #[test]
    fn a_page_with_only_images_is_not_empty() {
        use crate::testkit::FakeImages;
        use crate::PdfRect;
        let images = FakeImages::new(vec![
            vec![],
            vec![],
            vec![PdfRect::new(0.0, 0.0, 595.0, 842.0)],
        ]);
        let blank = PageRange::parse("3", 3).unwrap();
        let document = extract(
            &Sources::new(&source(), &source())
                .with_images(&images)
                .with_dpi(72),
            Path::new("x.pdf"),
            &blank,
        )
        .unwrap();
        assert!(matches!(document.pages[0].blocks[0], Block::Image { .. }));
    }

    #[test]
    fn slides_editable_and_faithful() {
        use crate::dominio::exportacao::slides::Shape;
        use crate::testkit::{FakeImages, PNG_2X2};
        use crate::PdfRect;
        let images = FakeImages::new(vec![
            vec![PdfRect::new(72.0, 720.0, 172.0, 770.0)],
            vec![],
            vec![],
        ]);
        let pages = PageRange::parse("1-2", 3).unwrap();
        let editable = extract_slides(
            &Sources::new(&source(), &source())
                .with_images(&images)
                .with_dpi(150),
            Path::new("x.pdf"),
            &pages,
            false,
        )
        .unwrap();
        assert_eq!(editable.len(), 2);
        assert_eq!(editable[0].size, (595.0, 842.0));
        let texts = editable[0]
            .shapes
            .iter()
            .filter(|s| matches!(s, Shape::Text { .. }))
            .count();
        assert_eq!(texts, 2, "título e texto da capa");
        assert!(editable[0].shapes.contains(&Shape::Picture {
            rect: crate::LayoutRect::new(72.0, 72.0, 172.0, 122.0),
            png: PNG_2X2.to_vec(),
        }));
        let faithful = extract_slides(
            &Sources::new(&source(), &source())
                .with_images(&images)
                .with_dpi(72),
            Path::new("x.pdf"),
            &pages,
            true,
        )
        .unwrap();
        assert_eq!(faithful[1].shapes.len(), 1);
        assert!(matches!(&faithful[1].shapes[0], Shape::Picture { png, .. } if png == PNG_2X2));
    }

    #[test]
    fn formats_come_from_the_extension() {
        assert_eq!(ExportFormat::from_output("a.DOCX"), Ok(ExportFormat::Docx));
        assert_eq!(ExportFormat::from_output("c.ODT"), Ok(ExportFormat::Odt));
        assert_eq!(ExportFormat::from_output("d.odg"), Ok(ExportFormat::Odg));
        assert_eq!(
            ExportFormat::from_output("b.md").unwrap().label(),
            "Markdown"
        );
        assert!(ExportFormat::from_output("e.desconhecido").is_err());
        assert!(ExportFormat::Csv.tables_only() && !ExportFormat::Pptx.tables_only());
        let request = ExportRequest::new("x.csv".into(), ExportFormat::Csv);
        assert_eq!(
            (request.separator, request.bom, request.images),
            (';', true, true)
        );
    }

    #[test]
    fn pages_without_text_are_read_by_ocr_when_asked() {
        use crate::testkit::FakeOcr;
        let blank = PageRange::parse("3", 3).unwrap();
        let ocr = FakeOcr::new(&[&["Texto", "digitalizado"]]);
        let poppler = source();
        let sources = Sources::new(&poppler, &poppler).with_ocr(&ocr, "por");
        let document = extract(&sources, Path::new("x.pdf"), &blank).unwrap();
        assert_eq!(
            document.pages[0].blocks,
            [Block::Paragraph {
                text: "Texto digitalizado".into()
            }]
        );
        assert_eq!(
            document.pages[0].size,
            (595.0, 842.0),
            "tamanho da página do PDF"
        );
        // Página com texto não passa pelo OCR.
        let first = PageRange::parse("1", 3).unwrap();
        let document = extract(&sources, Path::new("x.pdf"), &first).unwrap();
        assert!(
            matches!(&document.pages[0].blocks[0], Block::Heading { text, .. } if text == "Relatório")
        );
    }

    #[test]
    fn ruled_tables_come_from_the_page_strokes() {
        use crate::testkit::{FakePoppler, FakeRulings};
        // Duas linhas de texto (topos ≈ 133 e 153 com fonte 12) numa grade
        // de 2 × 2 desenhada em pontos com origem embaixo.
        let lines: &[crate::testkit::TextLine] = &[
            (72.0, 700.0, 12.0, "Item"),
            (200.0, 700.0, 12.0, "Descrição longa da linha um"),
            (72.0, 680.0, 12.0, "Dois"),
            (200.0, 680.0, 12.0, "Outra descrição bem comprida aqui"),
        ];
        let poppler = FakePoppler::new(vec![lines.to_vec()]);
        let (top, middle, bottom) = (842.0 - 125.0, 842.0 - 145.0, 842.0 - 165.0);
        let mut segments = Vec::new();
        for y in [top, middle, bottom] {
            segments.push(((70.0, y), (450.0, y)));
        }
        for x in [70.0, 190.0, 450.0] {
            segments.push(((x, bottom), (x, top)));
        }
        let rulings = FakeRulings::new(vec![segments]);
        let sources = Sources::new(&poppler, &poppler).with_rulings(&rulings);
        let pages = PageRange::parse("1", 1).unwrap();
        let document = extract(&sources, Path::new("x.pdf"), &pages).unwrap();
        assert_eq!(
            document.pages[0].blocks,
            [Block::Table {
                rows: vec![
                    vec!["Item".into(), "Descrição longa da linha um".into()],
                    vec!["Dois".into(), "Outra descrição bem comprida aqui".into()],
                ]
            }]
        );
    }

    #[test]
    fn progress_is_reported_per_page_and_can_cancel() {
        use std::cell::RefCell;
        let seen = RefCell::new(Vec::new());
        let poppler = source();
        let report = |done: usize, total: usize| {
            seen.borrow_mut().push((done, total));
            true
        };
        let all = PageRange::parse("", 3).unwrap();
        let sources = Sources::new(&poppler, &poppler).with_progress(&report);
        extract(&sources, Path::new("x.pdf"), &all).unwrap();
        assert_eq!(*seen.borrow(), [(0, 3), (1, 3), (2, 3)]);

        let stop_after_first = |done: usize, _: usize| done < 1;
        let sources = Sources::new(&poppler, &poppler).with_progress(&stop_after_first);
        assert_eq!(
            extract(&sources, Path::new("x.pdf"), &all),
            Err(ExportError::Cancelled)
        );
        let slides = extract_slides(&sources, Path::new("x.pdf"), &all, false);
        assert_eq!(slides.unwrap_err(), ExportError::Cancelled);
        assert_eq!(ExportError::Cancelled.to_string(), "Exportação cancelada");
    }
}
