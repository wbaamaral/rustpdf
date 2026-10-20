// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{write_one_page_pdf, AppResult};
use std::fs::File;
use std::io::Read;
use std::path::Path;

fn png_dimensions(path: &Path) -> AppResult<(f64, f64)> {
    let mut file = File::open(path).map_err(|e| format!("Imagem OCR inacessível: {e}"))?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header)
        .map_err(|e| format!("Cabeçalho PNG inválido: {e}"))?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return Err("Imagem OCR não é PNG".into());
    }
    let width = u32::from_be_bytes(header[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(header[20..24].try_into().unwrap());
    if width == 0 || height == 0 {
        return Err("Imagem OCR vazia".into());
    }
    Ok((width as f64, height as f64))
}

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

pub fn write_searchable_overlay(
    image: &Path,
    destination: &Path,
    page_width: f64,
    page_height: f64,
    tsv: &str,
) -> AppResult<usize> {
    if !page_width.is_finite()
        || !page_height.is_finite()
        || page_width <= 0.0
        || page_height <= 0.0
    {
        return Err("Dimensões da página inválidas".into());
    }
    let (image_width, image_height) = png_dimensions(image)?;
    let mut stream = Vec::new();
    let mut words = 0;
    for line in tsv.lines().skip(1) {
        let columns: Vec<&str> = line.splitn(12, '\t').collect();
        if columns.len() != 12 || columns[0] != "5" {
            continue;
        }
        let word = columns[11].trim();
        if word.is_empty() {
            continue;
        }
        let (Ok(left), Ok(top), Ok(width), Ok(height)) = (
            columns[6].parse::<f64>(),
            columns[7].parse::<f64>(),
            columns[8].parse::<f64>(),
            columns[9].parse::<f64>(),
        ) else {
            continue;
        };
        if [left, top, width, height].iter().any(|v| !v.is_finite())
            || left < 0.0
            || top < 0.0
            || width <= 0.0
            || height <= 0.0
            || left + width > image_width
            || top + height > image_height
        {
            continue;
        }
        let x = left / image_width * page_width;
        let y = page_height - (top + height) / image_height * page_height;
        let font_size = (height / image_height * page_height).clamp(1.0, 200.0);
        stream.extend_from_slice(
            format!("BT /F1 {font_size:.3} Tf 3 Tr 1 0 0 1 {x:.3} {y:.3} Tm (").as_bytes(),
        );
        stream.extend_from_slice(&pdf_text(word));
        stream.extend_from_slice(b") Tj ET\n");
        words += 1;
    }
    if words == 0 {
        return Err("O OCR não encontrou palavras nesta página".into());
    }
    write_one_page_pdf(destination, page_width, page_height, &stream, true)?;
    Ok(words)
}
