// SPDX-License-Identifier: AGPL-3.0-or-later
//! Diálogos da interface (objeto humilde): confirmação, pedido de texto,
//! senha, edição de texto em caixa e janela de leitura de texto.
//!
//! Todos seguem o mesmo desenho: título em destaque, texto de apoio, botões
//! com o verbo da ação (o principal destacado; o destrutivo em vermelho),
//! Enter confirma e Esc cancela. O conteúdo vem de
//! [`rustpdf::apresentacao::dialogos`].

use super::*;
use rustpdf::apresentacao::dialogos::{self as conteudo, Confirmation};
use std::cell::Cell;

#[link(name = "gtk-3")]
extern "C" {
    fn gtk_dialog_new() -> *mut c_void;
    fn gtk_dialog_add_button(
        dialog: *mut c_void,
        text: *const c_char,
        response: c_int,
    ) -> *mut c_void;
    fn gtk_dialog_response(dialog: *mut c_void, response: c_int);
    fn gtk_dialog_set_response_sensitive(dialog: *mut c_void, response: c_int, setting: c_int);
    fn gtk_window_set_transient_for(window: *mut c_void, parent: *mut c_void);
    fn gtk_window_set_modal(window: *mut c_void, modal: c_int);
    fn gtk_window_set_resizable(window: *mut c_void, resizable: c_int);
    fn gtk_window_set_destroy_with_parent(window: *mut c_void, setting: c_int);
    fn gtk_window_set_titlebar(window: *mut c_void, titlebar: *mut c_void);
    fn gtk_label_set_line_wrap(label: *mut c_void, wrap: c_int);
    fn gtk_label_set_max_width_chars(label: *mut c_void, n_chars: c_int);
    fn gtk_label_set_selectable(label: *mut c_void, setting: c_int);
    fn gtk_entry_set_placeholder_text(entry: *mut c_void, text: *const c_char);
    fn gtk_entry_set_icon_from_icon_name(entry: *mut c_void, position: c_int, name: *const c_char);
    fn gtk_entry_set_icon_tooltip_text(entry: *mut c_void, position: c_int, tooltip: *const c_char);
    fn gtk_entry_get_visibility(entry: *mut c_void) -> c_int;
    fn gtk_entry_set_input_purpose(entry: *mut c_void, purpose: c_int);
    fn gtk_widget_set_margin_start(widget: *mut c_void, margin: c_int);
    fn gtk_widget_set_margin_end(widget: *mut c_void, margin: c_int);
    fn gtk_widget_set_hexpand(widget: *mut c_void, expand: c_int);
    fn gtk_widget_set_vexpand(widget: *mut c_void, expand: c_int);
    fn gtk_widget_set_can_default(widget: *mut c_void, can_default: c_int);
    fn gtk_widget_grab_focus(widget: *mut c_void);
    fn gtk_widget_get_style_context(widget: *mut c_void) -> *mut c_void;
    fn gtk_style_context_add_class(context: *mut c_void, name: *const c_char);
    fn gtk_box_pack_end(
        container: *mut c_void,
        child: *mut c_void,
        expand: c_int,
        fill: c_int,
        padding: u32,
    );
    fn gtk_scrolled_window_set_min_content_height(window: *mut c_void, height: c_int);
    fn gtk_scrolled_window_set_policy(window: *mut c_void, h: c_int, v: c_int);
    fn gtk_text_view_set_left_margin(view: *mut c_void, margin: c_int);
    fn gtk_text_view_set_right_margin(view: *mut c_void, margin: c_int);
    fn gtk_text_view_set_top_margin(view: *mut c_void, margin: c_int);
    fn gtk_text_view_set_bottom_margin(view: *mut c_void, margin: c_int);
    fn gtk_text_view_set_accepts_tab(view: *mut c_void, accepts: c_int);
    fn gtk_text_buffer_get_start_iter(buffer: *mut c_void, iter: *mut TextIter);
    fn gtk_text_buffer_get_end_iter(buffer: *mut c_void, iter: *mut TextIter);
    fn gtk_text_buffer_get_text(
        buffer: *mut c_void,
        start: *const TextIter,
        end: *const TextIter,
        hidden: c_int,
    ) -> *mut c_char;
    fn gtk_text_buffer_select_range(
        buffer: *mut c_void,
        insert: *const TextIter,
        bound: *const TextIter,
    );
    fn gtk_header_bar_new() -> *mut c_void;
    fn gtk_header_bar_set_title(bar: *mut c_void, title: *const c_char);
    fn gtk_header_bar_set_subtitle(bar: *mut c_void, subtitle: *const c_char);
    fn gtk_header_bar_set_show_close_button(bar: *mut c_void, setting: c_int);
    fn gtk_header_bar_pack_end(bar: *mut c_void, child: *mut c_void);
    fn gtk_button_new_with_mnemonic(label: *const c_char) -> *mut c_void;
    fn gtk_widget_set_tooltip_text(widget: *mut c_void, text: *const c_char);
    fn gtk_combo_box_text_new() -> *mut c_void;
    fn gtk_combo_box_text_append_text(combo: *mut c_void, text: *const c_char);
    fn gtk_combo_box_set_active(combo: *mut c_void, index: c_int);
    fn gtk_combo_box_get_active(combo: *mut c_void) -> c_int;
}

