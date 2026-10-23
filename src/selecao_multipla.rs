// SPDX-License-Identifier: AGPL-3.0-or-later
//! Seleção de vários objetos (objeto humilde): retângulo de seleção
//! arrastado com o mouse, Ctrl/Ctrl+Shift para incluir/retirar e o desenho
//! das molduras. As regras ficam em [`rustpdf::apresentacao::selecao`].

use super::*;
use rustpdf::apresentacao::selecao::{self, Mode};

/// Retângulo de seleção em andamento.
struct Marquee {
    mode: Mode,
    /// Seleção antes do arraste (base para incluir/retirar).
    base: Vec<Selection>,
    /// Objetos da página (carregados uma vez, no início do arraste).
    objects: Vec<Selection>,
    /// Objetos que o retângulo atinge agora (pré-visualização).
    hits: Vec<Selection>,
}

thread_local! {
    /// Seleção com dois ou mais objetos (com um só, vale `AppState::selection`).
    static GROUP: RefCell<Vec<Selection>> = const { RefCell::new(Vec::new()) };
    static MARQUEE: RefCell<Option<Marquee>> = const { RefCell::new(None) };
    /// Prévia do arraste do grupo (deslocamento ainda não aplicado).
    static PREVIEW: Cell<Matrix> = const { Cell::new(IDENTITY) };
}

use std::cell::Cell;

/// Dois registros do mesmo objeto (mesmo tipo e mesma moldura).
pub fn same_object(a: &Selection, b: &Selection) -> bool {
    let kind = |selection: &Selection| match &selection.target {
        rustpdf::SelectionTarget::Annotation { index, .. } => (0, *index),
        rustpdf::SelectionTarget::Image { .. } => (1, 0),
        rustpdf::SelectionTarget::Text { .. } => (2, 0),
    };
    kind(a) == kind(b) && selecao::same_box(a.frame.bounds(), b.frame.bounds())
}

/// Há um grupo (dois ou mais objetos) selecionado.
pub fn active() -> bool {
    GROUP.with(|group| !group.borrow().is_empty())
}

/// Objetos selecionados: o grupo, ou a seleção simples.
pub fn items() -> Vec<Selection> {
    let group = GROUP.with(|group| group.borrow().clone());
    if group.is_empty() {
        current_selection().into_iter().collect()
    } else {
        group
    }
}

/// Esquece o grupo (chamado por `set_selection`: seleção simples o substitui).
pub fn forget() {
    GROUP.with(|group| group.borrow_mut().clear());
    PREVIEW.with(|preview| preview.set(IDENTITY));
}

/// Define a seleção: nenhum, um (seleção simples, com alças) ou vários.
pub fn select(mut selections: Vec<Selection>) {
    match selections.len() {
        0 => set_selection(None),
        1 => set_selection(selections.pop()),
        _ => {
            set_selection(None);
            GROUP.with(|group| *group.borrow_mut() = selections);
            redraw();
        }
    }
    status(&selecao::summary(items().len()));
    shell::update_sensitivity();
}

fn redraw() {
    STATE.with(|cell| {
        if let Some(app) = cell.borrow().as_ref() {
            unsafe { gtk_widget_queue_draw(app.image) };
        }
    });
}

/// O ponto cai dentro de algum objeto do grupo.
pub fn group_contains(point: (f64, f64)) -> bool {
    GROUP.with(|group| {
        group
            .borrow()
            .iter()
            .any(|item| item.frame.contains(point, 2.0))
    })
}

/// Ctrl+clique / Ctrl+Shift+clique: inclui ou retira o objeto sob o ponto.
pub fn toggle_at(point: (f64, f64), mode: Mode) -> AppResult<()> {
    let Some(hit) = with_session(|session| session.select_at(point))? else {
        return Ok(());
    };
    select(selecao::combine(&items(), &[hit], mode, same_object));
    Ok(())
}

/// Começa o retângulo de seleção (os objetos da página são lidos aqui).
pub fn begin_marquee(mode: Mode) -> AppResult<()> {
    let objects = with_session(|session| session.page_objects())?;
    let base = items();
    MARQUEE.with(|marquee| {
        *marquee.borrow_mut() = Some(Marquee {
            mode,
            base,
            objects,
            hits: Vec::new(),
        })
    });
    Ok(())
}

pub fn marquee_active() -> bool {
    MARQUEE.with(|marquee| marquee.borrow().is_some())
}

