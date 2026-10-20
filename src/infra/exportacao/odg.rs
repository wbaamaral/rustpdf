// SPDX-License-Identifier: AGPL-3.0-or-later
//! Desenhos → OpenDocument Graphics (.odg) para LibreOffice Draw:
//! uma página/prancheta por slide com caixas de texto e imagens nas posições
//! originais do PDF, ou a página como imagem fiel.

use super::odf::{
    OdfPackage, DRAW_NS, FO_NS, MIMETYPE_ODG, OFFICE_NS, STYLE_NS, SVG_NS, TEXT_NS, XLINK_NS,
    XML_HEADER,
};
use crate::dominio::exportacao::slides::{Shape, Slide};
use crate::infra::xml::escape;

/// Conversão de pontos (pt) para centímetros (cm).
const CM_PER_POINT: f64 = 2.54 / 72.0;

/// Desenho `.odg` completo empacotado em ZIP.
pub fn render(slides: &[Slide]) -> Result<Vec<u8>, String> {
    package(slides).finish()
}

/// Monta o pacote ODF antes do ZIP (testável antes da serialização).
pub fn package(slides: &[Slide]) -> OdfPackage {
    let (page_w_pt, page_h_pt) = slides
        .first()
        .map(|slide| slide.size)
        .unwrap_or((595.0, 842.0));

    let page_w_cm = page_w_pt * CM_PER_POINT;
    let page_h_cm = page_h_pt * CM_PER_POINT;

    let mut pkg = OdfPackage::new(MIMETYPE_ODG, "Desenho exportado pelo RustPDF");

    // 1. styles.xml
    let styles_xml = build_styles_xml(page_w_cm, page_h_cm);
    pkg.add_part("styles.xml", "text/xml", styles_xml.into_bytes());

    // 2. content.xml
    let content_xml = build_content_xml(slides, &mut pkg);
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
            "    xmlns:draw=\"{DRAW_NS}\"\n",
            "    xmlns:fo=\"{FO_NS}\"\n",
            "    xmlns:svg=\"{SVG_NS}\"\n",
            "    office:version=\"1.2\">\n",
            "  <office:styles>\n",
            "    <style:default-style style:family=\"graphics\">\n",
            "      <style:graphic-properties draw:fill=\"none\" draw:stroke=\"none\"/>\n",
            "      <style:text-properties fo:font-family=\"Liberation Sans, Arial, sans-serif\" fo:font-size=\"10pt\"/>\n",
            "    </style:default-style>\n",
            "    <style:default-style style:family=\"paragraph\">\n",
            "      <style:paragraph-properties fo:margin-top=\"0cm\" fo:margin-bottom=\"0cm\"/>\n",
            "      <style:text-properties fo:font-family=\"Liberation Sans, Arial, sans-serif\" fo:font-size=\"10pt\"/>\n",
            "    </style:default-style>\n",
            "  </office:styles>\n",
            "  <office:automatic-styles>\n",
            "    <style:page-layout style:name=\"PL1\">\n",
            "      <style:page-layout-properties\n",
            "          fo:page-width=\"{page_w_cm:.2}cm\"\n",
            "          fo:page-height=\"{page_h_cm:.2}cm\"\n",
            "          fo:margin-top=\"0cm\"\n",
            "          fo:margin-bottom=\"0cm\"\n",
            "          fo:margin-left=\"0cm\"\n",
            "          fo:margin-right=\"0cm\"{orientation}/>\n",
            "    </style:page-layout>\n",
            "  </office:automatic-styles>\n",
            "  <office:master-styles>\n",
            "    <draw:layer-set>\n",
            "      <draw:layer draw:name=\"layout\"/>\n",
            "      <draw:layer draw:name=\"background\"/>\n",
            "      <draw:layer draw:name=\"backgroundobjects\"/>\n",
            "      <draw:layer draw:name=\"controls\"/>\n",
            "      <draw:layer draw:name=\"measurelines\"/>\n",
            "    </draw:layer-set>\n",
            "    <style:master-page style:name=\"Default\" style:page-layout-name=\"PL1\"/>\n",
            "  </office:master-styles>\n",
            "</office:document-styles>"
        ),
        XML_HEADER = XML_HEADER,
        OFFICE_NS = OFFICE_NS,
        STYLE_NS = STYLE_NS,
        TEXT_NS = TEXT_NS,
        DRAW_NS = DRAW_NS,
        FO_NS = FO_NS,
        SVG_NS = SVG_NS,
        page_w_cm = page_w_cm,
        page_h_cm = page_h_cm,
        orientation = orientation,
    )
}

