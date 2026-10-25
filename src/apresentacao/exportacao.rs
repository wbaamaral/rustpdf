// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exportação pela interface: ações do menu, perguntas e mensagens.
use std::path::PathBuf;

pub use crate::app::exportar::{ExportFormat, ExportRequest};
use crate::dominio::paginas::PageRange;

/// Ação do primeiro item "Documento como…" do menu Exportar; os demais
/// seguem a ordem de [`MENU`].
pub const FIRST_ACTION: usize = 70;

/// Itens do menu Exportar › Documento como…, na ordem.
pub const MENU: [ExportFormat; 7] = [
    ExportFormat::Docx,
    ExportFormat::Pptx,
    ExportFormat::Odt,
    ExportFormat::Odg,
    ExportFormat::Xlsx,
    ExportFormat::Markdown,
    ExportFormat::Csv,
];

pub fn action_format(action: usize) -> Option<ExportFormat> {
    action
        .checked_sub(FIRST_ACTION)
        .and_then(|index| MENU.get(index).copied())
}

/// Intervalo de identificadores de ação do menu Exportar › Documento como…
pub fn export_actions() -> std::ops::Range<usize> {
    FIRST_ACTION..FIRST_ACTION + MENU.len()
}

/// Pergunta sim/não da opção principal do formato, se houver.
pub fn question(format: ExportFormat) -> Option<&'static str> {
    match format {
        ExportFormat::Docx | ExportFormat::Odt => Some("Manter as quebras de página do PDF?"),
        ExportFormat::Pptx => {
            Some("Exportar cada página como imagem (aparência fiel, sem texto editável)?")
        }
        ExportFormat::Odg => None,
        ExportFormat::Markdown => Some("Incluir as imagens das páginas?"),
        ExportFormat::Csv | ExportFormat::Xlsx => None,
    }
}

/// Pergunta para otimização da exportação para consumo por agentes de IA.
pub fn ai_question() -> &'static str {
    "Otimizar a exportação para consumo por agentes de IA (remover ruídos, cabeçalhos repetidos e imagens)?"
}

/// Pedido com as páginas digitadas (vazio = todas), validadas contra
/// `total`, e a resposta à [`question`] do formato.
pub fn request(
    output: PathBuf,
    format: ExportFormat,
    pages: &str,
    answer: bool,
    total: usize,
) -> Result<ExportRequest, String> {
    let pages = pages.trim();
    PageRange::parse(pages, total)?;
    let mut request = ExportRequest::new(output, format);
    request.pages = (!pages.is_empty()).then(|| pages.to_string());
    match format {
        ExportFormat::Docx | ExportFormat::Odt => request.page_breaks = answer,
        ExportFormat::Pptx | ExportFormat::Odg => request.faithful = answer,
        ExportFormat::Markdown => request.images = answer,
        ExportFormat::Csv | ExportFormat::Xlsx => {}
    }
    Ok(request)
}

/// Ação de "Cancelar exportação", logo depois dos formatos no menu.
pub const CANCEL_ACTION: usize = FIRST_ACTION + MENU.len();

/// Barra de status durante a exportação (`done` páginas já lidas).
pub fn progress_message(format: ExportFormat, done: usize, total: usize) -> String {
    format!(
        "{} (Exportar › Cancelar exportação para parar)",
        cli_progress_message(format, done, total)
    )
}

/// Andamento da exportação na CLI.
pub fn cli_progress_message(format: ExportFormat, done: usize, total: usize) -> String {
    format!(
        "Exportando para {}… página {} de {total}",
        format.label(),
        done + 1
    )
}

