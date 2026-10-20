// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tabelas exportadas → planilha Excel (SpreadsheetML): uma aba por tabela.

use super::ooxml::{Package, OFFICE_DOCUMENT, STYLES, XML_HEADER};
use crate::dominio::exportacao::modelo::Document;
use crate::dominio::exportacao::numeros::{cell_value, CellValue};
use crate::infra::xml::escape;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";

/// Estilos (`cellXfs`): 0 normal, 1 cabeçalho em negrito, 2 porcentagem, 3 reais.
const HEADER: u8 = 1;
const PERCENT: u8 = 2;
const CURRENCY: u8 = 3;

const STYLESHEET: &str = concat!(
    "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    "<numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"&quot;R$&quot; #,##0.00\"/></numFmts>",
    "<fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font>",
    "<font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>",
    "<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill>",
    "<fill><patternFill patternType=\"gray125\"/></fill></fills>",
    "<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>",
    "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
    "<cellXfs count=\"4\">",
    "<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>",
    "<xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>",
    "<xf numFmtId=\"10\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/>",
    "<xf numFmtId=\"164\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/>",
    "</cellXfs></styleSheet>",
);

/// Planilha `.xlsx` com uma aba por tabela do documento.
pub fn render(document: &Document) -> Result<Vec<u8>, String> {
    package(document).finish()
}

/// O pacote antes do ZIP (testável parte a parte).
pub fn package(document: &Document) -> Package {
    let mut package = Package::new("Tabelas exportadas pelo RustPDF");
    let workbook = "xl/workbook.xml";
    let mut sheets = String::new();
    for (index, table) in document.tables().iter().enumerate() {
        let number = index + 1;
        let path = format!("worksheets/sheet{number}.xml");
        package.add_part(
            &format!("xl/{path}"),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
            worksheet(table.rows).into_bytes(),
        );
        let id = package.relate(Some(workbook), WORKSHEET, &path);
        // "Pág. N – Tabela M" cabe nos 31 caracteres e não usa []:*?/\.
        let name = format!("Pág. {} – Tabela {}", table.page, table.index);
        sheets.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{number}\" r:id=\"{id}\"/>",
            escape(&name)
        ));
    }
    package.add_part(
        workbook,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        format!(
            "{XML_HEADER}<workbook xmlns=\"{MAIN}\" xmlns:r=\"{RELATIONSHIPS}\"><sheets>{sheets}</sheets></workbook>"
        )
        .into_bytes(),
    );
    package.add_part(
        "xl/styles.xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        format!("{XML_HEADER}{STYLESHEET}").into_bytes(),
    );
    package.relate(Some(workbook), STYLES, "styles.xml");
    package.relate(None, OFFICE_DOCUMENT, workbook);
    package
}

/// Aba com as linhas da tabela; a primeira é o cabeçalho.
fn worksheet(rows: &[Vec<String>]) -> String {
    let mut data = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        data.push_str(&format!("<row r=\"{}\">", row_index + 1));
        for (column, text) in row.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            let reference = format!("{}{}", column_name(column), row_index + 1);
            data.push_str(&cell(&reference, text, row_index == 0));
        }
        data.push_str("</row>");
    }
    format!("{XML_HEADER}<worksheet xmlns=\"{MAIN}\"><sheetData>{data}</sheetData></worksheet>")
}

fn cell(reference: &str, text: &str, header: bool) -> String {
    let inline = |style: &str| {
        let space = if text.trim() != text {
            " xml:space=\"preserve\""
        } else {
            ""
        };
        format!(
            "<c r=\"{reference}\"{style} t=\"inlineStr\"><is><t{space}>{}</t></is></c>",
            escape(text)
        )
    };
    if header {
        return inline(&format!(" s=\"{HEADER}\""));
    }
    let number = |value: f64, style: Option<u8>| {
        let style = style.map(|s| format!(" s=\"{s}\"")).unwrap_or_default();
        format!("<c r=\"{reference}\"{style}><v>{value}</v></c>")
    };
    match cell_value(text) {
        CellValue::Number(value) => number(value, None),
        CellValue::Percent(value) => number(value, Some(PERCENT)),
        CellValue::Currency(value) => number(value, Some(CURRENCY)),
        CellValue::Text(_) => inline(""),
    }
}

