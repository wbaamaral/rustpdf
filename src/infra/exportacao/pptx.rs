// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slides → PowerPoint (PresentationML): um slide por página, no tamanho da
//! página, com caixas de texto e imagens posicionadas.

use super::ooxml::{Package, IMAGE, OFFICE_DOCUMENT, XML_HEADER};
use crate::dominio::exportacao::slides::{Shape, Slide};
use crate::dominio::geometria::LayoutRect;
use crate::infra::xml::escape;

const NAMESPACES: &str = concat!(
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\""
);
const RELATIONSHIP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const EMU_PER_POINT: f64 = 12_700.0;
/// Limites de tamanho de slide aceitos pelo PowerPoint (1 a 56 polegadas).
const MIN_EMU: i64 = 914_400;
const MAX_EMU: i64 = 51_206_400;

/// Apresentação `.pptx` com um slide por item.
pub fn render(slides: &[Slide]) -> Result<Vec<u8>, String> {
    package(slides).finish()
}

/// O pacote antes do ZIP (testável parte a parte).
pub fn package(slides: &[Slide]) -> Package {
    let mut package = Package::new("Apresentação exportada pelo RustPDF");
    let presentation = "ppt/presentation.xml";
    let master = "ppt/slideMasters/slideMaster1.xml";
    let layout = "ppt/slideLayouts/slideLayout1.xml";
    package.relate(
        Some(presentation),
        &format!("{RELATIONSHIP}/slideMaster"),
        "slideMasters/slideMaster1.xml",
    );
    let mut slide_ids = String::new();
    let mut image_count = 0;
    for (index, slide) in slides.iter().enumerate() {
        let number = index + 1;
        let path = format!("ppt/slides/slide{number}.xml");
        package.relate(
            Some(&path),
            &format!("{RELATIONSHIP}/slideLayout"),
            "../slideLayouts/slideLayout1.xml",
        );
        let mut shapes = String::new();
        for (shape_index, shape) in slide.shapes.iter().enumerate() {
            let id = shape_index + 2; // 1 é o grupo raiz
            match shape {
                Shape::Text {
                    rect,
                    lines,
                    font_size,
                } => shapes.push_str(&text_box(id, *rect, lines, *font_size)),
                Shape::Picture { rect, png } => {
                    image_count += 1;
                    let media = format!("media/image{image_count}.png");
                    package.add_part(&format!("ppt/{media}"), "image/png", png.clone());
                    let relationship = package.relate(Some(&path), IMAGE, &format!("../{media}"));
                    shapes.push_str(&picture(id, *rect, &relationship));
                }
            }
        }
        package.add_part(
            &path,
            "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
            format!(
                "{XML_HEADER}<p:sld {NAMESPACES}><p:cSld>{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>",
                shape_tree(&shapes)
            )
            .into_bytes(),
        );
        let relationship = package.relate(
            Some(presentation),
            &format!("{RELATIONSHIP}/slide"),
            &format!("slides/slide{number}.xml"),
        );
        slide_ids.push_str(&format!(
            "<p:sldId id=\"{}\" r:id=\"{relationship}\"/>",
            255 + number
        ));
    }
    let (width, height) = slides
        .first()
        .map(|slide| slide.size)
        .unwrap_or((960.0, 540.0));
    let size = |points: f64| emu(points).clamp(MIN_EMU, MAX_EMU);
    package.add_part(
        presentation,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
        format!(
            "{XML_HEADER}<p:presentation {NAMESPACES}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx=\"{}\" cy=\"{}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>",
            size(width),
            size(height)
        )
        .into_bytes(),
    );
    package.add_part(
        master,
        "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml",
        format!(
            "{XML_HEADER}<p:sldMaster {NAMESPACES}><p:cSld>{}</p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst></p:sldMaster>",
            shape_tree("")
        )
        .into_bytes(),
    );
    package.relate(
        Some(master),
        &format!("{RELATIONSHIP}/slideLayout"),
        "../slideLayouts/slideLayout1.xml",
    );
    package.relate(
        Some(master),
        &format!("{RELATIONSHIP}/theme"),
        "../theme/theme1.xml",
    );
    package.add_part(
        layout,
        "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml",
        format!(
            "{XML_HEADER}<p:sldLayout {NAMESPACES} type=\"blank\" preserve=\"1\"><p:cSld name=\"Em branco\">{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
            shape_tree("")
        )
        .into_bytes(),
    );
    package.relate(
        Some(layout),
        &format!("{RELATIONSHIP}/slideMaster"),
        "../slideMasters/slideMaster1.xml",
    );
    package.add_part(
        "ppt/theme/theme1.xml",
        "application/vnd.openxmlformats-officedocument.theme+xml",
        format!("{XML_HEADER}{THEME}").into_bytes(),
    );
    package.relate(None, OFFICE_DOCUMENT, presentation);
    package
}

fn emu(points: f64) -> i64 {
    (points * EMU_PER_POINT).round() as i64
}

/// Posição e tamanho (`a:xfrm`).
fn transform(rect: LayoutRect) -> String {
    format!(
        "<a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
        emu(rect.left()),
        emu(rect.top()),
        emu(rect.width()),
        emu(rect.height())
    )
}

/// Árvore de formas com o grupo raiz obrigatório.
fn shape_tree(shapes: &str) -> String {
    format!(
        "<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree>"
    )
}

