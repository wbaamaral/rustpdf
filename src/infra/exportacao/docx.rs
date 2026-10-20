// SPDX-License-Identifier: AGPL-3.0-or-later
//! Documento exportado → Word (WordprocessingML): títulos com estilo,
//! parágrafos, listas com numeração do Word, tabelas e tamanho da página.

use super::ooxml::{Package, IMAGE, OFFICE_DOCUMENT, STYLES, XML_HEADER};
use crate::dominio::exportacao::modelo::{Block, Document};
use crate::infra::xml::escape;

const MAIN: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
/// EMU (unidade do DrawingML) por ponto.
const EMU_PER_POINT: f64 = 12_700.0;
const NUMBERING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
/// Lista com marcadores (`numId` fixo).
const BULLETS: usize = 1;
/// Margens de 2 cm, em vigésimos de ponto.
const MARGIN: u32 = 1134;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocxOptions {
    /// Quebra de página entre as páginas do PDF (padrão: texto corrido).
    pub page_breaks: bool,
}

/// Documento `.docx`.
pub fn render(document: &Document, options: &DocxOptions) -> Result<Vec<u8>, String> {
    package(document, options).finish()
}

/// O pacote antes do ZIP (testável parte a parte).
pub fn package(document: &Document, options: &DocxOptions) -> Package {
    let (width, height) = document
        .pages
        .first()
        .map(|page| page.size)
        .unwrap_or((595.0, 842.0));
    let mut writer = BodyWriter {
        text_width: width - 2.0 * f64::from(MARGIN) / 20.0,
        ..BodyWriter::default()
    };
    for (index, page) in document.pages.iter().enumerate() {
        if options.page_breaks && index > 0 {
            writer
                .body
                .push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
            writer.end_sequence();
        }
        for block in &page.blocks {
            writer.block(block);
        }
    }
    if writer.body.ends_with("</w:tbl>") {
        writer.body.push_str("<w:p/>"); // o corpo não termina em tabela
    }
    let orientation = if width > height {
        " w:orient=\"landscape\""
    } else {
        ""
    };
    let section = format!(
        "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"{orientation}/><w:pgMar w:top=\"{MARGIN}\" w:right=\"{MARGIN}\" w:bottom=\"{MARGIN}\" w:left=\"{MARGIN}\" w:header=\"709\" w:footer=\"709\" w:gutter=\"0\"/></w:sectPr>",
        twips(width),
        twips(height)
    );
    let mut package = Package::new("Documento exportado pelo RustPDF");
    let main = "word/document.xml";
    package.add_part(
        main,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        format!(
            "{XML_HEADER}<w:document xmlns:w=\"{MAIN}\" xmlns:r=\"{RELATIONSHIPS}\" xmlns:wp=\"{DRAWING}\"><w:body>{}{section}</w:body></w:document>",
            writer.body
        )
        .into_bytes(),
    );
    package.add_part(
        "word/styles.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
        format!("{XML_HEADER}{}", styles()).into_bytes(),
    );
    package.add_part(
        "word/numbering.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
        format!("{XML_HEADER}{}", numbering(&writer.sequences)).into_bytes(),
    );
    for (index, png) in writer.images.iter().enumerate() {
        let number = index + 1;
        package.add_part(
            &format!("word/media/image{number}.png"),
            "image/png",
            png.clone(),
        );
        package.relate_as(
            Some(main),
            &format!("rIdImagem{number}"),
            IMAGE,
            &format!("media/image{number}.png"),
        );
    }
    package.relate(Some(main), STYLES, "styles.xml");
    package.relate(Some(main), NUMBERING, "numbering.xml");
    package.relate(None, OFFICE_DOCUMENT, main);
    package
}

/// Pontos → vigésimos de ponto (twips).
fn twips(points: f64) -> u32 {
    (points * 20.0).round() as u32
}

/// Corpo do documento em montagem, com as sequências numeradas: cada uma
/// vira um `w:num` próprio que recomeça no número do primeiro item.
#[derive(Default)]
struct BodyWriter {
    body: String,
    /// Número inicial de cada sequência; o `numId` é `BULLETS + 1 + índice`.
    sequences: Vec<u32>,
    in_sequence: bool,
    /// Imagens na ordem do texto (`word/media/image<n>.png`).
    images: Vec<Vec<u8>>,
    /// Largura útil da página (entre as margens), em pontos.
    text_width: f64,
}