/// Letras da coluna: 0 → A, 25 → Z, 26 → AA.
fn column_name(index: usize) -> String {
    let mut name = Vec::new();
    let mut n = index + 1;
    while n > 0 {
        let remainder = (n - 1) % 26;
        name.push(b'A' + remainder as u8);
        n = (n - 1) / 26;
    }
    name.reverse();
    String::from_utf8(name).expect("letras ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::exportacao::modelo::{Block, Document, PageContent, A4};

    fn document() -> Document {
        let row = |cells: &[&str]| cells.iter().map(|c| c.to_string()).collect::<Vec<_>>();
        Document {
            pages: vec![PageContent {
                number: 2,
                size: A4,
                blocks: vec![
                    Block::Table {
                        rows: vec![
                            row(&["Item", "Valor", "Taxa", "Obs"]),
                            row(&["Lápis & cia", "1.234,56", "15%", ""]),
                            row(&["Caneta", "R$ 2,50", "", "<frágil>"]),
                        ],
                    },
                    Block::Table {
                        rows: vec![row(&["a"]), row(&["b"])],
                    },
                ],
            }],
        }
    }

    fn text(files: &[(String, Vec<u8>)], path: &str) -> String {
        let (_, data) = files.iter().find(|(name, _)| name == path).expect(path);
        String::from_utf8(data.clone()).unwrap()
    }

    #[test]
    fn one_sheet_per_table_named_by_page_and_position() {
        let files = package(&document()).into_files();
        let workbook = text(&files, "xl/workbook.xml");
        assert!(workbook.contains(r#"<sheet name="Pág. 2 – Tabela 1" sheetId="1" r:id="rId1"/>"#));
        assert!(workbook.contains(r#"<sheet name="Pág. 2 – Tabela 2" sheetId="2" r:id="rId2"/>"#));
        let rels = text(&files, "xl/_rels/workbook.xml.rels");
        assert!(rels.contains(r#"Target="worksheets/sheet1.xml""#));
        assert!(rels.contains(r#"Target="styles.xml""#));
        let types = text(&files, "[Content_Types].xml");
        assert!(types.contains("/xl/worksheets/sheet2.xml"));
        assert!(text(&files, "_rels/.rels").contains(r#"Target="xl/workbook.xml""#));
    }

    #[test]
    fn cells_keep_numbers_currency_percent_and_text() {
        let files = package(&document()).into_files();
        let sheet = text(&files, "xl/worksheets/sheet1.xml");
        assert!(
            sheet.contains(r#"<c r="A1" s="1" t="inlineStr"><is><t>Item</t></is></c>"#),
            "cabeçalho em negrito"
        );
        assert!(sheet.contains(r#"<c r="A2" t="inlineStr"><is><t>Lápis &amp; cia</t></is></c>"#));
        assert!(sheet.contains(r#"<c r="B2"><v>1234.56</v></c>"#));
        assert!(sheet.contains(r#"<c r="C2" s="2"><v>0.15</v></c>"#));
        assert!(sheet.contains(r#"<c r="B3" s="3"><v>2.5</v></c>"#));
        assert!(sheet.contains(r#"<c r="D3" t="inlineStr"><is><t>&lt;frágil&gt;</t></is></c>"#));
        assert!(!sheet.contains(r#"r="D2""#), "célula vazia não é escrita");
        assert!(text(&files, "xl/styles.xml").contains(r#"formatCode="&quot;R$&quot; #,##0.00""#));
    }

    #[test]
    fn column_letters_follow_the_spreadsheet_convention() {
        let names: Vec<String> = [0, 25, 26, 701, 702]
            .iter()
            .map(|&i| column_name(i))
            .collect();
        assert_eq!(names, ["A", "Z", "AA", "ZZ", "AAA"]);
    }

    #[test]
    fn render_produces_a_zip() {
        assert!(render(&document()).unwrap().starts_with(b"PK\x03\x04"));
    }
}
