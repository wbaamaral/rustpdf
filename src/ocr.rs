// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::dominio::layout::Page;
use crate::{write_one_page_pdf, AppResult};
use std::path::Path;

pub(crate) fn pdf_text(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for character in text.chars() {
        let code = match character {
            '€' => 0x80,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '–' => 0x96,
            '—' => 0x97,
            '…' => 0x85,
            _ if (character as u32) <= 0xff && !matches!(character as u32, 0x80..=0x9f) => {
                character as u8
            }
            _ => b'?',
        };
        if matches!(code, b'(' | b')' | b'\\') {
            bytes.push(b'\\');
        }
        bytes.push(code);
    }
    bytes
}

/// PDF de uma página com o texto reconhecido invisível (modo 3) sobre as
/// posições das palavras. `recognized` vem do OCR em pixels da imagem da
/// página; as caixas são levadas para pontos de `page_width` × `page_height`.
pub fn write_searchable_overlay(
    recognized: &Page,
    destination: &Path,
    page_width: f64,
    page_height: f64,
) -> AppResult<usize> {
    if !page_width.is_finite()
        || !page_height.is_finite()
        || page_width <= 0.0
        || page_height <= 0.0
    {
        return Err("Dimensões da página inválidas".into());
    }
    let scale_x = page_width / recognized.width;
    let scale_y = page_height / recognized.height;
    let mut stream = Vec::new();
    let mut words = 0;
    for word in recognized.words() {
        let rect = word.rect;
        let x = rect.left() * scale_x;
        let y = page_height - rect.bottom() * scale_y;
        let font_size = (rect.height() * scale_y).clamp(1.0, 200.0);
        stream.extend_from_slice(
            format!("BT /F1 {font_size:.3} Tf 3 Tr 1 0 0 1 {x:.3} {y:.3} Tm (").as_bytes(),
        );
        stream.extend_from_slice(&pdf_text(&word.text));
        stream.extend_from_slice(b") Tj ET\n");
        words += 1;
    }
    if words == 0 {
        return Err("O OCR não encontrou palavras nesta página".into());
    }
    write_one_page_pdf(destination, page_width, page_height, &stream, true)?;
    Ok(words)
}