impl BodyWriter {
    fn end_sequence(&mut self) {
        self.in_sequence = false;
    }

    fn block(&mut self, block: &Block) {
        let numbered = matches!(block, Block::ListItem { marker, .. } if number(marker).is_some());
        if !numbered {
            self.end_sequence();
        }
        match block {
            Block::Heading { level, text } => {
                let style = format!("<w:pStyle w:val=\"Heading{}\"/>", level.clamp(&1, &3));
                self.paragraph(&style, &run(text, false));
            }
            Block::Paragraph { text } => self.paragraph("", &run(text, false)),
            Block::ListItem {
                marker,
                level,
                text,
            } => {
                let level = u32::from(*level).min(5);
                let numbering = match number(marker) {
                    Some(start) => {
                        if !self.in_sequence {
                            self.sequences.push(start);
                            self.in_sequence = true;
                        }
                        Some(BULLETS + self.sequences.len())
                    }
                    None if marker.is_empty() => Some(BULLETS),
                    None => None,
                };
                let style = "<w:pStyle w:val=\"ListParagraph\"/>";
                match numbering {
                    Some(id) => self.paragraph(
                        &format!(
                            "{style}<w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{id}\"/></w:numPr>"
                        ),
                        &run(text, false),
                    ),
                    // Rótulo sem numeração do Word (`b)`, `iv.`): fica no texto.
                    None => {
                        let indent = if level > 0 {
                            format!("<w:ind w:left=\"{}\"/>", 720 * (level + 1))
                        } else {
                            String::new()
                        };
                        self.paragraph(&format!("{style}{indent}"), &run(&format!("{marker} {text}"), false));
                    }
                }
            }
            Block::Table { rows } => self.table(rows),
            Block::Image { png, size } => self.image(png, *size),
        }
    }

    /// Imagem em linha, no tamanho da página, reduzida à largura útil.
    fn image(&mut self, png: &[u8], (width, height): (f64, f64)) {
        self.images.push(png.to_vec());
        let number = self.images.len();
        let scale = if width > self.text_width {
            self.text_width / width
        } else {
            1.0
        };
        let cx = (width * scale * EMU_PER_POINT).round() as u64;
        let cy = (height * scale * EMU_PER_POINT).round() as u64;
        self.body.push_str(&format!(
            concat!(
                "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">",
                "<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Imagem {n}\"/>",
                "<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
                "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
                "<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
                "<pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"image{n}.png\"/><pic:cNvPicPr/></pic:nvPicPr>",
                "<pic:blipFill><a:blip r:embed=\"rIdImagem{n}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>",
                "<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>",
                "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>",
                "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
            ),
            cx = cx,
            cy = cy,
            n = number,
        ));
    }

    fn paragraph(&mut self, properties: &str, runs: &str) {
        if properties.is_empty() {
            self.body.push_str(&format!("<w:p>{runs}</w:p>"));
        } else {
            self.body
                .push_str(&format!("<w:p><w:pPr>{properties}</w:pPr>{runs}</w:p>"));
        }
    }

    fn table(&mut self, rows: &[Vec<String>]) {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        self.body.push_str(
            "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid>",
        );
        self.body.push_str(&"<w:gridCol/>".repeat(columns));
        self.body.push_str("</w:tblGrid>");
        for (index, row) in rows.iter().enumerate() {
            let header = index == 0;
            self.body.push_str("<w:tr>");
            if header {
                self.body.push_str("<w:trPr><w:tblHeader/></w:trPr>");
            }
            for column in 0..columns {
                let text = row.get(column).map(String::as_str).unwrap_or("");
                if text.is_empty() {
                    self.body.push_str("<w:tc><w:p/></w:tc>");
                } else {
                    self.body
                        .push_str(&format!("<w:tc><w:p>{}</w:p></w:tc>", run(text, header)));
                }
            }
            self.body.push_str("</w:tr>");
        }
        self.body.push_str("</w:tbl>");
    }
}

/// Trecho de texto (`bold`: negrito).
fn run(text: &str, bold: bool) -> String {
    let properties = if bold { "<w:rPr><w:b/></w:rPr>" } else { "" };
    format!(
        "<w:r>{properties}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
        escape(text)
    )
}

/// Número de um rótulo `3.` ou `3)`.
fn number(marker: &str) -> Option<u32> {
    marker
        .strip_suffix('.')
        .or_else(|| marker.strip_suffix(')'))?
        .parse()
        .ok()
}