#[link(name = "gdk-3")]
extern "C" {
    fn gdk_event_get_keyval(event: *mut c_void, keyval: *mut u32) -> c_int;
}

/// `GtkTextIter` opaco (80 bytes no GTK 3 de 64 bits; folga para outras ABIs).
#[repr(C, align(8))]
pub struct TextIter([u8; 160]);

impl TextIter {
    fn new() -> Self {
        Self([0; 160])
    }
}

const RESPONSE_ACCEPT: c_int = -3;
const RESPONSE_CANCEL: c_int = -6;
const ICON_SECONDARY: c_int = 1;
const INPUT_PURPOSE_PASSWORD: c_int = 8;
const WINDOW_POSITION_CENTER_ON_PARENT: c_int = 4;
const KEY_RETURN: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
const SHIFT_MASK: u32 = 1;
const MARGIN: c_int = 24;

fn parent_window() -> *mut c_void {
    STATE.with(|cell| {
        cell.borrow()
            .as_ref()
            .map_or(ptr::null_mut(), |app| app.window)
    })
}

unsafe fn add_class(widget: *mut c_void, class: &str) {
    gtk_style_context_add_class(
        gtk_widget_get_style_context(widget),
        cstring(class).as_ptr(),
    );
}

unsafe fn label(text: &str, class: &str) -> *mut c_void {
    let label = gtk_label_new(cstring(text).as_ptr());
    gtk_label_set_xalign(label, 0.0);
    gtk_label_set_line_wrap(label, 1);
    gtk_label_set_max_width_chars(label, 52);
    add_class(label, class);
    label
}

/// Diálogo modal vazio com a área de conteúdo pronta (coluna com margens).
unsafe fn dialog(title: &str) -> (*mut c_void, *mut c_void) {
    let dialog = gtk_dialog_new();
    gtk_window_set_title(dialog, cstring(title).as_ptr());
    let parent = parent_window();
    if !parent.is_null() {
        gtk_window_set_transient_for(dialog, parent);
    }
    gtk_window_set_modal(dialog, 1);
    gtk_window_set_destroy_with_parent(dialog, 1);
    gtk_window_set_resizable(dialog, 0);
    gtk_window_set_position(dialog, WINDOW_POSITION_CENTER_ON_PARENT);
    add_class(dialog, "rustpdf-dialog");
    let column = gtk_box_new(1, 8);
    gtk_widget_set_margin_start(column, MARGIN);
    gtk_widget_set_margin_end(column, MARGIN);
    gtk_widget_set_margin_top(column, MARGIN - 4);
    gtk_widget_set_margin_bottom(column, 12);
    gtk_widget_set_size_request(column, 360, -1);
    gtk_box_pack_start(gtk_dialog_get_content_area(dialog), column, 1, 1, 0);
    (dialog, column)
}

