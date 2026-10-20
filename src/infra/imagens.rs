// SPDX-License-Identifier: AGPL-3.0-or-later
//! Imagens e traços desenhados na página, pelo interpretador do conteúdo (qpdf).
use crate::app::erros::ToolError;
use crate::app::portas::{ImageSource, RulingSource};
use crate::dominio::geometria::{PdfRect, Segment};
use std::path::Path;

/// Localiza os desenhos de imagem pelo interpretador de `content.rs`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfImages;

impl ImageSource for PdfImages {
    fn image_boxes(&self, pdf: &Path, page: usize) -> Result<Vec<PdfRect>, ToolError> {
        let placements =
            crate::content::image_placements(pdf, page).map_err(|detail| ToolError::Failed {
                program: "qpdf".into(),
                detail,
            })?;
        Ok(placements
            .iter()
            .map(|image| {
                PdfRect::new(
                    image.x,
                    image.y,
                    image.x + image.width,
                    image.y + image.height,
                )
            })
            .collect())
    }
}

/// Traços retos pintados, pelo interpretador de `content.rs`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfRulings;

impl RulingSource for PdfRulings {
    fn segments(&self, pdf: &Path, page: usize) -> Result<Vec<Segment>, ToolError> {
        crate::content::page_segments(pdf, page).map_err(|detail| ToolError::Failed {
            program: "qpdf".into(),
            detail,
        })
    }
}
