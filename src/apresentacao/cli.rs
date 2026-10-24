// SPDX-License-Identifier: AGPL-3.0-or-later
//! Argumentos da CLI → comandos, sem executar nada.
use std::path::PathBuf;

pub const EXPORT_USAGE: &str = "Uso: rustpdf-cli export ENTRADA.pdf SAIDA.{md,docx,pptx,odt,odg,csv,xlsx} [--paginas 1-3,7] [--para-ia] [--sem-imagens] [--quebrar-paginas] [--fiel] [--ocr IDIOMA] [--separador ;] [--sem-bom]";

pub use crate::app::exportar::{ExportFormat, ExportRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportCommand {
    pub input: PathBuf,
    pub request: ExportRequest,
}

/// `ENTRADA SAIDA [--paginas LISTA] [--separador C] [--sem-bom]` (o que
/// vem depois de `export`).
pub fn parse_export(args: &[String]) -> Result<ExportCommand, String> {
    let [input, output, options @ ..] = args else {
        return Err(EXPORT_USAGE.into());
    };
    let format = ExportFormat::from_output(output)?;
    let mut command = ExportRequest::new(output.into(), format);
    let mut csv_option = false;
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--paginas" => {
                let value = options
                    .next()
                    .ok_or("--paginas precisa de um valor (ex.: 1-3,7)")?;
                command.pages = Some(value.clone());
            }
            "--separador" => {
                let invalid = "--separador precisa de um caractere (ex.: \";\", \",\" ou tab)";
                let value = options.next().ok_or(invalid)?;
                command.separator = match value.as_str() {
                    "tab" | "\\t" => '\t',
                    other => {
                        let mut characters = other.chars();
                        match (characters.next(), characters.next()) {
                            (Some(character), None) => character,
                            _ => return Err(invalid.into()),
                        }
                    }
                };
                csv_option = true;
            }
            "--quebrar-paginas" => {
                if !matches!(format, ExportFormat::Docx | ExportFormat::Odt) {
                    return Err("--quebrar-paginas só vale para .docx e .odt".into());
                }
                command.page_breaks = true;
            }
            "--fiel" => {
                if !matches!(format, ExportFormat::Pptx | ExportFormat::Odg) {
                    return Err("--fiel só vale para .pptx e .odg".into());
                }
                command.faithful = true;
            }
            "--para-ia" | "--ai" => {
                if format != ExportFormat::Markdown {
                    return Err("--para-ia só vale para .md".into());
                }
                command.ai_optimized = true;
                command.images = false;
            }
            "--sem-imagens" => {
                if !matches!(
                    format,
                    ExportFormat::Markdown | ExportFormat::Docx | ExportFormat::Odt
                ) {
                    return Err("--sem-imagens só vale para .md, .docx e .odt".into());
                }
                command.images = false;
            }
            "--ocr" => {
                let invalid = "--ocr precisa do idioma do Tesseract (ex.: por, eng, por+eng)";
                let language = options.next().ok_or(invalid)?;
                let valid = !language.is_empty()
                    && language
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '_' | '-'));
                if !valid {
                    return Err(invalid.into());
                }
                command.ocr = Some(language.clone());
            }
            "--sem-bom" => {
                command.bom = false;
                csv_option = true;
            }
            other => return Err(format!("Opção desconhecida: {other}")),
        }
    }
    if csv_option && format != ExportFormat::Csv {
        return Err("--separador e --sem-bom só valem para .csv".into());
    }
    Ok(ExportCommand {
        input: input.into(),
        request: command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn export_format_comes_from_the_output_extension() {
        let command = parse_export(&args(&["entrada.pdf", "saida.MD"])).unwrap();
        assert_eq!(command.input, PathBuf::from("entrada.pdf"));
        assert_eq!(
            command.request,
            ExportRequest::new("saida.MD".into(), ExportFormat::Markdown)
        );
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.odp"])).unwrap_err(),
            "Formato de exportação não suportado: b.odp (disponíveis: .md, .csv, .xlsx, .docx, .pptx, .odt, .odg)"
        );
        assert!(parse_export(&args(&["a.pdf", "sem-extensao"])).is_err());
    }

    #[test]
    fn export_accepts_a_page_range() {
        let command = parse_export(&args(&["a.pdf", "b.md", "--paginas", "1-3,7"])).unwrap();
        assert_eq!(command.request.pages.as_deref(), Some("1-3,7"));
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.md", "--paginas"])).unwrap_err(),
            "--paginas precisa de um valor (ex.: 1-3,7)"
        );
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.md", "--x"])).unwrap_err(),
            "Opção desconhecida: --x"
        );
        assert_eq!(parse_export(&args(&["a.pdf"])).unwrap_err(), EXPORT_USAGE);
    }

    #[test]
    fn csv_export_accepts_separator_and_bom_options() {
        let command = parse_export(&args(&["a.pdf", "b.csv"])).unwrap();
        assert_eq!(command.request.format, ExportFormat::Csv);
        assert_eq!(
            (command.request.separator, command.request.bom),
            (';', true)
        );
        let command =
            parse_export(&args(&["a.pdf", "b.csv", "--separador", ",", "--sem-bom"])).unwrap();
        assert_eq!(
            (command.request.separator, command.request.bom),
            (',', false)
        );
        let tab = parse_export(&args(&["a.pdf", "b.csv", "--separador", "tab"])).unwrap();
        assert_eq!(tab.request.separator, '\t');
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.csv", "--separador", ";;"])).unwrap_err(),
            "--separador precisa de um caractere (ex.: \";\", \",\" ou tab)"
        );
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.md", "--sem-bom"])).unwrap_err(),
            "--separador e --sem-bom só valem para .csv"
        );
    }

    #[test]
    fn docx_accepts_page_breaks() {
        let command = parse_export(&args(&["a.pdf", "b.docx", "--quebrar-paginas"])).unwrap();
        assert_eq!(
            (command.request.format, command.request.page_breaks),
            (ExportFormat::Docx, true)
        );
        let odt = parse_export(&args(&["a.pdf", "b.odt", "--quebrar-paginas"])).unwrap();
        assert_eq!(
            (odt.request.format, odt.request.page_breaks),
            (ExportFormat::Odt, true)
        );
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.md", "--quebrar-paginas"])).unwrap_err(),
            "--quebrar-paginas só vale para .docx e .odt"
        );
    }

    #[test]
    fn images_can_be_left_out_of_text_formats() {
        assert!(
            parse_export(&args(&["a.pdf", "b.md"]))
                .unwrap()
                .request
                .images
        );
        let command = parse_export(&args(&["a.pdf", "b.docx", "--sem-imagens"])).unwrap();
        assert!(!command.request.images);
        let odt = parse_export(&args(&["a.pdf", "b.odt", "--sem-imagens"])).unwrap();
        assert!(!odt.request.images);
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.csv", "--sem-imagens"])).unwrap_err(),
            "--sem-imagens só vale para .md, .docx e .odt"
        );
    }

    #[test]
    fn pptx_is_editable_unless_faithful() {
        let command = parse_export(&args(&["a.pdf", "b.pptx"])).unwrap();
        assert_eq!(
            (command.request.format, command.request.faithful),
            (ExportFormat::Pptx, false)
        );
        assert!(
            parse_export(&args(&["a.pdf", "b.pptx", "--fiel"]))
                .unwrap()
                .request
                .faithful
        );
        let odg = parse_export(&args(&["a.pdf", "b.odg", "--fiel"])).unwrap();
        assert_eq!(
            (odg.request.format, odg.request.faithful),
            (ExportFormat::Odg, true)
        );
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.docx", "--fiel"])).unwrap_err(),
            "--fiel só vale para .pptx e .odg"
        );
    }

    #[test]
    fn ocr_takes_a_language() {
        let command = parse_export(&args(&["a.pdf", "b.docx", "--ocr", "por+eng"])).unwrap();
        assert_eq!(command.request.ocr.as_deref(), Some("por+eng"));
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.md", "--ocr"])).unwrap_err(),
            "--ocr precisa do idioma do Tesseract (ex.: por, eng, por+eng)"
        );
        assert_eq!(
            parse_export(&args(&["a.pdf", "b.md", "--ocr", "p;rm"])).unwrap_err(),
            "--ocr precisa do idioma do Tesseract (ex.: por, eng, por+eng)"
        );
    }

    #[test]
    fn ai_optimization_option_in_cli() {
        let command = parse_export(&args(&["a.pdf", "b.md", "--para-ia"])).unwrap();
        assert!(command.request.ai_optimized);
        assert!(!command.request.images);

        let command_alias = parse_export(&args(&["a.pdf", "b.md", "--ai"])).unwrap();
        assert!(command_alias.request.ai_optimized);

        assert_eq!(
            parse_export(&args(&["a.pdf", "b.docx", "--para-ia"])).unwrap_err(),
            "--para-ia só vale para .md"
        );
    }
}
