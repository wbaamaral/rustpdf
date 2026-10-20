// SPDX-License-Identifier: AGPL-3.0-or-later
//! Geometria do PDF: matrizes afins (convenção de vetor-linha) e molduras
//! orientadas dos objetos selecionados.

/// Matriz afim do PDF `[a b c d e f]` (convenção de vetor-linha).
pub type Matrix = [f64; 6];

pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `first × second`: aplica `first` e depois `second`.
pub fn multiply(first: &Matrix, second: &Matrix) -> Matrix {
    [
        first[0] * second[0] + first[1] * second[2],
        first[0] * second[1] + first[1] * second[3],
        first[2] * second[0] + first[3] * second[2],
        first[2] * second[1] + first[3] * second[3],
        first[4] * second[0] + first[5] * second[2] + second[4],
        first[4] * second[1] + first[5] * second[3] + second[5],
    ]
}

/// Converte um deslocamento no espaço da página para o espaço local da matriz.
pub fn local_delta(ctm: &Matrix, dx: f64, dy: f64) -> Option<(f64, f64)> {
    let determinant = ctm[0] * ctm[3] - ctm[1] * ctm[2];
    if determinant.abs() < 1e-12 || !determinant.is_finite() {
        return None;
    }
    Some((
        (dx * ctm[3] - dy * ctm[2]) / determinant,
        (dy * ctm[0] - dx * ctm[1]) / determinant,
    ))
}

/// Inversa da matriz afim, se não for degenerada.
pub fn invert(matrix: &Matrix) -> Option<Matrix> {
    let [a, b, c, d, e, f] = *matrix;
    let determinant = a * d - b * c;
    if determinant.abs() < 1e-12 || !determinant.is_finite() {
        return None;
    }
    Some([
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
        (c * f - d * e) / determinant,
        (b * e - a * f) / determinant,
    ])
}

pub fn translation(dx: f64, dy: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, dx, dy]
}

pub fn apply(matrix: &Matrix, (x, y): (f64, f64)) -> (f64, f64) {
    (
        x * matrix[0] + y * matrix[2] + matrix[4],
        x * matrix[1] + y * matrix[3] + matrix[5],
    )
}

/// `T` expressa no espaço da página, reescrita no espaço local de `ctm`:
/// desenhar com `X cm` antes equivale a aplicar `T` depois de `ctm`.
pub fn conjugate(ctm: &Matrix, transform: &Matrix) -> Option<Matrix> {
    Some(multiply(&multiply(ctm, transform), &invert(ctm)?))
}

/// `T` dada no espaço local de `frame` (ex.: escala ao longo do eixo da
/// moldura), expressa no espaço da página: `frame⁻¹ · T · frame`.
/// Inversa de [`conjugate`].
pub fn from_local(frame: &Matrix, transform: &Matrix) -> Option<Matrix> {
    Some(multiply(&multiply(&invert(frame)?, transform), frame))
}

/// Escala `sx` × `sy` que mantém fixo o ponto `anchor`.
pub fn scale_about(sx: f64, sy: f64, (ax, ay): (f64, f64)) -> Matrix {
    [sx, 0.0, 0.0, sy, ax * (1.0 - sx), ay * (1.0 - sy)]
}

/// Segmento reto `(início, fim)` em pontos.
pub type Segment = ((f64, f64), (f64, f64));

/// Moldura orientada de um objeto: `origin + s·u + t·v`, com `s, t ∈ [0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub origin: (f64, f64),
    pub u: (f64, f64),
    pub v: (f64, f64),
}

impl Frame {
    /// Moldura alinhada aos eixos a partir de `[x0, y0, x1, y1]`.
    pub fn from_rect([x0, y0, x1, y1]: [f64; 4]) -> Self {
        Self {
            origin: (x0.min(x1), y0.min(y1)),
            u: ((x1 - x0).abs(), 0.0),
            v: (0.0, (y1 - y0).abs()),
        }
    }

    /// Quadrado unitário levado à página por `matrix` (imagens).
    pub fn from_matrix(matrix: &Matrix) -> Self {
        Self {
            origin: (matrix[4], matrix[5]),
            u: (matrix[0], matrix[1]),
            v: (matrix[2], matrix[3]),
        }
    }

    pub fn matrix(&self) -> Matrix {
        [
            self.u.0,
            self.u.1,
            self.v.0,
            self.v.1,
            self.origin.0,
            self.origin.1,
        ]
    }

    /// Ponto em coordenadas locais (`s`, `t`) da moldura.
    pub fn point(&self, s: f64, t: f64) -> (f64, f64) {
        apply(&self.matrix(), (s, t))
    }

    /// Coordenadas locais de um ponto da página.
    pub fn local(&self, point: (f64, f64)) -> Option<(f64, f64)> {
        Some(apply(&invert(&self.matrix())?, point))
    }

