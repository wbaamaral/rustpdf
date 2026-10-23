// SPDX-License-Identifier: AGPL-3.0-or-later
//! Menu de contexto da página (botão direito) e área de transferência
//! (objeto humilde). Quais itens valem em cada situação vem de
//! [`rustpdf::apresentacao::menu_contexto`].

use super::*;
use rustpdf::apresentacao::menu_contexto::{actions, paste_text, Context, Target};
use std::cell::Cell;

#[link(name = "gtk-3")]
extern "C" {
    fn gtk_menu_item_new_with_mnemonic(label: *const c_char) -> *mut c_void;
    fn gtk_separator_menu_item_new() -> *mut c_void;
    fn gtk_menu_popup_at_pointer(menu: *mut c_void, trigger_event: *const c_void);
    fn gtk_bin_get_child(bin: *mut c_void) -> *mut c_void;
    fn gtk_accel_label_set_accel(label: *mut c_void, key: u32, mods: c_int);
    fn gtk_widget_set_visible(widget: *mut c_void, visible: c_int);
    fn gtk_clipboard_get(selection: *mut c_void) -> *mut c_void;
    fn gtk_clipboard_set_text(clipboard: *mut c_void, text: *const c_char, len: c_int);
    fn gtk_clipboard_store(clipboard: *mut c_void);
    fn gtk_clipboard_wait_for_text(clipboard: *mut c_void) -> *mut c_char;
    fn gtk_clipboard_wait_is_text_available(clipboard: *mut c_void) -> c_int;
}

#[link(name = "gdk-3")]
extern "C" {
    fn gdk_atom_intern(name: *const c_char, only_if_exists: c_int) -> *mut c_void;
}

const KEY_Z: u32 = 0x7a;
const KEY_DELETE: u32 = 0xffff;
const CONTROL_MASK: c_int = 1 << 2;
/// Corpo do texto colado, em pontos.
const PASTE_SIZE: f64 = 12.0;

/// Itens que mudam de estado conforme o objeto sob o ponteiro.
struct Items {
    menu: *mut c_void,
    edit_inline: *mut c_void,
    edit_dialog: *mut c_void,
    edit_separator: *mut c_void,
    undo: *mut c_void,
    cut: *mut c_void,
    copy: *mut c_void,
    paste: *mut c_void,
    delete: *mut c_void,
    select_all: *mut c_void,
}

thread_local! {
    static ITEMS: RefCell<Option<Items>> = const { RefCell::new(None) };
    /// Ponto (página) do clique que abriu o menu.
    static POINT: Cell<(f64, f64)> = const { Cell::new((0.0, 0.0)) };
    /// "Selecionar tudo": caixas das linhas da página destacadas.
    static PAGE_SELECTION: RefCell<Vec<[f64; 4]>> = const { RefCell::new(Vec::new()) };
}

fn clipboard() -> *mut c_void {
    unsafe { gtk_clipboard_get(gdk_atom_intern(c"CLIPBOARD".as_ptr(), 0)) }
}

/// Copia `text` para a área de transferência do sistema.
pub fn copy_to_clipboard(text: &str) {
    unsafe {
        let clipboard = clipboard();
        let length = text.len().min(c_int::MAX as usize) as c_int;
        gtk_clipboard_set_text(clipboard, text.as_ptr().cast(), length);
        // Mantém o texto disponível mesmo depois de fechar o programa.
        gtk_clipboard_store(clipboard);
    }
}

fn clipboard_text() -> Option<String> {
    unsafe {
        let raw = gtk_clipboard_wait_for_text(clipboard());
        if raw.is_null() {
            return None;
        }
        let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
        g_free(raw.cast());
        Some(text)
    }
}

/// Há texto da página inteira selecionado ("Selecionar tudo").
pub fn page_selected() -> bool {
    PAGE_SELECTION.with(|lines| !lines.borrow().is_empty())
}

/// Desfaz o "Selecionar tudo" (clique, Esc, troca de página).
pub fn clear_page_selection() {
    let had = PAGE_SELECTION.with(|lines| !std::mem::take(&mut *lines.borrow_mut()).is_empty());
    if had {
        STATE.with(|cell| {
            if let Some(app) = cell.borrow().as_ref() {
                unsafe { gtk_widget_queue_draw(app.image) };
            }
        });
    }
}

