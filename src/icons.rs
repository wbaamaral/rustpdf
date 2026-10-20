// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ícones simbólicos próprios do RustPDF (16×16), para ferramentas de PDF que
//! o tema de ícones do sistema não cobre. Desenhos originais, em geometria
//! preenchida: o GTK 3 recolore ícones `-symbolic` forçando `fill` com a cor do
//! tema, então traços (`stroke`) virariam formas sólidas. Contornos usam
//! `fill-rule="evenodd"` (forma externa menos a interna).

use std::fs;
use std::path::PathBuf;

/// Ícone colorido do aplicativo (janela, barra de tarefas, Sobre).
pub const APP_ICON_NAME: &str = "rustpdf";
const APP_ICON: &str = include_str!("../packaging/rustpdf.svg");

/// Página com dobra, só o contorno (usada por vários ícones).
const PAGE: &str = "M3 1h7l3 3v11H3z M4.5 2.5v11h7V4.6L9.4 2.5z";

/// `(nome, desenho)`; o arquivo final é `<nome>.svg`.
pub const ICONS: &[(&str, &str)] = &[
    // --- Ações gerais (mesmo traço dos ícones de PDF, independentes do tema) ---
    (
        "rustpdf-new-symbolic",
        r#"<path fill-rule="evenodd" d="M2 2h6l3 3v10H2z M3.5 3.5v10h6V5.6L7.4 3.5z"/><path d="M12.25 .5h1.5v2.25H16v1.5h-2.25V6.5h-1.5V4.25H10v-1.5h2.25z"/>"#,
    ),
    (
        "rustpdf-open-symbolic",
        r#"<path fill-rule="evenodd" d="M1 2.5h5l1.5 1.5H15V14H1z M2.5 6v6.5h11V6z"/>"#,
    ),
    (
        "rustpdf-save-symbolic",
        r#"<path fill-rule="evenodd" d="M2 1h10.5L15 3.5V15H2z M3.5 2.5v11h10V4.1L11.9 2.5z"/><path d="M5 2h5v3.5H5z"/><path fill-rule="evenodd" d="M5 9h6v5H5z M6.5 10.5v3h3v-3z"/>"#,
    ),
    (
        "rustpdf-save-as-symbolic",
        r#"<path fill-rule="evenodd" d="M1 1h8.5L12 3.5V7h-1.5V4.1L8.9 2.5H2.5v10H6V14H1z"/><path d="M4 2h4v3H4z"/><path d="M15.5 9.6l-5.5 5.5H8.4v-1.6l5.5-5.5z"/>"#,
    ),
    (
        "rustpdf-undo-symbolic",
        r#"<path d="M5 2L1 6l4 4V7h4.5a3 3 0 0 1 0 6H6v1.5h3.5a4.5 4.5 0 0 0 0-9H5z"/>"#,
    ),
    (
        "rustpdf-redo-symbolic",
        r#"<path d="M11 2l4 4-4 4V7H6.5a3 3 0 0 0 0 6H10v1.5H6.5a4.5 4.5 0 0 1 0-9H11z"/>"#,
    ),
    (
        "rustpdf-delete-symbolic",
        r#"<path d="M5.5 1h5v2H14v1.5H2V3h3.5z"/><path fill-rule="evenodd" d="M3 5.5h10l-1 9.5H4z M4.7 7l.7 6.5h5.2L11.3 7z"/><path d="M6.6 8h1v4.5h-1z M8.4 8h1v4.5h-1z"/>"#,
    ),
    (
        "rustpdf-first-symbolic",
        r#"<path d="M2 1h12v1.5H2z"/><path d="M8 4l4.5 4.5-1.06 1.06-2.69-2.69V15h-1.5V6.87L4.56 9.56 3.5 8.5z"/>"#,
    ),
    (
        "rustpdf-previous-symbolic",
        r#"<path d="M8 2l4.5 4.5-1.06 1.06-2.69-2.69V14h-1.5V4.87L4.56 7.56 3.5 6.5z"/>"#,
    ),
    (
        "rustpdf-next-symbolic",
        r#"<path d="M8 14l4.5-4.5-1.06-1.06-2.69 2.69V2h-1.5v9.13L4.56 8.44 3.5 9.5z"/>"#,
    ),
    (
        "rustpdf-last-symbolic",
        r#"<path d="M2 13.5h12V15H2z"/><path d="M8 12l4.5-4.5-1.06-1.06-2.69 2.69V1h-1.5v8.13L4.56 6.44 3.5 7.5z"/>"#,
    ),
    (
        "rustpdf-zoom-in-symbolic",
        r#"<path fill-rule="evenodd" d="M6.5 1a5.5 5.5 0 0 1 4.37 8.85l4.13 4.12-1.06 1.06-4.12-4.13A5.5 5.5 0 1 1 6.5 1z M6.5 2.5a4 4 0 1 0 0 8 4 4 0 0 0 0-8z"/><path d="M5.75 4h1.5v1.75H9v1.5H7.25V9h-1.5V7.25H4v-1.5h1.75z"/>"#,
    ),
    (
        "rustpdf-zoom-out-symbolic",
        r#"<path fill-rule="evenodd" d="M6.5 1a5.5 5.5 0 0 1 4.37 8.85l4.13 4.12-1.06 1.06-4.12-4.13A5.5 5.5 0 1 1 6.5 1z M6.5 2.5a4 4 0 1 0 0 8 4 4 0 0 0 0-8z"/><path d="M4 5.75h5v1.5H4z"/>"#,
    ),
    (
        "rustpdf-zoom-actual-symbolic",
        r#"<path fill-rule="evenodd" d="M6.5 1a5.5 5.5 0 0 1 4.37 8.85l4.13 4.12-1.06 1.06-4.12-4.13A5.5 5.5 0 1 1 6.5 1z M6.5 2.5a4 4 0 1 0 0 8 4 4 0 0 0 0-8z"/><path d="M4.25 4.75h4.5V6h-4.5z M4.25 7h4.5v1.25h-4.5z"/>"#,
    ),
    (
        "rustpdf-rotate-left-symbolic",
        r#"<path d="M8 2.5a5.5 5.5 0 1 1-5.5 5.5H4a4 4 0 1 0 4-4v2.25L4.75 3.25 8 .25z"/>"#,
    ),
    (
        "rustpdf-rotate-right-symbolic",
        r#"<path d="M8 2.5a5.5 5.5 0 1 0 5.5 5.5H12a4 4 0 1 1-4-4v2.25l3.25-3L8 .25z"/>"#,
    ),
    (
        "rustpdf-insert-text-symbolic",
        r#"<path d="M1.5 2h10v2.75H10V3.5H7.25V13H9v1.5H4V13h1.75V3.5H3v1.25H1.5z"/><path d="M12 7.5h3.5v1h-1.25v4.5h1.25v1H12v-1h1.25V8.5H12z"/>"#,
    ),
    (
        "rustpdf-insert-image-symbolic",
        r#"<path fill-rule="evenodd" d="M1 2h14v12H1z M2.5 3.5v9h11v-9z"/><path d="M3.5 11.5l3-4 2 2.5 1.5-1.5 2.5 3z"/><path d="M11 4.5a1.25 1.25 0 1 1 0 2.5 1.25 1.25 0 0 1 0-2.5z"/>"#,
    ),
    (
        "rustpdf-checkbox-symbolic",
        r#"<path fill-rule="evenodd" d="M2 2h12v12H2z M3.5 3.5v9h9v-9z"/><path d="M11.2 5.2l1.06 1.06L7 11.5 4.2 8.7l1.06-1.06L7 9.4z"/>"#,
    ),
    (
        "rustpdf-radio-symbolic",
        r#"<path fill-rule="evenodd" d="M8 1.5a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13z M8 3a5 5 0 1 0 0 10 5 5 0 0 0 0-10z"/><path d="M8 5.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5z"/>"#,
    ),
    (
        "rustpdf-move-back-symbolic",
        r#"<path d="M2 8l4.5-4.5 1.06 1.06-2.69 2.69H14v1.5H4.87l2.69 2.69L6.5 12.5z"/>"#,
    ),
    (
        "rustpdf-move-forward-symbolic",
        r#"<path d="M14 8l-4.5-4.5-1.06 1.06 2.69 2.69H2v1.5h9.13l-2.69 2.69 1.06 1.06z"/>"#,
    ),
    // --- Ferramentas de PDF ---
    // Balão de nota.
    (
        "rustpdf-note-symbolic",
        r#"<path fill-rule="evenodd" d="M2 2h12v9H8.2L4.5 14.2V11H2z M3.5 3.5v6h2.5v1.4l1.6-1.4h4.9v-6z"/><path d="M5 5.5h6V7H5z"/>"#,
    ),
    // Marca-texto inclinado sobre uma faixa.
    (
        "rustpdf-highlight-symbolic",
        r#"<path d="M10.6 1.2l4.2 4.2-6.6 6.6H4v-4.2z"/><path d="M1 13.5h14V15H1z"/>"#,
    ),
    // Campo de texto: caixa com cursor.
    (
        "rustpdf-text-field-symbolic",
        r#"<path fill-rule="evenodd" d="M1 4h14v8H1z M2.5 5.5v5h11v-5z M4.25 6.5h1.5v3h-1.5z"/>"#,
    ),
    // Lista de opções: caixa com seta para baixo.
    (
        "rustpdf-choice-symbolic",
        r#"<path fill-rule="evenodd" d="M1 4h14v8H1z M2.5 5.5v5h11v-5z M9.5 7h3.5l-1.75 2.2z M4 7.25h4v1.5H4z"/>"#,
    ),
    // Recorte: duas cantoneiras cruzadas.
    (
        "rustpdf-crop-symbolic",
        r#"<path d="M4 1h1.5v9.5H15V12H4z"/><path d="M1 4h11v11h-1.5V5.5H1z"/>"#,
    ),
    // Página com +.
    (
        "rustpdf-page-add-symbolic",
        r#"<path fill-rule="evenodd" d="PAGE M7.25 6h1.5v1.75h1.75v1.5H8.75V11h-1.5V9.25H5.5v-1.5h1.75z"/>"#,
    ),
    // Página com −.
    (
        "rustpdf-page-remove-symbolic",
        r#"<path fill-rule="evenodd" d="PAGE M5.5 7.75h5v1.5h-5z"/>"#,
    ),
    // Página e seta saindo (extrair).
    (
        "rustpdf-page-extract-symbolic",
        r#"<path fill-rule="evenodd" d="M1 2h5l3 3v10H1z M2.5 3.5v10h5V5.6L5.4 3.5z"/><path d="M10 7.25h3V5l3 3-3 3V8.75h-3z"/>"#,
    ),
    // Seta entrando na página (inserir páginas de outro PDF).
    (
        "rustpdf-page-import-symbolic",
        r#"<path fill-rule="evenodd" d="M1 2h5l3 3v10H1z M2.5 3.5v10h5V5.6L5.4 3.5z"/><path d="M16 7.25h-3V5l-3 3 3 3V8.75h3z"/>"#,
    ),
    // Página com seta circular (girar página).
    (
        "rustpdf-page-rotate-symbolic",
        r#"<path fill-rule="evenodd" d="PAGE"/><path d="M6 8a2 2 0 1 1 2 2v1.5A3.5 3.5 0 1 0 4.5 8H3l2.25 2.5L7.5 8z"/>"#,
    ),
    // OCR: cantoneiras de digitalização e um T.
    (
        "rustpdf-ocr-symbolic",
        r#"<path d="M1 1h4v1.5H2.5V5H1z M11 1h4v4h-1.5V2.5H11z M1 11h1.5v2.5H5V15H1z M13.5 11H15v4h-4v-1.5h2.5z"/><path d="M5 5h6v1.5H8.75V12h-1.5V6.5H5z"/>"#,
    ),
    // OCR pesquisável: cantoneiras e lupa.
    (
        "rustpdf-ocr-search-symbolic",
        r#"<path d="M1 1h4v1.5H2.5V5H1z M11 1h4v4h-1.5V2.5H11z M1 11h1.5v2.5H5V15H1z"/><path fill-rule="evenodd" d="M7.5 4a3.5 3.5 0 0 1 2.9 5.45l3.3 3.3-1.06 1.06-3.3-3.3A3.5 3.5 0 1 1 7.5 4z M7.5 5.5a2 2 0 1 0 0 4 2 2 0 0 0 0-4z"/>"#,
    ),
    // Mão (rolar a página).
    (
        "rustpdf-hand-symbolic",
        r#"<path d="M4 6.5a1 1 0 0 1 2 0V8h.5V3a1 1 0 0 1 2 0v5H9V2.5a1 1 0 0 1 2 0V8h.5V4a1 1 0 0 1 2 0v6.5c0 2.5-1.8 4.5-4.5 4.5H8.2c-1.3 0-2.4-.6-3.1-1.6L2.3 9.6a1 1 0 0 1 1.5-1.3L4 8.6z"/>"#,
    ),
    // Ajustar à largura: setas para os lados entre barras.
    (
        "rustpdf-fit-width-symbolic",
        r#"<path d="M1 2h1.5v12H1z M13.5 2H15v12h-1.5z"/><path d="M3.5 8l3-3v2.25h3V5l3 3-3 3V8.75h-3V11z"/>"#,
    ),
    // Ajustar à página: página dentro de cantoneiras.
    (
        "rustpdf-fit-page-symbolic",
        r#"<path d="M1 1h4v1.5H2.5V5H1z M11 1h4v4h-1.5V2.5H11z M1 11h1.5v2.5H5V15H1z M13.5 11H15v4h-4v-1.5h2.5z"/><path fill-rule="evenodd" d="M5 4h6v8H5z M6.5 5.5v5h3v-5z"/>"#,
    ),
    // Seleção de objeto: seta com moldura tracejada.
    (
        "rustpdf-select-symbolic",
        r#"<path d="M1 1h2v1.5H1z M4.5 1h2v1.5h-2z M8 1h2v1.5H8z M1 4h1.5v2H1z M1 7.5h1.5v2H1z"/><path d="M5 5l9 3.6-3.7 1.2 3.2 3.2-1.4 1.4-3.2-3.2L7.7 15z"/>"#,
    ),
    // Painel de páginas: duas páginas empilhadas.
    (
        "rustpdf-pages-symbolic",
        r#"<path fill-rule="evenodd" d="M5 1h9v11H5z M6.5 2.5v8h6v-8z"/><path d="M2 4h1.5v10.5H11V16H2z"/>"#,
    ),
];

