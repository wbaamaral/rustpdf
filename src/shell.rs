// SPDX-License-Identifier: AGPL-3.0-or-later
//! Moldura da janela: menus, barras de ferramentas, painel lateral, barra de
//! status, estilo e navegação: ícones simbólicos em barras planas, ferramentas
//! exclusivas espelhadas no menu, miniaturas de página e atalhos de teclado
//! padrão.

use super::*;
use std::fs;

#[link(name = "gtk-3")]
extern "C" {
    fn gtk_toolbar_new() -> *mut c_void;
    fn gtk_toolbar_insert(toolbar: *mut c_void, item: *mut c_void, position: c_int);
    fn gtk_toolbar_set_style(toolbar: *mut c_void, style: c_int);
    fn gtk_toolbar_set_icon_size(toolbar: *mut c_void, size: c_int);
    fn gtk_tool_button_new(icon: *mut c_void, label: *const c_char) -> *mut c_void;
    fn gtk_radio_tool_button_new(group: *mut c_void) -> *mut c_void;
    fn gtk_radio_tool_button_new_from_widget(member: *mut c_void) -> *mut c_void;
    fn gtk_tool_button_set_icon_widget(button: *mut c_void, icon: *mut c_void);
    fn gtk_tool_button_set_label(button: *mut c_void, label: *const c_char);
    fn gtk_toggle_tool_button_set_active(button: *mut c_void, active: c_int);
    fn gtk_toggle_tool_button_get_active(button: *mut c_void) -> c_int;
    fn gtk_separator_tool_item_new() -> *mut c_void;
    fn gtk_tool_item_new() -> *mut c_void;
    fn gtk_image_new_from_icon_name(name: *const c_char, size: c_int) -> *mut c_void;
    fn gtk_widget_set_tooltip_markup(widget: *mut c_void, markup: *const c_char);
    fn gtk_accel_group_new() -> *mut c_void;
    fn gtk_window_add_accel_group(window: *mut c_void, group: *mut c_void);
    fn gtk_accelerator_parse(accelerator: *const c_char, key: *mut u32, mods: *mut c_int);
    fn gtk_widget_add_accelerator(
        widget: *mut c_void,
        signal: *const c_char,
        group: *mut c_void,
        key: u32,
        mods: c_int,
        flags: c_int,
    );
    fn gtk_menu_item_new_with_mnemonic(label: *const c_char) -> *mut c_void;
    fn gtk_check_menu_item_new_with_mnemonic(label: *const c_char) -> *mut c_void;
    fn gtk_radio_menu_item_new_with_mnemonic_from_widget(
        member: *mut c_void,
        label: *const c_char,
    ) -> *mut c_void;
    fn gtk_check_menu_item_set_active(item: *mut c_void, active: c_int);
    fn gtk_check_menu_item_get_active(item: *mut c_void) -> c_int;
    fn gtk_accel_label_set_accel(label: *mut c_void, key: u32, mods: c_int);
    fn gtk_bin_get_child(bin: *mut c_void) -> *mut c_void;
    fn gtk_icon_theme_get_default() -> *mut c_void;
    fn gtk_icon_theme_append_search_path(theme: *mut c_void, path: *const c_char);
    fn gtk_combo_box_text_new_with_entry() -> *mut c_void;
    fn gtk_combo_box_text_append_text(combo: *mut c_void, text: *const c_char);
    fn gtk_combo_box_get_active(combo: *mut c_void) -> c_int;
    fn gtk_stack_new() -> *mut c_void;
    fn gtk_stack_add_named(stack: *mut c_void, child: *mut c_void, name: *const c_char);
    fn gtk_stack_set_visible_child_name(stack: *mut c_void, name: *const c_char);
    fn gtk_radio_button_new_from_widget(member: *mut c_void) -> *mut c_void;
    fn gtk_toggle_button_set_mode(button: *mut c_void, draw_indicator: c_int);
    fn gtk_toggle_button_get_active(button: *mut c_void) -> c_int;
    fn gtk_button_set_image(button: *mut c_void, image: *mut c_void);
    fn gtk_button_set_relief(button: *mut c_void, relief: c_int);
    fn gtk_button_new() -> *mut c_void;
    fn gtk_css_provider_new() -> *mut c_void;
    fn gtk_css_provider_load_from_data(
        provider: *mut c_void,
        data: *const c_char,
        length: isize,
        error: *mut *mut c_void,
    ) -> c_int;
    fn gtk_style_context_add_provider_for_screen(
        screen: *mut c_void,
        provider: *mut c_void,
        priority: u32,
    );
    fn gtk_widget_get_style_context(widget: *mut c_void) -> *mut c_void;
    fn gtk_style_context_add_class(context: *mut c_void, name: *const c_char);
    fn gtk_style_context_remove_class(context: *mut c_void, name: *const c_char);
    fn gtk_widget_set_name(widget: *mut c_void, name: *const c_char);
    fn gtk_widget_get_visible(widget: *mut c_void) -> c_int;
    fn gtk_widget_set_visible(widget: *mut c_void, visible: c_int);
    fn gtk_window_fullscreen(window: *mut c_void);
    fn gtk_window_unfullscreen(window: *mut c_void);
    fn gtk_scrolled_window_get_hadjustment(window: *mut c_void) -> *mut c_void;
    fn gtk_scrolled_window_get_vadjustment(window: *mut c_void) -> *mut c_void;
    fn gtk_scrolled_window_set_policy(window: *mut c_void, h: c_int, v: c_int);
    fn gtk_adjustment_get_value(adjustment: *mut c_void) -> f64;
    fn gtk_adjustment_set_value(adjustment: *mut c_void, value: f64);
    fn gtk_widget_grab_focus(widget: *mut c_void);
    fn gtk_window_get_focus(window: *mut c_void) -> *mut c_void;
    fn gtk_editable_get_type() -> usize;
    fn gtk_label_set_markup(label: *mut c_void, markup: *const c_char);
    fn gtk_image_clear(image: *mut c_void);
    fn gtk_widget_set_hexpand(widget: *mut c_void, expand: c_int);
    fn gtk_widget_set_halign(widget: *mut c_void, align: c_int);
    fn gtk_widget_set_valign(widget: *mut c_void, align: c_int);
    fn gtk_widget_get_allocated_width(widget: *mut c_void) -> c_int;
    fn gtk_widget_get_allocated_height(widget: *mut c_void) -> c_int;
    fn gtk_window_set_icon_name(window: *mut c_void, name: *const c_char);
    fn gtk_window_set_transient_for(window: *mut c_void, parent: *mut c_void);
    fn gtk_about_dialog_new() -> *mut c_void;
    fn gtk_about_dialog_set_program_name(about: *mut c_void, name: *const c_char);
    fn gtk_about_dialog_set_version(about: *mut c_void, version: *const c_char);
    fn gtk_about_dialog_set_comments(about: *mut c_void, comments: *const c_char);
    fn gtk_about_dialog_set_copyright(about: *mut c_void, copyright: *const c_char);
    fn gtk_about_dialog_set_website(about: *mut c_void, website: *const c_char);
    fn gtk_about_dialog_set_website_label(about: *mut c_void, label: *const c_char);
    fn gtk_about_dialog_set_logo_icon_name(about: *mut c_void, name: *const c_char);
    fn gtk_about_dialog_set_license_type(about: *mut c_void, license: c_int);
    fn gtk_about_dialog_set_authors(about: *mut c_void, authors: *const *const c_char);
    fn gtk_about_dialog_add_credit_section(
        about: *mut c_void,
        section: *const c_char,
        people: *const *const c_char,
    );
    fn gtk_get_major_version() -> u32;
    fn gtk_get_minor_version() -> u32;
    fn gtk_get_micro_version() -> u32;
}

#[link(name = "qpdf")]
extern "C" {
    fn qpdf_get_qpdf_version() -> *const c_char;
}

#[link(name = "gdk-3")]
extern "C" {
    fn gdk_screen_get_default() -> *mut c_void;
    fn gdk_event_get_keyval(event: *mut c_void, keyval: *mut u32) -> c_int;
    fn gdk_event_get_root_coords(event: *mut c_void, x: *mut f64, y: *mut f64) -> c_int;
    fn gdk_event_get_scroll_direction(event: *mut c_void, direction: *mut c_int) -> c_int;
    fn gdk_event_get_scroll_deltas(event: *mut c_void, dx: *mut f64, dy: *mut f64) -> c_int;
    fn gdk_event_get_state(event: *mut c_void, state: *mut u32) -> c_int;
}

#[link(name = "gobject-2.0")]
extern "C" {
    fn g_type_check_instance_is_a(instance: *mut c_void, type_: usize) -> c_int;
}

#[link(name = "glib-2.0")]
extern "C" {
    fn g_idle_add(function: unsafe extern "C" fn(*mut c_void) -> c_int, data: *mut c_void) -> u32;
    fn g_markup_escape_text(text: *const c_char, length: isize) -> *mut c_char;
    fn g_timeout_add(
        interval: u32,
        function: unsafe extern "C" fn(*mut c_void) -> c_int,
        data: *mut c_void,
    ) -> u32;
}

const ICON_SIZE_SMALL_TOOLBAR: c_int = 2;
const ACCEL_VISIBLE: c_int = 1;
const RELIEF_NONE: c_int = 2;
const POLICY_AUTOMATIC: c_int = 1;
const POLICY_NEVER: c_int = 2;
const ALIGN_START: c_int = 1;
const ALIGN_CENTER: c_int = 3;
const GDK_CONTROL_MASK: u32 = 1 << 2;
const KEY_DELETE: u32 = 0xffff;
const KEY_KP_DELETE: u32 = 0xff9f;
const KEY_ESCAPE: u32 = 0xff1b;
/// Pontos PDF → pixels a 100% (96 dpi).
const PIXELS_PER_POINT: f64 = 96.0 / 72.0;
const THUMB_WIDTH: u32 = 110;

/// Como o zoom é calculado.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ZoomMode {
    Custom,
    FitPage,
    FitWidth,
}

