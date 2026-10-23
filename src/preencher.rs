// SPDX-License-Identifier: AGPL-3.0-or-later
//! Preenchimento de campos de formulário pela interface (objeto humilde):
//! clique duplo num campo abre a lista de opções (combo/lista), alterna a
//! caixa de seleção, marca a opção de rádio clicada ou pede o texto.

use super::*;
use rustpdf::{FieldInfo, FieldKind};

#[link(name = "gtk-3")]
extern "C" {
    fn gtk_check_menu_item_new_with_label(label: *const c_char) -> *mut c_void;
    fn gtk_check_menu_item_set_active(item: *mut c_void, active: c_int);
    fn gtk_check_menu_item_set_draw_as_radio(item: *mut c_void, draw_as_radio: c_int);
    fn gtk_menu_popup_at_rect(
        menu: *mut c_void,
        window: *mut c_void,
        rect: *const GdkRectangle,
        rect_anchor: c_int,
        menu_anchor: c_int,
        trigger_event: *const c_void,
    );
}

#[repr(C)]
struct GdkRectangle {
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
}

/// `GDK_GRAVITY_SOUTH_WEST` e `GDK_GRAVITY_NORTH_WEST`: a lista abre logo
/// abaixo do campo, alinhada à esquerda.
const GRAVITY_SOUTH_WEST: c_int = 7;
const GRAVITY_NORTH_WEST: c_int = 1;

/// Campo cujas opções estão no menu aberto.
struct Choosing {
    name: String,
    options: Vec<String>,
    point: (f64, f64),
}

/// Lista montada, esperando o botão do clique duplo ser solto para abrir
/// (aberta com o botão ainda apertado, a soltura a fecha na hora).
struct Pending {
    window: *mut c_void,
    rect: GdkRectangle,
}

thread_local! {
    static MENU: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    static CHOOSING: RefCell<Option<Choosing>> = const { RefCell::new(None) };
    /// Item que está sendo marcado ao montar o menu (não é escolha do usuário).
    static BUILDING: Cell<bool> = const { Cell::new(false) };
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
}

use std::cell::Cell;

/// Preenche `field` (sob `point`) conforme o tipo.
pub fn fill(field: FieldInfo, point: (f64, f64)) -> AppResult<()> {
    match field.kind {
        FieldKind::Combo | FieldKind::List => {
            if field.options.is_empty() {
                return Err(format!("O campo “{}” não tem opções", field.name));
            }
            show_options(field, point);
            Ok(())
        }
        FieldKind::Checkbox => {
            let checked = field.on_state.as_deref() == Some(field.value.as_str());
            with_session(|s| s.set_checkbox_value(&field.name, !checked))?;
            done(
                point,
                &format!(
                    "“{}”: {}",
                    field.name,
                    if checked { "desmarcada" } else { "marcada" }
                ),
            )
        }
        FieldKind::Radio => {
            let state = field.on_state.ok_or("Opção de rádio sem estado")?;
            with_session(|s| s.set_radio_state(&field.name, &state))?;
            done(point, &format!("“{}”: opção marcada", field.name))
        }
        FieldKind::Text => {
            let Some(value) =
                prompt_text_default(&format!("Valor do campo “{}”", field.name), &field.value)
            else {
                return Ok(());
            };
            if value == field.value {
                return Ok(());
            }
            with_session(|s| s.set_text_field_value(&field.name, &value))?;
            done(point, &format!("“{}” preenchido", field.name))
        }
        FieldKind::Other => Err("Este tipo de campo não pode ser preenchido aqui".into()),
    }
}

fn done(point: (f64, f64), message: &str) -> AppResult<()> {
    render()?;
    if let Ok(selection) = with_session(|s| s.select_at(point)) {
        set_selection(selection);
    }
    status(message);
    Ok(())
}

