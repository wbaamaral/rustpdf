// SPDX-License-Identifier: AGPL-3.0-or-later
//! Edição de texto no lugar (objeto humilde).
//!
//! Um campo `GtkEntry` sobreposto à página (via `GtkOverlay`, por fora do
//! `GtkEventBox` que captura os cliques) cobre o trecho de texto e recebe o
//! texto atual: Enter ou clique fora confirma, Esc cancela. A posição e o
//! tamanho vêm de [`rustpdf::apresentacao::edicao_texto::inline_box`].

use super::*;
use rustpdf::apresentacao::edicao_texto::inline_box;
use std::cell::Cell;

#[link(name = "gtk-3")]
extern "C" {
    fn gtk_overlay_new() -> *mut c_void;
    fn gtk_overlay_add_overlay(overlay: *mut c_void, widget: *mut c_void);
    fn gtk_widget_set_halign(widget: *mut c_void, align: c_int);
    fn gtk_widget_set_valign(widget: *mut c_void, align: c_int);
    fn gtk_widget_set_margin_start(widget: *mut c_void, margin: c_int);
    fn gtk_widget_set_no_show_all(widget: *mut c_void, no_show_all: c_int);
    fn gtk_widget_show(widget: *mut c_void);
    fn gtk_widget_hide(widget: *mut c_void);
    fn gtk_widget_grab_focus(widget: *mut c_void);
    fn gtk_editable_select_region(editable: *mut c_void, start: c_int, end: c_int);
    fn gtk_widget_get_style_context(widget: *mut c_void) -> *mut c_void;
    fn gtk_style_context_add_class(context: *mut c_void, name: *const c_char);
    fn gtk_style_context_add_provider(context: *mut c_void, provider: *mut c_void, priority: u32);
    fn gtk_css_provider_new() -> *mut c_void;
    fn gtk_css_provider_load_from_data(
        provider: *mut c_void,
        data: *const c_char,
        length: isize,
        error: *mut *mut c_void,
    ) -> c_int;
}

#[link(name = "gdk-3")]
extern "C" {
    fn gdk_event_get_keyval(event: *mut c_void, keyval: *mut u32) -> c_int;
}

const ALIGN_START: c_int = 1;
const KEY_ESCAPE: u32 = 0xff1b;
/// `GTK_STYLE_PROVIDER_PRIORITY_USER`: acima do tema e do CSS da aplicação.
const PRIORITY_USER: u32 = 800;

/// Edição em andamento: onde foi o clique e o texto original.
struct Pending {
    point: (f64, f64),
    word: String,
    original: String,
}

thread_local! {
    static OVERLAY: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    static ENTRY: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    static PROVIDER: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
}

