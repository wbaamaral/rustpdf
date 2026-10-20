// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pacote Office Open XML (OPC): partes com tipo de conteúdo, relações por
//! parte de origem, propriedades do documento e o ZIP final. Usado pelos
//! escritores de Word, Excel e PowerPoint.

use crate::infra::xml::escape;
use crate::infra::zip::ZipWriter;

/// Cabeçalho XML das partes.
pub const XML_HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

pub const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub const STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const CORE_PROPERTIES: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const EXTENDED_PROPERTIES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";

const PACKAGE_RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// Tipos por extensão (as demais partes ganham um `Override`).
const DEFAULTS: &[(&str, &str)] = &[
    (
        "rels",
        "application/vnd.openxmlformats-package.relationships+xml",
    ),
    ("xml", "application/xml"),
    ("png", "image/png"),
    ("jpeg", "image/jpeg"),
];

struct Part {
    path: String,
    content_type: String,
    data: Vec<u8>,
}

struct Relationship {
    /// Parte de origem; `None` = o pacote (`_rels/.rels`).
    source: Option<String>,
    id: String,
    kind: String,
    target: String,
}

/// Pacote em montagem.
pub struct Package {
    parts: Vec<Part>,
    relationships: Vec<Relationship>,
}

impl Package {
    /// Pacote com as propriedades do documento (título; aplicativo RustPDF).
    pub fn new(title: &str) -> Self {
        let mut package = Self {
            parts: Vec::new(),
            relationships: Vec::new(),
        };
        package.add_part(
            "docProps/core.xml",
            "application/vnd.openxmlformats-package.core-properties+xml",
            format!(
                "{XML_HEADER}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>RustPDF</dc:creator></cp:coreProperties>",
                escape(title)
            )
            .into_bytes(),
        );
        package.add_part(
            "docProps/app.xml",
            "application/vnd.openxmlformats-officedocument.extended-properties+xml",
            format!(
                "{XML_HEADER}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>RustPDF</Application></Properties>"
            )
            .into_bytes(),
        );
        package.relate(None, CORE_PROPERTIES, "docProps/core.xml");
        package.relate(None, EXTENDED_PROPERTIES, "docProps/app.xml");
        package
    }

    pub fn add_part(&mut self, path: &str, content_type: &str, data: Vec<u8>) {
        self.parts.push(Part {
            path: path.into(),
            content_type: content_type.into(),
            data,
        });
    }

    /// Relação de `source` (ou do pacote) para `target`, relativo à pasta da
    /// origem; devolve o id (`rId1`, `rId2`… por origem).
    pub fn relate(&mut self, source: Option<&str>, kind: &str, target: &str) -> String {
        let count = self
            .relationships
            .iter()
            .filter(|relationship| relationship.source.as_deref() == source)
            .count();
        let id = format!("rId{}", count + 1);
        self.relationships.push(Relationship {
            source: source.map(str::to_string),
            id: id.clone(),
            kind: kind.into(),
            target: target.into(),
        });
        id
    }

    /// Relação com `id` escolhido por quem chama (ex.: referência já
    /// escrita no XML da parte de origem).
    pub fn relate_as(&mut self, source: Option<&str>, id: &str, kind: &str, target: &str) {
        self.relationships.push(Relationship {
            source: source.map(str::to_string),
            id: id.into(),
            kind: kind.into(),
            target: target.into(),
        });
    }

    /// Arquivos do pacote na ordem do ZIP: tipos, relações, partes.
    pub fn into_files(self) -> Vec<(String, Vec<u8>)> {
        let mut files = vec![("[Content_Types].xml".to_string(), self.content_types())];
        let mut sources: Vec<Option<String>> = Vec::new();
        for relationship in &self.relationships {
            if !sources.contains(&relationship.source) {
                sources.push(relationship.source.clone());
            }
        }
        for source in sources {
            let entries: String = self
                .relationships
                .iter()
                .filter(|relationship| relationship.source == source)
                .map(|relationship| {
                    format!(
                        "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"/>",
                        relationship.id,
                        relationship.kind,
                        escape(&relationship.target)
                    )
                })
                .collect();
            let path = match &source {
                None => "_rels/.rels".to_string(),
                Some(part) => match part.rsplit_once('/') {
                    Some((folder, name)) => format!("{folder}/_rels/{name}.rels"),
                    None => format!("_rels/{part}.rels"),
                },
            };
            let xml = format!(
                "{XML_HEADER}<Relationships xmlns=\"{}\">{entries}</Relationships>",
                PACKAGE_RELATIONSHIPS
            );
            files.push((path, xml.into_bytes()));
        }
        files.extend(self.parts.into_iter().map(|part| (part.path, part.data)));
        files
    }