/// Caixa do campo sob `point` na janela principal (pixels), para ancorar a lista.
fn field_rect(point: (f64, f64)) -> Option<(*mut c_void, GdkRectangle)> {
    let selection = with_session(|s| s.select_at(point)).ok().flatten()?;
    STATE.with(|cell| {
        let borrowed = cell.borrow();
        let app = borrowed.as_ref()?;
        let mapping = PageMapping::of(app)?;
        let [x0, y0, x1, y1] = selection.frame.bounds();
        let (ax, ay) = mapping.to_widget((x0, y1));
        let (bx, by) = mapping.to_widget((x1, y0));
        let (mut left, mut top) = (0, 0);
        let translated = unsafe {
            gtk_widget_translate_coordinates(
                app.image,
                app.window,
                ax.round() as c_int,
                ay.round() as c_int,
                &mut left,
                &mut top,
            )
        } != 0;
        let window = unsafe { gtk_widget_get_window(app.window) };
        (translated && !window.is_null()).then_some((
            window,
            GdkRectangle {
                x: left,
                y: top,
                width: (bx - ax).round().max(1.0) as c_int,
                height: (by - ay).round().max(1.0) as c_int,
            },
        ))
    })
}

/// Abre a lista pendente (chamado ao soltar o botão). Devolve se abriu.
pub fn open_pending(event: *mut c_void) -> bool {
    let Some(pending) = PENDING.with(|cell| cell.borrow_mut().take()) else {
        return false;
    };
    let menu = MENU.with(Cell::get);
    unsafe {
        gtk_widget_set_size_request(menu, pending.rect.width, -1);
        gtk_menu_popup_at_rect(
            menu,
            pending.window,
            &pending.rect,
            GRAVITY_SOUTH_WEST,
            GRAVITY_NORTH_WEST,
            event,
        );
    }
    true
}

/// Monta a lista de opções (a atual marcada); abre ao soltar o botão,
/// logo abaixo do campo e com a largura dele.
fn show_options(field: FieldInfo, point: (f64, f64)) {
    let menu = MENU.with(|cell| {
        if cell.get().is_null() {
            cell.set(unsafe { gtk_menu_new() });
        }
        cell.get()
    });
    unsafe {
        clear_children(menu);
        BUILDING.with(|building| building.set(true));
        for (index, option) in field.options.iter().enumerate() {
            let item = gtk_check_menu_item_new_with_label(cstring(option).as_ptr());
            gtk_check_menu_item_set_draw_as_radio(item, 1);
            gtk_check_menu_item_set_active(item, c_int::from(*option == field.value));
            g_signal_connect_data(
                item,
                c"activate".as_ptr(),
                on_option as *const c_void,
                index as *mut c_void,
                ptr::null_mut(),
                0,
            );
            gtk_menu_shell_append(menu, item);
        }
        BUILDING.with(|building| building.set(false));
        gtk_widget_show_all(menu);
    }
    CHOOSING.with(|cell| {
        *cell.borrow_mut() = Some(Choosing {
            name: field.name,
            options: field.options,
            point,
        })
    });
    status("Escolha uma opção (Esc fecha sem mudar)");
    let pending = field_rect(point).map(|(window, rect)| Pending { window, rect });
    PENDING.with(|cell| *cell.borrow_mut() = pending);
}

unsafe extern "C" fn on_option(_item: *mut c_void, data: *mut c_void) {
    if BUILDING.with(Cell::get) {
        return;
    }
    let index = data as usize;
    let Some(choosing) = CHOOSING.with(|cell| cell.borrow_mut().take()) else {
        return;
    };
    let Some(option) = choosing.options.get(index).cloned() else {
        return;
    };
    let result = with_session(|s| s.set_choice_value(&choosing.name, &option))
        .and_then(|()| done(choosing.point, &format!("“{}”: {option}", choosing.name)));
    if let Err(error) = result {
        status(&error);
    }
}

/// Que tipo de campo o item do menu Formulários preenche.
#[derive(Clone, Copy, PartialEq)]
pub enum Wanted {
    Text,
    Checkbox,
    Radio,
    Choice,
}

impl Wanted {
    fn accepts(self, kind: &FieldKind) -> bool {
        match self {
            Wanted::Text => *kind == FieldKind::Text,
            Wanted::Checkbox => *kind == FieldKind::Checkbox,
            Wanted::Radio => *kind == FieldKind::Radio,
            Wanted::Choice => matches!(kind, FieldKind::Combo | FieldKind::List),
        }
    }

