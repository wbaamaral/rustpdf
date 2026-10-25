// SPDX-License-Identifier: AGPL-3.0-or-later
//! Portas: o que os casos de uso precisam do mundo externo. A infraestrutura
//! implementa (poppler, tesseract, qpdf); os testes usam dublês do `testkit`,
//! validados pelos mesmos testes de contrato (`tests/contrato_*.rs`).
use crate::app::erros::ToolError;
use crate::dominio::geometria::{LayoutRect, PdfRect, Segment};
use crate::dominio::layout::Page;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

/// Texto de um PDF com posições.
pub trait LayoutSource {
    /// Layout da página `page` (1-based); página inexistente é erro.
    fn layout(&self, pdf: &Path, page: usize) -> Result<Page, ToolError>;

    /// Texto da página, com as colunas preservadas por espaços.
    fn text(&self, pdf: &Path, page: usize) -> Result<String, ToolError>;
}

/// Páginas de um PDF como imagens PNG.
pub trait Renderer {
    /// PNG da página com `width` pixels de largura (altura proporcional).
    fn render_page(
        &self,
        pdf: &Path,
        page: usize,
        width: u32,
        destination: &Path,
    ) -> Result<(), ToolError>;

    /// PNG (bytes) da região `region` da página (pontos, origem no topo)
    /// com `dpi` pontos por polegada.
    fn render_region(
        &self,
        pdf: &Path,
        page: usize,
        region: LayoutRect,
        dpi: u32,
    ) -> Result<Vec<u8>, ToolError>;

    /// PNGs das páginas `pages` em `directory` (`p-00001.png`, …), na ordem.
    fn render_pages(
        &self,
        pdf: &Path,
        pages: RangeInclusive<usize>,
        width: u32,
        directory: &Path,
    ) -> Result<Vec<(usize, PathBuf)>, ToolError>;
}

/// Reconhecimento de texto em imagens.
pub trait OcrEngine {
    /// Texto reconhecido no PNG; `language` no formato do Tesseract
    /// (`por`, `eng+por`).
    fn text(&self, png: &[u8], language: Option<&str>) -> Result<String, ToolError>;

    /// Palavras reconhecidas no PNG com caixas em **pixels** da imagem
    /// (origem no topo); `width`/`height` da página são as dimensões dela.
    fn words(&self, png: &[u8], language: &str) -> Result<Page, ToolError>;

    /// Idiomas instalados (`por`, `eng`…); lista vazia = não foi possível
    /// saber, e o motor decide ao reconhecer.
    fn languages(&self) -> Result<Vec<String>, ToolError>;
}

/// Onde as imagens estão desenhadas numa página.
pub trait ImageSource {
    /// Caixa de cada desenho de imagem da página `page` (pontos, origem
    /// embaixo), na ordem do conteúdo; página inexistente é erro.
    fn image_boxes(&self, pdf: &Path, page: usize) -> Result<Vec<PdfRect>, ToolError>;
}

/// Traços retos desenhados numa página (linhas de tabelas, molduras).
pub trait RulingSource {
    /// Segmentos pintados da página `page` (pontos, origem embaixo); página
    /// inexistente é erro.
    fn segments(&self, pdf: &Path, page: usize) -> Result<Vec<Segment>, ToolError>;
}