/// Widgets e estado da moldura da janela.
pub struct Widgets {
    pub scroll: *mut c_void,
    pub page_total: *mut c_void,
    pub zoom_entry: *mut c_void,
    pub info: *mut c_void,
    pub sidebar: *mut c_void,
    pub sidebar_stack: *mut c_void,
    pub thumbs_box: *mut c_void,
    pub comments_box: *mut c_void,
    pub statusbar: *mut c_void,
    pub welcome: *mut c_void,
    pub zoom: f64,
    pub zoom_mode: ZoomMode,
    /// Documento e contador de mudanças refletidos nas miniaturas; botão e
    /// imagem de cada página.
    pub thumbs_state: Option<(PathBuf, u64)>,
    pub thumb_buttons: Vec<*mut c_void>,
    pub thumb_images: Vec<*mut c_void>,
    pub thumb_polling: bool,
    pub comments_key: Option<(u64, usize)>,
    pub doc_widgets: Vec<*mut c_void>,
    pub undo_widgets: Vec<*mut c_void>,
    pub redo_widgets: Vec<*mut c_void>,
    pub selection_widgets: Vec<*mut c_void>,
    /// Botões e itens de menu de ferramenta, por modo.
    pub tool_widgets: Vec<(ToolMode, *mut c_void, bool)>,
    pub syncing: bool,
    pub fullscreen: bool,
    pub refit_pending: bool,
    /// Arraste da ferramenta Mão: posição inicial (tela) e rolagem inicial.
    pub hand: Option<(f64, f64, f64, f64)>,
}

/// Ferramentas exclusivas, na ordem das barras e do menu.
const TOOLS: [ToolMode; 10] = [
    ToolMode::None,
    ToolMode::Hand,
    ToolMode::Text,
    ToolMode::Image,
    ToolMode::Note,
    ToolMode::Highlight,
    ToolMode::TextField,
    ToolMode::Checkbox,
    ToolMode::Radio,
    ToolMode::Choice,
];

fn tool_index(mode: ToolMode) -> usize {
    TOOLS.iter().position(|tool| *tool == mode).unwrap_or(0)
}

fn with_widgets<T>(f: impl FnOnce(&mut Widgets) -> T) -> Option<T> {
    STATE.with(|cell| {
        cell.try_borrow_mut()
            .ok()
            .and_then(|mut borrowed| borrowed.as_mut().map(|app| f(&mut app.ui)))
    })
}

fn escape(text: &str) -> String {
    unsafe {
        let escaped = g_markup_escape_text(cstring(text).as_ptr(), -1);
        let value = CStr::from_ptr(escaped).to_string_lossy().into_owned();
        g_free(escaped as *mut c_void);
        value
    }
}

/// Dica em duas linhas: título em negrito e explicação.
unsafe fn set_tip(widget: *mut c_void, title: &str, description: &str) {
    let markup = if description.is_empty() {
        format!("<b>{}</b>", escape(title))
    } else {
        format!("<b>{}</b>\n{}", escape(title), escape(description))
    };
    gtk_widget_set_tooltip_markup(widget, cstring(&markup).as_ptr());
}

unsafe fn icon(name: &str) -> *mut c_void {
    gtk_image_new_from_icon_name(cstring(name).as_ptr(), ICON_SIZE_SMALL_TOOLBAR)
}

unsafe fn add_class(widget: *mut c_void, class: &str) {
    gtk_style_context_add_class(
        gtk_widget_get_style_context(widget),
        cstring(class).as_ptr(),
    );
}

unsafe fn remove_class(widget: *mut c_void, class: &str) {
    gtk_style_context_remove_class(
        gtk_widget_get_style_context(widget),
        cstring(class).as_ptr(),
    );
}

/// Contexto de construção: grupo de atalhos e listas de sensibilidade.
struct Builder {
    accel: *mut c_void,
    ui: Widgets,
    tool_menu_group: *mut c_void,
}

/// Quando um widget fica habilitado.
#[derive(Clone, Copy, PartialEq)]
enum Needs {
    Nothing,
    Document,
    Undo,
    Redo,
    Selection,
}

impl Builder {
    fn track(&mut self, widget: *mut c_void, needs: Needs) {
        match needs {
            Needs::Nothing => {}
            Needs::Document => self.ui.doc_widgets.push(widget),
            Needs::Undo => self.ui.undo_widgets.push(widget),
            Needs::Redo => self.ui.redo_widgets.push(widget),
            Needs::Selection => self.ui.selection_widgets.push(widget),
        }
    }

    unsafe fn accelerate(&self, widget: *mut c_void, accel: &str, visible: bool) {
        let (mut key, mut mods) = (0u32, 0);
        gtk_accelerator_parse(cstring(accel).as_ptr(), &mut key, &mut mods);
        if key != 0 {
            gtk_widget_add_accelerator(
                widget,
                c"activate".as_ptr(),
                self.accel,
                key,
                mods,
                if visible { ACCEL_VISIBLE } else { 0 },
            );
        }
    }

    unsafe fn menu(&mut self, menubar: *mut c_void, label: &str) -> *mut c_void {
        let item = gtk_menu_item_new_with_mnemonic(cstring(label).as_ptr());
        let menu = gtk_menu_new();
        gtk_menu_item_set_submenu(item, menu);
        gtk_menu_shell_append(menubar, item);
        menu
    }

    unsafe fn submenu(&mut self, menu: *mut c_void, label: &str, needs: Needs) -> *mut c_void {
        let item = gtk_menu_item_new_with_mnemonic(cstring(label).as_ptr());
        let submenu = gtk_menu_new();
        gtk_menu_item_set_submenu(item, submenu);
        gtk_menu_shell_append(menu, item);
        self.track(item, needs);
        submenu
    }

    /// Item de menu que dispara `action`; `accel` no formato do GTK (`<Control>o`).
    unsafe fn item(
        &mut self,
        menu: *mut c_void,
        label: &str,
        action: usize,
        accel: &[&str],
        needs: Needs,
    ) -> *mut c_void {
        let item = gtk_menu_item_new_with_mnemonic(cstring(label).as_ptr());
        gtk_menu_shell_append(menu, item);
        g_signal_connect_data(
            item,
            c"activate".as_ptr(),
            on_clicked as *const c_void,
            action as *mut c_void,
            ptr::null_mut(),
            0,
        );
        for (index, accel) in accel.iter().enumerate() {
            self.accelerate(item, accel, index == 0);
        }
        self.track(item, needs);
        item
    }

    /// Item cuja tecla é tratada fora dos aceleradores (ex.: Del), só exibida.
    unsafe fn item_showing_key(
        &mut self,
        menu: *mut c_void,
        label: &str,
        action: usize,
        key: u32,
        needs: Needs,
    ) {
        let item = self.item(menu, label, action, &[], needs);
        gtk_accel_label_set_accel(gtk_bin_get_child(item), key, 0);
    }

    unsafe fn check(
        &mut self,
        menu: *mut c_void,
        label: &str,
        action: usize,
        accel: &str,
        active: bool,
    ) {
        let item = gtk_check_menu_item_new_with_mnemonic(cstring(label).as_ptr());
        gtk_check_menu_item_set_active(item, c_int::from(active));
        gtk_menu_shell_append(menu, item);
        g_signal_connect_data(
            item,
            c"toggled".as_ptr(),
            on_clicked as *const c_void,
            action as *mut c_void,
            ptr::null_mut(),
            0,
        );
        if !accel.is_empty() {
            self.accelerate(item, accel, true);
        }
    }

    /// Item de rádio de ferramenta (sincronizado com a barra de ferramentas).
    unsafe fn tool_item(&mut self, menu: *mut c_void, label: &str, mode: ToolMode, accel: &str) {
        let item = gtk_radio_menu_item_new_with_mnemonic_from_widget(
            self.tool_menu_group,
            cstring(label).as_ptr(),
        );
        self.tool_menu_group = item;
        gtk_menu_shell_append(menu, item);
        g_signal_connect_data(
            item,
            c"toggled".as_ptr(),
            on_tool_toggled as *const c_void,
            tool_index(mode) as *mut c_void,
            ptr::null_mut(),
            0,
        );
        if !accel.is_empty() {
            self.accelerate(item, accel, true);
        }
        if mode != ToolMode::None {
            self.track(item, Needs::Document);
        }
        self.ui.tool_widgets.push((mode, item, false));
    }

    unsafe fn separator(&mut self, menu: *mut c_void) {
        gtk_menu_shell_append(menu, gtk_separator_menu_item_new());
    }

    /// Botão plano só com ícone.
    unsafe fn button(
        &mut self,
        toolbar: *mut c_void,
        icon_name: &str,
        title: &str,
        description: &str,
        action: usize,
        needs: Needs,
    ) -> *mut c_void {
        let item = gtk_tool_button_new(icon(icon_name), cstring(title).as_ptr());
        set_tip(item, title, description);
        gtk_toolbar_insert(toolbar, item, -1);
        g_signal_connect_data(
            item,
            c"clicked".as_ptr(),
            on_clicked as *const c_void,
            action as *mut c_void,
            ptr::null_mut(),
            0,
        );
        self.track(item, needs);
        item
    }

    /// Botão de ferramenta exclusiva (grupo de rádio).
    unsafe fn tool_button(
        &mut self,
        toolbar: *mut c_void,
        group: &mut *mut c_void,
        icon_name: &str,
        title: &str,
        description: &str,
        mode: ToolMode,
    ) {
        let item = if group.is_null() {
            gtk_radio_tool_button_new(ptr::null_mut())
        } else {
            gtk_radio_tool_button_new_from_widget(*group)
        };
        *group = item;
        gtk_tool_button_set_icon_widget(item, icon(icon_name));
        gtk_tool_button_set_label(item, cstring(title).as_ptr());
        set_tip(item, title, description);
        gtk_toolbar_insert(toolbar, item, -1);
        g_signal_connect_data(
            item,
            c"toggled".as_ptr(),
            on_tool_toggled as *const c_void,
            tool_index(mode) as *mut c_void,
            ptr::null_mut(),
            0,
        );
        if mode != ToolMode::None {
            self.track(item, Needs::Document);
        }
        self.ui.tool_widgets.push((mode, item, true));
    }

    unsafe fn bar_separator(&mut self, toolbar: *mut c_void) {
        gtk_toolbar_insert(toolbar, gtk_separator_tool_item_new(), -1);
    }

