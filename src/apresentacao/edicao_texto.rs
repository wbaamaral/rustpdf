// SPDX-License-Identifier: AGPL-3.0-or-later
//! Edição de texto no lugar: onde e com que tamanho o campo de edição aparece
//! sobre a página, cobrindo o trecho de texto selecionado.

use crate::dominio::geometria::Frame;

/// Folga interna do campo, em pixels.
const PADDING: f64 = 3.0;
/// Fração da altura da moldura ocupada pelo corpo da fonte.
const FONT_RATIO: f64 = 0.8;
/// Largura média de um caractere, em fração do corpo da fonte.
const CHAR_WIDTH: f64 = 0.55;
/// Inclinação máxima (graus) para editar no lugar; acima, o campo horizontal
/// não cobriria o texto e a edição usa o diálogo.
const MAX_ANGLE: f64 = 1.0;

/// Caixa do campo de edição em pixels do widget da página.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineBox {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// Corpo da fonte do campo, em pixels.
    pub font_px: f64,
}

/// Caixa do campo sobre a moldura do texto. `to_widget` leva um ponto da
/// página (pontos, origem embaixo) aos pixels do widget (origem no topo).
/// `None` para texto girado ou espelhado.
pub fn inline_box(
    frame: &Frame,
    text: &str,
    to_widget: impl Fn((f64, f64)) -> (f64, f64),
) -> Option<InlineBox> {
    let angle = frame.u.1.atan2(frame.u.0).to_degrees();
    let upright = frame.u.0 * frame.v.1 - frame.u.1 * frame.v.0 > 0.0;
    if angle.abs() > MAX_ANGLE || !upright {
        return None;
    }
    let (ax, ay) = to_widget(frame.point(0.0, 0.0));
    let (bx, by) = to_widget(frame.point(1.0, 1.0));
    let values = [ax, ay, bx, by];
    if values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let (left, top) = (ax.min(bx), ay.min(by));
    let (width, height) = ((bx - ax).abs(), (by - ay).abs());
    let font_px = (height * FONT_RATIO).clamp(6.0, 160.0);
    let characters = text.chars().count().max(1) as f64;
    let field_width =
        (width.max(characters * font_px * CHAR_WIDTH) + 2.0 * PADDING + 8.0).max(60.0);
    let field_height = height.max(font_px * 1.3) + 2.0 * PADDING;
    Some(InlineBox {
        x: (left - PADDING).round() as i32,
        y: (top + height / 2.0 - field_height / 2.0).round() as i32,
        width: field_width.round() as i32,
        height: field_height.round() as i32,
        font_px,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::geometria::Matrix;

    /// Página de 800 pt mostrada em escala 2, sem margem.
    fn to_widget((x, y): (f64, f64)) -> (f64, f64) {
        (x * 2.0, (800.0 - y) * 2.0)
    }

    fn frame(matrix: Matrix) -> Frame {
        Frame::from_matrix(&matrix)
    }

    #[test]
    fn covers_horizontal_text_with_a_matching_font() {
        // Texto em (100, 700), 200 × 12 pt.
        let text = frame([200.0, 0.0, 0.0, 12.0, 100.0, 700.0]);
        let field = inline_box(&text, "Contrato", to_widget).unwrap();
        assert_eq!(field.x, 197);
        assert!((field.font_px - 19.2).abs() < 1e-9);
        assert!(field.width >= 400, "{field:?}");
        // Centrado na altura do texto: de 176 a 200 px no widget.
        let middle = field.y as f64 + field.height as f64 / 2.0;
        assert!((middle - 188.0).abs() <= 1.0, "{field:?}");
        assert!(field.height as f64 >= 24.0);
    }

    #[test]
    fn grows_with_the_text_and_keeps_a_minimum_size() {
        let short = frame([4.0, 0.0, 0.0, 2.0, 10.0, 10.0]);
        let field = inline_box(&short, "a", to_widget).unwrap();
        assert_eq!(field.font_px, 6.0);
        assert_eq!(field.width, 60);
        let long = inline_box(&short, &"x".repeat(100), to_widget).unwrap();
        assert!(long.width > 300, "{long:?}");
    }

    #[test]
    fn rotated_or_mirrored_text_uses_the_dialog() {
        let angle = 30f64.to_radians();
        let (c, s) = (angle.cos(), angle.sin());
        let rotated = frame([100.0 * c, 100.0 * s, -12.0 * s, 12.0 * c, 100.0, 100.0]);
        assert!(inline_box(&rotated, "x", to_widget).is_none());
        let mirrored = frame([100.0, 0.0, 0.0, -12.0, 100.0, 100.0]);
        assert!(inline_box(&mirrored, "x", to_widget).is_none());
        let broken = frame([f64::NAN, 0.0, 0.0, 12.0, 0.0, 0.0]);
        assert!(inline_box(&broken, "x", to_widget).is_none());
    }
}