/// Botões "cancelar" e "confirmar" (o último é o padrão do Enter).
unsafe fn buttons(dialog: *mut c_void, cancel: &str, confirm: &str, destructive: bool) {
    gtk_dialog_add_button(dialog, cstring(cancel).as_ptr(), RESPONSE_CANCEL);
    let button = gtk_dialog_add_button(dialog, cstring(confirm).as_ptr(), RESPONSE_ACCEPT);
    gtk_widget_set_can_default(button, 1);
    add_class(
        button,
        if destructive {
            "destructive-action"
        } else {
            "suggested-action"
        },
    );
    // Ação destrutiva não fica no Enter: exige escolher o botão.
    gtk_dialog_set_default_response(
        dialog,
        if destructive {
            RESPONSE_CANCEL
        } else {
            RESPONSE_ACCEPT
        },
    );
}

unsafe fn run(dialog: *mut c_void) -> bool {
    gtk_widget_show_all(dialog);
    gtk_dialog_run(dialog) == RESPONSE_ACCEPT
}

unsafe fn entry_text(entry: *mut c_void) -> Option<String> {
    let value = gtk_entry_get_text(entry);
    (!value.is_null()).then(|| CStr::from_ptr(value).to_string_lossy().into_owned())
}

/// Pergunta de sim/não; o botão diz a ação (ver [`conteudo::confirmation`]).
pub fn confirm(message: &str) -> bool {
    if parent_window().is_null() {
        return false;
    }
    let Confirmation {
        heading,
        body,
        confirm,
        cancel,
        destructive,
    } = conteudo::confirmation(message);
    unsafe {
        let (dialog, column) = dialog("RustPDF");
        gtk_box_pack_start(column, label(&heading, "rustpdf-dialog-heading"), 0, 0, 0);
        if let Some(body) = body {
            gtk_box_pack_start(column, label(&body, "rustpdf-dialog-text"), 0, 0, 0);
        }
        buttons(dialog, &cancel, &confirm, destructive);
        let accepted = run(dialog);
        gtk_widget_destroy(dialog);
        accepted
    }
}

/// Pede uma linha de texto; a parte entre parênteses do rótulo vira dica.
pub fn prompt_text_default(message: &str, default: &str) -> Option<String> {
    if parent_window().is_null() {
        return None;
    }
    let prompt = conteudo::prompt(message);
    unsafe {
        let (dialog, column) = dialog("RustPDF");
        gtk_box_pack_start(
            column,
            label(&prompt.heading, "rustpdf-dialog-heading"),
            0,
            0,
            0,
        );
        let entry = gtk_entry_new();
        gtk_widget_set_size_request(entry, 380, -1);
        gtk_entry_set_text(entry, cstring(default).as_ptr());
        gtk_entry_set_activates_default(entry, 1);
        gtk_widget_set_margin_top(entry, 4);
        gtk_box_pack_start(column, entry, 0, 0, 0);
        if let Some(hint) = &prompt.hint {
            gtk_entry_set_placeholder_text(entry, cstring(hint).as_ptr());
            gtk_box_pack_start(column, label(hint, "rustpdf-dialog-hint"), 0, 0, 0);
        }
        buttons(dialog, "Cancelar", "OK", false);
        let text = run(dialog).then(|| entry_text(entry)).flatten();
        gtk_widget_destroy(dialog);
        text
    }
}

/// Escolha de um item numa lista (seletor), com `current` já marcado.
/// Devolve o índice escolhido, ou `None` se o usuário desistir.
pub fn choose(
    heading: &str,
    body: Option<&str>,
    items: &[String],
    current: Option<usize>,
    confirm: &str,
) -> Option<usize> {
    if parent_window().is_null() || items.is_empty() {
        return None;
    }
    unsafe {
        let (dialog, column) = dialog("RustPDF");
        gtk_box_pack_start(column, label(heading, "rustpdf-dialog-heading"), 0, 0, 0);
        if let Some(body) = body {
            gtk_box_pack_start(column, label(body, "rustpdf-dialog-text"), 0, 0, 0);
        }
        let combo = gtk_combo_box_text_new();
        for item in items {
            gtk_combo_box_text_append_text(combo, cstring(item).as_ptr());
        }
        gtk_combo_box_set_active(combo, current.unwrap_or(0) as c_int);
        gtk_widget_set_size_request(combo, 380, -1);
        gtk_widget_set_margin_top(combo, 4);
        gtk_box_pack_start(column, combo, 0, 0, 0);
        buttons(dialog, "Cancelar", confirm, false);
        let chosen = run(dialog)
            .then(|| gtk_combo_box_get_active(combo))
            .and_then(|index| usize::try_from(index).ok());
        gtk_widget_destroy(dialog);
        chosen
    }
}