    /// Item de barra com um widget qualquer (campo de página, zoom).
    unsafe fn bar_widget(&mut self, toolbar: *mut c_void, widget: *mut c_void, needs: Needs) {
        let item = gtk_tool_item_new();
        gtk_container_add(item, widget);
        gtk_toolbar_insert(toolbar, item, -1);
        self.track(item, needs);
    }
}

unsafe fn new_toolbar() -> *mut c_void {
    let toolbar = gtk_toolbar_new();
    gtk_toolbar_set_style(toolbar, 0);
    gtk_toolbar_set_icon_size(toolbar, ICON_SIZE_SMALL_TOOLBAR);
    toolbar
}

const CSS: &str = "
toolbar { padding: 1px 4px; }
toolbar separator { margin: 4px 3px; }
toolbar button { padding: 4px; border-radius: 4px; }
toolbar button:checked { background-color: alpha(@theme_selected_bg_color, 0.55); box-shadow: inset 0 0 0 1px alpha(@theme_selected_bg_color, 0.9); }
.rustpdf-strip { padding: 4px 2px; border-right: 1px solid alpha(currentColor, 0.15); }
.rustpdf-strip button { padding: 6px; margin: 1px 0; border-radius: 4px; }
.rustpdf-panel-title { font-weight: bold; padding: 6px 8px; }
.rustpdf-panel-header { border-bottom: 1px solid alpha(currentColor, 0.15); }
.rustpdf-panel-header button { padding: 2px; min-height: 22px; min-width: 22px; }
.rustpdf-thumb { padding: 6px; border-radius: 4px; }
.rustpdf-thumb image { box-shadow: 0 1px 3px alpha(black, 0.4); }
.rustpdf-thumb.current { background-color: alpha(@theme_selected_bg_color, 0.55); }
.rustpdf-comment { padding: 4px 8px; }
#page-area { background-color: mix(@theme_bg_color, black, 0.25); }
.rustpdf-welcome { font-size: 120%; color: alpha(@theme_fg_color, 0.65); }
.rustpdf-statusbar { padding: 2px 8px; border-top: 1px solid alpha(currentColor, 0.15); }
.rustpdf-statusbar label { font-size: 92%; }
";