/// Grava os ícones em `~/.cache/rustpdf/icons-<versão>` (`%LOCALAPPDATA%` no
/// Windows; ou na pasta
/// temporária) e devolve a pasta, para o GTK procurá-los pelo nome.
pub fn install() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    let directory = base
        .join("rustpdf")
        .join(format!("icons-{}", env!("CARGO_PKG_VERSION")));
    fs::create_dir_all(&directory).ok()?;
    // Ícones de versões anteriores não são mais usados.
    if let Some(parent) = directory.parent() {
        for entry in fs::read_dir(parent).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("icons-") && entry.path() != directory {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    let app_icon = directory.join(format!("{APP_ICON_NAME}.svg"));
    if fs::read_to_string(&app_icon).ok().as_deref() != Some(APP_ICON) {
        fs::write(&app_icon, APP_ICON).ok()?;
    }
    for (name, body) in ICONS {
        let body = body.replace("PAGE", PAGE);
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><g fill=\"#2e3436\">{body}</g></svg>\n"
        );
        let path = directory.join(format!("{name}.svg"));
        if fs::read_to_string(&path).ok().as_deref() != Some(svg.as_str()) {
            fs::write(&path, svg).ok()?;
        }
    }
    Some(directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_valid_symbolic_svgs() {
        let directory =
            std::env::temp_dir().join(format!("rustpdf-icons-test-{}", std::process::id()));
        std::env::set_var("XDG_CACHE_HOME", &directory);
        let installed = install().expect("instalar ícones");
        for (name, _) in ICONS {
            assert!(name.ends_with("-symbolic"), "{name}");
            let svg = fs::read_to_string(installed.join(format!("{name}.svg"))).unwrap();
            assert!(svg.starts_with("<svg") && svg.contains("<path"), "{name}");
            assert!(!svg.contains("PAGE") && !svg.contains("stroke"), "{name}");
        }
        assert!(
            installed.join("rustpdf.svg").exists(),
            "ícone do aplicativo"
        );
        let _ = fs::remove_dir_all(directory);
    }
}
