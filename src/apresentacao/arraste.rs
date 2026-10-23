// SPDX-License-Identifier: AGPL-3.0-or-later
//! Limiar de arraste: um clique só vira arraste depois que o ponteiro se
//! afasta alguns pixels do ponto em que o botão foi apertado. Sem isso, o
//! tremor natural do mouse num clique movia o objeto recém-selecionado.

/// Distância mínima, em pixels de tela, para começar a arrastar.
pub const DRAG_THRESHOLD_PX: f64 = 4.0;

/// Acompanha um botão apertado e decide quando ele passa a ser arraste.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragGate {
    origin: (f64, f64),
    dragging: bool,
}

impl DragGate {
    /// Botão apertado em `origin` (pixels do widget).
    pub fn new(origin: (f64, f64)) -> Self {
        Self {
            origin,
            dragging: false,
        }
    }

    /// Ponteiro em `position`: devolve se já é arraste. Uma vez passado o
    /// limiar, continua arraste mesmo que o ponteiro volte para perto.
    pub fn update(&mut self, position: (f64, f64)) -> bool {
        if !self.dragging {
            let distance = (position.0 - self.origin.0).hypot(position.1 - self.origin.1);
            self.dragging = distance >= DRAG_THRESHOLD_PX;
        }
        self.dragging
    }

    pub fn dragging(&self) -> bool {
        self.dragging
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_jitter_is_still_a_click() {
        let mut gate = DragGate::new((100.0, 100.0));
        assert!(!gate.update((101.0, 100.0)));
        assert!(!gate.update((102.0, 102.0)));
        assert!(!gate.dragging());
    }

    #[test]
    fn crossing_the_threshold_latches_the_drag() {
        let mut gate = DragGate::new((100.0, 100.0));
        assert!(gate.update((104.0, 100.0)));
        // Voltar para perto da origem continua sendo arraste.
        assert!(gate.update((100.0, 100.0)));
        assert!(gate.dragging());
        let mut diagonal = DragGate::new((0.0, 0.0));
        assert!(diagonal.update((3.0, 3.0)));
    }
}