/// Monta a janela inteira e devolve o estado inicial.
pub unsafe fn build() -> AppState {
    if let Some(directory) = icons::install() {
        if let Ok(path) = path_cstring(&directory) {
            gtk_icon_theme_append_search_path(gtk_icon_theme_get_default(), path.as_ptr());
        }
    }
    let provider = gtk_css_provider_new();
    gtk_css_provider_load_from_data(provider, cstring(CSS).as_ptr(), -1, ptr::null_mut());
    gtk_style_context_add_provider_for_screen(gdk_screen_get_default(), provider, 600);

    let window = gtk_window_new(0);
    gtk_window_set_title(window, c"RustPDF".as_ptr());
    gtk_window_set_icon_name(window, cstring(icons::APP_ICON_NAME).as_ptr());
    gtk_window_set_default_size(window, 1280, 900);
    gtk_window_set_position(window, 1);
    let accel = gtk_accel_group_new();
    gtk_window_add_accel_group(window, accel);

    let mut b = Builder {
        accel,
        ui: Widgets {
            scroll: ptr::null_mut(),
            page_total: ptr::null_mut(),
            zoom_entry: ptr::null_mut(),
            info: ptr::null_mut(),
            sidebar: ptr::null_mut(),
            sidebar_stack: ptr::null_mut(),
            thumbs_box: ptr::null_mut(),
            comments_box: ptr::null_mut(),
            statusbar: ptr::null_mut(),
            welcome: ptr::null_mut(),
            zoom: 1.0,
            zoom_mode: ZoomMode::FitWidth,
            thumbs_state: None,
            thumb_buttons: Vec::new(),
            thumb_images: Vec::new(),
            thumb_polling: false,
            comments_key: None,
            doc_widgets: Vec::new(),
            undo_widgets: Vec::new(),
            redo_widgets: Vec::new(),
            selection_widgets: Vec::new(),
            tool_widgets: Vec::new(),
            syncing: false,
            fullscreen: false,
            refit_pending: false,
            hand: None,
        },
        tool_menu_group: ptr::null_mut(),
    };
    let root = gtk_box_new(1, 0);
    let menubar = build_menus(&mut b);
    let (bar_file, bar_tools) = build_toolbars(&mut b);
    let body = gtk_box_new(0, 0);
    let sidebar = build_sidebar(&mut b);
    gtk_box_pack_start(body, sidebar, 0, 0, 0);

    // Área da página: fundo escuro, página centralizada, rolagem.
    let scroll = gtk_scrolled_window_new(ptr::null_mut(), ptr::null_mut());
    gtk_scrolled_window_set_policy(scroll, POLICY_AUTOMATIC, POLICY_AUTOMATIC);
    let image = gtk_image_new();
    let event_box = gtk_event_box_new();
    gtk_widget_set_name(event_box, c"page-area".as_ptr());
    gtk_event_box_set_above_child(event_box, 1);
    gtk_widget_add_events(
        event_box,
        GDK_POINTER_MOTION_MASK | GDK_BUTTON_PRESS_MASK | GDK_BUTTON_RELEASE_MASK,
    );
    let page_box = gtk_box_new(1, 0);
    let welcome = gtk_label_new(ptr::null());
    gtk_label_set_markup(
        welcome,
        cstring("Abra um PDF (<b>Ctrl+O</b>) ou crie um documento novo (<b>Ctrl+N</b>)").as_ptr(),
    );
    add_class(welcome, "rustpdf-welcome");
    gtk_box_pack_start(page_box, welcome, 1, 1, 0);
    gtk_box_pack_start(page_box, image, 1, 1, 0);
    gtk_container_add(event_box, page_box);
    g_signal_connect_data(
        image,
        c"draw".as_ptr(),
        on_image_draw as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        G_CONNECT_AFTER,
    );
    gtk_container_add(scroll, event_box);
    for (signal, handler) in [
        (c"button-press-event", on_button_press as *const c_void),
        (c"button-release-event", on_button_release as *const c_void),
        (c"motion-notify-event", on_motion as *const c_void),
    ] {
        g_signal_connect_data(
            event_box,
            signal.as_ptr(),
            handler,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
    }
    g_signal_connect_data(
        scroll,
        c"scroll-event".as_ptr(),
        on_scroll as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
    g_signal_connect_data(
        scroll,
        c"size-allocate".as_ptr(),
        on_area_resized as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
    gtk_box_pack_start(body, scroll, 1, 1, 0);

    // Barra de status: mensagens à esquerda, página/formato/zoom à direita.
    let statusbar = gtk_box_new(0, 12);
    add_class(statusbar, "rustpdf-statusbar");
    let status_label = gtk_label_new(c"Pronto".as_ptr());
    gtk_label_set_xalign(status_label, 0.0);
    gtk_widget_set_hexpand(status_label, 1);
    let info = gtk_label_new(ptr::null());
    gtk_box_pack_start(statusbar, status_label, 1, 1, 0);
    gtk_box_pack_start(statusbar, info, 0, 0, 0);

    gtk_box_pack_start(root, menubar, 0, 0, 0);
    gtk_box_pack_start(root, bar_file, 0, 0, 0);
    gtk_box_pack_start(root, bar_tools, 0, 0, 0);
    gtk_box_pack_start(root, body, 1, 1, 0);
    gtk_box_pack_start(root, statusbar, 0, 0, 0);
    gtk_container_add(window, root);
    g_signal_connect_data(
        window,
        c"destroy".as_ptr(),
        on_destroy as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
    g_signal_connect_data(
        window,
        c"delete-event".as_ptr(),
        on_delete_event as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
    g_signal_connect_data(
        window,
        c"key-press-event".as_ptr(),
        on_key_press as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );

    b.ui.scroll = scroll;
    b.ui.info = info;
    b.ui.sidebar = sidebar;
    b.ui.statusbar = statusbar;
    b.ui.welcome = welcome;
    AppState {
        window,
        image,
        status: status_label,
        session: None,
        tool: ToolMode::None,
        drag: None,
        pending_image: None,
        event_box,
        selection: None,
        drag_transform: IDENTITY,
        rubber_band: None,
        cursor: "",
        page_size: None,
        ui: b.ui,
    }
}

unsafe fn build_menus(b: &mut Builder) -> *mut c_void {
    use Needs::*;
    let menubar = gtk_menu_bar_new();

    let m = b.menu(menubar, "_Arquivo");
    b.item(m, "_Novo", 2, &["<Control>n"], Nothing);
    b.item(m, "_Abrir…", 1, &["<Control>o"], Nothing);
    b.separator(m);
    b.item(m, "_Salvar", 50, &["<Control>s"], Document);
    b.item(m, "Salvar _como…", 3, &["<Control><Shift>s"], Document);
    let export = b.submenu(m, "_Exportar", Document);
    b.item(
        export,
        "Página como _PDF…",
        11,
        &["<Control><Shift>e"],
        Document,
    );
    b.item(export, "Página como _imagem PNG…", 12, &[], Document);
    b.item(export, "_Texto da página", 13, &[], Document);
    b.separator(m);
    b.item(m, "_Fechar documento", 59, &["<Control>w"], Document);
    b.item(m, "Sai_r", 99, &["<Control>q"], Nothing);

    let m = b.menu(menubar, "_Editar");
    b.item(m, "_Desfazer", 20, &["<Control>z"], Undo);
    b.item(
        m,
        "_Refazer",
        21,
        &["<Control>y", "<Control><Shift>z"],
        Redo,
    );
    b.separator(m);
    b.item_showing_key(m, "_Excluir objeto selecionado", 58, KEY_DELETE, Selection);
    b.separator(m);
    b.item(m, "Textos da _página", 37, &[], Document);
    b.item(m, "_Substituir texto…", 38, &[], Document);

    let m = b.menu(menubar, "E_xibir");
    let go = b.submenu(m, "_Ir para", Document);
    b.item(go, "_Primeira página", 51, &["<Control>Home"], Document);
    b.item(go, "Página _anterior", 4, &["Page_Up"], Document);
    b.item(go, "Página _seguinte", 5, &["Page_Down"], Document);
    b.item(go, "Ú_ltima página", 52, &["<Control>End"], Document);
    b.item(go, "_Ir para a página…", 53, &["<Control>g"], Document);
    let zoom = b.submenu(m, "_Zoom", Document);
    b.item(
        zoom,
        "_Aumentar",
        6,
        &["<Control>plus", "<Control>equal", "<Control>KP_Add"],
        Document,
    );
    b.item(
        zoom,
        "_Diminuir",
        7,
        &["<Control>minus", "<Control>KP_Subtract"],
        Document,
    );
    b.item(zoom, "_Tamanho atual", 54, &["<Control>0"], Document);
    b.item(zoom, "Ajustar à _página", 55, &["<Control>1"], Document);
    b.item(zoom, "Ajustar à _largura", 56, &["<Control>2"], Document);
    b.separator(m);
    b.check(m, "Painel _lateral", 60, "F9", true);
    b.check(m, "Barra de _status", 61, "", true);
    b.item(m, "_Tela cheia", 62, &["F11"], Nothing);

    let m = b.menu(menubar, "_Documento");
    b.item(
        m,
        "Inserir página em _branco",
        16,
        &["<Control><Shift>n"],
        Document,
    );
    b.item(
        m,
        "_Inserir páginas de PDF…",
        10,
        &["<Control><Shift>i"],
        Document,
    );
    b.item(m, "_Apagar página", 9, &["<Alt>Delete"], Document);
    b.item(m, "Mover página para _trás", 17, &[], Document);
    b.item(m, "Mover página para a _frente", 18, &[], Document);
    b.separator(m);
    b.item(m, "_Recortar margens…", 22, &["<Control>k"], Document);
    b.item(
        m,
        "_Girar página no sentido horário",
        8,
        &["<Control>r"],
        Document,
    );
    b.item(
        m,
        "Girar página no sentido _anti-horário",
        57,
        &["<Control><Shift>r"],
        Document,
    );
    b.separator(m);
    b.item(m, "_OCR da página", 14, &[], Document);
    b.item(m, "OCR _pesquisável", 24, &[], Document);

    let m = b.menu(menubar, "_Comentários");
    b.item(m, "_Nota", 15, &[], Document);
    b.item(m, "_Destaque com o mouse", 32, &[], Document);
    b.item(m, "Destacar área por _coordenadas…", 31, &[], Document);
    b.separator(m);
    b.item(m, "_Lista de comentários", 67, &[], Document);

    let m = b.menu(menubar, "_Formulários");
    b.item(m, "Campo de _texto", 25, &["<Control>e"], Document);
    b.item(m, "_Caixa de seleção", 27, &[], Document);
    b.item(m, "Botão _radial", 33, &[], Document);
    b.item(m, "_Lista de opções", 35, &[], Document);
    b.separator(m);
    b.item(m, "Listar _campos", 29, &[], Document);
    b.item(m, "_Preencher campo de texto…", 26, &[], Document);
    b.item(m, "_Marcar/desmarcar caixa…", 28, &[], Document);
    b.item(m, "_Selecionar opção de rádio…", 34, &[], Document);
    b.item(m, "Preencher lista de _opções…", 36, &[], Document);

    let m = b.menu(menubar, "_Ferramentas");
    b.tool_item(m, "_Selecionar e editar", ToolMode::None, "<Alt>1");
    b.tool_item(m, "Ferramenta _mão", ToolMode::Hand, "<Alt>2");
    b.tool_item(m, "Inserir _texto", ToolMode::Text, "<Control>t");
    b.tool_item(m, "Inserir _imagem", ToolMode::Image, "<Control>i");
    b.tool_item(m, "_Nota", ToolMode::Note, "");
    b.tool_item(m, "_Destaque", ToolMode::Highlight, "");
    b.tool_item(m, "_Campo de texto", ToolMode::TextField, "");
    b.tool_item(m, "Cai_xa de seleção", ToolMode::Checkbox, "");
    b.tool_item(m, "Botão _radial", ToolMode::Radio, "");
    b.tool_item(m, "_Lista de opções", ToolMode::Choice, "");
    b.separator(m);
    let images = b.submenu(m, "I_magens", Document);
    b.item(images, "_Listar imagens da página", 39, &[], Document);
    b.item(images, "_Substituir imagem…", 40, &[], Document);
    b.item(images, "_Mover imagem…", 41, &[], Document);
    b.item(images, "_Remover imagem", 42, &[], Document);
    b.item(
        images,
        "_Posicionar imagem por coordenadas…",
        30,
        &[],
        Document,
    );

    let m = b.menu(menubar, "Aj_uda");
    b.item(m, "_Atalhos de teclado", 63, &["F1"], Nothing);
    b.item(m, "_Sobre o RustPDF", 98, &[], Nothing);
    menubar
}

unsafe fn build_toolbars(b: &mut Builder) -> (*mut c_void, *mut c_void) {
    use Needs::*;
    let bar = new_toolbar();
    b.button(
        bar,
        "rustpdf-new-symbolic",
        "Novo",
        "Criar um documento PDF em branco (Ctrl+N)",
        2,
        Nothing,
    );
    b.button(
        bar,
        "rustpdf-open-symbolic",
        "Abrir",
        "Abrir um arquivo PDF (Ctrl+O)",
        1,
        Nothing,
    );
    b.button(
        bar,
        "rustpdf-save-symbolic",
        "Salvar",
        "Salvar as alterações no arquivo atual (Ctrl+S)",
        50,
        Document,
    );
    b.button(
        bar,
        "rustpdf-save-as-symbolic",
        "Salvar como",
        "Salvar o documento com um novo nome (Ctrl+Shift+S)",
        3,
        Document,
    );
    b.bar_separator(bar);
    b.button(
        bar,
        "rustpdf-undo-symbolic",
        "Desfazer",
        "Desfazer a última ação (Ctrl+Z)",
        20,
        Undo,
    );
    b.button(
        bar,
        "rustpdf-redo-symbolic",
        "Refazer",
        "Refazer a última ação desfeita (Ctrl+Y)",
        21,
        Redo,
    );
    b.button(
        bar,
        "rustpdf-delete-symbolic",
        "Excluir objeto",
        "Excluir o objeto selecionado na página (Del)",
        58,
        Selection,
    );
    b.bar_separator(bar);
    b.button(
        bar,
        "rustpdf-first-symbolic",
        "Primeira página",
        "Ir para a primeira página (Ctrl+Home)",
        51,
        Document,
    );
    b.button(
        bar,
        "rustpdf-previous-symbolic",
        "Página anterior",
        "Ir para a página anterior (Page Up)",
        4,
        Document,
    );
    let page_box = gtk_box_new(0, 4);
    let page_entry = gtk_entry_new();
    gtk_entry_set_width_chars(page_entry, 4);
    gtk_widget_set_valign(page_entry, ALIGN_CENTER);
    set_tip(
        page_entry,
        "Página atual",
        "Digite o número e pressione Enter (Ctrl+G)",
    );
    g_signal_connect_data(
        page_entry,
        c"activate".as_ptr(),
        on_goto_page as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
    let page_total = gtk_label_new(c"/ 0".as_ptr());
    gtk_box_pack_start(page_box, page_entry, 0, 0, 0);
    gtk_box_pack_start(page_box, page_total, 0, 0, 0);
    b.bar_widget(bar, page_box, Document);
    b.button(
        bar,
        "rustpdf-next-symbolic",
        "Página seguinte",
        "Ir para a próxima página (Page Down)",
        5,
        Document,
    );
    b.button(
        bar,
        "rustpdf-last-symbolic",
        "Última página",
        "Ir para a última página (Ctrl+End)",
        52,
        Document,
    );
    b.bar_separator(bar);
    b.button(
        bar,
        "rustpdf-zoom-out-symbolic",
        "Diminuir zoom",
        "Reduzir a página (Ctrl+−)",
        7,
        Document,
    );
    b.button(
        bar,
        "rustpdf-zoom-actual-symbolic",
        "Tamanho atual",
        "Exibir em 100% (Ctrl+0)",
        54,
        Document,
    );
    let zoom = gtk_combo_box_text_new_with_entry();
    for label in [
        "50%",
        "75%",
        "100%",
        "125%",
        "150%",
        "200%",
        "300%",
        "400%",
        "Ajustar à página",
        "Ajustar à largura",
    ] {
        gtk_combo_box_text_append_text(zoom, cstring(label).as_ptr());
    }
    let zoom_entry = gtk_bin_get_child(zoom);
    gtk_entry_set_width_chars(zoom_entry, 6);
    gtk_widget_set_valign(zoom, ALIGN_CENTER);
    set_tip(
        zoom,
        "Zoom",
        "Escolha um valor ou digite a porcentagem e pressione Enter",
    );
    g_signal_connect_data(
        zoom,
        c"changed".as_ptr(),
        on_zoom_changed as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
    g_signal_connect_data(
        zoom_entry,
        c"activate".as_ptr(),
        on_zoom_typed as *const c_void,
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
    b.bar_widget(bar, zoom, Document);
    b.button(
        bar,
        "rustpdf-zoom-in-symbolic",
        "Aumentar zoom",
        "Ampliar a página (Ctrl++)",
        6,
        Document,
    );
    b.button(
        bar,
        "rustpdf-fit-page-symbolic",
        "Ajustar à página",
        "Mostrar a página inteira (Ctrl+1)",
        55,
        Document,
    );
    b.button(
        bar,
        "rustpdf-fit-width-symbolic",
        "Ajustar à largura",
        "Ocupar a largura da área (Ctrl+2)",
        56,
        Document,
    );
    b.bar_separator(bar);
    b.button(
        bar,
        "rustpdf-rotate-left-symbolic",
        "Girar no sentido anti-horário",
        "Girar a página atual 90° (Ctrl+Shift+R)",
        57,
        Document,
    );
    b.button(
        bar,
        "rustpdf-rotate-right-symbolic",
        "Girar no sentido horário",
        "Girar a página atual 90° (Ctrl+R)",
        8,
        Document,
    );

    let tools = new_toolbar();
    let mut group = ptr::null_mut();
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-select-symbolic",
        "Selecionar e editar",
        "Selecionar, mover, redimensionar e girar objetos; clique duplo edita texto (Alt+1)",
        ToolMode::None,
    );
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-hand-symbolic",
        "Ferramenta mão",
        "Arrastar para rolar a página (Alt+2)",
        ToolMode::Hand,
    );
    b.bar_separator(tools);
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-insert-text-symbolic",
        "Inserir texto",
        "Clique na página para inserir texto (Ctrl+T)",
        ToolMode::Text,
    );
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-insert-image-symbolic",
        "Inserir imagem",
        "Escolha uma imagem e clique na página (Ctrl+I)",
        ToolMode::Image,
    );
    b.bar_separator(tools);
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-note-symbolic",
        "Nota",
        "Clique na página para adicionar uma nota",
        ToolMode::Note,
    );
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-highlight-symbolic",
        "Destaque",
        "Arraste sobre a página para destacar uma área",
        ToolMode::Highlight,
    );
    b.bar_separator(tools);
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-text-field-symbolic",
        "Campo de texto",
        "Clique na página para criar um campo de texto (Ctrl+E)",
        ToolMode::TextField,
    );
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-checkbox-symbolic",
        "Caixa de seleção",
        "Clique na página para criar uma caixa de seleção",
        ToolMode::Checkbox,
    );
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-radio-symbolic",
        "Botão radial",
        "Clique na página para criar um grupo de botões radiais",
        ToolMode::Radio,
    );
    b.tool_button(
        tools,
        &mut group,
        "rustpdf-choice-symbolic",
        "Lista de opções",
        "Clique na página para criar uma lista de opções",
        ToolMode::Choice,
    );
    b.bar_separator(tools);
    b.button(
        tools,
        "rustpdf-ocr-symbolic",
        "OCR",
        "Reconhecer o texto da página atual",
        14,
        Document,
    );
    b.button(
        tools,
        "rustpdf-ocr-search-symbolic",
        "OCR pesquisável",
        "Adicionar camada de texto pesquisável à página",
        24,
        Document,
    );
    b.button(
        tools,
        "rustpdf-crop-symbolic",
        "Recortar margens",
        "Recortar as margens da página atual (Ctrl+K)",
        22,
        Document,
    );

    b.ui.page_total = page_total;
    b.ui.zoom_entry = zoom_entry;
    PAGE_ENTRY.with(|cell| cell.set(page_entry));
    (bar, tools)
}