    pub fn contains(&self, point: (f64, f64), tolerance: f64) -> bool {
        let Some((s, t)) = self.local(point) else {
            return false;
        };
        let width = self.u.0.hypot(self.u.1).max(1e-9);
        let height = self.v.0.hypot(self.v.1).max(1e-9);
        let (ts, tt) = (tolerance / width, tolerance / height);
        (-ts..=1.0 + ts).contains(&s) && (-tt..=1.0 + tt).contains(&t)
    }

    pub fn transformed(&self, transform: &Matrix) -> Self {
        Self::from_matrix(&multiply(&self.matrix(), transform))
    }

    /// Escala ao longo dos eixos da moldura, mantendo fixo o ponto local
    /// `anchor` (`(0.5, 0.5)` = centro), expressa no espaço da página.
    /// Moldura degenerada: identidade.
    pub fn scaling_about(&self, sx: f64, sy: f64, anchor: (f64, f64)) -> Matrix {
        from_local(&self.matrix(), &scale_about(sx, sy, anchor)).unwrap_or(IDENTITY)
    }

    pub fn center(&self) -> (f64, f64) {
        self.point(0.5, 0.5)
    }

    /// Caixa alinhada aos eixos que contém a moldura.
    pub fn bounds(&self) -> [f64; 4] {
        let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for (s, t) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let (x, y) = self.point(s, t);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
        bounds
    }
}

/// Caixa alinhada aos eixos, normalizada (`min ≤ max`). Núcleo comum dos
/// retângulos tipados; o sistema de coordenadas é dado pelo tipo que a envolve.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Span {
    min: (f64, f64),
    max: (f64, f64),
}

impl Span {
    fn new(a: f64, b: f64, c: f64, d: f64) -> Self {
        Self {
            min: (a.min(c), b.min(d)),
            max: (a.max(c), b.max(d)),
        }
    }

    fn width(&self) -> f64 {
        self.max.0 - self.min.0
    }

    fn height(&self) -> f64 {
        self.max.1 - self.min.1
    }

    fn contains(&self, (x, y): (f64, f64)) -> bool {
        (self.min.0..=self.max.0).contains(&x) && (self.min.1..=self.max.1).contains(&y)
    }

    /// Distância euclidiana do ponto à caixa (0 dentro).
    fn distance_to(&self, (x, y): (f64, f64)) -> f64 {
        let dx = (self.min.0 - x).max(x - self.max.0).max(0.0);
        let dy = (self.min.1 - y).max(y - self.max.1).max(0.0);
        dx.hypot(dy)
    }

    fn union(&self, other: &Self) -> Self {
        Self {
            min: (self.min.0.min(other.min.0), self.min.1.min(other.min.1)),
            max: (self.max.0.max(other.max.0), self.max.1.max(other.max.1)),
        }
    }
}

/// Retângulo no espaço do PDF: pontos, origem **embaixo** à esquerda.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfRect(Span);

impl PdfRect {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self(Span::new(x0, y0, x1, y1))
    }

    pub fn from_array([x0, y0, x1, y1]: [f64; 4]) -> Self {
        Self::new(x0, y0, x1, y1)
    }

    /// `[x0, y0, x1, y1]`, com `x0 ≤ x1` e `y0 ≤ y1`.
    pub fn to_array(&self) -> [f64; 4] {
        [self.0.min.0, self.0.min.1, self.0.max.0, self.0.max.1]
    }

    /// O mesmo retângulo com origem no topo de uma página de `page_height`.
    pub fn to_layout(&self, page_height: f64) -> LayoutRect {
        let [x0, y0, x1, y1] = self.to_array();
        LayoutRect::new(x0, page_height - y1, x1, page_height - y0)
    }

    pub fn width(&self) -> f64 {
        self.0.width()
    }

    pub fn height(&self) -> f64 {
        self.0.height()
    }

    pub fn contains(&self, point: (f64, f64)) -> bool {
        self.0.contains(point)
    }

    pub fn distance_to(&self, point: (f64, f64)) -> f64 {
        self.0.distance_to(point)
    }

    pub fn union(&self, other: &Self) -> Self {
        Self(self.0.union(&other.0))
    }
}

/// Retângulo de layout (poppler, OCR): pontos, origem **no topo** à esquerda.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRect(Span);