fn build_content_xml(slides: &[Slide], pkg: &mut OdfPackage) -> String {
    let mut pages = String::new();
    let mut font_styles = std::collections::BTreeMap::new();

    for (index, slide) in slides.iter().enumerate() {
        let page_num = index + 1;
        pages.push_str(&format!(
            "    <draw:page draw:name=\"page{page_num}\" draw:style-name=\"dp1\" draw:master-page-name=\"Default\">\n"
        ));

        for shape in &slide.shapes {
            match shape {
                Shape::Text {
                    rect,
                    lines,
                    font_size,
                } => {
                    let x_cm = rect.left() * CM_PER_POINT;
                    let y_cm = rect.top() * CM_PER_POINT;
                    let w_cm = (rect.width() * CM_PER_POINT * 1.25 + 0.8).max(0.5);
                    let h_cm = (rect.height() * CM_PER_POINT + 0.2).max(0.2);
                    let size_pt = (*font_size).max(6.0);
                    let style_id = (size_pt * 10.0).round() as u32;
                    let style_name = format!("P_F{style_id}");
                    font_styles.entry(style_id).or_insert(size_pt);

                    pages.push_str(&format!(
                        "      <draw:frame draw:style-name=\"gr1\" svg:x=\"{x_cm:.2}cm\" svg:y=\"{y_cm:.2}cm\" svg:width=\"{w_cm:.2}cm\" svg:height=\"{h_cm:.2}cm\">\n"
                    ));
                    pages.push_str("        <draw:text-box>\n");
                    for line in lines {
                        pages.push_str(&format!(
                            "          <text:p text:style-name=\"{style_name}\">{}</text:p>\n",
                            escape(line)
                        ));
                    }
                    pages.push_str("        </draw:text-box>\n");
                    pages.push_str("      </draw:frame>\n");
                }
                Shape::Picture { rect, png } => {
                    let x_cm = rect.left() * CM_PER_POINT;
                    let y_cm = rect.top() * CM_PER_POINT;
                    let w_cm = (rect.width() * CM_PER_POINT).max(0.1);
                    let h_cm = (rect.height() * CM_PER_POINT).max(0.1);

                    let img_path = pkg.add_image(png);
                    pages.push_str(&format!(
                        "      <draw:frame draw:style-name=\"gr1\" svg:x=\"{x_cm:.2}cm\" svg:y=\"{y_cm:.2}cm\" svg:width=\"{w_cm:.2}cm\" svg:height=\"{h_cm:.2}cm\">\n"
                    ));
                    pages.push_str(&format!(
                        "        <draw:image xlink:href=\"{img_path}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>\n"
                    ));
                    pages.push_str("      </draw:frame>\n");
                }
                Shape::Line { start, end } => {
                    let x1_cm = start.0 * CM_PER_POINT;
                    let y1_cm = start.1 * CM_PER_POINT;
                    let x2_cm = end.0 * CM_PER_POINT;
                    let y2_cm = end.1 * CM_PER_POINT;
                    pages.push_str(&format!(
                        "      <draw:line draw:style-name=\"gr_line\" svg:x1=\"{x1_cm:.3}cm\" svg:y1=\"{y1_cm:.3}cm\" svg:x2=\"{x2_cm:.3}cm\" svg:y2=\"{y2_cm:.3}cm\"><text:p/></draw:line>\n"
                    ));
                }
            }
        }
        pages.push_str("    </draw:page>\n");
    }

    let mut auto_styles = String::new();
    auto_styles.push_str("    <style:style style:name=\"dp1\" style:family=\"drawing-page\"/>\n");
    auto_styles.push_str(concat!(
        "    <style:style style:name=\"gr1\" style:family=\"graphics\">\n",
        "      <style:graphic-properties\n",
        "          draw:stroke=\"none\"\n",
        "          draw:fill=\"none\"\n",
        "          draw:auto-grow-width=\"true\"\n",
        "          draw:auto-grow-height=\"true\"\n",
        "          draw:textarea-horizontal-align=\"left\"\n",
        "          draw:textarea-vertical-align=\"top\"\n",
        "          fo:padding-top=\"0cm\"\n",
        "          fo:padding-bottom=\"0cm\"\n",
        "          fo:padding-left=\"0cm\"\n",
        "          fo:padding-right=\"0cm\"/>\n",
        "    </style:style>\n",
        "    <style:style style:name=\"gr_line\" style:family=\"graphics\">\n",
        "      <style:graphic-properties\n",
        "          draw:stroke=\"solid\"\n",
        "          svg:stroke-width=\"0.02cm\"\n",
        "          svg:stroke-color=\"#000000\"/>\n",
        "    </style:style>\n"
    ));
    for (style_id, size_pt) in font_styles {
        auto_styles.push_str(&format!(
            "    <style:style style:name=\"P_F{style_id}\" style:family=\"paragraph\">\n      <style:paragraph-properties fo:wrap-option=\"no-wrap\"/>\n      <style:text-properties fo:font-size=\"{size_pt:.1}pt\" fo:font-family=\"Liberation Sans, Arial, sans-serif\"/>\n    </style:style>\n"
        ));
    }

    format!(
        concat!(
            "{XML_HEADER}",
            "<office:document-content\n",
            "    xmlns:office=\"{OFFICE_NS}\"\n",
            "    xmlns:style=\"{STYLE_NS}\"\n",
            "    xmlns:text=\"{TEXT_NS}\"\n",
            "    xmlns:draw=\"{DRAW_NS}\"\n",
            "    xmlns:fo=\"{FO_NS}\"\n",
            "    xmlns:xlink=\"{XLINK_NS}\"\n",
            "    xmlns:svg=\"{SVG_NS}\"\n",
            "    office:version=\"1.2\">\n",
            "  <office:automatic-styles>\n",
            "{auto_styles}",
            "  </office:automatic-styles>\n",
            "  <office:body>\n",
            "    <office:drawing>\n",
            "{pages}",
            "    </office:drawing>\n",
            "  </office:body>\n",
            "</office:document-content>"
        ),
        XML_HEADER = XML_HEADER,
        OFFICE_NS = OFFICE_NS,
        STYLE_NS = STYLE_NS,
        TEXT_NS = TEXT_NS,
        DRAW_NS = DRAW_NS,
        FO_NS = FO_NS,
        XLINK_NS = XLINK_NS,
        SVG_NS = SVG_NS,
        auto_styles = auto_styles,
        pages = pages,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::geometria::LayoutRect;

    fn sample_slides() -> Vec<Slide> {
        vec![
            Slide {
                size: (595.0, 842.0),
                shapes: vec![
                    Shape::Text {
                        rect: LayoutRect::new(50.0, 100.0, 300.0, 150.0),
                        lines: vec!["Título no Draw".into(), "Segunda linha".into()],
                        font_size: 16.0,
                    },
                    Shape::Picture {
                        rect: LayoutRect::new(50.0, 200.0, 250.0, 400.0),
                        png: vec![0x89, 0x50, 0x4e, 0x47],
                    },
                    Shape::Line {
                        start: (50.0, 450.0),
                        end: (300.0, 450.0),
                    },
                ],
            },
            Slide {
                size: (595.0, 842.0),
                shapes: vec![Shape::Text {
                    rect: LayoutRect::new(50.0, 50.0, 500.0, 100.0),
                    lines: vec!["Página 2".into()],
                    font_size: 12.0,
                }],
            },
        ]
    }

    #[test]
    fn renders_odg_with_slides() {
        let slides = sample_slides();
        let bytes = render(&slides).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);
    }

    #[test]
    fn odg_content_xml_has_pages_frames_and_text() {
        let slides = sample_slides();
        let mut pkg = OdfPackage::new(MIMETYPE_ODG, "Teste");
        let xml = build_content_xml(&slides, &mut pkg);

        assert!(xml.contains("<draw:page draw:name=\"page1\""));
        assert!(xml.contains("<draw:page draw:name=\"page2\""));
        assert!(xml.contains("Título no Draw"));
        assert!(xml.contains("fo:font-size=\"16.0pt\""));
        assert!(xml.contains("<draw:image xlink:href=\"Pictures/image1.png\""));
        assert!(xml.contains("<draw:line draw:style-name=\"gr_line\""));
    }

    #[test]
    fn libreoffice_accepts_and_converts_odg_when_available() {
        let dir = crate::testkit::TempDir::new("odg_test");
        let odg_file = dir.join("desenho.odg");
        let slides = sample_slides();
        let bytes = render(&slides).unwrap();
        std::fs::write(&odg_file, bytes).unwrap();

        let profile = format!(
            "-env:UserInstallation=file://{}",
            dir.path().join("perfil").display()
        );
        let Ok(output) = crate::infra::ferramentas::tool_command("soffice")
            .arg(&profile)
            .arg("--headless")
            .arg("--convert-to")
            .arg("pdf")
            .arg(&odg_file)
            .arg("--outdir")
            .arg(dir.path())
            .output()
        else {
            return;
        };

        assert!(
            output.status.success(),
            "soffice falhou: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let pdf_file = dir.join("desenho.pdf");
        assert!(pdf_file.is_file(), "PDF convertido de ODG deve existir");
    }
}
