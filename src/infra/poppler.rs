// SPDX-License-Identifier: AGPL-3.0-or-later
//! Poppler (`pdftotext`, `pdftoppm`) como adaptador das portas `LayoutSource`
//! e `Renderer`, e leitura do layout (`pdftotext -bbox-layout`).

use crate::app::erros::ToolError;
use crate::app::portas::{LayoutSource, Renderer};
use crate::dominio::geometria::LayoutRect;
use crate::dominio::layout::{Block, Line, Page, Word};
use crate::infra::ferramentas::run;
use crate::infra::xml;
use std::ffi::OsString;
use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

/// Poppler instalado no sistema (ou embutido no pacote Windows).
#[derive(Debug, Clone, Copy, Default)]
pub struct Poppler;

/// `-f first -l last`.
fn page_range(first: usize, last: usize) -> Vec<OsString> {
    vec![
        "-f".into(),
        first.to_string().into(),
        "-l".into(),
        last.to_string().into(),
    ]
}

/// Largura exata em pixels; altura proporcional à página.
fn scale_to_width(width: u32) -> Vec<OsString> {
    vec![
        "-scale-to-x".into(),
        width.to_string().into(),
        "-scale-to-y".into(),
        "-1".into(),
        "-png".into(),
    ]
}

impl LayoutSource for Poppler {
    fn layout(&self, pdf: &Path, page: usize) -> Result<Page, ToolError> {
        let mut args = page_range(page, page);
        args.extend(["-bbox-layout".into(), pdf.into(), "-".into()]);
        let xml = run("pdftotext", &args)?;
        parse_bbox_layout(&xml)
            .into_iter()
            .next()
            .ok_or_else(|| ToolError::InvalidOutput {
                program: "pdftotext".into(),
                detail: format!("layout sem a página {page}"),
            })
    }

    fn text(&self, pdf: &Path, page: usize) -> Result<String, ToolError> {
        let mut args = page_range(page, page);
        args.extend(["-layout".into(), pdf.into(), "-".into()]);
        run("pdftotext", &args)
    }
}

impl Renderer for Poppler {
    fn render_page(
        &self,
        pdf: &Path,
        page: usize,
        width: u32,
        destination: &Path,
    ) -> Result<(), ToolError> {
        // O pdftoppm acrescenta ".png" ao prefixo.
        let prefix = destination.with_extension("");
        let mut args = page_range(page, page);
        args.push("-singlefile".into());
        args.extend(scale_to_width(width));
        args.extend([pdf.into(), prefix.clone().into_os_string()]);
        run("pdftoppm", &args)?;
        // `with_extension` trocaria o “.1” de `pagina.1` em vez de acrescentar.
        let mut produced = prefix.into_os_string();
        produced.push(".png");
        let produced = std::path::PathBuf::from(produced);
        if produced != destination {
            crate::replace_file(&produced, destination).map_err(|e| ToolError::Failed {
                program: "pdftoppm".into(),
                detail: e.to_string(),
            })?;
        }
        Ok(())
    }

    fn render_region(
        &self,
        pdf: &Path,
        page: usize,
        region: LayoutRect,
        dpi: u32,
    ) -> Result<Vec<u8>, ToolError> {
        let pixels = |points: f64| ((points * f64::from(dpi) / 72.0).round() as i64).to_string();
        // `-W 0`/`-H 0` no pdftoppm significa “até a borda da página”.
        let extent = |points: f64| {
            ((points * f64::from(dpi) / 72.0).round() as i64)
                .max(1)
                .to_string()
        };
        let failure = |e: std::io::Error| ToolError::Failed {
            program: "pdftoppm".into(),
            detail: e.to_string(),
        };
        let directory = crate::infra::arquivos::private_temp_dir("regiao").map_err(failure)?;
        let prefix = directory.join("regiao");
        let mut args = page_range(page, page);
        args.extend([
            "-singlefile".into(),
            "-png".into(),
            "-r".into(),
            dpi.to_string().into(),
            "-x".into(),
            pixels(region.left()).into(),
            "-y".into(),
            pixels(region.top()).into(),
            "-W".into(),
            extent(region.width()).into(),
            "-H".into(),
            extent(region.height()).into(),
        ]);
        args.extend([pdf.into(), prefix.clone().into_os_string()]);
        let result = run("pdftoppm", &args)
            .and_then(|_| fs::read(prefix.with_extension("png")).map_err(failure));
        let _ = fs::remove_dir_all(&directory);
        result
    }

