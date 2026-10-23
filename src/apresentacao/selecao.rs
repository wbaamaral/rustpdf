// SPDX-License-Identifier: AGPL-3.0-or-later
//! Seleção de vários objetos: modo pelo teclado (Ctrl inclui, Ctrl+Shift
//! retira), combinação com a seleção atual e retângulo de seleção.

/// O que um clique ou retângulo faz com a seleção atual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Sem teclas: a seleção passa a ser só o que foi atingido.
    Replace,
    /// Ctrl: inclui o que foi atingido.
    Add,
    /// Ctrl+Shift: retira o que foi atingido.
    Remove,
}

/// Modo a partir das teclas pressionadas (Shift sozinho não muda o modo:
/// ele já serve para redimensionar sem manter a proporção).
pub fn mode(control: bool, shift: bool) -> Mode {
    match (control, shift) {
        (true, true) => Mode::Remove,
        (true, false) => Mode::Add,
        _ => Mode::Replace,
    }
}

/// Nova seleção: `current` combinada com `hits` conforme `mode`, sem
/// repetir objetos (`same` diz quando dois são o mesmo objeto).
pub fn combine<T: Clone>(
    current: &[T],
    hits: &[T],
    mode: Mode,
    same: impl Fn(&T, &T) -> bool,
) -> Vec<T> {
    let mut result: Vec<T> = match mode {
        Mode::Replace => Vec::new(),
        Mode::Add => current.to_vec(),
        Mode::Remove => {
            return current
                .iter()
                .filter(|item| !hits.iter().any(|hit| same(item, hit)))
                .cloned()
                .collect();
        }
    };
    for hit in hits {
        if !result.iter().any(|item| same(item, hit)) {
            result.push(hit.clone());
        }
    }
    result
}

/// Retângulo `[x0, y0, x1, y1]` normalizado a partir de dois cantos.
pub fn rect_between(a: (f64, f64), b: (f64, f64)) -> [f64; 4] {
    [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
}

/// O retângulo de seleção pega o objeto se as caixas se tocam (basta
/// cobrir parte dele, como nos editores de desenho).
pub fn touches(selection: [f64; 4], object: [f64; 4]) -> bool {
    selection[0] <= object[2]
        && object[0] <= selection[2]
        && selection[1] <= object[3]
        && object[1] <= selection[3]
}

/// Duas caixas são do mesmo objeto (tolerância de meio ponto: a mesma
/// moldura medida por caminhos diferentes).
pub fn same_box(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 0.5)
}

/// Texto da barra de status para `count` objetos selecionados.
pub fn summary(count: usize) -> String {
    match count {
        0 => "Nada selecionado".into(),
        1 => "1 objeto selecionado".into(),
        count => format!(
            "{count} objetos selecionados — arraste um deles para mover todos; Delete apaga; \
             Ctrl+clique inclui, Ctrl+Shift+clique retira"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &i32, b: &i32) -> bool {
        a == b
    }

    #[test]
    fn keys_choose_the_mode() {
        assert_eq!(mode(false, false), Mode::Replace);
        assert_eq!(mode(false, true), Mode::Replace);
        assert_eq!(mode(true, false), Mode::Add);
        assert_eq!(mode(true, true), Mode::Remove);
    }

    #[test]
    fn combine_replaces_adds_and_removes_without_repeats() {
        let current = [1, 2, 3];
        assert_eq!(combine(&current, &[3, 4], Mode::Replace, same), [3, 4]);
        assert_eq!(combine(&current, &[3, 4, 4], Mode::Add, same), [1, 2, 3, 4]);
        assert_eq!(combine(&current, &[2, 9], Mode::Remove, same), [1, 3]);
        assert!(combine(&current, &[], Mode::Replace, same).is_empty());
        assert_eq!(combine(&[], &[5], Mode::Remove, same), Vec::<i32>::new());
    }

    #[test]
    fn rectangle_touches_partially_covered_objects() {
        let marquee = rect_between((100.0, 200.0), (50.0, 100.0));
        assert_eq!(marquee, [50.0, 100.0, 100.0, 200.0]);
        assert!(touches(marquee, [90.0, 190.0, 300.0, 210.0]));
        assert!(touches(marquee, [60.0, 120.0, 70.0, 130.0]));
        assert!(!touches(marquee, [101.0, 150.0, 200.0, 160.0]));
        assert!(!touches(marquee, [60.0, 201.0, 70.0, 230.0]));
    }

    #[test]
    fn boxes_match_with_half_point_tolerance() {
        assert!(same_box([1.0, 2.0, 3.0, 4.0], [1.4, 2.0, 3.0, 3.6]));
        assert!(!same_box([1.0, 2.0, 3.0, 4.0], [1.6, 2.0, 3.0, 4.0]));
    }

    #[test]
    fn summary_explains_what_can_be_done() {
        assert_eq!(summary(0), "Nada selecionado");
        assert_eq!(summary(1), "1 objeto selecionado");
        assert!(summary(3).starts_with("3 objetos selecionados"));
    }
}
