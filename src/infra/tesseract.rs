// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tesseract como adaptador da porta `OcrEngine`, e leitura do TSV.

use crate::app::erros::ToolError;
use crate::app::portas::OcrEngine;
use crate::dominio::geometria::LayoutRect;
use crate::dominio::layout::{Block, Line, Page, Word};
use crate::infra::ferramentas::run;
use std::ffi::OsString;
use std::path::Path;

/// Tesseract instalado no sistema (ou embutido no pacote Windows).
#[derive(Debug, Clone, Copy, Default)]
pub struct Tesseract;

fn arguments(image: &Path, language: Option<&str>) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![image.into(), "stdout".into()];
    if let Some(language) = language {
        args.extend(["-l".into(), language.into()]);
    }
    args
}

impl OcrEngine for Tesseract {
    fn text(&self, png: &[u8], language: Option<&str>) -> Result<String, ToolError> {
        with_image(png, |image| run("tesseract", &arguments(image, language)))
    }

    fn words(&self, png: &[u8], language: &str) -> Result<Page, ToolError> {
        let tsv = with_image(png, |image| {
            let mut args = arguments(image, Some(language));
            args.push("tsv".into());
            run("tesseract", &args)
        })?;
        parse_tsv(&tsv).ok_or_else(|| ToolError::InvalidOutput {
            program: "tesseract".into(),
            detail: "TSV sem a linha da página".into(),
        })
    }
}

/// Grava o PNG numa pasta temporária privada só durante `work`.
fn with_image<T>(
    png: &[u8],
    work: impl FnOnce(&Path) -> Result<T, ToolError>,
) -> Result<T, ToolError> {
    let failure = |e: std::io::Error| ToolError::Failed {
        program: "tesseract".into(),
        detail: e.to_string(),
    };
    let directory = crate::infra::arquivos::private_temp_dir("ocr").map_err(failure)?;
    let image = directory.join("pagina.png");
    let result = std::fs::write(&image, png)
        .map_err(failure)
        .and_then(|_| work(&image));
    let _ = std::fs::remove_dir_all(&directory);
    result
}

/// Layout do TSV do Tesseract, em **pixels** da imagem (origem no topo).
/// A linha de nível 1 dá o tamanho da imagem; sem ela, `None`. Palavras
/// vazias, malformadas ou fora da imagem são ignoradas.
pub fn parse_tsv(tsv: &str) -> Option<Page> {
    let mut page: Option<Page> = None;
    for row in tsv.lines().skip(1) {
        let columns: Vec<&str> = row.splitn(12, '\t').collect();
        if columns.len() != 12 {
            continue;
        }
        let numbers: Option<Vec<f64>> = columns[6..10]
            .iter()
            .map(|value| value.parse::<f64>().ok().filter(|v| v.is_finite()))
            .collect();
        let Some(&[left, top, width, height]) = numbers.as_deref() else {
            continue;
        };
        match columns[0] {
            "1" => {
                page = Some(Page {
                    width,
                    height,
                    blocks: Vec::new(),
                })
            }
            "2" => {
                if let Some(page) = page.as_mut() {
                    page.blocks.push(Block { lines: Vec::new() });
                }
            }
            "4" => {
                if let Some(block) = page.as_mut().and_then(|page| page.blocks.last_mut()) {
                    block.lines.push(Line { words: Vec::new() });
                }
            }
            "5" => {
                let Some(page) = page.as_mut() else {
                    continue;
                };
                let text = columns[11].trim();
                let inside = left >= 0.0
                    && top >= 0.0
                    && width > 0.0
                    && height > 0.0
                    && left + width <= page.width
                    && top + height <= page.height;
                if text.is_empty() || !inside {
                    continue;
                }
                if let Some(line) = page
                    .blocks
                    .last_mut()
                    .and_then(|block| block.lines.last_mut())
                {
                    line.words.push(Word {
                        text: text.into(),
                        rect: LayoutRect::new(left, top, left + width, top + height),
                    });
                }
            }
            _ => {}
        }
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::geometria::LayoutRect;

    const TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext
1\t1\t0\t0\t0\t0\t0\t0\t1200\t1699\t-1\t
2\t1\t1\t0\t0\t0\t152\t226\t569\t62\t-1\t
3\t1\t1\t1\t0\t0\t152\t226\t569\t62\t-1\t
4\t1\t1\t1\t1\t0\t152\t226\t569\t62\t-1\t
5\t1\t1\t1\t1\t1\t152\t226\t371\t62\t92.1\tRUSTPDF
5\t1\t1\t1\t1\t2\t549\t232\t172\t56\t91.9\ttesta
2\t1\t2\t0\t0\t0\t148\t428\t345\t78\t-1\t
4\t1\t2\t1\t1\t0\t148\t428\t345\t78\t-1\t
5\t1\t2\t1\t1\t1\t148\t428\t173\t62\t88.8\tOCR
5\t1\t2\t1\t1\t2\t349\t429\t144\t77\t66.7\t   
5\t1\t2\t1\t1\t3\t1190\t429\t144\t77\t66.7\tfora
5\t1\t2\t1\t1\t4\tx\t429\t144\t77\t66.7\tlixo
5\t1\t2\t1\t1";

    #[test]
    fn reads_blocks_lines_and_words_in_pixels() {
        let page = parse_tsv(TSV).unwrap();
        assert_eq!((page.width, page.height), (1200.0, 1699.0));
        assert_eq!(page.blocks.len(), 2);
        let lines: Vec<String> = page.lines().map(|line| line.text()).collect();
        assert_eq!(lines, ["RUSTPDF testa", "OCR"]);
        let first = &page.blocks[0].lines[0].words[0];
        assert_eq!(first.rect, LayoutRect::new(152.0, 226.0, 523.0, 288.0));
    }

    #[test]
    fn skips_blank_malformed_and_out_of_image_words() {
        let page = parse_tsv(TSV).unwrap();
        let words: Vec<&str> = page.words().map(|word| word.text.as_str()).collect();
        assert_eq!(words, ["RUSTPDF", "testa", "OCR"]);
    }

    #[test]
    fn needs_the_page_row_for_the_image_size() {
        assert!(parse_tsv("").is_none());
        assert!(parse_tsv("level\tpage_num\n5\t1\t1\t1\t1\t1\t1\t1\t1\t1\t1\tx").is_none());
    }
}