fn styles() -> String {
    let heading = |level: u32, size: u32| {
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading{level}\"><w:name w:val=\"heading {level}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"{}\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"{size}\"/></w:rPr></w:style>",
            level - 1
        )
    };
    format!(
        concat!(
            "<w:styles xmlns:w=\"{main}\">",
            "<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/><w:sz w:val=\"22\"/><w:lang w:val=\"pt-BR\"/></w:rPr></w:rPrDefault>",
            "<w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"259\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>",
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>",
            "{h1}{h2}{h3}",
            "<w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"0\"/><w:ind w:left=\"720\"/></w:pPr></w:style>",
            "<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/><w:tblPr><w:tblInd w:w=\"0\" w:type=\"dxa\"/><w:tblCellMar><w:left w:w=\"108\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr></w:style>",
            "<w:style w:type=\"table\" w:styleId=\"TableGrid\"><w:name w:val=\"Table Grid\"/><w:basedOn w:val=\"TableNormal\"/><w:pPr><w:spacing w:after=\"0\"/></w:pPr><w:tblPr><w:tblBorders>",
            "<w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
            "<w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
            "<w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>",
            "</w:tblBorders></w:tblPr></w:style>",
            "</w:styles>"
        ),
        main = MAIN,
        h1 = heading(1, 32),
        h2 = heading(2, 28),
        h3 = heading(3, 24),
    )
}

