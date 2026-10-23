// SPDX-License-Identifier: AGPL-3.0-or-later
//! Regras dos campos de formulário: onde fica o rótulo visível de uma caixa
//! de seleção ou de uma opção de rádio e como uma lista aberta é desenhada.

/// Distância entre o widget e o rótulo, em pontos.
const LABEL_GAP: f64 = 6.0;

/// Rótulo ao lado de um widget `(x, y, largura, altura)` em pontos visíveis
/// (origem embaixo): posição da linha de base e corpo da fonte.
pub fn label_beside(x: f64, y: f64, width: f64, height: f64) -> (f64, f64, f64) {
    let size = (height * 0.7).clamp(8.0, 14.0);
    // Centrado na altura do widget: a linha de base fica ~0,35 do corpo abaixo do meio.
    let baseline = y + height / 2.0 - size * 0.35;
    (x + width + LABEL_GAP, baseline, size)
}

/// Altura de cada opção de uma lista aberta, em pontos (corpo 12).
pub const LIST_ROW_HEIGHT: f64 = 14.0;

/// Conteúdo da aparência de uma lista aberta (`/Ch` sem combo) de
/// `width` × `height`: as opções de cima para baixo, a selecionada com fundo
/// de destaque e texto branco, tudo recortado na caixa. `rows` são as
/// opções já codificadas na fonte `/RustPDFHelv` (WinAnsi), sem escape.
pub fn list_appearance(
    width: f64,
    height: f64,
    rows: &[Vec<u8>],
    selected: Option<usize>,
) -> Vec<u8> {
    let mut content = format!(
        "q 0 0 {width:.3} {height:.3} re W n {}",
        field_frame(width, height)
    )
    .into_bytes();
    for (index, row) in rows.iter().enumerate() {
        let top = height - 2.0 - index as f64 * LIST_ROW_HEIGHT;
        let bottom = top - LIST_ROW_HEIGHT;
        // Só opções inteiras (a primeira sempre): meia linha cortada confunde.
        if index > 0 && bottom < 0.0 {
            break;
        }
        let chosen = selected == Some(index);
        if chosen {
            content.extend_from_slice(
                format!(
                    "0.153 0.42 0.83 rg {} f\n",
                    rounded_rect(2.0, bottom, width - 4.0, LIST_ROW_HEIGHT, 2.0)
                )
                .as_bytes(),
            );
        }
        let color = if chosen { "1 g" } else { "0.1 g" };
        let baseline = bottom + 3.5;
        content.extend_from_slice(
            format!("BT /RustPDFHelv 12 Tf {color} 1 0 0 1 6 {baseline:.3} Tm (").as_bytes(),
        );
        push_literal(&mut content, row);
        content.extend_from_slice(b") Tj ET\n");
    }
    content.extend_from_slice(b"Q\n");
    content
}

/// Raio dos cantos de combos e listas, em pontos.
const CORNER: f64 = 3.0;

/// Retângulo de cantos arredondados como caminho PDF (curvas de Bézier).
fn rounded_rect(x: f64, y: f64, width: f64, height: f64, radius: f64) -> String {
    let r = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let k = r * 0.552_284_749_831;
    let (x1, y1) = (x + width, y + height);
    format!(
        "{:.3} {y:.3} m {:.3} {y:.3} l {:.3} {y:.3} {x1:.3} {:.3} {x1:.3} {:.3} c \
         {x1:.3} {:.3} l {x1:.3} {:.3} {:.3} {y1:.3} {:.3} {y1:.3} c \
         {:.3} {y1:.3} l {:.3} {y1:.3} {x:.3} {:.3} {x:.3} {:.3} c \
         {x:.3} {:.3} l {x:.3} {:.3} {:.3} {y:.3} {:.3} {y:.3} c h",
        x + r,
        x1 - r,
        x1 - r + k,
        y + r - k,
        y + r,
        y1 - r,
        y1 - r + k,
        x1 - r + k,
        x1 - r,
        x + r,
        x + r - k,
        y1 - r + k,
        y1 - r,
        y + r,
        y + r - k,
        x + r - k,
        x + r,
    )
}

/// Fundo branco e borda cinza suave de cantos arredondados (combo e lista).
fn field_frame(width: f64, height: f64) -> String {
    let path = rounded_rect(0.5, 0.5, width - 1.0, height - 1.0, CORNER);
    format!("1 g {path} f 0.62 G 0.8 w {path} S\n")
}

/// Texto escapado para string literal PDF.
fn push_literal(content: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        if matches!(byte, b'(' | b')' | b'\\') {
            content.push(b'\\');
        }
        content.push(*byte);
    }
}

/// Conteúdo da aparência de uma caixa de seleção de `width` × `height`, no
/// mesmo traço do rádio: contorno de 1 pt com cantos levemente arredondados
/// e, marcada, um "✓" de pontas arredondadas centrado com folga.
pub fn checkbox_appearance(width: f64, height: f64, checked: bool) -> String {
    let side = width.min(height);
    let path = rounded_rect(0.5, 0.5, width - 1.0, height - 1.0, side * 0.12);
    let mut content = format!("q 1 g {path} f 0 G 1 w {path} S\n");
    if checked {
        let line = (side * 0.11).clamp(1.2, 2.4);
        content.push_str(&format!(
            "0 G {line:.3} w 1 J 1 j {:.3} {:.3} m {:.3} {:.3} l {:.3} {:.3} l S\n",
            width * 0.24,
            height * 0.52,
            width * 0.42,
            height * 0.3,
            width * 0.76,
            height * 0.72,
        ));
    }
    content.push_str("Q\n");
    content
}