thread_local! {
    /// Campo de página (lido pelo `on_goto_page` de `main.rs`).
    pub static PAGE_ENTRY: std::cell::Cell<*mut c_void> = const { std::cell::Cell::new(ptr::null_mut()) };
}

unsafe fn panel_header(
    b: &mut Builder,
    title: &str,
    buttons: &[(&str, &str, &str, usize)],
) -> *mut c_void {
    let header = gtk_box_new(0, 2);
    add_class(header, "rustpdf-panel-header");
    let label = gtk_label_new(cstring(title).as_ptr());
    gtk_label_set_xalign(label, 0.0);
    add_class(label, "rustpdf-panel-title");
    gtk_box_pack_start(header, label, 1, 1, 0);
    for (icon_name, tip_title, tip, action) in buttons {
        let button = gtk_button_new();
        gtk_button_set_image(button, icon(icon_name));
        gtk_button_set_relief(button, RELIEF_NONE);
        gtk_widget_set_valign(button, ALIGN_CENTER);
        set_tip(button, tip_title, tip);
        g_signal_connect_data(
            button,
            c"clicked".as_ptr(),
            on_clicked as *const c_void,
            *action as *mut c_void,
            ptr::null_mut(),
            0,
        );
        gtk_box_pack_start(header, button, 0, 0, 0);
        b.track(button, Needs::Document);
    }
    header
}

unsafe fn panel(b: &mut Builder, header: *mut c_void) -> (*mut c_void, *mut c_void) {
    let panel = gtk_box_new(1, 0);
    gtk_widget_set_size_request(panel, 190, -1);
    gtk_box_pack_start(panel, header, 0, 0, 0);
    let scroll = gtk_scrolled_window_new(ptr::null_mut(), ptr::null_mut());
    gtk_scrolled_window_set_policy(scroll, POLICY_NEVER, POLICY_AUTOMATIC);
    let list = gtk_box_new(1, 4);
    gtk_widget_set_margin_top(list, 6);
    gtk_widget_set_margin_bottom(list, 6);
    gtk_container_add(scroll, list);
    gtk_box_pack_start(panel, scroll, 1, 1, 0);
    let _ = b;
    (panel, list)
}

unsafe fn build_sidebar(b: &mut Builder) -> *mut c_void {
    let sidebar = gtk_box_new(0, 0);
    let strip = gtk_box_new(1, 0);
    add_class(strip, "rustpdf-strip");
    let stack = gtk_stack_new();

    let pages_header = panel_header(
        b,
        "Páginas",
        &[
            (
                "rustpdf-page-add-symbolic",
                "Inserir página em branco",
                "Depois da página atual (Ctrl+Shift+N)",
                16,
            ),
            (
                "rustpdf-page-import-symbolic",
                "Inserir páginas de PDF",
                "Acrescentar as páginas de outro arquivo (Ctrl+Shift+I)",
                10,
            ),
            (
                "rustpdf-page-remove-symbolic",
                "Apagar página",
                "Excluir a página atual (Alt+Del)",
                9,
            ),
            (
                "rustpdf-page-extract-symbolic",
                "Extrair página",
                "Salvar a página atual como PDF (Ctrl+Shift+E)",
                11,
            ),
            (
                "rustpdf-move-back-symbolic",
                "Mover para trás",
                "Trocar a página atual com a anterior",
                17,
            ),
            (
                "rustpdf-move-forward-symbolic",
                "Mover para a frente",
                "Trocar a página atual com a seguinte",
                18,
            ),
        ],
    );
    let (pages_panel, thumbs) = panel(b, pages_header);
    gtk_stack_add_named(stack, pages_panel, c"pages".as_ptr());
    let comments_header = panel_header(b, "Comentários", &[]);
    let (comments_panel, comments) = panel(b, comments_header);
    gtk_stack_add_named(stack, comments_panel, c"comments".as_ptr());

    let mut first = ptr::null_mut();
    for (icon_name, title, description, name) in [
        (
            "rustpdf-pages-symbolic",
            "Páginas",
            "Miniaturas das páginas e operações de página",
            "pages",
        ),
        (
            "rustpdf-note-symbolic",
            "Comentários",
            "Notas, destaques e campos da página atual",
            "comments",
        ),
    ] {
        let button = gtk_radio_button_new_from_widget(first);
        if first.is_null() {
            first = button;
        }
        gtk_toggle_button_set_mode(button, 0);
        gtk_button_set_image(button, icon(icon_name));
        gtk_button_set_relief(button, RELIEF_NONE);
        set_tip(button, title, description);
        let name = Box::leak(cstring(name).into_boxed_c_str());
        g_signal_connect_data(
            button,
            c"toggled".as_ptr(),
            on_panel_toggled as *const c_void,
            name.as_ptr() as *mut c_void,
            ptr::null_mut(),
            0,
        );
        gtk_box_pack_start(strip, button, 0, 0, 0);
    }
    gtk_box_pack_start(sidebar, strip, 0, 0, 0);
    gtk_box_pack_start(sidebar, stack, 0, 0, 0);
    b.ui.sidebar_stack = stack;
    b.ui.thumbs_box = thumbs;
    b.ui.comments_box = comments;
    sidebar
}

unsafe extern "C" fn on_panel_toggled(button: *mut c_void, name: *mut c_void) {
    if gtk_toggle_button_get_active(button) == 0 {
        return;
    }
    if let Some(stack) = with_widgets(|ui| ui.sidebar_stack) {
        gtk_stack_set_visible_child_name(stack, name as *const c_char);
    }
}

/// Mostra o painel lateral `name` ("pages" ou "comments").
pub fn show_panel(name: &str) {
    if let Some((stack, sidebar)) = with_widgets(|ui| (ui.sidebar_stack, ui.sidebar)) {
        unsafe {
            gtk_widget_set_visible(sidebar, 1);
            gtk_stack_set_visible_child_name(stack, cstring(name).as_ptr());
        }
    }
}

// ---------------------------------------------------------------- ferramentas

unsafe extern "C" fn on_tool_toggled(widget: *mut c_void, data: *mut c_void) {
    let Some((syncing, is_button)) = with_widgets(|ui| {
        let is_button = ui
            .tool_widgets
            .iter()
            .find(|(_, item, _)| *item == widget)
            .is_some_and(|(_, _, button)| *button);
        (ui.syncing, is_button)
    }) else {
        return;
    };
    let active = if is_button {
        gtk_toggle_tool_button_get_active(widget)
    } else {
        gtk_check_menu_item_get_active(widget)
    };
    if syncing || active == 0 {
        return;
    }
    activate_tool(TOOLS[data as usize]);
}

/// Ativa a ferramenta (pedindo a imagem, no caso de Inserir imagem).
pub fn activate_tool(mode: ToolMode) {
    if mode != ToolMode::None && with_session(|_| Ok(())).is_err() {
        status("Abra um PDF primeiro");
        sync_tools();
        return;
    }
    if mode == ToolMode::Image {
        let Some(path) = chooser("Imagem a inserir (PNG ou JPEG)", false, FileKind::Image) else {
            sync_tools();
            return;
        };
        STATE.with(|cell| {
            if let Some(app) = cell.borrow_mut().as_mut() {
                app.pending_image = Some(path);
            }
        });
    }
    set_tool(mode);
    status(match mode {
        ToolMode::None => "Selecionar e editar: clique em um objeto; arraste para mover",
        ToolMode::Hand => "Ferramenta mão: arraste para rolar a página",
        ToolMode::Text => "Clique na página onde o texto deve começar",
        ToolMode::Image => "Clique na página para posicionar a imagem",
        ToolMode::Note => "Clique na página para adicionar a nota",
        ToolMode::Highlight => "Arraste sobre a página para destacar a área",
        ToolMode::TextField => "Clique na página para criar o campo de texto",
        ToolMode::Checkbox => "Clique na página para criar a caixa de seleção",
        ToolMode::Radio => "Clique na página para criar o grupo de botões radiais",
        ToolMode::Choice => "Clique na página para criar a lista de opções",
    });
}