    /// O pacote como arquivo ZIP.
    pub fn finish(self) -> Result<Vec<u8>, String> {
        let mut zip = ZipWriter::new();
        for (path, data) in self.into_files() {
            zip.add(&path, &data)?;
        }
        zip.finish()
    }

    fn content_types(&self) -> Vec<u8> {
        let mut xml = format!(
            "{XML_HEADER}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">"
        );
        for (extension, content_type) in DEFAULTS {
            xml.push_str(&format!(
                "<Default Extension=\"{extension}\" ContentType=\"{content_type}\"/>"
            ));
        }
        for part in &self.parts {
            let by_extension = DEFAULTS.iter().any(|(extension, content_type)| {
                part.path.ends_with(&format!(".{extension}")) && *content_type == part.content_type
            });
            if !by_extension {
                xml.push_str(&format!(
                    "<Override PartName=\"/{}\" ContentType=\"{}\"/>",
                    escape(&part.path),
                    part.content_type
                ));
            }
        }
        xml.push_str("</Types>");
        xml.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(files: &[(String, Vec<u8>)], path: &str) -> String {
        let (_, data) = files
            .iter()
            .find(|(name, _)| name == path)
            .unwrap_or_else(|| panic!("sem {path}"));
        String::from_utf8(data.clone()).unwrap()
    }

    fn package() -> Package {
        let mut package = Package::new("Relatório & cia");
        package.add_part(
            "xl/workbook.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
            b"<workbook/>".to_vec(),
        );
        package.relate(None, OFFICE_DOCUMENT, "xl/workbook.xml");
        package.add_part("xl/media/image1.png", "image/png", vec![1, 2, 3]);
        let id = package.relate(Some("xl/workbook.xml"), IMAGE, "media/image1.png");
        assert_eq!(id, "rId1", "ids contam por origem");
        package
    }

    #[test]
    fn content_types_cover_every_part() {
        let files = package().into_files();
        assert_eq!(files[0].0, "[Content_Types].xml", "primeira entrada do ZIP");
        let types = text(&files, "[Content_Types].xml");
        assert!(types.contains(r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#));
        assert!(types.contains(r#"<Default Extension="png" ContentType="image/png"/>"#));
        assert!(types.contains(r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#));
        assert!(types.contains(r#"<Override PartName="/docProps/core.xml""#));
        assert!(
            !types.contains("image1.png"),
            "mídia usa o tipo padrão da extensão"
        );
    }

    #[test]
    fn relationships_are_written_next_to_their_source() {
        let files = package().into_files();
        let root = text(&files, "_rels/.rels");
        assert!(root.contains(r#"Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml""#));
        assert!(root.contains("docProps/core.xml") && root.contains("docProps/app.xml"));
        let part = text(&files, "xl/_rels/workbook.xml.rels");
        assert!(part.contains(r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>"#));
    }

    #[test]
    fn properties_escape_the_title_and_name_the_application() {
        let files = package().into_files();
        assert!(
            text(&files, "docProps/core.xml").contains("<dc:title>Relatório &amp; cia</dc:title>")
        );
        assert!(text(&files, "docProps/app.xml").contains("<Application>RustPDF</Application>"));
    }

    #[test]
    fn finish_zips_all_files() {
        let zip = package().finish().unwrap();
        assert!(zip.starts_with(b"PK\x03\x04"));
        assert_eq!(package().finish().unwrap(), zip, "reprodutível");
    }
}
