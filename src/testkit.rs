// SPDX-License-Identifier: AGPL-3.0-or-later
//! Apoio aos testes: pastas temporárias e PDFs mínimos gerados em código.
//!
//! Público (e oculto da documentação) para servir também aos testes de
//! integração em `tests/`. Nenhum teste monta PDF à mão fora daqui.
use crate::app::erros::ToolError;
use crate::app::portas::{ImageSource, LayoutSource, OcrEngine, Renderer, RulingSource};
use crate::dominio::geometria::{LayoutRect, PdfRect, Segment};
use crate::dominio::layout::{Block, Line, Page, Word};
use crate::infra::pdf_objetos;
use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// PNG RGBA de 2 × 2 pixels, válido, para inserir e substituir imagens.
pub const PNG_2X2: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0,
    0, 0, 114, 182, 13, 36, 0, 0, 0, 18, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 12,
    129, 52, 24, 0, 0, 73, 200, 9, 247, 249, 171, 182, 13, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
    130,
];

/// Pasta temporária exclusiva do teste, apagada ao sair de escopo.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rustpdf-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("criar pasta temporária de teste");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Número de um objeto do PDF em construção (`N 0 R`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectId(pub usize);

impl std::fmt::Display for ObjectId {
    /// Referência indireta: `{id}` vira `N 0 R`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} 0 R", self.0)
    }
}

/// PDF mínimo montado em código. Os objetos 1 (catálogo) e 2 (árvore de
/// páginas) são reservados e preenchidos em `build`.
pub struct PdfBuilder {
    objects: Vec<Vec<u8>>,
    pages: Vec<ObjectId>,
    helvetica: Option<ObjectId>,
}

impl Default for PdfBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfBuilder {
    pub fn new() -> Self {
        Self {
            objects: vec![Vec::new(), Vec::new()],
            pages: Vec::new(),
            helvetica: None,
        }
    }

    /// Objeto com o corpo dado.
    pub fn object(&mut self, body: impl Into<Vec<u8>>) -> ObjectId {
        self.objects.push(body.into());
        ObjectId(self.objects.len())
    }

    /// Stream com entradas extras no dicionário (ex.: `/Type /XObject …`).
    pub fn stream(&mut self, dictionary_entries: &str, data: &[u8]) -> ObjectId {
        let body = pdf_objetos::stream(&prefixed(dictionary_entries), data);
        self.object(body)
    }

    /// Fonte Helvetica (WinAnsi), criada uma só vez.
    pub fn helvetica(&mut self) -> ObjectId {
        if let Some(font) = self.helvetica {
            return font;
        }
        let font = self.object(
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        );
        self.helvetica = Some(font);
        font
    }

    /// Form XObject com a caixa e os recursos dados.
    pub fn form(&mut self, bbox: [f64; 4], resources: &str, content: &str) -> ObjectId {
        let [x0, y0, x1, y1] = bbox;
        self.stream(
            &format!(
                "/Type /XObject /Subtype /Form /BBox [{x0} {y0} {x1} {y1}] /Resources << {resources} >>"
            ),
            content.as_bytes(),
        )
    }

    /// Imagem RGB de 8 bits sem compressão (`pixels` = largura × altura × 3).
    pub fn image_rgb(&mut self, width: usize, height: usize, pixels: &[u8]) -> ObjectId {
        assert_eq!(pixels.len(), width * height * 3, "pixels RGB incompletos");
        self.stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8"
            ),
            pixels,
        )
    }

    /// Página com o tamanho, os recursos (conteúdo do dicionário) e o conteúdo dados.
    pub fn page(&mut self, width: f64, height: f64, resources: &str, content: &str) -> ObjectId {
        let contents = self.stream("", content.as_bytes());
        self.page_with_contents(width, height, resources, contents)
    }

    /// Página que reaproveita um stream de conteúdo já criado.
    pub fn page_with_contents(
        &mut self,
        width: f64,
        height: f64,
        resources: &str,
        contents: ObjectId,
    ) -> ObjectId {
        let page = self.object(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Resources << {resources} >> /Contents {contents} >>"
        ));
        self.pages.push(page);
        page
    }

    /// Página A4 com textos em Helvetica (ver [`TextLine`]).
    pub fn text_page(&mut self, lines: &[(f64, f64, f64, &str)]) -> ObjectId {
        let font = self.helvetica();
        let content: String = lines
            .iter()
            .map(|(x, y, size, text)| {
                format!(
                    "BT /F1 {size} Tf 1 0 0 1 {x} {y} Tm ({}) Tj ET\n",
                    escape(text)
                )
            })
            .collect();
        self.page(595.0, 842.0, &format!("/Font << /F1 {font} >>"), &content)
    }

    pub fn build(mut self) -> Vec<u8> {
        assert!(!self.pages.is_empty(), "PDF sem páginas");
        let kids: Vec<String> = self.pages.iter().map(ToString::to_string).collect();
        self.objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
        self.objects[1] = format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            self.pages.len()
        )
        .into_bytes();
        pdf_objetos::serialize(&self.objects)
    }

    pub fn write(self, destination: &Path) {
        fs::write(destination, self.build()).expect("gravar PDF de teste");
    }
}