/// Reflete a ferramenta atual na barra e no menu Ferramentas.
pub fn sync_tools() {
    let Some((tool, widgets)) = STATE.with(|cell| {
        let mut borrowed = cell.try_borrow_mut().ok()?;
        let app = borrowed.as_mut()?;
        app.ui.syncing = true;
        Some((app.tool, app.ui.tool_widgets.clone()))
    }) else {
        return;
    };
    for (mode, widget, is_button) in widgets {
        let active = c_int::from(mode == tool);
        unsafe {
            if is_button {
                gtk_toggle_tool_button_set_active(widget, active);
            } else {
                gtk_check_menu_item_set_active(widget, active);
            }
        }
    }
    with_widgets(|ui| ui.syncing = false);
}

// ---------------------------------------------------------------- zoom

fn page_points() -> Option<(f64, f64)> {
    STATE.with(|cell| cell.borrow().as_ref().and_then(|app| app.page_size))
}

/// Zoom que faz a página caber na área visível (largura ou página inteira).
fn fit_zoom(mode: ZoomMode) -> Option<f64> {
    let (width, height) = page_points()?;
    let scroll = with_widgets(|ui| ui.scroll)?;
    let (area_width, area_height) = unsafe {
        (
            gtk_widget_get_allocated_width(scroll) as f64 - 28.0,
            gtk_widget_get_allocated_height(scroll) as f64 - 28.0,
        )
    };
    if area_width < 50.0 || area_height < 50.0 {
        return None;
    }
    let by_width = area_width / (width * PIXELS_PER_POINT);
    let by_height = area_height / (height * PIXELS_PER_POINT);
    Some(match mode {
        ZoomMode::FitWidth => by_width,
        ZoomMode::FitPage => by_width.min(by_height),
        ZoomMode::Custom => return None,
    })
}

pub fn set_zoom(mode: ZoomMode, zoom: Option<f64>) -> AppResult<()> {
    with_widgets(|ui| {
        ui.zoom_mode = mode;
        if let Some(zoom) = zoom {
            ui.zoom = zoom.clamp(0.1, 8.0);
        }
    });
    render()
}

/// Multiplica o zoom atual (passa a ser zoom personalizado).
pub fn scale_zoom(factor: f64) -> AppResult<()> {
    let current = with_widgets(|ui| ui.zoom).unwrap_or(1.0);
    set_zoom(ZoomMode::Custom, Some(current * factor))
}

unsafe extern "C" fn on_zoom_changed(combo: *mut c_void, _data: *mut c_void) {
    if with_widgets(|ui| ui.syncing).unwrap_or(true) {
        return;
    }
    let result = match gtk_combo_box_get_active(combo) {
        index @ 0..=7 => {
            let values = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];
            set_zoom(ZoomMode::Custom, Some(values[index as usize]))
        }
        8 => set_zoom(ZoomMode::FitPage, None),
        9 => set_zoom(ZoomMode::FitWidth, None),
        _ => return,
    };
    if let Err(error) = result {
        status(&error);
    }
}

unsafe extern "C" fn on_zoom_typed(entry: *mut c_void, _data: *mut c_void) {
    let text = CStr::from_ptr(gtk_entry_get_text(entry))
        .to_string_lossy()
        .into_owned();
    match text
        .trim()
        .trim_end_matches('%')
        .trim()
        .replace(',', ".")
        .parse::<f64>()
    {
        Ok(percent) if percent > 0.0 => {
            if let Err(error) = set_zoom(ZoomMode::Custom, Some(percent / 100.0)) {
                status(&error);
            }
        }
        _ => status("Zoom inválido: digite uma porcentagem, ex.: 150%"),
    }
}

/// Ctrl + roda do mouse: zoom.
unsafe extern "C" fn on_scroll(
    _widget: *mut c_void,
    event: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    let mut state = 0u32;
    if gdk_event_get_state(event, &mut state) == 0 || state & GDK_CONTROL_MASK == 0 {
        return 0;
    }
    let mut direction = 0;
    let mut factor = 0.0;
    if gdk_event_get_scroll_direction(event, &mut direction) != 0 {
        factor = match direction {
            0 => 1.1,
            1 => 1.0 / 1.1,
            _ => 0.0,
        };
    } else {
        let (mut dx, mut dy) = (0.0, 0.0);
        if gdk_event_get_scroll_deltas(event, &mut dx, &mut dy) != 0 && dy != 0.0 {
            factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
        }
    }
    if factor > 0.0 && with_session(|_| Ok(())).is_ok() {
        if let Err(error) = scale_zoom(factor) {
            status(&error);
        }
    }
    1
}

/// Área redimensionada: reajusta o zoom nos modos de ajuste.
unsafe extern "C" fn on_area_resized(
    _widget: *mut c_void,
    _allocation: *mut c_void,
    _data: *mut c_void,
) {
    let Some((mode, zoom, pending)) = with_widgets(|ui| (ui.zoom_mode, ui.zoom, ui.refit_pending))
    else {
        return;
    };
    if mode == ZoomMode::Custom || pending {
        return;
    }
    if let Some(fit) = fit_zoom(mode) {
        if (fit - zoom).abs() / zoom > 0.02 {
            with_widgets(|ui| ui.refit_pending = true);
            g_idle_add(refit_idle, ptr::null_mut());
        }
    }
}

unsafe extern "C" fn refit_idle(_data: *mut c_void) -> c_int {
    with_widgets(|ui| ui.refit_pending = false);
    if let Err(error) = render() {
        status(&error);
    }
    0
}

// ---------------------------------------------------------------- mão

pub fn hand_press(event: *mut c_void) {
    let (mut x, mut y) = (0.0, 0.0);
    unsafe {
        if gdk_event_get_root_coords(event, &mut x, &mut y) == 0 {
            return;
        }
    }
    with_widgets(|ui| unsafe {
        let h = gtk_adjustment_get_value(gtk_scrolled_window_get_hadjustment(ui.scroll));
        let v = gtk_adjustment_get_value(gtk_scrolled_window_get_vadjustment(ui.scroll));
        ui.hand = Some((x, y, h, v));
    });
    set_cursor("grabbing");
}

/// Devolve `true` se havia arraste da mão em andamento.
pub fn hand_motion(event: *mut c_void) -> bool {
    let Some(Some((x0, y0, h0, v0))) = with_widgets(|ui| ui.hand) else {
        return false;
    };
    let (mut x, mut y) = (0.0, 0.0);
    unsafe {
        if gdk_event_get_root_coords(event, &mut x, &mut y) == 0 {
            return true;
        }
    }
    with_widgets(|ui| unsafe {
        gtk_adjustment_set_value(
            gtk_scrolled_window_get_hadjustment(ui.scroll),
            h0 - (x - x0),
        );
        gtk_adjustment_set_value(
            gtk_scrolled_window_get_vadjustment(ui.scroll),
            v0 - (y - y0),
        );
    });
    true
}

pub fn hand_release() -> bool {
    let was = with_widgets(|ui| ui.hand.take().is_some()).unwrap_or(false);
    if was {
        set_cursor("grab");
    }
    was
}

// ---------------------------------------------------------------- teclado

unsafe extern "C" fn on_key_press(
    window: *mut c_void,
    event: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    let mut key = 0u32;
    if gdk_event_get_keyval(event, &mut key) == 0 {
        return 0;
    }
    // Teclas de edição ficam com campos de texto em foco.
    let focus = gtk_window_get_focus(window);
    if !focus.is_null() && g_type_check_instance_is_a(focus, gtk_editable_get_type()) != 0 {
        return 0;
    }
    match key {
        KEY_DELETE | KEY_KP_DELETE if current_selection().is_some() => {
            if let Err(error) = dispatch(58) {
                status(&error);
            }
            1
        }
        KEY_ESCAPE => {
            clear_drag();
            set_selection(None);
            if current_tool() != ToolMode::None {
                activate_tool(ToolMode::None);
            }
            1
        }
        _ => 0,
    }
}

/// Coloca o foco no campo de página (Ctrl+G).
pub fn focus_page_entry() {
    PAGE_ENTRY.with(|cell| unsafe { gtk_widget_grab_focus(cell.get()) });
}

// ---------------------------------------------------------------- janela

pub fn toggle_sidebar() {
    if let Some(sidebar) = with_widgets(|ui| ui.sidebar) {
        unsafe {
            gtk_widget_set_visible(sidebar, c_int::from(gtk_widget_get_visible(sidebar) == 0))
        };
    }
}

pub fn toggle_statusbar() {
    if let Some(bar) = with_widgets(|ui| ui.statusbar) {
        unsafe { gtk_widget_set_visible(bar, c_int::from(gtk_widget_get_visible(bar) == 0)) };
    }
}

pub fn toggle_fullscreen() {
    let window = STATE.with(|cell| cell.borrow().as_ref().map(|app| app.window));
    let Some(window) = window else {
        return;
    };
    let fullscreen = with_widgets(|ui| {
        ui.fullscreen = !ui.fullscreen;
        ui.fullscreen
    })
    .unwrap_or(false);
    unsafe {
        if fullscreen {
            gtk_window_fullscreen(window);
        } else {
            gtk_window_unfullscreen(window);
        }
    }
}

/// Habilita/desabilita ações conforme documento, histórico e seleção.
pub fn update_sensitivity() {
    let Some((has_doc, can_undo, can_redo, has_selection, ui)) = STATE.with(|cell| {
        let borrowed = cell.try_borrow().ok()?;
        let app = borrowed.as_ref()?;
        let session = app.session.as_ref();
        Some((
            session.is_some(),
            session.is_some_and(PdfSession::can_undo),
            session.is_some_and(PdfSession::can_redo),
            app.selection.is_some(),
            (
                app.ui.doc_widgets.clone(),
                app.ui.undo_widgets.clone(),
                app.ui.redo_widgets.clone(),
                app.ui.selection_widgets.clone(),
            ),
        ))
    }) else {
        return;
    };
    let (doc, undo, redo, selection) = ui;
    unsafe {
        for widget in doc {
            gtk_widget_set_sensitive(widget, c_int::from(has_doc));
        }
        for widget in undo {
            gtk_widget_set_sensitive(widget, c_int::from(can_undo));
        }
        for widget in redo {
            gtk_widget_set_sensitive(widget, c_int::from(can_redo));
        }
        for widget in selection {
            gtk_widget_set_sensitive(widget, c_int::from(has_doc && has_selection));
        }
    }
}