/// Aviso com um só botão (ex.: não há campos do tipo pedido).
pub fn inform(heading: &str, body: &str) {
    if parent_window().is_null() {
        return;
    }
    unsafe {
        let (dialog, column) = dialog("RustPDF");
        gtk_box_pack_start(column, label(heading, "rustpdf-dialog-heading"), 0, 0, 0);
        gtk_box_pack_start(column, label(body, "rustpdf-dialog-text"), 0, 0, 0);
        let button = gtk_dialog_add_button(dialog, c"OK".as_ptr(), RESPONSE_ACCEPT);
        add_class(button, "suggested-action");
        gtk_dialog_set_default_response(dialog, RESPONSE_ACCEPT);
        run(dialog);
        gtk_widget_destroy(dialog);
    }
}

unsafe extern "C" fn on_reveal_password(
    entry: *mut c_void,
    _position: c_int,
    _event: *mut c_void,
    _data: *mut c_void,
) {
    let hidden = gtk_entry_get_visibility(entry) == 0;
    gtk_entry_set_visibility(entry, c_int::from(hidden));
    let (icon, tip) = if hidden {
        (c"view-conceal-symbolic", c"Ocultar senha")
    } else {
        (c"view-reveal-symbolic", c"Mostrar senha")
    };
    gtk_entry_set_icon_from_icon_name(entry, ICON_SECONDARY, icon.as_ptr());
    gtk_entry_set_icon_tooltip_text(entry, ICON_SECONDARY, tip.as_ptr());
}

