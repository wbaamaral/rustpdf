// SPDX-License-Identifier: AGPL-3.0-or-later
//! Regras do OCR pesquisável: idioma aceito e camada de texto invisível.

use crate::dominio::fonte::FontCodec;
use crate::dominio::layout::Page;

/// Idioma no formato do Tesseract (`por`, `por+eng`, `chi_sim`): letras,
/// dígitos, `_` e `-`, partes separadas por `+`, nenhuma vazia.
pub fn valid_language(language: &str) -> bool {
    !language.is_empty()
        && language.split('+').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        })
}

/// Texto para uma string literal do PDF com Helvetica em /WinAnsiEncoding;
/// caracteres fora da tabela viram `?`.
pub fn pdf_text(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4];
    for character in text.chars() {
        let code = FontCodec::WinAnsi
            .encode(character.encode_utf8(&mut buffer))
            .map_or(b'?', |encoded| encoded[0]);
        if matches!(code, b'(' | b')' | b'\\') {
            bytes.push(b'\\');
        }
        bytes.push(code);
    }
    bytes
}

/// Conteúdo da página com cada palavra reconhecida em texto invisível
/// (modo 3) sobre a posição dela. `recognized` está em **pontos** da página
/// (origem no topo); a fonte `/F1` é a Helvetica.
pub fn searchable_layer(recognized: &Page) -> Vec<u8> {
    let mut stream = Vec::new();
    for word in recognized.words() {
        let rect = word.rect;
        let x = rect.left();
        let y = recognized.height - rect.bottom();
        let font_size = rect.height().clamp(1.0, 200.0);
        stream.extend_from_slice(
            format!("BT /F1 {font_size:.3} Tf 3 Tr 1 0 0 1 {x:.3} {y:.3} Tm (").as_bytes(),
        );
        stream.extend_from_slice(&pdf_text(&word.text));
        stream.extend_from_slice(b") Tj ET\n");
    }
    stream
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::geometria::LayoutRect;
    use crate::dominio::layout::{Block, Line, Word};

    #[test]
    fn languages_follow_the_tesseract_format() {
        for valid in ["por", "por+eng", "chi_sim", "osd", "Latin-1"] {
            assert!(valid_language(valid), "{valid}");
        }
        for invalid in [
            "", "+", "por+", "+eng", "por eng", "p;rm", "../x", "por++eng",
        ] {
            assert!(!valid_language(invalid), "{invalid}");
        }
    }

    #[test]
    fn pdf_text_uses_the_whole_win_ansi_table() {
        assert_eq!(pdf_text("• ™ Œ (x)"), b"\x95 \x99 \x8c \\(x\\)".to_vec());
        assert_eq!(pdf_text("中"), b"?".to_vec());
    }

    fn word(text: &str, rect: LayoutRect) -> Word {
        Word {
            text: text.into(),
            rect,
        }
    }

    #[test]
    fn layer_places_invisible_words_on_the_baseline() {
        let page = Page {
            width: 600.0,
            height: 800.0,
            blocks: vec![Block {
                lines: vec![Line {
                    words: vec![
                        word("Olá", LayoutRect::new(72.0, 100.0, 120.0, 112.0)),
                        word("(a)", LayoutRect::new(130.0, 100.0, 150.0, 100.5)),
                    ],
                }],
            }],
        };
        let layer = String::from_utf8_lossy(&searchable_layer(&page)).into_owned();
        assert_eq!(
            layer,
            "BT /F1 12.000 Tf 3 Tr 1 0 0 1 72.000 688.000 Tm (Ol\u{fffd}) Tj ET\n\
             BT /F1 1.000 Tf 3 Tr 1 0 0 1 130.000 699.500 Tm (\\(a\\)) Tj ET\n"
        );
        let empty = Page {
            blocks: Vec::new(),
            ..page
        };
        assert!(searchable_layer(&empty).is_empty());
    }
}