fn text_box(id: usize, rect: LayoutRect, lines: &[String], font_size: f64) -> String {
    let size = (font_size * 100.0).round() as i64;
    let paragraphs: String = lines
        .iter()
        .map(|line| {
            format!(
                "<a:p><a:r><a:rPr lang=\"pt-BR\" sz=\"{size}\"/><a:t>{}</a:t></a:r></a:p>",
                escape(line)
            )
        })
        .collect();
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Texto {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"none\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\"/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>",
        transform(rect)
    )
}

fn picture(id: usize, rect: LayoutRect, relationship: &str) -> String {
    format!(
        "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Imagem {id}\"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{relationship}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
        transform(rect)
    )
}

/// Tema mínimo exigido pelo mestre dos slides.
const THEME: &str = concat!(
    "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"RustPDF\"><a:themeElements>",
    "<a:clrScheme name=\"RustPDF\">",
    "<a:dk1><a:srgbClr val=\"000000\"/></a:dk1><a:lt1><a:srgbClr val=\"FFFFFF\"/></a:lt1>",
    "<a:dk2><a:srgbClr val=\"1F2937\"/></a:dk2><a:lt2><a:srgbClr val=\"F3F4F6\"/></a:lt2>",
    "<a:accent1><a:srgbClr val=\"2563EB\"/></a:accent1><a:accent2><a:srgbClr val=\"DC2626\"/></a:accent2>",
    "<a:accent3><a:srgbClr val=\"16A34A\"/></a:accent3><a:accent4><a:srgbClr val=\"CA8A04\"/></a:accent4>",
    "<a:accent5><a:srgbClr val=\"7C3AED\"/></a:accent5><a:accent6><a:srgbClr val=\"0891B2\"/></a:accent6>",
    "<a:hlink><a:srgbClr val=\"2563EB\"/></a:hlink><a:folHlink><a:srgbClr val=\"7C3AED\"/></a:folHlink>",
    "</a:clrScheme>",
    "<a:fontScheme name=\"RustPDF\"><a:majorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>",
    "<a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme>",
    "<a:fmtScheme name=\"RustPDF\">",
    "<a:fillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:fillStyleLst>",
    "<a:lnStyleLst><a:ln w=\"6350\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln><a:ln w=\"12700\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln><a:ln w=\"19050\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln></a:lnStyleLst>",
    "<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>",
    "<a:bgFillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:bgFillStyleLst>",
    "</a:fmtScheme></a:themeElements></a:theme>"
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::exportacao::slides::{Shape, Slide};
    use crate::dominio::geometria::LayoutRect;

    fn slides() -> Vec<Slide> {
        vec![
            Slide {
                size: (595.0, 842.0),
                shapes: vec![
                    Shape::Text {
                        rect: LayoutRect::new(72.0, 60.0, 272.0, 90.0),
                        lines: vec!["Título & cia".into(), "linha 2".into()],
                        font_size: 9.5,
                    },
                    Shape::Picture {
                        rect: LayoutRect::new(10.0, 20.0, 110.0, 70.0),
                        png: vec![1, 2],
                    },
                ],
            },
            Slide {
                size: (595.0, 842.0),
                shapes: vec![],
            },
        ]
    }

    fn text(files: &[(String, Vec<u8>)], path: &str) -> String {
        let (_, data) = files.iter().find(|(name, _)| name == path).expect(path);
        String::from_utf8(data.clone()).unwrap()
    }

    #[test]
    fn presentation_lists_slides_at_page_size() {
        let files = package(&slides()).into_files();
        let presentation = text(&files, "ppt/presentation.xml");
        // 595 × 842 pt = 7 556 500 × 10 693 400 EMU.
        assert!(presentation.contains(r#"<p:sldSz cx="7556500" cy="10693400"/>"#));
        assert!(presentation
            .contains(r#"<p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/>"#));
        let rels = text(&files, "ppt/_rels/presentation.xml.rels");
        assert!(rels.contains(r#"Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml""#));
        assert!(rels.contains(r#"Target="slides/slide2.xml""#));
        for part in [
            "ppt/slideMasters/slideMaster1.xml",
            "ppt/slideLayouts/slideLayout1.xml",
            "ppt/theme/theme1.xml",
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        ] {
            assert!(files.iter().any(|(name, _)| name == part), "{part}");
        }
        assert!(
            text(&files, "[Content_Types].xml").contains("presentationml.presentation.main+xml")
        );
    }

    #[test]
    fn shapes_are_positioned_in_emu() {
        let files = package(&slides()).into_files();
        let slide = text(&files, "ppt/slides/slide1.xml");
        assert!(
            slide.contains(r#"<a:off x="914400" y="762000"/><a:ext cx="2540000" cy="381000"/>"#),
            "{slide}"
        );
        assert!(slide.contains(r#"<a:rPr lang="pt-BR" sz="950"/><a:t>Título &amp; cia</a:t>"#));
        assert!(slide.contains("<a:t>linha 2</a:t>"));
        assert!(slide.contains(r#"<a:blip r:embed="rId2"/>"#));
        assert!(
            slide.contains(r#"<a:off x="127000" y="254000"/><a:ext cx="1270000" cy="635000"/>"#)
        );
        let rels = text(&files, "ppt/slides/_rels/slide1.xml.rels");
        assert!(rels.contains(r#"Target="../slideLayouts/slideLayout1.xml""#));
        assert!(rels.contains(r#"Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png""#));
        let (_, media) = files
            .iter()
            .find(|(name, _)| name == "ppt/media/image1.png")
            .unwrap();
        assert_eq!(media, &vec![1, 2]);
    }

    #[test]
    fn render_produces_a_zip() {
        assert!(render(&slides()).unwrap().starts_with(b"PK\x03\x04"));
    }
}
