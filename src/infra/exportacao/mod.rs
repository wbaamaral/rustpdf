// SPDX-License-Identifier: AGPL-3.0-or-later
//! Escritores dos formatos de exportação.
pub mod csv;
pub mod docx;
pub mod markdown;
pub mod ooxml;
pub mod pptx;
pub mod xlsx;

use crate::app::exportar::{
    extract, extract_slides, require_tables, ExportFormat, ExportRequest, Sources,
};
use crate::dominio::paginas::PageRange;
use crate::infra::imagens::{PdfImages, PdfRulings};
use crate::infra::poppler::Poppler;
use crate::infra::tesseract::Tesseract;
use std::path::{Path, PathBuf};

/// Resolução das imagens exportadas.
pub const IMAGE_DPI: u32 = 150;

/// Exporta `pdf` (com `total_pages` páginas) conforme `request` e devolve
/// os arquivos gravados, na ordem. Usado pela CLI e pela interface.
/// `progress(feitas, total)` é chamado antes de cada página; `false` cancela.
pub fn export_to_files(
    pdf: &Path,
    total_pages: usize,
    request: &ExportRequest,
    progress: &dyn Fn(usize, usize) -> bool,
) -> Result<Vec<PathBuf>, String> {
    let pages = PageRange::parse(request.pages.as_deref().unwrap_or(""), total_pages)?;
    let mut written = Vec::new();
    let mut write = |path: &Path, content: &[u8]| -> Result<(), String> {
        std::fs::write(path, content)
            .map_err(|e| format!("Não foi possível gravar {}: {e}", path.display()))?;
        written.push(path.to_path_buf());
        Ok(())
    };
    let output = &request.output;
    let mut sources = Sources::new(&Poppler, &Poppler)
        .with_dpi(IMAGE_DPI)
        .with_rulings(&PdfRulings)
        .with_progress(progress);
    if let Some(language) = request.ocr.as_deref() {
        sources = sources.with_ocr(&Tesseract, language);
    }
    let with_images = request.format == ExportFormat::Pptx
        || (request.images
            && matches!(request.format, ExportFormat::Markdown | ExportFormat::Docx));
    if with_images {
        sources = sources.with_images(&PdfImages);
    }
    if request.format == ExportFormat::Pptx {
        let slides = extract_slides(&sources, pdf, &pages, request.faithful)?;
        write(output, &pptx::render(&slides)?)?;
        return Ok(written);
    }
    let document = extract(&sources, pdf, &pages)?;
    match request.format {
        ExportFormat::Markdown => {
            // Imagens em "<nome>_arquivos/", ao lado do .md.
            let stem = output
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let rendered = markdown::render(&document, &format!("{stem}_arquivos"));
            let base = output.parent().unwrap_or(Path::new(""));
            for (relative, png) in &rendered.images {
                let path = base.join(relative);
                if let Some(folder) = path.parent() {
                    std::fs::create_dir_all(folder)
                        .map_err(|e| format!("Não foi possível criar {}: {e}", folder.display()))?;
                }
                write(&path, png)?;
            }
            write(output, rendered.text.as_bytes())?;
        }
        ExportFormat::Docx => {
            let options = docx::DocxOptions {
                page_breaks: request.page_breaks,
            };
            write(output, &docx::render(&document, &options)?)?;
        }
        ExportFormat::Xlsx => {
            require_tables(&document)?;
            write(output, &xlsx::render(&document)?)?;
        }
        ExportFormat::Csv => {
            let tables = require_tables(&document)?;
            let names: Vec<(usize, usize)> = tables.iter().map(|t| (t.page, t.index)).collect();
            let options = csv::CsvOptions {
                separator: request.separator,
                bom: request.bom,
            };
            for (table, path) in tables.iter().zip(csv::output_paths(output, &names)) {
                write(&path, csv::render(table.rows, &options).as_bytes())?;
            }
        }
        ExportFormat::Pptx => unreachable!("tratado acima"),
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::exportar::{ExportFormat, ExportRequest};
    use crate::testkit::{PdfBuilder, TempDir};

    #[test]
    fn writes_every_file_and_reports_them() {
        let dir = TempDir::new("exportar");
        let pdf = dir.join("in.pdf");
        let mut builder = PdfBuilder::new();
        let font = builder.helvetica();
        let image = builder.image_rgb(1, 1, &[9, 9, 9]);
        builder.page(
            595.0,
            842.0,
            &format!("/Font << /F1 {font} >> /XObject << /Im {image} >>"),
            "BT /F1 12 Tf 1 0 0 1 72 760 Tm (Texto) Tj ET q 50 0 0 50 72 600 cm /Im Do Q",
        );
        builder.text_page(&[
            (72.0, 700.0, 12.0, "A"),
            (250.0, 700.0, 12.0, "B"),
            (72.0, 684.0, 12.0, "1"),
            (250.0, 684.0, 12.0, "2"),
            (72.0, 400.0, 12.0, "C"),
            (250.0, 400.0, 12.0, "D"),
            (72.0, 384.0, 12.0, "3"),
            (250.0, 384.0, 12.0, "4"),
        ]);
        builder.write(&pdf);

        let markdown = ExportRequest::new(dir.join("saida.md"), ExportFormat::Markdown);
        let files = export_to_files(&pdf, 2, &markdown, &|_, _| true).unwrap();
        assert_eq!(
            files,
            [dir.join("saida_arquivos/p1-i1.png"), dir.join("saida.md")]
        );
        assert!(files.iter().all(|file| file.is_file()));

        let mut csv = ExportRequest::new(dir.join("t.csv"), ExportFormat::Csv);
        csv.pages = Some("2".into());
        let files = export_to_files(&pdf, 2, &csv, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("t-p2-t1.csv"), dir.join("t-p2-t2.csv")]);

        csv.pages = Some("3".into());
        assert_eq!(
            export_to_files(&pdf, 2, &csv, &|_, _| true).unwrap_err(),
            "Página fora de 1..=2"
        );
    }
}