/// Mensagem da barra de status ao terminar.
pub fn done_message(files: &[PathBuf]) -> String {
    let names: Vec<String> = files
        .iter()
        .map(|file| {
            file.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    match names.as_slice() {
        [one] => format!("Exportado: {one}"),
        _ => {
            let shown = names.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            let more = if names.len() > 3 { "…" } else { "" };
            format!("Exportados {} arquivos: {shown}{more}", names.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_actions_map_to_formats() {
        let formats: Vec<Option<ExportFormat>> = (69..=77).map(action_format).collect();
        assert_eq!(
            formats,
            [
                None,
                Some(ExportFormat::Docx),
                Some(ExportFormat::Pptx),
                Some(ExportFormat::Odt),
                Some(ExportFormat::Odg),
                Some(ExportFormat::Xlsx),
                Some(ExportFormat::Markdown),
                Some(ExportFormat::Csv),
                None
            ]
        );
        assert_eq!(FIRST_ACTION + 2, 72);
    }

    #[test]
    fn export_actions_covers_all_menu_items_and_excludes_cancel() {
        for (index, &format) in MENU.iter().enumerate() {
            let action = FIRST_ACTION + index;
            assert!(export_actions().contains(&action));
            assert_eq!(action_format(action), Some(format));
        }
        assert!(!export_actions().contains(&CANCEL_ACTION));
        assert_eq!(action_format(CANCEL_ACTION), None);
    }

    #[test]
    fn each_format_asks_its_own_question() {
        assert_eq!(
            question(ExportFormat::Docx),
            Some("Manter as quebras de página do PDF?")
        );
        assert_eq!(
            question(ExportFormat::Odt),
            Some("Manter as quebras de página do PDF?")
        );
        assert!(question(ExportFormat::Pptx).unwrap().contains("imagem"));
        assert_eq!(question(ExportFormat::Odg), None);
        assert_eq!(
            question(ExportFormat::Markdown),
            Some("Incluir as imagens das páginas?")
        );
        assert_eq!(question(ExportFormat::Csv), None);
        assert!(ai_question().contains("agentes de IA"));
    }

    #[test]
    fn request_applies_pages_and_the_answer() {
        let docx = request("a.docx".into(), ExportFormat::Docx, " 1-2 ", true, 3).unwrap();
        assert_eq!(
            (docx.pages.as_deref(), docx.page_breaks),
            (Some("1-2"), true)
        );
        let odt = request("a.odt".into(), ExportFormat::Odt, " 1 ", false, 3).unwrap();
        assert_eq!((odt.pages.as_deref(), odt.page_breaks), (Some("1"), false));
        let pptx = request("a.pptx".into(), ExportFormat::Pptx, "", true, 3).unwrap();
        assert_eq!((pptx.pages, pptx.faithful), (None, true));
        let odg = request("a.odg".into(), ExportFormat::Odg, "", false, 3).unwrap();
        assert_eq!((odg.pages, odg.faithful), (None, false));
        let markdown = request("a.md".into(), ExportFormat::Markdown, "", false, 3).unwrap();
        assert!(!markdown.images);
        assert_eq!(
            request("a.csv".into(), ExportFormat::Csv, "4", true, 3).unwrap_err(),
            "Página fora de 1..=3"
        );
    }

    #[test]
    fn done_message_names_the_files() {
        assert_eq!(done_message(&["/tmp/r.docx".into()]), "Exportado: r.docx");
        let many: Vec<PathBuf> = (1..=4)
            .map(|n| format!("/x/t-p1-t{n}.csv").into())
            .collect();
        assert_eq!(
            done_message(&many),
            "Exportados 4 arquivos: t-p1-t1.csv, t-p1-t2.csv, t-p1-t3.csv…"
        );
    }

    #[test]
    fn progress_and_cancel_messages() {
        assert_eq!(
            progress_message(ExportFormat::Docx, 1, 5),
            "Exportando para Word… página 2 de 5 (Exportar › Cancelar exportação para parar)"
        );
        assert_eq!(
            cli_progress_message(ExportFormat::Csv, 0, 1),
            "Exportando para CSV… página 1 de 1"
        );
        assert_eq!(CANCEL_ACTION, FIRST_ACTION + MENU.len());
        assert_eq!(action_format(CANCEL_ACTION), None);
    }
}