/// Conteúdo da aparência de um combo (lista suspensa) de `width` ×
/// `height`: cantos arredondados, borda suave, o valor à esquerda
/// (recortado antes da seta) e a seta "⌄" traçada à direita — sem ela o
/// combo parecia um campo de texto. `value` já codificado na fonte
/// `/RustPDFHelv` (WinAnsi), sem escape.
pub fn combo_appearance(width: f64, height: f64, value: &[u8]) -> Vec<u8> {
    let button = height.clamp(12.0, 20.0).min(width / 2.0);
    let left = width - button;
    let size = (height * 0.55).clamp(6.0, 12.0);
    let baseline = (height - size * 0.7) / 2.0;
    let (cx, cy) = (left + button / 2.0, height / 2.0);
    let arrow = (button * 0.18).clamp(2.0, 3.5);
    let mut content = format!(
        "q 0 0 {width:.3} {height:.3} re W n {}\
         0.35 G 1.2 w 1 J 1 j {:.3} {:.3} m {cx:.3} {:.3} l {:.3} {:.3} l S\n\
         q 1 1 {:.3} {:.3} re W n BT /RustPDFHelv {size:.2} Tf 0.1 g 1 0 0 1 6 {baseline:.3} Tm (",
        field_frame(width, height),
        cx - arrow,
        cy + arrow * 0.5,
        cy - arrow * 0.5,
        cx + arrow,
        cy + arrow * 0.5,
        (left - 2.0).max(0.0),
        (height - 2.0).max(0.0),
    )
    .into_bytes();
    push_literal(&mut content, value);
    content.extend_from_slice(b") Tj ET Q Q\n");
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_shows_every_option_and_highlights_the_selected_one() {
        let rows = vec![b"SP".to_vec(), b"RJ".to_vec(), b"(MG)".to_vec()];
        let content = String::from_utf8(list_appearance(160.0, 60.0, &rows, Some(1))).unwrap();
        assert!(content.contains("(SP) Tj") && content.contains("(RJ) Tj"));
        assert!(content.contains("(\\(MG\\)) Tj"), "{content}");
        // Um só fundo de destaque, na segunda linha, e texto branco nela.
        assert_eq!(
            content.matches("0.153 0.42 0.83 rg").count(),
            1,
            "{content}"
        );
        assert!(
            content.contains("1 g 1 0 0 1 6 33.500 Tm (RJ)"),
            "{content}"
        );
        assert!(
            content.contains("0.1 g 1 0 0 1 6 47.500 Tm (SP)"),
            "{content}"
        );
        assert!(content.starts_with("q 0 0 160.000 60.000 re W n"));
        assert!(content.ends_with("Q\n"));
        // Opções que não cabem não são desenhadas; nada selecionado, sem destaque.
        let short = String::from_utf8(list_appearance(160.0, 20.0, &rows, None)).unwrap();
        assert!(short.contains("(SP)") && !short.contains("(RJ)"));
        assert_eq!(short.matches("0.153 0.42 0.83 rg").count(), 0);
    }

    #[test]
    fn combo_draws_value_and_a_chevron_in_a_rounded_frame() {
        let content = String::from_utf8(combo_appearance(180.0, 28.0, b"SP (x)")).unwrap();
        // Moldura arredondada (curvas) com borda cinza suave.
        assert!(content.contains("0.62 G 0.8 w"), "{content}");
        assert!(content.matches(" c ").count() >= 8, "{content}");
        // Seta "⌄" traçada com pontas arredondadas, centrada no botão de 20 pt.
        assert!(content.contains("0.35 G 1.2 w 1 J 1 j"), "{content}");
        assert!(content.contains("170.000 12.250 l"), "{content}");
        // Valor recortado antes da seta e escapado.
        assert!(
            content.contains("q 1 1 158.000 26.000 re W n BT"),
            "{content}"
        );
        assert!(content.contains("(SP \\(x\\)) Tj ET Q Q"), "{content}");
        // Campo estreito: a seta não passa de metade da largura.
        let narrow = String::from_utf8(combo_appearance(20.0, 28.0, b"")).unwrap();
        assert!(narrow.contains("15.000 13.000 l"), "{narrow}");
    }

    #[test]
    fn checkbox_matches_the_radio_stroke_and_marks_inside_the_box() {
        let off = checkbox_appearance(18.0, 18.0, false);
        assert!(off.contains("0 G 1 w"), "{off}");
        assert!(!off.contains(" 1 J "), "desmarcada não tem marca: {off}");
        let on = checkbox_appearance(18.0, 18.0, true);
        assert!(on.contains("1.980 w 1 J 1 j"), "{on}");
        // A marca fica dentro da caixa, longe da borda.
        assert!(
            on.contains("4.320 9.360 m 7.560 5.400 l 13.680 12.960 l S"),
            "{on}"
        );
        assert!(on.starts_with("q ") && on.ends_with("Q\n"));
    }

    #[test]
    fn rounded_rect_is_closed_and_clamps_the_radius() {
        let path = rounded_rect(0.0, 0.0, 4.0, 2.0, 3.0);
        assert!(
            path.starts_with("1.000 0.000 m") && path.ends_with(" h"),
            "{path}"
        );
    }

    #[test]
    fn label_sits_to_the_right_centered_on_the_widget() {
        let (x, y, size) = label_beside(72.0, 600.0, 18.0, 18.0);
        assert_eq!(x, 96.0);
        assert!((size - 12.6).abs() < 1e-9);
        assert!(y > 600.0 && y < 609.0, "{y}");
        assert_eq!(label_beside(0.0, 0.0, 10.0, 4.0).2, 8.0);
        assert_eq!(label_beside(0.0, 0.0, 10.0, 40.0).2, 14.0);
    }
}
