// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pacote OpenDocument (ODF): arquivo `mimetype` como primeira entrada sem
//! compressão, `META-INF/manifest.xml`, `meta.xml`, partes e imagens em
//! `Pictures/`. Usado pelos exportadores ODT e ODG.

use crate::infra::xml::escape;
use crate::infra::zip::ZipWriter;

pub const XML_HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

pub const MIMETYPE_ODT: &str = "application/vnd.oasis.opendocument.text";
pub const MIMETYPE_ODG: &str = "application/vnd.oasis.opendocument.graphics";

pub const MANIFEST_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0";
pub const OFFICE_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
pub const TEXT_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
pub const TABLE_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
pub const STYLE_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
pub const FO_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
pub const DRAW_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
pub const SVG_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0";
pub const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
pub const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
pub const META_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:meta:1.0";

struct Part {
    path: String,
    media_type: String,
    data: Vec<u8>,
}

/// Pacote OpenDocument (ODF) em montagem.
pub struct OdfPackage {
    mimetype: &'static str,
    title: String,
    parts: Vec<Part>,
    image_count: usize,
}

impl OdfPackage {
    /// Inicia um pacote ODF com o tipo de mídia (`MIMETYPE_ODT` ou `MIMETYPE_ODG`)
    /// e o título do documento.
    pub fn new(mimetype: &'static str, title: &str) -> Self {
        Self {
            mimetype,
            title: title.to_string(),
            parts: Vec::new(),
            image_count: 0,
        }
    }

    /// Acrescenta uma parte com caminho relativo e seu tipo MIME.
    pub fn add_part(&mut self, path: &str, media_type: &str, data: Vec<u8>) {
        self.parts.push(Part {
            path: path.into(),
            media_type: media_type.into(),
            data,
        });
    }

    /// Acrescenta uma imagem PNG ao pacote em `Pictures/imageN.png` e devolve
    /// o caminho relativo para ser usado em `xlink:href`.
    pub fn add_image(&mut self, png: &[u8]) -> String {
        self.image_count += 1;
        let path = format!("Pictures/image{}.png", self.image_count);
        self.add_part(&path, "image/png", png.to_vec());
        path
    }

    /// Monta o XML do arquivo `META-INF/manifest.xml`.
    fn build_manifest(&self) -> String {
        let mut manifest = String::from(XML_HEADER);
        manifest.push_str(&format!(
            "<manifest:manifest xmlns:manifest=\"{MANIFEST_NS}\" manifest:version=\"1.2\">\n"
        ));
        manifest.push_str(&format!(
            "  <manifest:file-entry manifest:full-path=\"/\" manifest:version=\"1.2\" manifest:media-type=\"{}\"/>\n",
            self.mimetype
        ));
        manifest.push_str(
            "  <manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>\n",
        );
        for part in &self.parts {
            manifest.push_str(&format!(
                "  <manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{}\"/>\n",
                escape(&part.path),
                escape(&part.media_type)
            ));
        }
        manifest.push_str("</manifest:manifest>");
        manifest
    }

    /// Monta o XML do arquivo `meta.xml`.
    fn build_meta(&self) -> String {
        format!(
            "{XML_HEADER}<office:document-meta xmlns:office=\"{OFFICE_NS}\" xmlns:dc=\"{DC_NS}\" xmlns:meta=\"{META_NS}\" office:version=\"1.2\">\n  <office:meta>\n    <dc:title>{}</dc:title>\n    <meta:generator>RustPDF</meta:generator>\n  </office:meta>\n</office:document-meta>",
            escape(&self.title)
        )
    }

    /// Finaliza o pacote gerando o arquivo ZIP. A especificação ODF exige que
    /// o arquivo `mimetype` seja o primeiro no arquivo ZIP e gravado sem
    /// compressão (método 0, STORED).
    pub fn finish(self) -> Result<Vec<u8>, String> {
        let manifest_content = self.build_manifest().into_bytes();
        let meta_content = self.build_meta().into_bytes();

        let mut zip = ZipWriter::new();
        // 1. OBRIGATÓRIO: mimetype é a primeira entrada e não é compactado.
        zip.add_stored("mimetype", self.mimetype.as_bytes())?;

        // 2. META-INF/manifest.xml
        zip.add("META-INF/manifest.xml", &manifest_content)?;

        // 3. meta.xml
        zip.add("meta.xml", &meta_content)?;

        // 4. As demais partes (content.xml, styles.xml, Pictures/...)
        for part in self.parts {
            zip.add(&part.path, &part.data)?;
        }

        zip.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odf_package_has_mimetype_first_uncompressed() {
        let mut pkg = OdfPackage::new(MIMETYPE_ODT, "Documento de Teste");
        pkg.add_part("content.xml", "text/xml", b"<content/>".to_vec());
        let img = pkg.add_image(b"fake png data");
        assert_eq!(img, "Pictures/image1.png");

        let zip_bytes = pkg.finish().unwrap();
        // Leitura básica dos primeiros bytes do ZIP (Local file header: 0x04034b50)
        assert_eq!(&zip_bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        // Método de compressão na posição 8..10 do header deve ser 0 (STORED)
        assert_eq!(&zip_bytes[8..10], &[0x00, 0x00]);
        // Nome do arquivo na posição 30..38 deve ser "mimetype"
        assert_eq!(&zip_bytes[30..38], b"mimetype");
        // Conteúdo logo após o nome: MIMETYPE_ODT
        let mime_bytes = MIMETYPE_ODT.as_bytes();
        assert_eq!(&zip_bytes[38..38 + mime_bytes.len()], mime_bytes);
    }

    #[test]
    fn odf_manifest_lists_root_meta_and_parts() {
        let mut pkg = OdfPackage::new(MIMETYPE_ODG, "Desenho");
        pkg.add_part("content.xml", "text/xml", b"<drawing/>".to_vec());
        let manifest = pkg.build_manifest();

        assert!(manifest.contains(&format!("manifest:media-type=\"{MIMETYPE_ODG}\"")));
        assert!(manifest.contains("manifest:full-path=\"content.xml\""));
        assert!(manifest.contains("manifest:full-path=\"meta.xml\""));
    }

    #[test]
    fn odf_meta_contains_title_and_generator() {
        let pkg = OdfPackage::new(MIMETYPE_ODT, "Meu <Título> & Teste");
        let meta = pkg.build_meta();
        assert!(meta.contains("<dc:title>Meu &lt;Título&gt; &amp; Teste</dc:title>"));
        assert!(meta.contains("<meta:generator>RustPDF</meta:generator>"));
    }
}