/// Retângulo atual `rect` (pontos visíveis): atualiza a pré-visualização.
pub fn update_marquee(rect: [f64; 4]) {
    let summary = MARQUEE.with(|marquee| {
        let mut marquee = marquee.borrow_mut();
        let marquee = marquee.as_mut()?;
        marquee.hits = marquee
            .objects
            .iter()
            .filter(|object| selecao::touches(rect, object.frame.bounds()))
            .cloned()
            .collect();
        let verb = match marquee.mode {
            Mode::Replace => "Selecionar",
            Mode::Add => "Incluir",
            Mode::Remove => "Retirar",
        };
        Some(format!(
            "{verb}: {} objeto(s) no retângulo",
            marquee.hits.len()
        ))
    });
    if let Some(summary) = summary {
        status(&summary);
    }
    set_rubber_band(Some(rect));
}

/// Solta o retângulo: aplica o modo aos objetos atingidos.
pub fn finish_marquee() {
    let Some(marquee) = MARQUEE.with(|marquee| marquee.borrow_mut().take()) else {
        return;
    };
    set_rubber_band(None);
    select(selecao::combine(
        &marquee.base,
        &marquee.hits,
        marquee.mode,
        same_object,
    ));
}

/// Cancela o retângulo sem mudar a seleção.
pub fn cancel_marquee() {
    if MARQUEE
        .with(|marquee| marquee.borrow_mut().take())
        .is_some()
    {
        set_rubber_band(None);
    }
}

/// Prévia do grupo sendo arrastado.
pub fn set_preview(transform: Matrix) {
    PREVIEW.with(|preview| preview.set(transform));
    redraw();
}

/// Aplica `transform` a todo o grupo (uma edição só) e mantém o grupo
/// selecionado na nova posição.
pub fn transform_group(transform: &Matrix) -> AppResult<()> {
    let group = items();
    PREVIEW.with(|preview| preview.set(IDENTITY));
    with_session(|session| session.transform_selections(&group, transform))?;
    render()?;
    let moved: Vec<Selection> = group
        .iter()
        .map(|item| item.transformed(transform))
        .collect();
    select(moved);
    status(&format!("{} objetos movidos", group.len()));
    Ok(())
}

/// Apaga todo o grupo (uma edição só).
pub fn delete_group() -> AppResult<()> {
    let group = items();
    with_session(|session| session.delete_selections(&group))?;
    set_selection(None);
    render()?;
    status(&format!("{} objetos apagados", group.len()));
    Ok(())
}

/// Texto dos trechos de texto do grupo, um por linha.
pub fn group_text() -> String {
    items()
        .iter()
        .filter_map(|item| match &item.target {
            rustpdf::SelectionTarget::Text { line, .. } => Some(line.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// O grupo tem algum trecho de texto.
pub fn group_has_text() -> bool {
    GROUP.with(|group| {
        group
            .borrow()
            .iter()
            .any(|item| matches!(item.target, rustpdf::SelectionTarget::Text { .. }))
    })
}

/// Cantos da moldura no widget, afastados `padding` px para fora.
fn corners(frame: &textmove::Frame, mapping: PageMapping, padding: f64) -> Vec<(f64, f64)> {
    let center = mapping.to_widget(frame.center());
    [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
        .into_iter()
        .map(|(s, t)| {
            let (x, y) = mapping.to_widget(frame.point(s, t));
            let (dx, dy) = (x - center.0, y - center.1);
            let length = dx.hypot(dy).max(1e-6);
            (x + dx / length * padding, y + dy / length * padding)
        })
        .collect()
}

/// Grupo selecionado (contorno de cada objeto e caixa do conjunto) e
/// objetos que o retângulo em andamento vai atingir.
pub unsafe fn draw(cr: *mut c_void, mapping: PageMapping) {
    let preview = PREVIEW.with(Cell::get);
    let group = GROUP.with(|group| group.borrow().clone());
    let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for item in &group {
        let frame = item.frame.transformed(&preview);
        let points = corners(&frame, mapping, 3.0);
        outline(cr, &points, ACCENT_FILL, 1.25);
        for (x, y) in points {
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
    }
    if group.len() > 1 {
        let [x0, y0, x1, y1] = bounds;
        let pad = 5.0;
        let box_points = [
            (x0 - pad, y0 - pad),
            (x1 + pad, y0 - pad),
            (x1 + pad, y1 + pad),
            (x0 - pad, y1 + pad),
        ];
        dotted(cr, &box_points, 0.0);
    }
    MARQUEE.with(|marquee| {
        if let Some(marquee) = marquee.borrow().as_ref() {
            let removing = marquee.mode == Mode::Remove;
            for hit in &marquee.hits {
                let points = corners(&hit.frame, mapping, 2.0);
                path_polygon(cr, &points);
                if removing {
                    cairo_set_source_rgba(cr, 0.85, 0.2, 0.2, 0.18);
                } else {
                    cairo_set_source_rgba(cr, ACCENT.0, ACCENT.1, ACCENT.2, 0.18);
                }
                cairo_fill_preserve(cr);
                cairo_set_line_width(cr, 1.0);
                cairo_stroke(cr);
            }
        }
    });
}