/// Definições de numeração: 0 = marcadores, 1 = decimal; `numId` 1 usa os
/// marcadores e cada sequência numerada ganha o seu, recomeçando no início dela.
fn numbering(sequences: &[u32]) -> String {
    let levels = |decimal: bool| -> String {
        (0..6u32)
            .map(|level| {
                let (format, text) = if decimal {
                    ("decimal", format!("%{}.", level + 1))
                } else {
                    ("bullet", ["•", "◦", "▪"][level as usize % 3].to_string())
                };
                format!(
                    "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/><w:lvlText w:val=\"{text}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
                    720 * (level + 1)
                )
            })
            .collect()
    };
    let mut xml = format!(
        "<w:numbering xmlns:w=\"{MAIN}\"><w:abstractNum w:abstractNumId=\"0\">{}</w:abstractNum><w:abstractNum w:abstractNumId=\"1\">{}</w:abstractNum>",
        levels(false),
        levels(true)
    );
    xml.push_str(&format!(
        "<w:num w:numId=\"{BULLETS}\"><w:abstractNumId w:val=\"0\"/></w:num>"
    ));
    for (index, start) in sequences.iter().enumerate() {
        xml.push_str(&format!(
            "<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"{start}\"/></w:lvlOverride></w:num>",
            BULLETS + 1 + index
        ));
    }
    xml.push_str("</w:numbering>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::exportacao::modelo::{Block, Document, PageContent, A4};

    fn item(marker: &str, level: u8, text: &str) -> Block {
        Block::ListItem {
            marker: marker.into(),
            level,
            text: text.into(),
        }
    }

    fn document() -> Document {
        Document {
            pages: vec![
                PageContent {
                    number: 1,
                    size: A4,
                    blocks: vec![
                        Block::Heading {
                            level: 1,
                            text: "Relatório".into(),
                        },
                        Block::Paragraph {
                            text: "Texto & <mais>".into(),
                        },
                        item("", 0, "Ponto"),
                        item("", 1, "Subponto"),
                        item("3.", 0, "Terceiro"),
                        item("4.", 0, "Quarto"),
                        Block::Paragraph {
                            text: "Entre listas.".into(),
                        },
                        item("1)", 0, "Outra lista"),
                        item("b)", 0, "Letra"),
                    ],
                },
                PageContent {
                    number: 2,
                    size: A4,
                    blocks: vec![Block::Table {
                        rows: vec![
                            vec!["Item".into(), "Preço".into()],
                            vec!["Lápis".into(), "".into()],
                        ],
                    }],
                },
            ],
        }
    }

    fn text(files: &[(String, Vec<u8>)], path: &str) -> String {
        let (_, data) = files.iter().find(|(name, _)| name == path).expect(path);
        String::from_utf8(data.clone()).unwrap()
    }

    fn body(options: &DocxOptions) -> String {
        text(
            &package(&document(), options).into_files(),
            "word/document.xml",
        )
    }

    #[test]
    fn headings_and_paragraphs() {
        let xml = body(&DocxOptions::default());
        assert!(xml.contains(r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t xml:space="preserve">Relatório</w:t></w:r></w:p>"#));
        assert!(xml.contains(r#"<w:t xml:space="preserve">Texto &amp; &lt;mais&gt;</w:t>"#));
    }

    #[test]
    fn lists_use_word_numbering_and_restart_per_sequence() {
        let xml = body(&DocxOptions::default());
        assert!(
            xml.contains(r#"<w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr>"#),
            "marcador, nível 1"
        );
        assert!(xml.contains(r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t xml:space="preserve">Terceiro"#));
        assert!(xml.contains(r#"<w:numId w:val="3"/></w:numPr></w:pPr><w:r><w:t xml:space="preserve">Outra lista"#), "nova sequência");
        assert!(xml.contains(
            r#"<w:pStyle w:val="ListParagraph"/></w:pPr><w:r><w:t xml:space="preserve">b) Letra"#
        ));
        let numbering = text(
            &package(&document(), &DocxOptions::default()).into_files(),
            "word/numbering.xml",
        );
        assert!(numbering.contains(r#"<w:num w:numId="2"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/></w:lvlOverride></w:num>"#));
        assert!(numbering.contains(r#"<w:num w:numId="3"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride></w:num>"#));
    }

    #[test]
    fn tables_with_repeating_bold_header() {
        let xml = body(&DocxOptions::default());
        assert!(xml.contains(r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>"#));
        assert!(xml.contains(r#"<w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Item</w:t>"#));
        assert!(
            xml.contains(r#"<w:tc><w:p/></w:tc>"#),
            "célula vazia ainda tem parágrafo"
        );
    }

    #[test]
    fn page_size_and_optional_page_breaks() {
        let flowing = body(&DocxOptions::default());
        assert!(flowing.contains(r#"<w:pgSz w:w="11900" w:h="16840"/>"#));
        assert!(!flowing.contains(r#"w:type="page""#));
        let broken = body(&DocxOptions { page_breaks: true });
        assert_eq!(
            broken.matches(r#"<w:br w:type="page"/>"#).count(),
            1,
            "entre as 2 páginas"
        );
    }

    #[test]
    fn package_has_styles_numbering_and_main_document() {
        let files = package(&document(), &DocxOptions::default()).into_files();
        let styles = text(&files, "word/styles.xml");
        for style in [
            "Heading1",
            "Heading2",
            "Heading3",
            "ListParagraph",
            "TableGrid",
        ] {
            assert!(
                styles.contains(&format!(r#"w:styleId="{style}""#)),
                "{style}"
            );
        }
        let rels = text(&files, "word/_rels/document.xml.rels");
        assert!(
            rels.contains(r#"Target="styles.xml""#) && rels.contains(r#"Target="numbering.xml""#)
        );
        assert!(text(&files, "[Content_Types].xml").contains("wordprocessingml.document.main+xml"));
        assert!(render(&document(), &DocxOptions::default())
            .unwrap()
            .starts_with(b"PK"));
    }

    #[test]
    fn images_are_embedded_and_scaled_to_the_text_width() {
        let doc = Document {
            pages: vec![PageContent {
                number: 1,
                size: A4,
                blocks: vec![
                    Block::Image {
                        png: vec![1, 2, 3],
                        size: (200.0, 100.0),
                    },
                    Block::Image {
                        png: vec![4],
                        size: (1000.0, 500.0),
                    },
                ],
            }],
        };
        let files = package(&doc, &DocxOptions::default()).into_files();
        let xml = text(&files, "word/document.xml");
        // 200 × 100 pt = 2 540 000 × 1 270 000 EMU.
        assert!(
            xml.contains(r#"<wp:extent cx="2540000" cy="1270000"/>"#),
            "{xml}"
        );
        assert!(xml.contains(r#"<a:blip r:embed="rIdImagem1"/>"#));
        // 1000 pt não cabe: reduzida à largura útil (595 − 2 × 56,7 pt).
        assert!(
            xml.contains(r#"<wp:extent cx="6116320" cy="3058160"/>"#),
            "{xml}"
        );
        let (_, media) = files
            .iter()
            .find(|(name, _)| name == "word/media/image1.png")
            .unwrap();
        assert_eq!(media, &vec![1, 2, 3]);
        let rels = text(&files, "word/_rels/document.xml.rels");
        assert!(rels.contains(r#"Id="rIdImagem2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image2.png""#));
    }
}