/// Envolve a área da página num overlay com o campo de edição (oculto).
pub fn wrap(page_area: *mut c_void) -> *mut c_void {
    unsafe {
        let overlay = gtk_overlay_new();
        gtk_container_add(overlay, page_area);
        let entry = gtk_entry_new();
        gtk_widget_set_no_show_all(entry, 1);
        gtk_widget_set_halign(entry, ALIGN_START);
        gtk_widget_set_valign(entry, ALIGN_START);
        let context = gtk_widget_get_style_context(entry);
        gtk_style_context_add_class(context, c"rustpdf-inline-edit".as_ptr());
        let provider = gtk_css_provider_new();
        gtk_style_context_add_provider(context, provider, PRIORITY_USER);
        gtk_overlay_add_overlay(overlay, entry);
        g_signal_connect_data(
            entry,
            c"activate".as_ptr(),
            on_activate as *const c_void,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
        g_signal_connect_data(
            entry,
            c"key-press-event".as_ptr(),
            on_key_press as *const c_void,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
        OVERLAY.with(|cell| cell.set(overlay));
        ENTRY.with(|cell| cell.set(entry));
        PROVIDER.with(|cell| cell.set(provider));
        overlay
    }
}

/// Há um campo de edição aberto sobre a página.
pub fn active() -> bool {
    PENDING.with(|pending| pending.borrow().is_some())
}

/// Abre o campo sobre o texto em `point`. `Ok(false)` quando o texto não
/// pode ser editado no lugar (girado, sem prévia): o chamador usa o diálogo.
pub fn begin(point: (f64, f64), word: &str, original: &str) -> AppResult<bool> {
    let Some(selection) = with_session(|session| session.select_at(point))? else {
        return Ok(false);
    };
    if !matches!(selection.target, rustpdf::SelectionTarget::Text { .. }) {
        return Ok(false);
    }
    let placement = STATE.with(|cell| {
        let borrowed = cell.borrow();
        let app = borrowed.as_ref()?;
        let mapping = PageMapping::of(app)?;
        let field = inline_box(&selection.frame, original, |point| mapping.to_widget(point))?;
        Some((field, app.image))
    });
    let Some((field, image)) = placement else {
        return Ok(false);
    };
    let (overlay, entry) = (OVERLAY.with(Cell::get), ENTRY.with(Cell::get));
    if overlay.is_null() || entry.is_null() {
        return Ok(false);
    }
    // Pixels do widget da prévia → pixels do overlay.
    let (mut dx, mut dy) = (0, 0);
    if unsafe { gtk_widget_translate_coordinates(image, overlay, 0, 0, &mut dx, &mut dy) } == 0 {
        return Ok(false);
    }
    let css = format!(
        "entry.rustpdf-inline-edit {{ font-size: {:.1}px; padding: 0 2px; min-height: 0; \
         border-radius: 0; background: #ffffff; color: #000000; \
         border: 1px solid @theme_selected_bg_color; \
         box-shadow: 0 0 0 2px alpha(@theme_selected_bg_color, 0.35); }} \
         entry.rustpdf-inline-edit selection {{ background-color: #3584e4; color: #ffffff; }}",
        field.font_px
    );
    // A moldura e as alças ficariam por trás do campo; voltam ao confirmar.
    drop(selection);
    set_selection(None);
    PENDING.with(|pending| {
        *pending.borrow_mut() = Some(Pending {
            point,
            word: word.to_string(),
            original: original.to_string(),
        })
    });
    unsafe {
        gtk_css_provider_load_from_data(
            PROVIDER.with(Cell::get),
            cstring(&css).as_ptr(),
            -1,
            ptr::null_mut(),
        );
        gtk_widget_set_margin_start(entry, (field.x + dx).max(0));
        gtk_widget_set_margin_top(entry, (field.y + dy).max(0));
        gtk_widget_set_size_request(entry, field.width, field.height);
        gtk_entry_set_text(entry, cstring(original).as_ptr());
        gtk_widget_show(entry);
        gtk_widget_grab_focus(entry);
        gtk_editable_select_region(entry, 0, -1);
    }
    Ok(true)
}

/// Fecha o campo e devolve a edição pendente (se havia uma).
fn close() -> Option<Pending> {
    // Tira o estado antes de esconder: esconder pode reentrar aqui.
    let pending = PENDING.with(|pending| pending.borrow_mut().take())?;
    let entry = ENTRY.with(Cell::get);
    if !entry.is_null() {
        unsafe { gtk_widget_hide(entry) };
    }
    Some(pending)
}

/// Descarta a edição em andamento (Esc, troca de página, zoom…).
pub fn cancel() {
    if close().is_some() {
        status("Edição de texto cancelada");
    }
}

/// Grava o texto do campo, se mudou.
pub fn commit() {
    let entry = ENTRY.with(Cell::get);
    if entry.is_null() || !active() {
        return;
    }
    let text = unsafe {
        let value = gtk_entry_get_text(entry);
        (!value.is_null()).then(|| CStr::from_ptr(value).to_string_lossy().into_owned())
    };
    let Some(pending) = close() else {
        return;
    };
    let Some(text) = text else {
        return;
    };
    if let Err(error) = apply_text_edit(pending.point, &pending.word, &pending.original, &text) {
        status(&error);
    }
}

unsafe extern "C" fn on_activate(_entry: *mut c_void, _data: *mut c_void) {
    commit();
}

unsafe extern "C" fn on_key_press(
    _entry: *mut c_void,
    event: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    let mut key = 0u32;
    if gdk_event_get_keyval(event, &mut key) != 0 && key == KEY_ESCAPE {
        cancel();
        return 1;
    }
    0
}