    fn render_pages(
        &self,
        pdf: &Path,
        pages: RangeInclusive<usize>,
        width: u32,
        directory: &Path,
    ) -> Result<Vec<(usize, PathBuf)>, ToolError> {
        let (first, last) = (*pages.start(), *pages.end());
        let stem = format!("raw-{first}-{last}");
        let mut args = page_range(first, last);
        args.extend(scale_to_width(width));
        args.extend([pdf.into(), directory.join(&stem).into_os_string()]);
        run("pdftoppm", &args)?;
        let failure = |e: std::io::Error| ToolError::Failed {
            program: "pdftoppm".into(),
            detail: e.to_string(),
        };
        // O pdftoppm numera com largura variável (raw-1-9-1.png, raw-1-9-01.png…).
        let prefix = format!("{stem}-");
        let mut produced = Vec::new();
        for entry in fs::read_dir(directory).map_err(failure)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(number) = name
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(".png"))
                .and_then(|number| number.parse::<usize>().ok())
            else {
                continue;
            };
            let target = directory.join(format!("p-{number:05}.png"));
            crate::replace_file(&entry.path(), &target).map_err(failure)?;
            produced.push((number, target));
        }
        produced.sort();
        Ok(produced)
    }
}

/// Páginas do XML de `pdftotext -bbox-layout`, na ordem do documento.
/// Palavras sem as quatro coordenadas são ignoradas.
pub fn parse_bbox_layout(source: &str) -> Vec<Page> {
    let mut pages: Vec<Page> = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>').map(|end| start + end) else {
            break;
        };
        let tag = &rest[start + 1..end];
        rest = &rest[end + 1..];
        let name = tag.split_whitespace().next().unwrap_or("");
        match name {
            "page" => pages.push(Page {
                width: attribute(tag, "width").unwrap_or(0.0),
                height: attribute(tag, "height").unwrap_or(0.0),
                blocks: Vec::new(),
            }),
            "block" => {
                if let Some(page) = pages.last_mut() {
                    page.blocks.push(Block { lines: Vec::new() });
                }
            }
            "line" => {
                if let Some(block) = pages.last_mut().and_then(|page| page.blocks.last_mut()) {
                    block.lines.push(Line { words: Vec::new() });
                }
            }
            "word" => {
                let text_end = rest.find("</word>").unwrap_or(rest.len());
                let text = xml::unescape(&rest[..text_end]);
                rest = &rest[text_end..];
                let line = pages
                    .last_mut()
                    .and_then(|page| page.blocks.last_mut())
                    .and_then(|block| block.lines.last_mut());
                if let (Some(line), Some(rect)) = (line, word_rect(tag)) {
                    line.words.push(Word { text, rect });
                }
            }
            _ => {}
        }
    }
    pages
}

fn word_rect(tag: &str) -> Option<LayoutRect> {
    Some(LayoutRect::new(
        attribute(tag, "xMin")?,
        attribute(tag, "yMin")?,
        attribute(tag, "xMax")?,
        attribute(tag, "yMax")?,
    ))
}

/// Valor numérico de `name="…"` dentro da tag.
fn attribute(tag: &str, name: &str) -> Option<f64> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = start + tag[start..].find('"')?;
    tag[start..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::geometria::LayoutRect;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN">
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title></title></head>
<body>
<doc>
  <page width="595.000000" height="842.000000">
    <flow>
      <block xMin="100.0" yMin="527.0" xMax="198.0" yMax="560.0">
        <line xMin="100.0" yMin="527.0" xMax="198.0" yMax="546.0">
          <word xMin="100.0" yMin="527.0" xMax="131.0" yMax="546.0">Ola</word>
          <word xMin="136.0" yMin="527.0" xMax="198.0" yMax="546.0">mundo &amp; cia</word>
        </line>
        <line xMin="100.0" yMin="548.0" xMax="120.0" yMax="560.0">
          <word xMin="100.0" yMin="548.0" xMax="120.0" yMax="560.0">fim</word>
          <word yMin="548.0">sem caixa</word>
        </line>
      </block>
      <block xMin="300.0" yMin="10.0" xMax="320.0" yMax="20.0">
        <line xMin="300.0" yMin="10.0" xMax="320.0" yMax="20.0">
          <word xMin="300.0" yMin="10.0" xMax="320.0" yMax="20.0">topo</word>
        </line>
      </block>
    </flow>
  </page>
  <page width="300" height="400">
  </page>
</doc>
</body>
</html>"#;

    #[test]
    fn reads_pages_blocks_lines_and_words() {
        let pages = parse_bbox_layout(XML);
        assert_eq!(pages.len(), 2);
        let first = &pages[0];
        assert_eq!((first.width, first.height), (595.0, 842.0));
        assert_eq!(first.blocks.len(), 2);
        assert_eq!(first.blocks[0].lines.len(), 2);
        let line = &first.blocks[0].lines[0];
        assert_eq!(line.text(), "Ola mundo & cia");
        assert_eq!(
            line.words[1].rect,
            LayoutRect::new(136.0, 527.0, 198.0, 546.0)
        );
        assert_eq!(
            first.blocks[0].lines[1].text(),
            "fim",
            "palavra sem caixa é ignorada"
        );
        assert_eq!(first.blocks[1].lines[0].text(), "topo");
        assert_eq!((pages[1].width, pages[1].height), (300.0, 400.0));
        assert!(pages[1].blocks.is_empty());
    }

    #[test]
    fn empty_or_foreign_input_has_no_pages() {
        assert!(parse_bbox_layout("").is_empty());
        assert!(parse_bbox_layout("<html><body>nada</body></html>").is_empty());
    }
}