/// Senha de um PDF protegido, com botão para mostrar/ocultar.
pub fn prompt_password() -> Option<String> {
    if parent_window().is_null() {
        return None;
    }
    unsafe {
        let (dialog, column) = dialog("Documento protegido");
        gtk_box_pack_start(
            column,
            label("Documento protegido", "rustpdf-dialog-heading"),
            0,
            0,
            0,
        );
        gtk_box_pack_start(
            column,
            label("Digite a senha para abrir este PDF.", "rustpdf-dialog-text"),
            0,
            0,
            0,
        );
        let entry = gtk_entry_new();
        gtk_widget_set_size_request(entry, 340, -1);
        gtk_widget_set_margin_top(entry, 4);
        gtk_entry_set_visibility(entry, 0);
        gtk_entry_set_input_purpose(entry, INPUT_PURPOSE_PASSWORD);
        gtk_entry_set_placeholder_text(entry, c"Senha".as_ptr());
        gtk_entry_set_activates_default(entry, 1);
        gtk_entry_set_icon_from_icon_name(entry, ICON_SECONDARY, c"view-reveal-symbolic".as_ptr());
        gtk_entry_set_icon_tooltip_text(entry, ICON_SECONDARY, c"Mostrar senha".as_ptr());
        g_signal_connect_data(
            entry,
            c"icon-press".as_ptr(),
            on_reveal_password as *const c_void,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
        gtk_box_pack_start(column, entry, 0, 0, 0);
        buttons(dialog, "Cancelar", "Abrir", false);
        let password = run(dialog).then(|| entry_text(entry)).flatten();
        gtk_widget_destroy(dialog);
        password
    }
}

unsafe fn buffer_text(buffer: *mut c_void) -> String {
    let (mut start, mut end) = (TextIter::new(), TextIter::new());
    gtk_text_buffer_get_start_iter(buffer, &mut start);
    gtk_text_buffer_get_end_iter(buffer, &mut end);
    let raw = gtk_text_buffer_get_text(buffer, &start, &end, 0);
    if raw.is_null() {
        return String::new();
    }
    let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
    g_free(raw.cast());
    text
}

/// Caixa de texto com margens internas, rolagem e borda.
unsafe fn text_area(height: c_int) -> (*mut c_void, *mut c_void) {
    let scroll = gtk_scrolled_window_new(ptr::null_mut(), ptr::null_mut());
    gtk_scrolled_window_set_policy(scroll, 2, 1);
    gtk_scrolled_window_set_min_content_height(scroll, height);
    add_class(scroll, "rustpdf-text-area");
    let view = gtk_text_view_new();
    gtk_text_view_set_wrap_mode(view, 3);
    gtk_text_view_set_left_margin(view, 12);
    gtk_text_view_set_right_margin(view, 12);
    gtk_text_view_set_top_margin(view, 10);
    gtk_text_view_set_bottom_margin(view, 10);
    gtk_container_add(scroll, view);
    (scroll, view)
}

/// Estado do diálogo de edição aberto (um por vez: é modal).
struct EditState {
    dialog: *mut c_void,
    counter: *mut c_void,
    original: String,
}

thread_local! {
    static EDITING: RefCell<Option<EditState>> = const { RefCell::new(None) };
}

unsafe extern "C" fn on_edit_changed(buffer: *mut c_void, _data: *mut c_void) {
    let text = buffer_text(buffer);
    EDITING.with(|cell| {
        if let Some(state) = cell.borrow().as_ref() {
            let line = conteudo::single_line(&text);
            gtk_label_set_text(
                state.counter,
                cstring(&conteudo::character_count(&line)).as_ptr(),
            );
            gtk_dialog_set_response_sensitive(
                state.dialog,
                RESPONSE_ACCEPT,
                c_int::from(conteudo::can_apply(&state.original, &text)),
            );
        }
    });
}

/// Enter aplica (o trecho de texto do PDF é uma linha só); Shift+Enter
/// também não quebra linha.
unsafe extern "C" fn on_edit_key(
    view: *mut c_void,
    event: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    let mut key = 0u32;
    if gdk_event_get_keyval(event, &mut key) == 0 || !matches!(key, KEY_RETURN | KEY_KP_ENTER) {
        return 0;
    }
    let mut state = 0u32;
    let shift = gdk_event_get_state(event, &mut state) != 0 && state & SHIFT_MASK != 0;
    let text = buffer_text(gtk_text_view_get_buffer(view));
    EDITING.with(|cell| {
        if let Some(editing) = cell.borrow().as_ref() {
            if !shift && conteudo::can_apply(&editing.original, &text) {
                gtk_dialog_response(editing.dialog, RESPONSE_ACCEPT);
            }
        }
    });
    1
}

/// Edita `original` numa caixa de texto ampla. `None` se o usuário desistir.
pub fn edit_text(original: &str) -> Option<String> {
    if parent_window().is_null() {
        return None;
    }
    unsafe {
        let (dialog, column) = dialog("Editar texto");
        gtk_window_set_resizable(dialog, 1);
        gtk_window_set_default_size(dialog, 560, -1);
        gtk_box_pack_start(
            column,
            label("Editar texto", "rustpdf-dialog-heading"),
            0,
            0,
            0,
        );
        gtk_box_pack_start(
            column,
            label(
                "Altere o texto do trecho selecionado. Caracteres que a fonte do documento \
                 não tiver serão recusados ao aplicar.",
                "rustpdf-dialog-text",
            ),
            0,
            0,
            0,
        );
        let (area, view) = text_area(120);
        gtk_widget_set_margin_top(area, 6);
        gtk_widget_set_vexpand(area, 1);
        gtk_text_view_set_accepts_tab(view, 0);
        gtk_box_pack_start(column, area, 1, 1, 0);
        let footer = gtk_box_new(0, 12);
        let counter = label(&conteudo::character_count(original), "rustpdf-dialog-hint");
        let keys = label("Enter aplica · Esc cancela", "rustpdf-dialog-hint");
        gtk_widget_set_hexpand(counter, 1);
        gtk_box_pack_start(footer, counter, 1, 1, 0);
        gtk_box_pack_end(footer, keys, 0, 0, 0);
        gtk_box_pack_start(column, footer, 0, 0, 0);
        buttons(dialog, "Cancelar", "Aplicar", false);
        gtk_dialog_set_response_sensitive(dialog, RESPONSE_ACCEPT, 0);

        let buffer = gtk_text_view_get_buffer(view);
        let length = original.len().min(c_int::MAX as usize) as c_int;
        gtk_text_buffer_set_text(buffer, original.as_ptr().cast(), length);
        let (mut start, mut end) = (TextIter::new(), TextIter::new());
        gtk_text_buffer_get_start_iter(buffer, &mut start);
        gtk_text_buffer_get_end_iter(buffer, &mut end);
        gtk_text_buffer_select_range(buffer, &end, &start);
        EDITING.with(|cell| {
            *cell.borrow_mut() = Some(EditState {
                dialog,
                counter,
                original: original.to_string(),
            })
        });
        g_signal_connect_data(
            buffer,
            c"changed".as_ptr(),
            on_edit_changed as *const c_void,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
        g_signal_connect_data(
            view,
            c"key-press-event".as_ptr(),
            on_edit_key as *const c_void,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
        gtk_widget_show_all(dialog);
        gtk_widget_grab_focus(view);
        let accepted = gtk_dialog_run(dialog) == RESPONSE_ACCEPT;
        let text = buffer_text(buffer);
        EDITING.with(|cell| cell.borrow_mut().take());
        gtk_widget_destroy(dialog);
        accepted.then(|| conteudo::single_line(&text))
    }
}

thread_local! {
    /// Texto mostrado em cada janela de leitura (para o botão Copiar).
    static SHOWN: RefCell<Vec<(usize, String)>> = const { RefCell::new(Vec::new()) };
    static NEXT_SHOWN: Cell<usize> = const { Cell::new(1) };
}

unsafe extern "C" fn on_copy_shown(_button: *mut c_void, data: *mut c_void) {
    let key = data as usize;
    if let Some(text) = SHOWN.with(|shown| {
        shown
            .borrow()
            .iter()
            .find(|(id, _)| *id == key)
            .map(|(_, text)| text.clone())
    }) {
        crate::menu_contexto::copy_to_clipboard(&text);
        status("Texto copiado para a área de transferência");
    }
}

unsafe extern "C" fn on_shown_destroy(_window: *mut c_void, data: *mut c_void) {
    let key = data as usize;
    SHOWN.with(|shown| shown.borrow_mut().retain(|(id, _)| *id != key));
}

/// Janela de leitura (texto extraído, OCR, listas), com Copiar no título.
pub fn show_text(title: &str, content: &str) {
    let key = NEXT_SHOWN.with(|next| {
        let key = next.get();
        next.set(key + 1);
        key
    });
    SHOWN.with(|shown| shown.borrow_mut().push((key, content.to_string())));
    unsafe {
        let window = gtk_window_new(0);
        let parent = parent_window();
        if !parent.is_null() {
            gtk_window_set_transient_for(window, parent);
        }
        gtk_window_set_default_size(window, 720, 560);
        gtk_window_set_position(window, WINDOW_POSITION_CENTER_ON_PARENT);
        let header = gtk_header_bar_new();
        gtk_header_bar_set_title(header, cstring(title).as_ptr());
        let lines = content.lines().count();
        let subtitle = format!(
            "{} · {} {}",
            conteudo::character_count(content),
            lines,
            if lines == 1 { "linha" } else { "linhas" }
        );
        gtk_header_bar_set_subtitle(header, cstring(&subtitle).as_ptr());
        gtk_header_bar_set_show_close_button(header, 1);
        let copy = gtk_button_new_with_mnemonic(c"_Copiar".as_ptr());
        gtk_widget_set_tooltip_text(copy, c"Copiar todo o texto".as_ptr());
        g_signal_connect_data(
            copy,
            c"clicked".as_ptr(),
            on_copy_shown as *const c_void,
            key as *mut c_void,
            ptr::null_mut(),
            0,
        );
        gtk_header_bar_pack_end(header, copy);
        gtk_window_set_titlebar(window, header);
        let (area, view) = text_area(200);
        gtk_text_view_set_editable(view, 0);
        add_class(view, "rustpdf-reader");
        let buffer = gtk_text_view_get_buffer(view);
        let length = content.len().min(c_int::MAX as usize) as c_int;
        gtk_text_buffer_set_text(buffer, content.as_ptr().cast(), length);
        if content.trim().is_empty() {
            let empty = label("Nenhum texto encontrado.", "rustpdf-dialog-text");
            gtk_label_set_selectable(empty, 0);
            gtk_container_add(window, empty);
        } else {
            gtk_container_add(window, area);
        }
        g_signal_connect_data(
            window,
            c"destroy".as_ptr(),
            on_shown_destroy as *const c_void,
            key as *mut c_void,
            ptr::null_mut(),
            0,
        );
        gtk_widget_show_all(window);
    }
}