/// Linha de texto de teste: `(x, y da linha de base, tamanho, texto)`, em
/// pontos com origem embaixo — a mesma especificação gera o PDF
/// ([`PdfBuilder::text_page`]) e o layout do dublê ([`FakePoppler`]).
pub type TextLine = (f64, f64, f64, &'static str);

/// Dublê do poppler para páginas A4 de [`TextLine`]s: layout calculado pela
/// especificação (largura aproximada da Helvetica) e PNGs fixos. Validado
/// contra o poppler real em `tests/contrato_poppler.rs`.
pub struct FakePoppler {
    pages: Vec<Vec<TextLine>>,
}

impl FakePoppler {
    const WIDTH: f64 = 595.0;
    const HEIGHT: f64 = 842.0;

    pub fn new(pages: Vec<Vec<TextLine>>) -> Self {
        Self { pages }
    }

    fn page(&self, page: usize) -> Result<&[TextLine], ToolError> {
        page.checked_sub(1)
            .and_then(|index| self.pages.get(index))
            .map(Vec::as_slice)
            .ok_or_else(|| ToolError::Failed {
                program: "dublê".into(),
                detail: format!("página {page} inexistente"),
            })
    }

    fn line(&(x, y, size, text): &TextLine) -> Line {
        let top = Self::HEIGHT - y - 0.75 * size;
        let bottom = Self::HEIGHT - y + 0.25 * size;
        let mut left = x;
        let words = text
            .split_whitespace()
            .map(|word| {
                let right = left + 0.5 * size * word.chars().count() as f64;
                let rect = LayoutRect::new(left, top, right, bottom);
                left = right + 0.28 * size;
                Word {
                    text: word.into(),
                    rect,
                }
            })
            .collect();
        Line { words }
    }
}

impl LayoutSource for FakePoppler {
    fn layout(&self, _pdf: &Path, page: usize) -> Result<Page, ToolError> {
        let blocks = self
            .page(page)?
            .iter()
            .map(|line| Block {
                lines: vec![Self::line(line)],
            })
            .collect();
        Ok(Page {
            width: Self::WIDTH,
            height: Self::HEIGHT,
            blocks,
        })
    }

    fn text(&self, _pdf: &Path, page: usize) -> Result<String, ToolError> {
        let lines: Vec<&str> = self.page(page)?.iter().map(|line| line.3).collect();
        Ok(lines.join("\n") + "\n")
    }
}

impl Renderer for FakePoppler {
    fn render_page(
        &self,
        _pdf: &Path,
        page: usize,
        _width: u32,
        destination: &Path,
    ) -> Result<(), ToolError> {
        self.page(page)?;
        fs::write(destination, PNG_2X2).map_err(|e| ToolError::Failed {
            program: "dublê".into(),
            detail: e.to_string(),
        })
    }

    fn render_region(
        &self,
        _pdf: &Path,
        page: usize,
        _region: LayoutRect,
        _dpi: u32,
    ) -> Result<Vec<u8>, ToolError> {
        self.page(page)?;
        Ok(PNG_2X2.to_vec())
    }

    fn render_pages(
        &self,
        pdf: &Path,
        pages: RangeInclusive<usize>,
        width: u32,
        directory: &Path,
    ) -> Result<Vec<(usize, PathBuf)>, ToolError> {
        pages
            .map(|page| {
                let target = directory.join(format!("p-{page:05}.png"));
                self.render_page(pdf, page, width, &target)?;
                Ok((page, target))
            })
            .collect()
    }
}

/// Dublê do OCR: "reconhece" as linhas dadas em qualquer imagem PNG,
/// empilhadas a partir do alto, com o tamanho da página igual ao da imagem.
/// Validado contra o Tesseract em `tests/contrato_ocr.rs`.
pub struct FakeOcr {
    lines: Vec<Vec<&'static str>>,
}

impl FakeOcr {
    pub fn new(lines: &[&[&'static str]]) -> Self {
        Self {
            lines: lines.iter().map(|line| line.to_vec()).collect(),
        }
    }
}

impl OcrEngine for FakeOcr {
    fn text(&self, png: &[u8], _language: Option<&str>) -> Result<String, ToolError> {
        self.words(png, "")?;
        let lines: Vec<String> = self.lines.iter().map(|line| line.join(" ")).collect();
        Ok(lines.join("\n") + "\n")
    }

    fn words(&self, png: &[u8], _language: &str) -> Result<Page, ToolError> {
        let (width, height) =
            crate::infra::png::dimensions(png).ok_or_else(|| ToolError::Failed {
                program: "dublê".into(),
                detail: "imagem não é PNG".into(),
            })?;
        let (width, height) = (f64::from(width), f64::from(height));
        let line_height = height / 20.0;
        let blocks = self
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let top = height / 10.0 + index as f64 * 2.0 * line_height;
                let mut left = width / 10.0;
                let words = line
                    .iter()
                    .map(|text| {
                        let right = (left + line_height * 0.6 * text.len() as f64).min(width);
                        let rect = LayoutRect::new(left, top, right, top + line_height);
                        left = (right + line_height * 0.3).min(width);
                        Word {
                            text: (*text).into(),
                            rect,
                        }
                    })
                    .collect();
                Block {
                    lines: vec![Line { words }],
                }
            })
            .collect();
        Ok(Page {
            width,
            height,
            blocks,
        })
    }

    fn languages(&self) -> Result<Vec<String>, ToolError> {
        Ok(vec!["eng".into(), "por".into()])
    }
}

/// Dublê de [`ImageSource`]: caixas fixas por página. Validado contra o
/// interpretador em `tests/contrato_imagens.rs`.
pub struct FakeImages {
    pages: Vec<Vec<PdfRect>>,
}

impl FakeImages {
    pub fn new(pages: Vec<Vec<PdfRect>>) -> Self {
        Self { pages }
    }
}

impl ImageSource for FakeImages {
    fn image_boxes(&self, _pdf: &Path, page: usize) -> Result<Vec<PdfRect>, ToolError> {
        page.checked_sub(1)
            .and_then(|index| self.pages.get(index))
            .cloned()
            .ok_or_else(|| ToolError::Failed {
                program: "dublê".into(),
                detail: format!("página {page} inexistente"),
            })
    }
}

/// Dublê de [`RulingSource`]: segmentos fixos por página.
pub struct FakeRulings {
    pages: Vec<Vec<Segment>>,
}

impl FakeRulings {
    pub fn new(pages: Vec<Vec<Segment>>) -> Self {
        Self { pages }
    }
}

impl RulingSource for FakeRulings {
    fn segments(&self, _pdf: &Path, page: usize) -> Result<Vec<Segment>, ToolError> {
        page.checked_sub(1)
            .and_then(|index| self.pages.get(index))
            .cloned()
            .ok_or_else(|| ToolError::Failed {
                program: "dublê".into(),
                detail: format!("página {page} inexistente"),
            })
    }
}

fn prefixed(entries: &str) -> String {
    if entries.is_empty() {
        String::new()
    } else {
        format!(" {entries}")
    }
}

/// Texto literal do PDF: escapa `(`, `)` e `\`.
fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '(' | ')' | '\\' => vec!['\\', c],
            _ => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn tool(program: &str, args: &[&str]) -> String {
        let output = Command::new(program).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn temp_dir_is_unique_and_removed_on_drop() {
        let first = TempDir::new("testkit");
        let second = TempDir::new("testkit");
        assert_ne!(first.path(), second.path());
        assert!(first.path().is_dir());
        fs::write(first.join("a.txt"), "x").unwrap();
        let kept = first.path().to_path_buf();
        drop(first);
        assert!(!kept.exists());
    }

    #[test]
    fn references_render_as_indirect_objects() {
        assert_eq!(ObjectId(7).to_string(), "7 0 R");
        assert_eq!(escape(r"a(b)c\d"), r"a\(b\)c\\d");
    }

    #[test]
    fn builds_valid_pdf_with_pages_forms_and_images() {
        let dir = TempDir::new("testkit-pdf");
        let path = dir.join("montado.pdf");
        let mut pdf = PdfBuilder::new();
        pdf.text_page(&[(72.0, 700.0, 14.0, "Primeira (pagina)")]);
        let font = pdf.helvetica();
        let form = pdf.form(
            [0.0, 0.0, 200.0, 50.0],
            &format!("/Font << /H {font} >>"),
            "BT /H 12 Tf 1 0 0 1 10 10 Tm (No form) Tj ET",
        );
        let image = pdf.image_rgb(2, 1, &[255, 0, 0, 0, 0, 255]);
        pdf.page(
            300.0,
            400.0,
            &format!("/XObject << /Fm {form} /Im {image} >>"),
            "q 1 0 0 1 20 300 cm /Fm Do Q q 50 0 0 25 20 20 cm /Im Do Q",
        );
        pdf.write(&path);

        let file = path.to_str().unwrap();
        tool("qpdf", &["--check", file]);
        assert_eq!(tool("qpdf", &["--show-npages", file]).trim(), "2");
        let text = tool("pdftotext", &[file, "-"]);
        assert!(text.contains("Primeira (pagina)"), "{text}");
        assert!(text.contains("No form"), "{text}");
        assert_eq!(tool("pdfimages", &["-list", file]).lines().count(), 3);
    }
}