    /// Nome do tipo e o item do menu que cria um.
    fn noun(self) -> (&'static str, &'static str) {
        match self {
            Wanted::Text => ("campo de texto", "Campo de texto"),
            Wanted::Checkbox => ("caixa de seleção", "Caixa de seleção"),
            Wanted::Radio => ("grupo de rádio", "Botão radial"),
            Wanted::Choice => ("lista de opções", "Lista de opções"),
        }
    }
}

/// Valor atual em palavras, para a lista de campos.
fn describe(field: &FieldInfo) -> String {
    let value = match field.kind {
        FieldKind::Checkbox if field.states.contains(&field.value) => "marcada".to_string(),
        FieldKind::Checkbox => "desmarcada".to_string(),
        FieldKind::Radio => field
            .states
            .iter()
            .position(|state| *state == field.value)
            .and_then(|index| field.options.get(index).cloned())
            .unwrap_or_else(|| "nenhuma opção".into()),
        _ if field.value.is_empty() => "vazio".into(),
        _ => field.value.clone(),
    };
    format!("{} — {value}", field.name)
}

/// Menu Formulários › Preencher…: escolhe o campo (sem digitar o nome
/// interno) e o novo valor entre os possíveis.
pub fn fill_from_menu(wanted: Wanted) -> AppResult<()> {
    let fields: Vec<FieldInfo> = with_session(|s| s.form_fields())?
        .into_iter()
        .filter(|field| wanted.accepts(&field.kind))
        .collect();
    let (noun, menu) = wanted.noun();
    if fields.is_empty() {
        dialogos::inform(
            &format!("O documento não tem {noun}"),
            &format!("Crie um em Formulários › {menu} e clique na página onde ele deve ficar."),
        );
        return Ok(());
    }
    let field = if fields.len() == 1 {
        fields.into_iter().next().expect("um campo")
    } else {
        let items: Vec<String> = fields.iter().map(describe).collect();
        let Some(index) =
            dialogos::choose(&format!("Qual {noun}?"), None, &items, None, "Continuar")
        else {
            return Ok(());
        };
        fields.into_iter().nth(index).expect("índice escolhido")
    };
    match field.kind {
        FieldKind::Text => {
            let Some(value) =
                prompt_text_default(&format!("Valor do campo “{}”", field.name), &field.value)
            else {
                return Ok(());
            };
            with_session(|s| s.set_text_field_value(&field.name, &value))?;
        }
        FieldKind::Checkbox => {
            let checked = field.states.contains(&field.value);
            let states = ["Marcada".to_string(), "Desmarcada".to_string()];
            let current = Some(if checked { 0 } else { 1 });
            let Some(index) = dialogos::choose(
                &format!("“{}”", field.name),
                None,
                &states,
                current,
                "Aplicar",
            ) else {
                return Ok(());
            };
            with_session(|s| s.set_checkbox_value(&field.name, index == 0))?;
        }
        FieldKind::Radio => {
            let current = field.states.iter().position(|state| *state == field.value);
            let Some(index) = dialogos::choose(
                &format!("Opção de “{}”", field.name),
                None,
                &field.options,
                current,
                "Aplicar",
            ) else {
                return Ok(());
            };
            let state = field.states.get(index).ok_or("Opção de rádio sem estado")?;
            with_session(|s| s.set_radio_state(&field.name, state))?;
        }
        _ => {
            let current = field
                .options
                .iter()
                .position(|option| *option == field.value);
            let Some(index) = dialogos::choose(
                &format!("Opção de “{}”", field.name),
                None,
                &field.options,
                current,
                "Aplicar",
            ) else {
                return Ok(());
            };
            with_session(|s| s.set_choice_value(&field.name, &field.options[index]))?;
        }
    }
    render()?;
    let updated = with_session(|s| s.form_fields())?
        .into_iter()
        .find(|candidate| candidate.name == field.name)
        .map(|candidate| describe(&candidate))
        .unwrap_or(field.name);
    status(&format!("Campo atualizado: {updated}"));
    Ok(())
}