/// Destaque das linhas selecionadas por "Selecionar tudo".
pub unsafe fn draw_page_selection(cr: *mut c_void, mapping: PageMapping) {
    PAGE_SELECTION.with(|lines| {
        for [x0, y0, x1, y1] in lines.borrow().iter() {
            let (ax, ay) = mapping.to_widget((*x0, *y1));
            let (bx, by) = mapping.to_widget((*x1, *y0));
            cairo_new_path(cr);
            cairo_rectangle(cr, ax - 1.0, ay - 1.0, bx - ax + 2.0, by - ay + 2.0);
            cairo_set_source_rgba(cr, 0.21, 0.52, 0.89, 0.28);
            cairo_fill_preserve(cr);
            cairo_new_path(cr);
        }
    });
}

unsafe fn menu_item(
    menu: *mut c_void,
    label: &CStr,
    handler: unsafe extern "C" fn(*mut c_void, *mut c_void),
) -> *mut c_void {
    let item = gtk_menu_item_new_with_mnemonic(label.as_ptr());
    g_signal_connect_data(
        item,
        c"activate".as_ptr(),
        handler as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
    gtk_menu_shell_append(menu, item);
    item
}

unsafe fn separator(menu: *mut c_void) -> *mut c_void {
    let separator = gtk_separator_menu_item_new();
    gtk_menu_shell_append(menu, separator);
    separator
}

/// Mostra o atalho de teclado ao lado do item (só os que funcionam na página).
unsafe fn shortcut(item: *mut c_void, key: u32, mods: c_int) {
    let label = gtk_bin_get_child(item);
    if !label.is_null() {
        gtk_accel_label_set_accel(label, key, mods);
    }
}

fn menu() -> *mut c_void {
    ITEMS.with(|cell| {
        if let Some(items) = cell.borrow().as_ref() {
            return items.menu;
        }
        unsafe {
            let menu = gtk_menu_new();
            let edit_inline = menu_item(menu, c"_Editar texto", on_edit_inline);
            let edit_dialog = menu_item(menu, c"Editar em _caixa de texto…", on_edit_dialog);
            let edit_separator = separator(menu);
            let undo = menu_item(menu, c"_Desfazer", on_undo);
            shortcut(undo, KEY_Z, CONTROL_MASK);
            separator(menu);
            let cut = menu_item(menu, c"Recor_tar", on_cut);
            let copy = menu_item(menu, c"C_opiar", on_copy);
            let paste = menu_item(menu, c"Co_lar", on_paste);
            let delete = menu_item(menu, c"_Apagar", on_delete);
            shortcut(delete, KEY_DELETE, 0);
            separator(menu);
            let select_all = menu_item(menu, c"_Selecionar tudo", on_select_all);
            gtk_widget_show_all(menu);
            *cell.borrow_mut() = Some(Items {
                menu,
                edit_inline,
                edit_dialog,
                edit_separator,
                undo,
                cut,
                copy,
                paste,
                delete,
                select_all,
            });
            menu
        }
    })
}

/// Botão direito em `point` (página): seleciona o objeto sob o ponteiro e
/// abre o menu. Devolve se o clique foi tratado.
pub fn open(event: *mut c_void, point: (f64, f64)) -> bool {
    edicao_inline::commit();
    if with_session(|_| Ok(())).is_err() {
        return false;
    }
    // Clique dentro do grupo selecionado: o menu age sobre o grupo.
    let group = crate::selecao_multipla::group_contains(point);
    let target = if group {
        if crate::selecao_multipla::group_has_text() {
            Target::Text
        } else {
            Target::Other
        }
    } else {
        let selection = match with_session(|session| session.select_at(point)) {
            Ok(selection) => selection,
            Err(error) => {
                status(&error);
                return true;
            }
        };
        let target = match &selection {
            None => Target::Nothing,
            Some(selection)
                if matches!(selection.target, rustpdf::SelectionTarget::Text { .. }) =>
            {
                Target::Text
            }
            Some(_) => Target::Other,
        };
        if target != Target::Nothing {
            clear_page_selection();
        }
        set_selection(selection);
        target
    };
    POINT.with(|cell| cell.set(point));
    let enabled = actions(Context {
        target,
        page_text_selected: page_selected(),
        clipboard_has_text: unsafe { gtk_clipboard_wait_is_text_available(clipboard()) } != 0,
        can_undo: with_session(|session| Ok(session.can_undo())).unwrap_or(false),
        group,
    });
    let menu = menu();
    ITEMS.with(|cell| {
        if let Some(items) = cell.borrow().as_ref() {
            unsafe {
                // Editar só aparece sobre texto; o resto fica esmaecido.
                for widget in [items.edit_inline, items.edit_dialog, items.edit_separator] {
                    gtk_widget_set_visible(widget, c_int::from(enabled.edit_inline));
                }
                for (widget, on) in [
                    (items.undo, enabled.undo),
                    (items.cut, enabled.cut),
                    (items.copy, enabled.copy),
                    (items.paste, enabled.paste),
                    (items.delete, enabled.delete),
                    (items.select_all, enabled.select_all),
                ] {
                    gtk_widget_set_sensitive(widget, c_int::from(on));
                }
            }
        }
    });
    unsafe { gtk_menu_popup_at_pointer(menu, event) };
    true
}

fn report(result: AppResult<()>) {
    if let Err(error) = result {
        status(&error);
    }
}

unsafe extern "C" fn on_edit_inline(_item: *mut c_void, _data: *mut c_void) {
    report(edit_text_at(POINT.with(Cell::get)));
}

unsafe extern "C" fn on_edit_dialog(_item: *mut c_void, _data: *mut c_void) {
    report(edit_text_in_dialog(POINT.with(Cell::get)));
}

unsafe extern "C" fn on_undo(_item: *mut c_void, _data: *mut c_void) {
    report(dispatch(20));
}

/// Texto do trecho sob o ponto do menu.
fn text_under_menu() -> AppResult<String> {
    let (_, run) = locate_text(POINT.with(Cell::get))?;
    Ok(run)
}

unsafe extern "C" fn on_copy(_item: *mut c_void, _data: *mut c_void) {
    report((|| {
        let text = if page_selected() {
            with_session(|session| session.extract_text())?
        } else if crate::selecao_multipla::active() {
            crate::selecao_multipla::group_text()
        } else {
            text_under_menu()?
        };
        copy_to_clipboard(text.trim_end());
        status("Texto copiado para a área de transferência");
        Ok(())
    })());
}

unsafe extern "C" fn on_cut(_item: *mut c_void, _data: *mut c_void) {
    report((|| {
        let text = if crate::selecao_multipla::active() {
            crate::selecao_multipla::group_text()
        } else {
            text_under_menu()?
        };
        copy_to_clipboard(&text);
        dispatch(58)?;
        status("Texto recortado para a área de transferência");
        Ok(())
    })());
}

unsafe extern "C" fn on_paste(_item: *mut c_void, _data: *mut c_void) {
    report((|| {
        let text = clipboard_text()
            .as_deref()
            .and_then(paste_text)
            .ok_or("A área de transferência não tem texto")?;
        let (x, y) = POINT.with(Cell::get);
        with_session(|session| session.insert_visible_text_at(&text, x, y, PASTE_SIZE))?;
        render()?;
        status("Texto colado");
        Ok(())
    })());
}

unsafe extern "C" fn on_delete(_item: *mut c_void, _data: *mut c_void) {
    report(dispatch(58));
}

unsafe extern "C" fn on_select_all(_item: *mut c_void, _data: *mut c_void) {
    report((|| {
        let lines = with_session(|session| session.page_text_lines())?;
        if lines.is_empty() {
            return Err("Esta página não tem texto".into());
        }
        let count = lines.len();
        set_selection(None);
        PAGE_SELECTION.with(|cell| *cell.borrow_mut() = lines);
        STATE.with(|cell| {
            if let Some(app) = cell.borrow().as_ref() {
                gtk_widget_queue_draw(app.image);
            }
        });
        status(&format!(
            "Todo o texto da página selecionado ({count} {}) — clique direito › Copiar",
            if count == 1 { "linha" } else { "linhas" }
        ));
        Ok(())
    })());
}