// ---------------------------------------------------------------- renderização

/// Redesenha prévia, título, navegação, painel lateral e barra de status.
pub fn render_all() -> AppResult<()> {
    let _trace = rustpdf::trace(|| " render".into());
    let snapshot = STATE.with(|cell| -> AppResult<Option<_>> {
        let mut borrowed = cell.borrow_mut();
        let app = borrowed.as_mut().ok_or("Interface ausente")?;
        let Some(session) = app.session.as_ref() else {
            return Ok(None);
        };
        app.page_size = session.page_dimensions().ok();
        Ok(Some((
            session.page,
            session.pages,
            session.revision(),
            session.dirty,
            session
                .source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            session.paper_label().unwrap_or_default(),
        )))
    })?;
    let Some((page, pages, revision, dirty, filename, paper)) = snapshot else {
        render_empty();
        update_sensitivity();
        return Ok(());
    };
    let mode = with_widgets(|ui| ui.zoom_mode).unwrap_or(ZoomMode::Custom);
    if let Some(fit) = fit_zoom(mode) {
        with_widgets(|ui| ui.zoom = fit);
    }
    let zoom = with_widgets(|ui| ui.zoom).unwrap_or(1.0);
    let width = page_points().map_or(800.0, |(w, _)| w * PIXELS_PER_POINT * zoom);
    let width = width.round().clamp(80.0, 8000.0) as u32;
    let preview = with_session(|session| session.render(width))?;
    STATE.with(|cell| -> AppResult<()> {
        let borrowed = cell.borrow();
        let app = borrowed.as_ref().ok_or("Interface ausente")?;
        let preview = path_cstring(&preview)?;
        unsafe {
            gtk_widget_set_visible(app.ui.welcome, 0);
            gtk_image_set_from_file(app.image, preview.as_ptr());
            let title = format!("{filename}{} — RustPDF", if dirty { "*" } else { "" });
            gtk_window_set_title(app.window, cstring(&title).as_ptr());
            gtk_label_set_text(app.ui.page_total, cstring(&format!("/ {pages}")).as_ptr());
            PAGE_ENTRY
                .with(|entry| gtk_entry_set_text(entry.get(), cstring(&page.to_string()).as_ptr()));
            let info = format!(
                "Página {page} de {pages}  ·  {paper}  ·  {:.0}%{}",
                zoom * 100.0,
                if dirty { "  ·  não salvo" } else { "" }
            );
            gtk_label_set_text(app.ui.info, cstring(&info).as_ptr());
        }
        Ok(())
    })?;
    // Texto do zoom sem disparar o "changed" do combo.
    if let Some(entry) = with_widgets(|ui| {
        ui.syncing = true;
        ui.zoom_entry
    }) {
        let label = match mode {
            ZoomMode::FitPage => "Página".to_string(),
            ZoomMode::FitWidth => "Largura".to_string(),
            ZoomMode::Custom => format!("{:.0}%", zoom * 100.0),
        };
        unsafe { gtk_entry_set_text(entry, cstring(&label).as_ptr()) };
        with_widgets(|ui| ui.syncing = false);
    }
    refresh_thumbnails(page, pages);
    refresh_comments(page, revision);
    update_sensitivity();
    Ok(())
}

fn render_empty() {
    STATE.with(|cell| {
        let borrowed = cell.borrow();
        let Some(app) = borrowed.as_ref() else {
            return;
        };
        unsafe {
            gtk_image_clear(app.image);
            gtk_widget_set_visible(app.ui.welcome, 1);
            gtk_window_set_title(app.window, c"RustPDF".as_ptr());
            gtk_label_set_text(app.ui.page_total, c"/ 0".as_ptr());
            PAGE_ENTRY.with(|entry| gtk_entry_set_text(entry.get(), c"".as_ptr()));
            gtk_label_set_text(app.ui.info, c"".as_ptr());
            clear_children(app.ui.thumbs_box);
            clear_children(app.ui.comments_box);
        }
    });
    THUMB_JOB.store(0, std::sync::atomic::Ordering::SeqCst);
    with_widgets(|ui| {
        ui.thumbs_state = None;
        ui.comments_key = None;
        ui.thumb_buttons.clear();
        ui.thumb_images.clear();
    });
}

/// Trabalho de miniaturas ativo; trabalhos antigos param ao ver outro número.
static THUMB_JOB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Miniaturas prontas (`página`, arquivo) de um trabalho, e a pasta dele.
type ThumbReceiver = (std::sync::mpsc::Receiver<(usize, PathBuf)>, PathBuf);

thread_local! {
    /// Receptor do trabalho de miniaturas ativo.
    static THUMB_RECEIVER: RefCell<Option<ThumbReceiver>> = const { RefCell::new(None) };
}

/// Miniaturas sem travar a janela: os botões existem desde já (com a
/// miniatura anterior, quando houver) e as imagens das páginas alteradas são
/// geradas numa thread, a página atual primeiro, e trocadas conforme ficam
/// prontas. Só as páginas que mudaram são refeitas.
fn refresh_thumbnails(page: usize, pages: usize) {
    let Some((state, container, buttons)) = with_widgets(|ui| {
        (
            ui.thumbs_state.clone(),
            ui.thumbs_box,
            ui.thumb_buttons.len(),
        )
    }) else {
        return;
    };
    let Ok((document, counter, scope)) = with_session(|session| {
        let document = session.work_path().to_path_buf();
        let since = match &state {
            Some((previous, counter)) if *previous == document => *counter,
            _ => 0,
        };
        let (counter, scope) = session.changes_since(since);
        Ok((document, counter, scope))
    }) else {
        return;
    };
    let same_document = state
        .as_ref()
        .is_some_and(|(previous, _)| *previous == document);
    let all: Vec<usize> = (1..=pages).collect();
    let stale: Vec<usize> = if !same_document || buttons != pages {
        build_thumbnail_buttons(container, pages);
        all
    } else if state
        .as_ref()
        .is_some_and(|(_, previous)| *previous == counter)
    {
        Vec::new()
    } else {
        match scope {
            Some(changed) => changed
                .into_iter()
                .filter(|number| *number <= pages)
                .collect(),
            None => all,
        }
    };
    with_widgets(|ui| ui.thumbs_state = Some((document, counter)));
    if let Some(buttons) = with_widgets(|ui| ui.thumb_buttons.clone()) {
        for (index, button) in buttons.into_iter().enumerate() {
            unsafe {
                if index + 1 == page {
                    add_class(button, "current");
                } else {
                    remove_class(button, "current");
                }
            }
        }
    }
    if !stale.is_empty() {
        if let Err(error) = start_thumbnail_job(page, stale) {
            status(&error);
        }
    }
}

/// Botões das páginas com imagem vazia do tamanho de uma página retrato.
fn build_thumbnail_buttons(container: *mut c_void, pages: usize) {
    let mut buttons = Vec::with_capacity(pages);
    let mut images = Vec::with_capacity(pages);
    unsafe {
        clear_children(container);
        for number in 1..=pages {
            let button = gtk_button_new();
            gtk_button_set_relief(button, RELIEF_NONE);
            add_class(button, "rustpdf-thumb");
            gtk_widget_set_halign(button, ALIGN_CENTER);
            let column = gtk_box_new(1, 4);
            let image = gtk_image_new();
            gtk_widget_set_size_request(
                image,
                THUMB_WIDTH as c_int,
                (THUMB_WIDTH as f64 * 1.414) as c_int,
            );
            gtk_box_pack_start(column, image, 0, 0, 0);
            gtk_box_pack_start(
                column,
                gtk_label_new(cstring(&number.to_string()).as_ptr()),
                0,
                0,
                0,
            );
            gtk_container_add(button, column);
            set_tip(
                button,
                &format!("Página {number}"),
                "Clique para ir para a página",
            );
            g_signal_connect_data(
                button,
                c"clicked".as_ptr(),
                on_page_clicked as *const c_void,
                number as *mut c_void,
                ptr::null_mut(),
                0,
            );
            gtk_box_pack_start(container, button, 0, 0, 0);
            buttons.push(button);
            images.push(image);
        }
        gtk_widget_show_all(container);
    }
    with_widgets(|ui| {
        ui.thumb_buttons = buttons;
        ui.thumb_images = images;
    });
}

