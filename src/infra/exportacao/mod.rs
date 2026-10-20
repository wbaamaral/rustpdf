// SPDX-License-Identifier: AGPL-3.0-or-later
//! Escritores dos formatos de exportação.
pub mod csv;
pub mod docx;
pub mod markdown;
pub mod odf;
pub mod odg;
pub mod odt;
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
    let with_images = (matches!(request.format, ExportFormat::Pptx | ExportFormat::Odg)
        || (request.images
            && matches!(
                request.format,
                ExportFormat::Markdown | ExportFormat::Docx | ExportFormat::Odt
            )))
        && !request.ai_optimized;
    if with_images {
        sources = sources.with_images(&PdfImages);
    }
    if request.format == ExportFormat::Pptx {
        let slides = extract_slides(&sources, pdf, &pages, request.faithful)?;
        write(output, &pptx::render(&slides)?)?;
        return Ok(written);
    }
    if request.format == ExportFormat::Odg {
        if !request.faithful {
            if let Ok(true) = try_export_odg_via_soffice(pdf, &pages, total_pages, output) {
                written.push(output.to_path_buf());
                return Ok(written);
            }
        }
        let slides = extract_slides(&sources, pdf, &pages, request.faithful)?;
        write(output, &odg::render(&slides)?)?;
        return Ok(written);
    }
    let mut document = extract(&sources, pdf, &pages)?;
    if request.ai_optimized {
        document = crate::dominio::exportacao::ia::optimize_for_ai(
            &document,
            &crate::dominio::exportacao::ia::AiOptimizationOptions::default(),
        );
    }
    match request.format {
        ExportFormat::Markdown => {
            // Imagens em "<nome>_arquivos/", ao lado do .md.
            let stem = output
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let options = markdown::MarkdownOptions {
                ai_mode: request.ai_optimized,
            };
            let rendered =
                markdown::render_with_options(&document, &format!("{stem}_arquivos"), &options);
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
        ExportFormat::Odt => {
            let options = odt::OdtOptions {
                page_breaks: request.page_breaks,
            };
            write(output, &odt::render(&document, &options)?)?;
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
        ExportFormat::Pptx | ExportFormat::Odg => unreachable!("tratado acima"),
    }
    Ok(written)
}

/// Tenta exportar para ODG vetorizado via LibreOffice Draw (`draw_pdf_import`).
/// Devolve `Ok(true)` se o LibreOffice concluiu a conversão com sucesso, ou
/// `Ok(false)` se a ferramenta não estiver disponível ou falhar.
fn try_export_odg_via_soffice(
    pdf: &Path,
    pages: &PageRange,
    total_pages: usize,
    output: &Path,
) -> Result<bool, String> {
    use crate::infra::ferramentas::tool_command;

    let work_dir = std::env::temp_dir().join(format!(
        "rustpdf_odg_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if std::fs::create_dir_all(&work_dir).is_err() {
        return Ok(false);
    }
    struct Cleaner<'a>(&'a Path);
    impl Drop for Cleaner<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0);
        }
    }
    let _cleaner = Cleaner(&work_dir);

    let pdf_to_convert = if pages.pages().len() < total_pages {
        let sub_pdf = work_dir.join("sub.pdf");
        let page_list = pages
            .pages()
            .iter()
            .map(|p| p.get().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let qpdf_status = tool_command("qpdf")
            .arg(pdf)
            .arg("--pages")
            .arg(pdf)
            .arg(&page_list)
            .arg("--")
            .arg(&sub_pdf)
            .status();
        match qpdf_status {
            Ok(status) if status.success() || status.code() == Some(3) => sub_pdf,
            _ => return Ok(false),
        }
    } else {
        pdf.to_path_buf()
    };

    let profile = format!(
        "-env:UserInstallation=file://{}",
        work_dir.join("perfil").display()
    );
    let soffice_status = tool_command("soffice")
        .arg(&profile)
        .arg("--headless")
        .arg("--infilter=draw_pdf_import")
        .arg("--convert-to")
        .arg("odg")
        .arg(&pdf_to_convert)
        .arg("--outdir")
        .arg(&work_dir)
        .status();

    match soffice_status {
        Ok(status) if status.success() => {
            let stem = pdf_to_convert
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "documento".into());
            let generated = work_dir.join(format!("{stem}.odg"));
            if generated.is_file() {
                std::fs::copy(&generated, output)
                    .map_err(|e| format!("Não foi possível gravar {}: {e}", output.display()))?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    }
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

        let docx = ExportRequest::new(dir.join("saida.docx"), ExportFormat::Docx);
        let files = export_to_files(&pdf, 2, &docx, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("saida.docx")]);

        let odt = ExportRequest::new(dir.join("saida.odt"), ExportFormat::Odt);
        let files = export_to_files(&pdf, 2, &odt, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("saida.odt")]);

        let mut xlsx = ExportRequest::new(dir.join("saida.xlsx"), ExportFormat::Xlsx);
        xlsx.pages = Some("2".into());
        let files = export_to_files(&pdf, 2, &xlsx, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("saida.xlsx")]);

        let mut pptx = ExportRequest::new(dir.join("saida.pptx"), ExportFormat::Pptx);
        let files = export_to_files(&pdf, 2, &pptx, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("saida.pptx")]);
        pptx.faithful = true;
        let files = export_to_files(&pdf, 2, &pptx, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("saida.pptx")]);

        let mut odg = ExportRequest::new(dir.join("saida.odg"), ExportFormat::Odg);
        let files = export_to_files(&pdf, 2, &odg, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("saida.odg")]);
        odg.pages = Some("1".into());
        let files = export_to_files(&pdf, 2, &odg, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("saida.odg")]);
        odg.faithful = true;
        let files = export_to_files(&pdf, 2, &odg, &|_, _| true).unwrap();
        assert_eq!(files, [dir.join("saida.odg")]);

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
