// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::dominio::layout::Page;
use crate::dominio::ocr::searchable_layer;
use crate::{infra, write_pdf_objects, AppResult};
use std::path::Path;

/// PDF com uma página por item de `recognized`, cada uma só com o texto
/// reconhecido invisível sobre as posições das palavras (pontos da página).
/// É sobreposto às páginas do documento com `qpdf --overlay`.
pub fn write_searchable_overlay(recognized: &[&Page], destination: &Path) -> AppResult<()> {
    if recognized.is_empty() {
        return Err("Nenhuma página reconhecida pelo OCR".into());
    }
    if recognized.iter().any(|page| {
        !page.width.is_finite()
            || !page.height.is_finite()
            || page.width <= 0.0
            || page.height <= 0.0
    }) {
        return Err("Dimensões da página inválidas".into());
    }
    // 1 catálogo, 2 árvore de páginas, 3 fonte; depois página e conteúdo de cada uma.
    let page_object = |index: usize| 4 + 2 * index;
    let kids: Vec<String> = (0..recognized.len())
        .map(|index| format!("{} 0 R", page_object(index)))
        .collect();
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            recognized.len()
        )
        .into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ];
    for (index, page) in recognized.iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                page.width,
                page.height,
                page_object(index) + 1
            )
            .into_bytes(),
        );
        objects.push(infra::pdf_objetos::stream("", &searchable_layer(page)));
    }
    write_pdf_objects(destination, &objects)
}
