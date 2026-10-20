// SPDX-License-Identifier: AGPL-3.0-or-later
//! Documento exportado → OpenDocument Text (.odt) para LibreOffice Writer:
//! títulos com estilos de cabeçalho, parágrafos, listas, tabelas e imagens.

use super::odf::{
    OdfPackage, DRAW_NS, FO_NS, MIMETYPE_ODT, OFFICE_NS, STYLE_NS, SVG_NS, TABLE_NS, TEXT_NS,
    XLINK_NS, XML_HEADER,
};
use crate::dominio::exportacao::modelo::{Block, Document};
use crate::infra::xml::escape;

/// Conversão de pontos (pt) para centímetros (cm).
const CM_PER_POINT: f64 = 2.54 / 72.0;
/// Margens padrão de 2 cm.
const MARGIN_CM: f64 = 2.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OdtOptions {
    /// Quebra de página entre as páginas do PDF (padrão: texto corrido).
    pub page_breaks: bool,
}

/// Documento `.odt` completo empacotado em ZIP.
pub fn render(document: &Document, options: &OdtOptions) -> Result<Vec<u8>, String> {
    package(document, options).finish()
}

/// Monta o pacote ODF antes do ZIP (testável antes da serialização).
pub fn package(document: &Document, options: &OdtOptions) -> OdfPackage {
    let (page_w_pt, page_h_pt) = document
        .pages
        .first()
        .map(|page| page.size)
        .unwrap_or((595.0, 842.0));

    let page_w_cm = page_w_pt * CM_PER_POINT;
    let page_h_cm = page_h_pt * CM_PER_POINT;
    let text_w_cm = (page_w_cm - 2.0 * MARGIN_CM).max(1.0);

    let mut pkg = OdfPackage::new(MIMETYPE_ODT, "Documento exportado pelo RustPDF");

    // 1. styles.xml: configuração da página padrão
    let styles_xml = build_styles_xml(page_w_cm, page_h_cm);
    pkg.add_part("styles.xml", "text/xml", styles_xml.into_bytes());

    // 2. content.xml: corpo do documento e estilos automáticos
    let content_xml = build_content_xml(document, options, &mut pkg, text_w_cm);
    pkg.add_part("content.xml", "text/xml", content_xml.into_bytes());

    pkg
}

fn build_styles_xml(page_w_cm: f64, page_h_cm: f64) -> String {
    let orientation = if page_w_cm > page_h_cm {
        " style:print-orientation=\"landscape\""
    } else {
        ""
    };
    format!(
        concat!(
            "{XML_HEADER}",
            "<office:document-styles\n",
            "    xmlns:office=\"{OFFICE_NS}\"\n",
            "    xmlns:style=\"{STYLE_NS}\"\n",
            "    xmlns:text=\"{TEXT_NS}\"\n",
            "    xmlns:table=\"{TABLE_NS}\"\n",
            "    xmlns:fo=\"{FO_NS}\"\n",
            "    office:version=\"1.2\">\n",
            "  <office:styles>\n",
            "    <style:default-style style:family=\"paragraph\">\n",
            "      <style:paragraph-properties fo:margin-top=\"0cm\" fo:margin-bottom=\"0.2cm\"/>\n",
            "      <style:text-properties fo:font-size=\"11pt\" fo:font-family=\"Liberation Sans, Arial, sans-serif\"/>\n",
            "    </style:default-style>\n",
            "  </office:styles>\n",
            "  <office:automatic-styles>\n",
            "    <style:page-layout style:name=\"PL1\">\n",
            "      <style:page-layout-properties\n",
            "          fo:page-width=\"{page_w_cm:.2}cm\"\n",
            "          fo:page-height=\"{page_h_cm:.2}cm\"\n",
            "          fo:margin-top=\"{MARGIN_CM:.2}cm\"\n",
            "          fo:margin-bottom=\"{MARGIN_CM:.2}cm\"\n",
            "          fo:margin-left=\"{MARGIN_CM:.2}cm\"\n",
            "          fo:margin-right=\"{MARGIN_CM:.2}cm\"{orientation}/>\n",
            "    </style:page-layout>\n",
            "  </office:automatic-styles>\n",
            "  <office:master-styles>\n",
            "    <style:master-page style:name=\"Standard\" style:page-layout-name=\"PL1\"/>\n",
            "  </office:master-styles>\n",
            "</office:document-styles>"
        ),
        XML_HEADER = XML_HEADER,
        OFFICE_NS = OFFICE_NS,
        STYLE_NS = STYLE_NS,
        TEXT_NS = TEXT_NS,
        TABLE_NS = TABLE_NS,
        FO_NS = FO_NS,
        page_w_cm = page_w_cm,
        page_h_cm = page_h_cm,
        MARGIN_CM = MARGIN_CM,
        orientation = orientation,
    )
}