/// Gera as miniaturas de `pages` numa thread (atual primeiro, depois em lotes).
fn start_thumbnail_job(current: usize, mut pages: Vec<usize>) -> AppResult<()> {
    use std::sync::atomic::Ordering;
    let (source, directory) = with_session(|session| session.thumbnail_source())?;
    let job = THUMB_JOB.fetch_add(1, Ordering::SeqCst) + 1;
    pages.sort_unstable();
    // Lotes de páginas consecutivas: a atual sozinha, depois o resto.
    let mut batches: Vec<(usize, usize)> = Vec::new();
    if pages.contains(&current) {
        batches.push((current, current));
    }
    for number in pages.into_iter().filter(|number| *number != current) {
        match batches.last_mut() {
            Some((first, last))
                if *last + 1 == number && *last - *first < 7 && *first != current =>
            {
                *last = number
            }
            _ => batches.push((number, number)),
        }
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker_directory = directory.clone();
    std::thread::spawn(move || {
        let mut cancelled = false;
        for (first, last) in batches {
            if THUMB_JOB.load(Ordering::SeqCst) != job {
                cancelled = true;
                break;
            }
            match rustpdf::render_thumbnails(&source, &worker_directory, first, last, THUMB_WIDTH) {
                Ok(produced) => {
                    for item in produced {
                        if sender.send(item).is_err() {
                            cancelled = true;
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
            if cancelled {
                break;
            }
        }
        let _ = fs::remove_file(&source);
        if cancelled {
            let _ = fs::remove_dir_all(&worker_directory);
        }
    });
    THUMB_RECEIVER.with(|cell| *cell.borrow_mut() = Some((receiver, directory)));
    let start_polling =
        with_widgets(|ui| !std::mem::replace(&mut ui.thumb_polling, true)).unwrap_or(false);
    if start_polling {
        unsafe { g_timeout_add(60, poll_thumbnails, ptr::null_mut()) };
    }
    Ok(())
}

/// Troca as imagens das miniaturas que ficaram prontas (thread principal).
unsafe extern "C" fn poll_thumbnails(_data: *mut c_void) -> c_int {
    use std::sync::mpsc::TryRecvError;
    let images = with_widgets(|ui| ui.thumb_images.clone()).unwrap_or_default();
    let finished = THUMB_RECEIVER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some((receiver, directory)) = slot.as_ref() else {
            return true;
        };
        loop {
            match receiver.try_recv() {
                Ok((page, path)) => {
                    if let (Some(image), Ok(file)) = (images.get(page - 1), path_cstring(&path)) {
                        gtk_image_set_from_file(*image, file.as_ptr());
                    }
                    let _ = fs::remove_file(&path);
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => {
                    let _ = fs::remove_dir_all(directory);
                    *slot = None;
                    return true;
                }
            }
        }
    });
    if finished {
        with_widgets(|ui| ui.thumb_polling = false);
        0
    } else {
        1
    }
}

/// Lista de comentários (anotações e campos) da página atual.
fn refresh_comments(page: usize, revision: u64) {
    let _trace = rustpdf::trace(|| " comentários".into());
    let Some((key, container)) = with_widgets(|ui| (ui.comments_key, ui.comments_box)) else {
        return;
    };
    if key == Some((revision, page)) {
        return;
    }
    let annotations = with_session(|session| session.list_annotations()).unwrap_or_default();
    unsafe {
        clear_children(container);
        if annotations.is_empty() {
            let empty = gtk_label_new(c"Nenhum comentário nesta página".as_ptr());
            add_class(empty, "rustpdf-comment");
            gtk_box_pack_start(container, empty, 0, 0, 0);
        }
        for annotation in &annotations {
            let button = gtk_button_new();
            gtk_button_set_relief(button, RELIEF_NONE);
            add_class(button, "rustpdf-comment");
            let row = gtk_box_new(0, 6);
            let icon_name = match annotation.kind.as_str() {
                "nota" => "rustpdf-note-symbolic",
                "destaque" => "rustpdf-highlight-symbolic",
                "campo" => "rustpdf-text-field-symbolic",
                _ => "rustpdf-note-symbolic",
            };
            gtk_box_pack_start(row, icon(icon_name), 0, 0, 0);
            let text: String = annotation.text.chars().take(28).collect();
            let label_text = if text.is_empty() {
                annotation.kind.clone()
            } else {
                format!("{}: {text}", annotation.kind)
            };
            let label = gtk_label_new(cstring(&label_text).as_ptr());
            gtk_label_set_xalign(label, 0.0);
            gtk_box_pack_start(row, label, 1, 1, 0);
            gtk_container_add(button, row);
            gtk_widget_set_halign(button, ALIGN_START);
            set_tip(button, &label_text, "Clique para selecionar na página");
            g_signal_connect_data(
                button,
                c"clicked".as_ptr(),
                on_comment_clicked as *const c_void,
                annotation.index as *mut c_void,
                ptr::null_mut(),
                0,
            );
            gtk_box_pack_start(container, button, 0, 0, 0);
        }
        gtk_widget_show_all(container);
    }
    with_widgets(|ui| ui.comments_key = Some((revision, page)));
}

unsafe extern "C" fn on_comment_clicked(_button: *mut c_void, data: *mut c_void) {
    let index = data as usize;
    let result = (|| -> AppResult<()> {
        let annotations = with_session(|session| session.list_annotations())?;
        let annotation = annotations
            .iter()
            .find(|annotation| annotation.index == index)
            .ok_or("Comentário não encontrado")?;
        let [x0, y0, x1, y1] = annotation.rect;
        let center = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let selection = with_session(|session| session.select_at(center))?;
        // Voltar a Selecionar antes: trocar de ferramenta limpa a seleção.
        activate_tool(ToolMode::None);
        if let Some(selection) = &selection {
            status(&format!("{} selecionado", selection.label()));
        }
        set_selection(selection);
        Ok(())
    })();
    if let Err(error) = result {
        status(&error);
    }
}

// ---------------------------------------------------------------- sobre

/// Endereço do projeto (repositório).
const WEBSITE: &str = "https://github.com/wbaamaral/rustpdf";
/// `GTK_LICENSE_AGPL_3_0` (AGPL-3.0 ou posterior).
const LICENSE_AGPL_3_0_OR_LATER: c_int = 13;

/// Primeira linha da saída de uma ferramenta (`stdout` ou `stderr`), ou "não encontrado".
fn tool_version(program: &str, args: &[&str]) -> String {
    rustpdf::tool_command(program)
        .args(args)
        .output()
        .ok()
        .and_then(|output| {
            let text = if output.stdout.is_empty() {
                output.stderr
            } else {
                output.stdout
            };
            String::from_utf8_lossy(&text)
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string())
        })
        .unwrap_or_else(|| "não encontrado".into())
}

/// Bibliotecas e programas usados, com licença e página do projeto (o GTK
/// transforma "Nome https://…" em link). Mantida em sincronia com TERCEIROS.md.
const LIBRARIES: &[&str] = &[
    "GTK 3 — LGPL-2.1+ https://www.gtk.org",
    "GLib / GObject — LGPL-2.1+ https://gitlab.gnome.org/GNOME/glib",
    "gdk-pixbuf — LGPL-2.0+ https://gitlab.gnome.org/GNOME/gdk-pixbuf",
    "Pango — LGPL-2.0+ https://pango.gnome.org",
    "Cairo — LGPL-2.1 ou MPL-1.1 https://www.cairographics.org",
    "qpdf — Apache-2.0 https://qpdf.sourceforge.io",
    "zlib — Zlib https://zlib.net",
    "libjpeg-turbo — BSD-3-Clause / IJG https://libjpeg-turbo.org",
    "Poppler (pdftoppm, pdftotext, pdfinfo) — GPL https://poppler.freedesktop.org",
    "Tesseract OCR — Apache-2.0 https://github.com/tesseract-ocr/tesseract",
    "Biblioteca padrão do Rust — MIT / Apache-2.0 https://www.rust-lang.org",
];

/// Versões dos componentes em uso (para suporte e relatos de problema).
pub fn component_versions() -> Vec<String> {
    let (major, minor, micro) = unsafe {
        (
            gtk_get_major_version(),
            gtk_get_minor_version(),
            gtk_get_micro_version(),
        )
    };
    let qpdf = unsafe {
        let pointer = qpdf_get_qpdf_version();
        if pointer.is_null() {
            "?".to_string()
        } else {
            CStr::from_ptr(pointer).to_string_lossy().into_owned()
        }
    };
    vec![
        format!("GTK {major}.{minor}.{micro}"),
        format!("qpdf {qpdf}"),
        format!("Poppler: {}", tool_version("pdftoppm", &["-v"])),
        format!("Tesseract: {}", tool_version("tesseract", &["--version"])),
    ]
}

/// Janela "Sobre o RustPDF".
pub fn show_about() {
    let Some(parent) = STATE.with(|cell| cell.borrow().as_ref().map(|app| app.window)) else {
        return;
    };
    // Listas terminadas em NULL para o GTK; as CStrings vivem até o fim da função.
    let owned =
        |items: &[String]| -> Vec<CString> { items.iter().map(|item| cstring(item)).collect() };
    let pointers = |items: &[CString]| -> Vec<*const c_char> {
        items
            .iter()
            .map(|item| item.as_ptr())
            .chain(std::iter::once(ptr::null()))
            .collect()
    };
    let authors = owned(&["Welyqrson Bastos Amaral".to_string()]);
    let assisted = owned(&["Claude (Anthropic), via Claude Code".to_string()]);
    let components = owned(&component_versions());
    let libraries = owned(
        &LIBRARIES
            .iter()
            .map(|entry| entry.to_string())
            .collect::<Vec<_>>(),
    );
    unsafe {
        let about = gtk_about_dialog_new();
        gtk_window_set_transient_for(about, parent);
        gtk_about_dialog_set_program_name(about, c"RustPDF".as_ptr());
        gtk_about_dialog_set_version(about, cstring(env!("CARGO_PKG_VERSION")).as_ptr());
        gtk_about_dialog_set_logo_icon_name(about, cstring(icons::APP_ICON_NAME).as_ptr());
        gtk_about_dialog_set_comments(
            about,
            cstring(
                "Editor de PDF em Rust: páginas, anotações, formulários, edição de texto e \
                 imagens já presentes, OCR e criptografia preservada ao salvar.",
            )
            .as_ptr(),
        );
        gtk_about_dialog_set_copyright(about, c"© 2026 Welyqrson Bastos Amaral".as_ptr());
        gtk_about_dialog_set_website(about, cstring(WEBSITE).as_ptr());
        gtk_about_dialog_set_website_label(about, c"github.com/wbaamaral/rustpdf".as_ptr());
        // Mostra o aviso da AGPL e o botão "Licença" com o link para o texto.
        gtk_about_dialog_set_license_type(about, LICENSE_AGPL_3_0_OR_LATER);
        gtk_about_dialog_set_authors(about, pointers(&authors).as_ptr());
        gtk_about_dialog_add_credit_section(
            about,
            c"Desenvolvimento assistido por".as_ptr(),
            pointers(&assisted).as_ptr(),
        );
        gtk_about_dialog_add_credit_section(
            about,
            c"Bibliotecas".as_ptr(),
            pointers(&libraries).as_ptr(),
        );
        gtk_about_dialog_add_credit_section(
            about,
            c"Versões em uso".as_ptr(),
            pointers(&components).as_ptr(),
        );
        gtk_dialog_run(about);
        gtk_widget_destroy(about);
    }
}