impl LayoutRect {
    pub fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self(Span::new(left, top, right, bottom))
    }

    pub fn left(&self) -> f64 {
        self.0.min.0
    }

    pub fn top(&self) -> f64 {
        self.0.min.1
    }

    pub fn right(&self) -> f64 {
        self.0.max.0
    }

    pub fn bottom(&self) -> f64 {
        self.0.max.1
    }

    /// O mesmo retângulo com origem embaixo de uma página de `page_height`.
    pub fn to_pdf(&self, page_height: f64) -> PdfRect {
        PdfRect::new(
            self.left(),
            page_height - self.bottom(),
            self.right(),
            page_height - self.top(),
        )
    }

    pub fn width(&self) -> f64 {
        self.0.width()
    }

    pub fn height(&self) -> f64 {
        self.0.height()
    }

    pub fn contains(&self, point: (f64, f64)) -> bool {
        self.0.contains(point)
    }

    pub fn distance_to(&self, point: (f64, f64)) -> f64 {
        self.0.distance_to(point)
    }

    pub fn union(&self, other: &Self) -> Self {
        Self(self.0.union(&other.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_and_conjugate_math() {
        let frame = Frame::from_rect([10.0, 20.0, 110.0, 70.0]);
        assert_eq!(frame.point(1.0, 1.0), (110.0, 70.0));
        assert!(frame.contains((60.0, 45.0), 0.0));
        assert!(!frame.contains((200.0, 45.0), 0.0));
        assert_eq!(frame.local((60.0, 45.0)), Some((0.5, 0.5)));
        let scale = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let inverse = invert(&scale).unwrap();
        assert_eq!(multiply(&scale, &inverse), IDENTITY);
        // Com CTM 2×, transladar 10 pt na página = transladar 5 no espaço local.
        let local = conjugate(&scale, &translation(10.0, 0.0)).unwrap();
        assert_eq!(local, [1.0, 0.0, 0.0, 1.0, 5.0, 0.0]);
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn from_local_expresses_a_local_transform_in_page_space() {
        // Moldura girada 90° na posição (100, 50): eixo local s aponta para +y.
        let frame = [0.0, 1.0, -1.0, 0.0, 100.0, 50.0];
        // Escala 2× em s, no espaço local.
        let local = [2.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let page = from_local(&frame, &local).unwrap();
        // A origem da moldura fica parada; o ponto local (1, 0) vai para (2, 0).
        assert!(close(
            apply(&page, apply(&frame, (0.0, 0.0))),
            (100.0, 50.0)
        ));
        assert!(close(
            apply(&page, apply(&frame, (1.0, 0.0))),
            apply(&frame, (2.0, 0.0))
        ));
        // É a operação inversa de `conjugate`.
        assert_eq!(conjugate(&invert(&frame).unwrap(), &local), Some(page));
        assert_eq!(from_local(&[0.0; 6], &local), None);
    }

    #[test]
    fn scale_about_keeps_the_anchor_fixed() {
        let scale = scale_about(3.0, 0.5, (0.25, 1.0));
        assert!(close(apply(&scale, (0.25, 1.0)), (0.25, 1.0)));
        assert!(close(apply(&scale, (1.25, 1.0)), (3.25, 1.0)));
        assert!(close(apply(&scale, (0.25, 3.0)), (0.25, 2.0)));
    }

    #[test]
    fn frame_scales_along_its_own_axes() {
        // Moldura de 100 × 50 em (10, 20); dobra a largura em torno do centro.
        let frame = Frame::from_rect([10.0, 20.0, 110.0, 70.0]);
        let page = frame.scaling_about(2.0, 1.0, (0.5, 0.5));
        assert_eq!(
            frame.transformed(&page).bounds(),
            [-40.0, 20.0, 160.0, 70.0]
        );
        let flat = Frame::from_rect([0.0, 0.0, 0.0, 0.0]);
        assert_eq!(flat.scaling_about(2.0, 2.0, (0.5, 0.5)), IDENTITY);
    }

    #[test]
    fn layout_and_pdf_rects_convert_through_page_height() {
        // Palavra a 100 pt do topo numa página de 800 pt de altura.
        let layout = LayoutRect::new(10.0, 100.0, 60.0, 112.0);
        let pdf = layout.to_pdf(800.0);
        assert_eq!(pdf, PdfRect::new(10.0, 688.0, 60.0, 700.0));
        assert_eq!(pdf.to_layout(800.0), layout);
        assert_eq!(pdf.to_array(), [10.0, 688.0, 60.0, 700.0]);
        assert_eq!((layout.width(), layout.height()), (50.0, 12.0));
    }

    #[test]
    fn rects_are_normalized_and_measure_distance() {
        assert_eq!(
            PdfRect::new(5.0, 9.0, 1.0, 2.0),
            PdfRect::new(1.0, 2.0, 5.0, 9.0)
        );
        assert_eq!(
            LayoutRect::new(5.0, 9.0, 1.0, 2.0),
            LayoutRect::new(1.0, 2.0, 5.0, 9.0)
        );
        let rect = LayoutRect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(rect.distance_to((5.0, 5.0)), 0.0);
        assert_eq!(rect.distance_to((13.0, 14.0)), 5.0);
        assert!(rect.contains((10.0, 0.0)));
        assert!(!rect.contains((10.1, 0.0)));
        let joined = rect.union(&LayoutRect::new(20.0, -5.0, 25.0, 2.0));
        assert_eq!(joined, LayoutRect::new(0.0, -5.0, 25.0, 10.0));
    }
}