fn build_content_xml(
    document: &Document,
    options: &OdtOptions,
    pkg: &mut OdfPackage,
    max_image_w_cm: f64,
) -> String {
    let mut body = String::new();
    let mut in_list = false;

    let end_list_if_open = |body: &mut String, in_list: &mut bool| {
        if *in_list {
            body.push_str("    </text:list>\n");
            *in_list = false;
        }
    };

    for (page_idx, page) in document.pages.iter().enumerate() {
        if options.page_breaks && page_idx > 0 {
            end_list_if_open(&mut body, &mut in_list);
            body.push_str("    <text:p text:style-name=\"P_Break\"/>\n");
        }

        for block in &page.blocks {
            match block {
                Block::Heading { level, text } => {
                    end_list_if_open(&mut body, &mut in_list);
                    let lvl = (*level).clamp(1, 3);
                    body.push_str(&format!(
                        "    <text:h text:outline-level=\"{lvl}\" text:style-name=\"Heading_{lvl}\">{}</text:h>\n",
                        escape(text)
                    ));
                }
                Block::Paragraph { text } => {
                    end_list_if_open(&mut body, &mut in_list);
                    body.push_str(&format!(
                        "    <text:p text:style-name=\"Standard\">{}</text:p>\n",
                        escape(text)
                    ));
                }
                Block::ListItem { marker, text, .. } => {
                    if !in_list {
                        body.push_str("    <text:list text:style-name=\"L1\">\n");
                        in_list = true;
                    }
                    let prefix = if marker.is_empty() {
                        String::new()
                    } else {
                        format!("{} ", escape(marker))
                    };
                    body.push_str(&format!(
                        "      <text:list-item><text:p text:style-name=\"Standard\">{prefix}{}</text:p></text:list-item>\n",
                        escape(text)
                    ));
                }
                Block::Table { rows } => {
                    end_list_if_open(&mut body, &mut in_list);
                    if rows.is_empty() {
                        continue;
                    }
                    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
                    if col_count == 0 {
                        continue;
                    }
                    body.push_str("    <table:table>\n");
                    body.push_str(&format!(
                        "      <table:table-column table:number-columns-repeated=\"{col_count}\"/>\n"
                    ));

                    for (row_idx, row) in rows.iter().enumerate() {
                        let is_header = row_idx == 0;
                        if is_header {
                            body.push_str("      <table:table-header-rows>\n");
                        }
                        body.push_str("        <table:table-row>\n");
                        for cell in row {
                            let cell_style = if is_header {
                                "Table_Header_Cell"
                            } else {
                                "Table_Cell"
                            };
                            let cell_text = if is_header {
                                format!(
                                    "<text:p><text:span text:style-name=\"Bold\">{}</text:span></text:p>",
                                    escape(cell)
                                )
                            } else {
                                format!("<text:p>{}</text:p>", escape(cell))
                            };
                            body.push_str(&format!(
                                "          <table:table-cell office:value-type=\"string\" table:style-name=\"{cell_style}\">\n            {cell_text}\n          </table:table-cell>\n"
                            ));
                        }
                        body.push_str("        </table:table-row>\n");
                        if is_header {
                            body.push_str("      </table:table-header-rows>\n");
                        }
                    }
                    body.push_str("    </table:table>\n");
                }
                Block::Image { png, size } => {
                    end_list_if_open(&mut body, &mut in_list);
                    let img_path = pkg.add_image(png);
                    let orig_w_cm = size.0 * CM_PER_POINT;
                    let orig_h_cm = size.1 * CM_PER_POINT;

                    let (w_cm, h_cm) = if orig_w_cm > max_image_w_cm && orig_w_cm > 0.0 {
                        let factor = max_image_w_cm / orig_w_cm;
                        (max_image_w_cm, orig_h_cm * factor)
                    } else {
                        (orig_w_cm.max(0.5), orig_h_cm.max(0.5))
                    };

                    body.push_str(&format!(
                        "    <text:p><draw:frame text:anchor-type=\"paragraph\" svg:width=\"{w_cm:.2}cm\" svg:height=\"{h_cm:.2}cm\"><draw:image xlink:href=\"{img_path}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame></text:p>\n"
                    ));
                }
            }
        }
    }
    end_list_if_open(&mut body, &mut in_list);

    format!(
        concat!(
            "{XML_HEADER}",
            "<office:document-content\n",
            "    xmlns:office=\"{OFFICE_NS}\"\n",
            "    xmlns:style=\"{STYLE_NS}\"\n",
            "    xmlns:text=\"{TEXT_NS}\"\n",
            "    xmlns:table=\"{TABLE_NS}\"\n",
            "    xmlns:draw=\"{DRAW_NS}\"\n",
            "    xmlns:fo=\"{FO_NS}\"\n",
            "    xmlns:xlink=\"{XLINK_NS}\"\n",
            "    xmlns:svg=\"{SVG_NS}\"\n",
            "    office:version=\"1.2\">\n",
            "  <office:automatic-styles>\n",
            "    <style:style style:name=\"P_Break\" style:family=\"paragraph\">\n",
            "      <style:paragraph-properties fo:break-before=\"page\"/>\n",
            "    </style:style>\n",
            "    <style:style style:name=\"Heading_1\" style:family=\"paragraph\">\n",
            "      <style:text-properties fo:font-size=\"18pt\" fo:font-weight=\"bold\"/>\n",
            "    </style:style>\n",
            "    <style:style style:name=\"Heading_2\" style:family=\"paragraph\">\n",
            "      <style:text-properties fo:font-size=\"14pt\" fo:font-weight=\"bold\"/>\n",
            "    </style:style>\n",
            "    <style:style style:name=\"Heading_3\" style:family=\"paragraph\">\n",
            "      <style:text-properties fo:font-size=\"12pt\" fo:font-weight=\"bold\"/>\n",
            "    </style:style>\n",
            "    <style:style style:name=\"Bold\" style:family=\"text\">\n",
            "      <style:text-properties fo:font-weight=\"bold\"/>\n",
            "    </style:style>\n",
            "    <style:style style:name=\"Table_Cell\" style:family=\"table-cell\">\n",
            "      <style:table-cell-properties fo:border=\"0.5pt solid #000000\" fo:padding=\"0.1cm\"/>\n",
            "    </style:style>\n",
            "    <style:style style:name=\"Table_Header_Cell\" style:family=\"table-cell\">\n",
            "      <style:table-cell-properties fo:border=\"0.5pt solid #000000\" fo:padding=\"0.1cm\" fo:background-color=\"#f0f0f0\"/>\n",
            "    </style:style>\n",
            "  </office:automatic-styles>\n",
            "  <office:body>\n",
            "    <office:text>\n",
            "{body}",
            "    </office:text>\n",
            "  </office:body>\n",
            "</office:document-content>"
        ),
        XML_HEADER = XML_HEADER,
        OFFICE_NS = OFFICE_NS,
        STYLE_NS = STYLE_NS,
        TEXT_NS = TEXT_NS,
        TABLE_NS = TABLE_NS,
        DRAW_NS = DRAW_NS,
        FO_NS = FO_NS,
        XLINK_NS = XLINK_NS,
        SVG_NS = SVG_NS,
        body = body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::exportacao::modelo::{Block, PageContent, A4};

    fn sample_document() -> Document {
        Document {
            pages: vec![
                PageContent {
                    number: 1,
                    size: A4,
                    blocks: vec![
                        Block::Heading {
                            level: 1,
                            text: "Título Principal".into(),
                        },
                        Block::Paragraph {
                            text: "Primeiro parágrafo com caracteres <especiais> & acentos.".into(),
                        },
                        Block::ListItem {
                            marker: String::new(),
                            level: 0,
                            text: "Item bullet".into(),
                        },
                        Block::ListItem {
                            marker: "1.".into(),
                            level: 0,
                            text: "Item numerado".into(),
                        },
                        Block::Table {
                            rows: vec![
                                vec!["Coluna A".into(), "Coluna B".into()],
                                vec!["Valor 1".into(), "Valor 2".into()],
                            ],
                        },
                        Block::Image {
                            png: vec![0x89, 0x50, 0x4e, 0x47],
                            size: (100.0, 50.0),
                        },
                    ],
                },
                PageContent {
                    number: 2,
                    size: A4,
                    blocks: vec![Block::Paragraph {
                        text: "Texto da página 2".into(),
                    }],
                },
            ],
        }
    }

    #[test]
    fn renders_odt_with_all_blocks() {
        let doc = sample_document();
        let bytes = render(&doc, &OdtOptions { page_breaks: true }).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);
    }

    #[test]
    fn odt_content_xml_has_headings_tables_and_page_break() {
        let doc = sample_document();
        let mut pkg = OdfPackage::new(MIMETYPE_ODT, "Teste");
        let xml = build_content_xml(&doc, &OdtOptions { page_breaks: true }, &mut pkg, 17.0);

        assert!(xml.contains("<text:h text:outline-level=\"1\" text:style-name=\"Heading_1\">Título Principal</text:h>"));
        assert!(xml.contains("Primeiro parágrafo com caracteres &lt;especiais&gt; &amp; acentos."));
        assert!(xml.contains("<text:list text:style-name=\"L1\">"));
        assert!(xml.contains("<text:p text:style-name=\"Standard\">Item bullet</text:p>"));
        assert!(xml.contains("<text:p text:style-name=\"Standard\">1. Item numerado</text:p>"));
        assert!(xml.contains("<table:table>"));
        assert!(xml.contains("Table_Header_Cell"));
        assert!(xml.contains("<text:span text:style-name=\"Bold\">Coluna A</text:span>"));
        assert!(xml.contains("Table_Cell"));
        assert!(xml.contains("<text:p text:style-name=\"P_Break\"/>"));
        assert!(xml.contains("<draw:image xlink:href=\"Pictures/image1.png\""));
    }

    #[test]
    fn libreoffice_accepts_and_converts_odt_when_available() {
        let dir = crate::testkit::TempDir::new("odt_test");
        let odt_file = dir.join("documento.odt");
        let doc = sample_document();
        let bytes = render(&doc, &OdtOptions::default()).unwrap();
        std::fs::write(&odt_file, bytes).unwrap();

        let profile = format!(
            "-env:UserInstallation=file://{}",
            dir.path().join("perfil").display()
        );
        let Ok(output) = crate::infra::ferramentas::tool_command("soffice")
            .arg(&profile)
            .arg("--headless")
            .arg("--convert-to")
            .arg("pdf")
            .arg(&odt_file)
            .arg("--outdir")
            .arg(dir.path())
            .output()
        else {
            return; // LibreOffice não disponível no ambiente
        };

        assert!(
            output.status.success(),
            "soffice falhou: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let pdf_file = dir.join("documento.pdf");
        assert!(pdf_file.is_file(), "PDF convertido deve existir");
    }
}
