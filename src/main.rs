// SPDX-License-Identifier: AGPL-3.0-or-later
//! Interface GTK 3 para a camada de PDF. As chamadas C são mantidas aqui para
//! permitir compilação offline, sem crates externas.
// No Windows a interface não abre janela de console.
#![cfg_attr(windows, windows_subsystem = "windows")]
mod dialogos;
mod edicao_inline;
mod icons;
mod menu_contexto;
mod preencher;
mod selecao_multipla;
mod shell;

use rustpdf::apresentacao::arraste::DragGate;
use rustpdf::apresentacao::exportacao::{self, ExportFormat};
use rustpdf::apresentacao::formularios;
use rustpdf::apresentacao::selecao::{self as regras_selecao, Mode as SelectionMode};
use rustpdf::textmove::{self, IDENTITY};
use rustpdf::{
    is_encrypted, AppResult, Matrix, Occurrence, PdfSession, Selection, TextEditOutcome,
};
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr;

#[link(name = "gtk-3")]
extern "C" {
    fn gtk_init(argc: *mut c_int, argv: *mut *mut *mut c_char);
    fn gtk_main();
    fn gtk_main_quit();
    fn gtk_window_new(kind: c_int) -> *mut c_void;
    fn gtk_window_set_title(window: *mut c_void, title: *const c_char);
    fn gtk_window_set_default_size(window: *mut c_void, width: c_int, height: c_int);
    fn gtk_box_new(orientation: c_int, spacing: c_int) -> *mut c_void;
    fn gtk_box_pack_start(
        box_: *mut c_void,
        child: *mut c_void,
        expand: c_int,
        fill: c_int,
        padding: c_int,
    );
    fn gtk_scrolled_window_new(hadjustment: *mut c_void, vadjustment: *mut c_void) -> *mut c_void;
    fn gtk_image_new() -> *mut c_void;
    fn gtk_image_set_from_file(image: *mut c_void, filename: *const c_char);
    fn gtk_image_get_pixbuf(image: *mut c_void) -> *mut c_void;
    fn gtk_event_box_new() -> *mut c_void;
    fn gtk_event_box_set_above_child(event_box: *mut c_void, above_child: c_int);
    fn gtk_widget_add_events(widget: *mut c_void, events: c_int);
    fn gtk_widget_translate_coordinates(
        source: *mut c_void,
        destination: *mut c_void,
        source_x: c_int,
        source_y: c_int,
        destination_x: *mut c_int,
        destination_y: *mut c_int,
    ) -> c_int;
    fn gtk_label_new(text: *const c_char) -> *mut c_void;
    fn gtk_label_set_text(label: *mut c_void, text: *const c_char);
    fn gtk_container_add(container: *mut c_void, widget: *mut c_void);
    fn gtk_widget_show_all(widget: *mut c_void);
    fn gtk_widget_destroy(widget: *mut c_void);
    fn gtk_widget_set_sensitive(widget: *mut c_void, sensitive: c_int);
    fn gtk_file_chooser_dialog_new(
        title: *const c_char,
        parent: *mut c_void,
        action: c_int,
        first_button_text: *const c_char,
        ...
    ) -> *mut c_void;
    fn gtk_file_chooser_get_filename(chooser: *mut c_void) -> *mut c_char;
    fn gtk_file_chooser_set_do_overwrite_confirmation(chooser: *mut c_void, setting: c_int);
    fn gtk_file_chooser_add_filter(chooser: *mut c_void, filter: *mut c_void);
    fn gtk_file_chooser_set_filter(chooser: *mut c_void, filter: *mut c_void);
    fn gtk_file_chooser_set_current_name(chooser: *mut c_void, name: *const c_char);
    fn gtk_file_filter_new() -> *mut c_void;
    fn gtk_file_filter_set_name(filter: *mut c_void, name: *const c_char);
    fn gtk_file_filter_add_pattern(filter: *mut c_void, pattern: *const c_char);
    fn gtk_file_filter_add_mime_type(filter: *mut c_void, mime_type: *const c_char);
    fn gtk_dialog_run(dialog: *mut c_void) -> c_int;
    fn gtk_text_view_new() -> *mut c_void;
    fn gtk_text_view_set_editable(view: *mut c_void, setting: c_int);
    fn gtk_text_view_set_wrap_mode(view: *mut c_void, mode: c_int);
    fn gtk_text_view_get_buffer(view: *mut c_void) -> *mut c_void;
    fn gtk_text_buffer_set_text(buffer: *mut c_void, text: *const c_char, len: c_int);
    fn gtk_dialog_get_content_area(dialog: *mut c_void) -> *mut c_void;
    fn gtk_entry_new() -> *mut c_void;
    fn gtk_entry_get_text(entry: *mut c_void) -> *const c_char;
    fn gtk_entry_set_text(entry: *mut c_void, text: *const c_char);
    fn gtk_entry_set_visibility(entry: *mut c_void, visible: c_int);
    fn gtk_entry_set_activates_default(entry: *mut c_void, setting: c_int);
    fn gtk_dialog_set_default_response(dialog: *mut c_void, response_id: c_int);
    fn gtk_menu_bar_new() -> *mut c_void;
    fn gtk_menu_new() -> *mut c_void;
    fn gtk_menu_item_set_submenu(menu_item: *mut c_void, submenu: *mut c_void);
    fn gtk_menu_shell_append(menu_shell: *mut c_void, child: *mut c_void);
    fn gtk_separator_menu_item_new() -> *mut c_void;
    fn gtk_container_get_children(container: *mut c_void) -> *mut c_void;
    fn gtk_entry_set_width_chars(entry: *mut c_void, n_chars: c_int);
    fn gtk_label_set_xalign(label: *mut c_void, xalign: f32);
    fn gtk_widget_set_margin_top(widget: *mut c_void, margin: c_int);
    fn gtk_widget_set_margin_bottom(widget: *mut c_void, margin: c_int);
    fn gtk_window_set_position(window: *mut c_void, position: c_int);
    fn gtk_widget_set_size_request(widget: *mut c_void, width: c_int, height: c_int);
    fn gtk_widget_get_allocated_width(widget: *mut c_void) -> c_int;
    fn gtk_widget_queue_draw(widget: *mut c_void);
    fn gtk_widget_get_window(widget: *mut c_void) -> *mut c_void;
    fn gtk_widget_get_allocated_height(widget: *mut c_void) -> c_int;
}

#[link(name = "gdk-3")]
extern "C" {
    fn gdk_event_get_coords(event: *mut c_void, x: *mut f64, y: *mut f64) -> c_int;
    fn gdk_event_get_button(event: *mut c_void, button: *mut u32) -> c_int;
    fn gdk_event_get_event_type(event: *mut c_void) -> c_int;
    fn gdk_event_get_state(event: *mut c_void, state: *mut u32) -> c_int;
    fn gdk_display_get_default() -> *mut c_void;
    fn gdk_cursor_new_from_name(display: *mut c_void, name: *const c_char) -> *mut c_void;
    fn gdk_window_set_cursor(window: *mut c_void, cursor: *mut c_void);
}

/// `GDK_SHIFT_MASK` em `GdkModifierType`.
const GDK_SHIFT_MASK: u32 = 1;
/// `GDK_CONTROL_MASK` em `GdkModifierType`.
const GDK_CONTROL_MASK: u32 = 1 << 2;

/// `GDK_2BUTTON_PRESS`: segundo clique de um clique duplo.
const GDK_DOUBLE_BUTTON_PRESS: c_int = 5;

/// Máscaras `GdkEventMask` usadas para capturar cliques e o arraste do mouse.
const GDK_POINTER_MOTION_MASK: c_int = 1 << 2;
const GDK_BUTTON_PRESS_MASK: c_int = 1 << 8;
const GDK_BUTTON_RELEASE_MASK: c_int = 1 << 9;

#[link(name = "cairo")]
extern "C" {
    fn cairo_save(cr: *mut c_void);
    fn cairo_restore(cr: *mut c_void);
    fn cairo_set_source_rgba(cr: *mut c_void, red: f64, green: f64, blue: f64, alpha: f64);
    fn cairo_set_line_width(cr: *mut c_void, width: f64);
    fn cairo_set_dash(cr: *mut c_void, dashes: *const f64, count: c_int, offset: f64);
    fn cairo_rectangle(cr: *mut c_void, x: f64, y: f64, width: f64, height: f64);
    fn cairo_fill_preserve(cr: *mut c_void);
    fn cairo_stroke(cr: *mut c_void);
    fn cairo_new_path(cr: *mut c_void);
    fn cairo_move_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_line_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_close_path(cr: *mut c_void);
    fn cairo_arc(cr: *mut c_void, xc: f64, yc: f64, radius: f64, angle1: f64, angle2: f64);
}

/// `G_CONNECT_AFTER`: o handler roda depois do desenho padrão do widget.
const G_CONNECT_AFTER: c_int = 1;

#[link(name = "gdk_pixbuf-2.0")]
extern "C" {
    fn gdk_pixbuf_get_width(pixbuf: *mut c_void) -> c_int;
    fn gdk_pixbuf_get_height(pixbuf: *mut c_void) -> c_int;
}

#[link(name = "gobject-2.0")]
extern "C" {
    fn g_signal_connect_data(
        instance: *mut c_void,
        detailed_signal: *const c_char,
        c_handler: *const c_void,
        data: *mut c_void,
        destroy_data: *mut c_void,
        connect_flags: c_int,
    ) -> u64;
}

#[link(name = "glib-2.0")]
extern "C" {
    fn g_free(mem: *mut c_void);
    fn g_list_free(list: *mut c_void);
}

/// Cabeçalho `GList` do GLib (dados + próximo + anterior) em 64 bits.
#[repr(C)]
struct GList {
    data: *mut c_void,
    next: *mut GList,
    prev: *mut GList,
}

fn glist_next(node: *mut GList) -> *mut GList {
    if node.is_null() {
        ptr::null_mut()
    } else {
        unsafe { (*node).next }
    }
}

fn glist_data(node: *mut GList) -> *mut c_void {
    if node.is_null() {
        ptr::null_mut()
    } else {
        unsafe { (*node).data }
    }
}

/// Modo ativo de edição por clique na página (WYSIWYG).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ToolMode {
    None,
    Hand,
    Text,
    Note,
    Image,
    TextField,
    Checkbox,
    Radio,
    Choice,
    Highlight,
}

/// Arraste em andamento: mover elemento existente ou destacar.
#[derive(Clone, Debug)]
enum DragState {
    /// Mover, redimensionar ou girar a seleção a partir de `start` (pontos da página).
    Transform {
        mode: TransformMode,
        start: (f64, f64),
        selection: Selection,
        /// Só vira arraste depois do limiar em pixels (tremor do clique não move).
        gate: DragGate,
    },
    Highlight {
        start: (f64, f64),
    },
    /// Retângulo de seleção (ou Ctrl+clique, se não passar do limiar).
    Marquee {
        start: (f64, f64),
        mode: SelectionMode,
        gate: DragGate,
    },
    /// Arrastar o grupo selecionado.
    MoveGroup {
        start: (f64, f64),
        gate: DragGate,
    },
}

/// O que o arraste faz com a seleção.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TransformMode {
    Move,
    /// Alça em coordenadas locais da moldura (`0`, `0.5` ou `1` em cada eixo).
    Resize {
        hx: f64,
        hy: f64,
    },
    Rotate,
}

struct AppState {
    window: *mut c_void,
    image: *mut c_void,
    status: *mut c_void,
    session: Option<PdfSession>,
    tool: ToolMode,
    drag: Option<DragState>,
    pending_image: Option<PathBuf>,
    event_box: *mut c_void,
    /// Objeto selecionado, com moldura em pontos da página.
    selection: Option<Selection>,
    /// Transformação do arraste em andamento, aplicada à moldura (prévia).
    drag_transform: Matrix,
    /// Retângulo da ferramenta Destaque enquanto é arrastado.
    rubber_band: Option<[f64; 4]>,
    /// Cursor atual sobre a página (nome CSS do GDK).
    cursor: &'static str,
    /// Tamanho da página atual em pontos (evita consultar o pdfinfo a cada evento).
    page_size: Option<(f64, f64)>,
    /// Moldura da janela (menus, barras, painel lateral, status).
    ui: shell::Widgets,
}

thread_local! { static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) }; }

fn cstring(text: &str) -> CString {
    CString::new(text).expect("string sem NUL")
}

/// Nome de arquivo devolvido pelo GTK (bytes do sistema no Unix, UTF-8 no Windows).
unsafe fn path_from_glib(raw: *const c_char) -> PathBuf {
    let bytes = CStr::from_ptr(raw).to_bytes();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

fn status(message: &str) {
    STATE.with(|cell| {
        if let Some(app) = cell.borrow().as_ref() {
            let message = cstring(message);
            unsafe { gtk_label_set_text(app.status, message.as_ptr()) };
        }
    });
}

/// Tipo de arquivo de um diálogo: define os filtros (o primeiro fica ativo)
/// e a extensão acrescentada ao salvar.
#[derive(Clone, Copy, PartialEq, Debug)]
enum FileKind {
    Pdf,
    Image,
    Png,
    /// Saída de "Exportar › Documento como…".
    Export(ExportFormat),
}

/// `(nome do filtro, extensões, tipos MIME)`, na ordem da caixa de filtros.
type FilterSpec = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

impl FileKind {
    fn filters(self) -> &'static [FilterSpec] {
        const ALL: FilterSpec = ("Todos os arquivos", &[], &[]);
        match self {
            FileKind::Pdf => &[
                ("Documentos PDF (*.pdf)", &["pdf"], &["application/pdf"]),
                ALL,
            ],
            FileKind::Image => &[
                (
                    "Imagens PNG e JPEG (*.png, *.jpg, *.jpeg)",
                    &["png", "jpg", "jpeg"],
                    &["image/png", "image/jpeg"],
                ),
                ("Imagem PNG (*.png)", &["png"], &["image/png"]),
                (
                    "Imagem JPEG (*.jpg, *.jpeg)",
                    &["jpg", "jpeg"],
                    &["image/jpeg"],
                ),
                ALL,
            ],
            FileKind::Png => &[("Imagem PNG (*.png)", &["png"], &["image/png"]), ALL],
            FileKind::Export(ExportFormat::Docx) => &[(
                "Documento do Word (*.docx)",
                &["docx"],
                &["application/vnd.openxmlformats-officedocument.wordprocessingml.document"],
            )],
            FileKind::Export(ExportFormat::Pptx) => &[(
                "Apresentação do PowerPoint (*.pptx)",
                &["pptx"],
                &["application/vnd.openxmlformats-officedocument.presentationml.presentation"],
            )],
            FileKind::Export(ExportFormat::Odt) => &[(
                "Documento OpenDocument Text (*.odt)",
                &["odt"],
                &["application/vnd.oasis.opendocument.text"],
            )],
            FileKind::Export(ExportFormat::Odg) => &[(
                "Desenho OpenDocument Graphics (*.odg)",
                &["odg"],
                &["application/vnd.oasis.opendocument.graphics"],
            )],
            FileKind::Export(ExportFormat::Xlsx) => &[(
                "Planilha do Excel (*.xlsx)",
                &["xlsx"],
                &["application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"],
            )],
            FileKind::Export(ExportFormat::Markdown) => {
                &[("Markdown (*.md)", &["md"], &["text/markdown"])]
            }
            FileKind::Export(ExportFormat::Csv) => &[("CSV (*.csv)", &["csv"], &["text/csv"])],
        }
    }

    fn extension(self) -> Option<&'static str> {
        match self {
            FileKind::Pdf => Some("pdf"),
            FileKind::Png => Some("png"),
            FileKind::Image => None,
            FileKind::Export(format) => Some(format.extension()),
        }
    }
}

/// Padrão sem distinção de maiúsculas: `pdf` → `*.[pP][dD][fF]`.
fn case_insensitive_pattern(extension: &str) -> String {
    let classes: String = extension
        .chars()
        .map(|character| {
            let (lower, upper) = (
                character.to_ascii_lowercase(),
                character.to_ascii_uppercase(),
            );
            if lower == upper {
                character.to_string()
            } else {
                format!("[{lower}{upper}]")
            }
        })
        .collect();
    format!("*.{classes}")
}

/// Ao salvar sem extensão, acrescenta a do tipo (`relatorio` → `relatorio.pdf`).
fn with_extension(path: PathBuf, kind: FileKind) -> PathBuf {
    match kind.extension() {
        Some(extension)
            if !path
                .extension()
                .is_some_and(|current| current.eq_ignore_ascii_case(extension)) =>
        {
            let mut name = path.into_os_string();
            name.push(".");
            name.push(extension);
            PathBuf::from(name)
        }
        _ => path,
    }
}

/// Nome sugerido ao salvar: o do documento aberto, com a extensão do tipo.
fn suggested_name(kind: FileKind) -> String {
    let stem = STATE
        .with(|cell| {
            cell.borrow().as_ref().and_then(|app| {
                app.session.as_ref().and_then(|session| {
                    session
                        .source
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                })
            })
        })
        .unwrap_or_else(|| "documento".into());
    match kind.extension() {
        Some(extension) => format!("{stem}.{extension}"),
        None => stem,
    }
}

fn chooser(title: &str, save: bool, kind: FileKind) -> Option<PathBuf> {
    let parent = STATE.with(|cell| cell.borrow().as_ref().map(|app| app.window))?;
    let title = cstring(title);
    let cancel = cstring("Cancelar");
    let accept = cstring(if save { "Salvar" } else { "Abrir" });
    unsafe {
        let dialog = gtk_file_chooser_dialog_new(
            title.as_ptr(),
            parent,
            if save { 1 } else { 0 },
            cancel.as_ptr(),
            -6,
            accept.as_ptr(),
            -3,
            ptr::null::<c_char>(),
        );
        // Caixa de filtros: o primeiro (tipo aceito) fica ativo; o usuário
        // pode trocar, inclusive para "Todos os arquivos".
        for (index, (name, extensions, mime_types)) in kind.filters().iter().enumerate() {
            let filter = gtk_file_filter_new();
            gtk_file_filter_set_name(filter, cstring(name).as_ptr());
            if extensions.is_empty() {
                gtk_file_filter_add_pattern(filter, c"*".as_ptr());
            }
            for extension in *extensions {
                gtk_file_filter_add_pattern(
                    filter,
                    cstring(&case_insensitive_pattern(extension)).as_ptr(),
                );
            }
            for mime in *mime_types {
                gtk_file_filter_add_mime_type(filter, cstring(mime).as_ptr());
            }
            gtk_file_chooser_add_filter(dialog, filter);
            if index == 0 {
                gtk_file_chooser_set_filter(dialog, filter);
            }
        }
        if save {
            gtk_file_chooser_set_do_overwrite_confirmation(dialog, 1);
            gtk_file_chooser_set_current_name(dialog, cstring(&suggested_name(kind)).as_ptr());
        }
        let selected = if gtk_dialog_run(dialog) == -3 {
            let raw = gtk_file_chooser_get_filename(dialog);
            if raw.is_null() {
                None
            } else {
                let path = path_from_glib(raw);
                g_free(raw.cast());
                Some(if save {
                    with_extension(path, kind)
                } else {
                    path
                })
            }
        } else {
            None
        };
        gtk_widget_destroy(dialog);
        selected
    }
}

fn confirm(message: &str) -> bool {
    dialogos::confirm(message)
}

fn prompt_text(message: &str) -> Option<String> {
    prompt_text_default(message, "")
}

fn prompt_text_default(message: &str, default: &str) -> Option<String> {
    dialogos::prompt_text_default(message, default)
}

fn prompt_password() -> Option<String> {
    dialogos::prompt_password()
}

fn show_text(title: &str, content: &str) {
    dialogos::show_text(title, content);
}

unsafe fn clear_children(container: *mut c_void) {
    let children = gtk_container_get_children(container);
    let mut node = children as *mut GList;
    while !node.is_null() {
        let data = glist_data(node);
        if !data.is_null() {
            gtk_widget_destroy(data);
        }
        node = glist_next(node);
    }
    if !children.is_null() {
        g_list_free(children);
    }
}

fn render() -> AppResult<()> {
    shell::render_all()
}

fn with_session<T>(f: impl FnOnce(&mut PdfSession) -> AppResult<T>) -> AppResult<T> {
    STATE.with(|cell| {
        let mut borrowed = cell.borrow_mut();
        let session = borrowed
            .as_mut()
            .and_then(|app| app.session.as_mut())
            .ok_or("Abra um PDF primeiro")?;
        f(session)
    })
}

fn preview_point(event_box: *mut c_void, event: *mut c_void, clamp: bool) -> AppResult<(f64, f64)> {
    let mut event_x = 0.0;
    let mut event_y = 0.0;
    if unsafe { gdk_event_get_coords(event, &mut event_x, &mut event_y) } == 0 {
        return Err("Não foi possível ler a posição do mouse".into());
    }
    STATE.with(|cell| {
        let borrowed = cell.borrow();
        let app = borrowed.as_ref().ok_or("Interface ausente")?;
        let session = app.session.as_ref().ok_or("Abra um PDF primeiro")?;
        let pixbuf = unsafe { gtk_image_get_pixbuf(app.image) };
        if pixbuf.is_null() {
            return Err("Prévia da página indisponível".into());
        }
        let (image_width, image_height) = unsafe {
            (
                gdk_pixbuf_get_width(pixbuf) as f64,
                gdk_pixbuf_get_height(pixbuf) as f64,
            )
        };
        if image_width <= 0.0 || image_height <= 0.0 {
            return Err("Tamanho inválido da prévia".into());
        }
        let mut image_x = 0;
        let mut image_y = 0;
        if unsafe {
            gtk_widget_translate_coordinates(
                event_box,
                app.image,
                event_x.round() as c_int,
                event_y.round() as c_int,
                &mut image_x,
                &mut image_y,
            )
        } == 0
        {
            return Err("Não foi possível mapear a posição do mouse".into());
        }
        // O GtkImage centraliza a página quando a área visível é maior que
        // a prévia; desconta essa margem para obter a posição no pixbuf.
        let (allocated_width, allocated_height) = unsafe {
            (
                gtk_widget_get_allocated_width(app.image) as f64,
                gtk_widget_get_allocated_height(app.image) as f64,
            )
        };
        let image_x = image_x as f64 - ((allocated_width - image_width) / 2.0).max(0.0);
        let image_y = image_y as f64 - ((allocated_height - image_height) / 2.0).max(0.0);
        if !clamp
            && (image_x < 0.0 || image_y < 0.0 || image_x > image_width || image_y > image_height)
        {
            return Err("Clique dentro da página".into());
        }
        let (page_width, page_height) = match app.page_size {
            Some(size) => size,
            None => session.page_dimensions()?,
        };
        Ok((
            image_x.clamp(0.0, image_width) * page_width / image_width,
            (image_height - image_y.clamp(0.0, image_height)) * page_height / image_height,
        ))
    })
}

fn set_session(path: &Path) -> AppResult<()> {
    let session = if is_encrypted(path)? {
        let Some(password) = prompt_password() else {
            return Ok(());
        };
        PdfSession::open_with_password(path, &password)?
    } else {
        PdfSession::open(path)?
    };
    STATE.with(|cell| cell.borrow_mut().as_mut().unwrap().session = Some(session));
    render()
}

/// Para PDF protegido, pergunta se a criptografia deve ser mantida.
///
/// `Some(false)`: manter (padrão); `Some(true)`: remover, após confirmação
/// explícita; `None`: cancelado.
/// Arquivo › Exportar › Documento como…: destino, páginas e a opção do
/// formato; a exportação roda em segundo plano sobre uma cópia.
fn export_document(format: ExportFormat) -> AppResult<()> {
    if shell::export_running() {
        return Err("Já há uma exportação em andamento".into());
    }
    if with_session(|s| Ok(s.is_protected()))?
        && !confirm(
            "Este PDF está protegido por senha.\nO arquivo exportado não terá proteção. Continuar?",
        )
    {
        return Ok(());
    }
    let title = format!("Exportar documento como {}", format.label());
    let Some(output) = chooser(&title, true, FileKind::Export(format)) else {
        return Ok(());
    };
    let Some(pages) = prompt_text_default("Páginas a exportar (vazio = todas; ex.: 1-3,7):", "")
    else {
        return Ok(());
    };
    let answer = exportacao::question(format).is_some_and(confirm);
    let total = with_session(|s| Ok(s.pages))?;
    let mut request = exportacao::request(output, format, &pages, answer, total)?;
    if format == ExportFormat::Markdown && confirm(exportacao::ai_question()) {
        request.ai_optimized = true;
        request.images = false;
    }
    let snapshot = with_session(|s| s.export_snapshot())?;
    shell::start_export(snapshot, total, request);
    status(&format!("Exportando para {}…", format.label()));
    Ok(())
}

fn ask_remove_protection(action: &str) -> AppResult<Option<bool>> {
    if !with_session(|s| Ok(s.is_protected()))? {
        return Ok(Some(false));
    }
    if confirm(&format!(
        "Este PDF está protegido por senha.\n{action} mantendo a mesma senha e permissões?"
    )) {
        return Ok(Some(false));
    }
    if confirm("Remover a proteção? Qualquer pessoa poderá abrir o arquivo salvo sem senha.") {
        Ok(Some(true))
    } else {
        Ok(None)
    }
}

/// Pergunta nome da imagem e, se ela aparece mais de uma vez, a ocorrência.
fn ask_image_target(action: &str) -> AppResult<Option<(String, Occurrence)>> {
    let Some(name) = prompt_text(&format!(
        "Nome da imagem a {action} (ou selecione-a na página):"
    )) else {
        return Ok(None);
    };
    let name = name.trim().trim_start_matches('/').to_string();
    let count = with_session(|s| s.image_placements())?
        .iter()
        .filter(|draw| draw.name == name)
        .count();
    if count <= 1 {
        return Ok(Some((name, Occurrence::Only)));
    }
    let Some(answer) = prompt_text_default(
        &format!(
            "'{name}' aparece {count} vezes nesta página. Ocorrência (1..{count}) ou 'todas':"
        ),
        "todas",
    ) else {
        return Ok(None);
    };
    let occurrence = match answer.trim() {
        "todas" => Occurrence::All,
        text => Occurrence::Index(
            text.parse::<usize>()
                .ok()
                .filter(|n| (1..=count).contains(n))
                .ok_or_else(|| format!("Ocorrência inválida: {text}"))?,
        ),
    };
    Ok(Some((name, occurrence)))
}

fn image_report(action: &str, outcome: &rustpdf::ImageEditOutcome) -> String {
    let mut message = format!("Imagem {action} ({} desenho(s))", outcome.draws);
    if outcome.cloned_forms > 0 {
        message.push_str(" — cópia independente criada para as outras ocorrências");
    }
    message
}

fn has_unsaved() -> bool {
    STATE.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|app| app.session.as_ref())
            .is_some_and(|session| session.dirty)
    })
}

/// OCR (texto ou camada pesquisável) das páginas escolhidas, em segundo
/// plano sobre uma cópia, com barra de progresso.
fn start_ocr_from_menu(searchable: bool) -> AppResult<()> {
    let (total, current) = with_session(|s| Ok((s.pages, s.page)))?;
    let kind = if searchable {
        let Some(language) = prompt_text_default(
            "OCR pesquisável: idiomas do Tesseract (ex.: por+eng):",
            rustpdf::apresentacao::ocr::DEFAULT_LANGUAGE,
        ) else {
            return Ok(());
        };
        let language = language.trim().to_string();
        rustpdf::app::ocr::check_language(&rustpdf::infra::tesseract::Tesseract, &language)?;
        shell::OcrKind::Searchable { language }
    } else {
        shell::OcrKind::Text { language: None }
    };
    let (question, default) = if searchable {
        (
            "OCR pesquisável: páginas (vazio = todas; ex.: 1-3,7):",
            String::new(),
        )
    } else {
        (
            "OCR: páginas a ler (vazio = todas; ex.: 1-3,7):",
            current.to_string(),
        )
    };
    let Some(pages) = prompt_text_default(question, &default) else {
        return Ok(());
    };
    let pages = rustpdf::apresentacao::ocr::pages(&pages, total)?;
    let (snapshot, work, revision) = with_session(|s| {
        Ok((
            s.export_snapshot()?,
            s.work_path().to_path_buf(),
            s.revision(),
        ))
    })?;
    shell::start_ocr(snapshot, work, revision, pages, kind);
    status("OCR em andamento…");
    Ok(())
}

fn dispatch(action: usize) -> AppResult<()> {
    let _trace = rustpdf::trace(|| format!("ação {action}"));
    // Durante o OCR a janela fica bloqueada; isto cobre atalhos que escapem.
    if shell::ocr_running() {
        return Err("Aguarde o OCR terminar (Esc ou Cancelar para parar)".into());
    }
    // Zoom mantém a seleção (está em pontos da página); o resto a descarta.
    // Ações de imagem usam a imagem selecionada, guardada antes de limpar.
    let selected_image = current_selection()
        .filter(|selection| matches!(selection.target, rustpdf::SelectionTarget::Image { .. }));
    if !matches!(action, 6 | 7 | 54 | 55 | 56 | 58 | 60..=63) {
        set_selection(None);
    }
    match action {
        1 => {
            // abrir
            if has_unsaved() && !confirm("Descartar alterações não salvas?") {
                return Ok(());
            }
            if let Some(path) = chooser("Abrir PDF", false, FileKind::Pdf) {
                set_session(&path)?;
                STATE.with(|cell| {
                    let mut borrowed = cell.borrow_mut();
                    let app = borrowed.as_mut().unwrap();
                    app.tool = ToolMode::None;
                    app.drag = None;
                });
            }
        }
        2 => {
            // novo
            if has_unsaved() && !confirm("Descartar alterações não salvas?") {
                return Ok(());
            }
            let session = PdfSession::new_blank()?;
            STATE.with(|cell| {
                let mut borrowed = cell.borrow_mut();
                let app = borrowed.as_mut().unwrap();
                app.session = Some(session);
                app.tool = ToolMode::None;
                app.drag = None;
            });
            render()?;
        }
        3 => {
            // salvar como
            let Some(remove_protection) = ask_remove_protection("Salvar")? else {
                return Ok(());
            };
            if let Some(path) = chooser("Salvar PDF como", true, FileKind::Pdf) {
                with_session(|session| {
                    if remove_protection {
                        session.save_as_unencrypted(&path)
                    } else {
                        session.save_as(&path)
                    }
                })?;
                render()?;
                status(if with_session(|s| Ok(s.is_protected()))? {
                    "PDF salvo com a mesma senha e permissões do original"
                } else {
                    "PDF salvo"
                });
            }
        }
        4 => {
            with_session(|s| {
                if s.page > 1 {
                    s.page -= 1;
                }
                Ok(())
            })?;
            render()?;
        }
        5 => {
            with_session(|s| {
                if s.page < s.pages {
                    s.page += 1;
                }
                Ok(())
            })?;
            render()?;
        }
        6 => shell::scale_zoom(1.25)?,
        7 => shell::scale_zoom(0.8)?,
        8 => {
            with_session(|s| s.rotate_current())?;
            render()?;
        }
        9 => {
            if confirm("Excluir a página atual?") {
                with_session(|s| s.delete_current())?;
                render()?;
            }
        }
        10 => {
            if let Some(path) = chooser("Adicionar páginas de PDF", false, FileKind::Pdf) {
                with_session(|s| s.append_pdf(&path))?;
                render()?;
            }
        }
        11 => {
            let Some(remove_protection) = ask_remove_protection("Exportar a página")? else {
                return Ok(());
            };
            if let Some(path) = chooser("Extrair página como PDF", true, FileKind::Pdf) {
                with_session(|s| {
                    if remove_protection {
                        s.extract_current_unencrypted(&path)
                    } else {
                        s.extract_current(&path)
                    }
                })?;
                status("Página exportada como PDF");
            }
        }
        12 => {
            if let Some(path) = chooser("Exportar página como PNG", true, FileKind::Png) {
                with_session(|s| s.export_png(&path))?;
                status("Página exportada como PNG");
            }
        }
        exportacao::CANCEL_ACTION => {
            if shell::cancel_export() {
                status("Cancelando a exportação…");
            } else {
                status("Nenhuma exportação em andamento");
            }
        }
        action if exportacao::export_actions().contains(&action) => {
            let Some(format) = exportacao::action_format(action) else {
                return Ok(());
            };
            export_document(format)?;
        }
        13 => {
            let text = with_session(|s| s.extract_text())?;
            show_text("Texto da página", &text);
        }
        14 => start_ocr_from_menu(false)?,
        15 => shell::activate_tool(ToolMode::Note),
        31 => {
            let Some(position) = prompt_text_default(
                "Destaque: X Y largura altura (pontos PDF, origem no canto inferior esquerdo):",
                "72 700 180 18",
            ) else {
                return Ok(());
            };
            let values = position
                .split_whitespace()
                .map(|part| {
                    part.parse::<f64>()
                        .map_err(|_| "Coordenada inválida".to_string())
                })
                .collect::<AppResult<Vec<_>>>()?;
            if values.len() != 4 {
                return Err("Informe X Y largura altura".into());
            }
            with_session(|s| s.highlight_at(values[0], values[1], values[2], values[3]))?;
            render()?;
        }
        32 => shell::activate_tool(ToolMode::Highlight),
        19 => shell::activate_tool(ToolMode::Text),
        20 => {
            with_session(|s| s.undo())?;
            render()?;
        }
        21 => {
            with_session(|s| s.redo())?;
            render()?;
        }
        22 => {
            if let Some(value) = prompt_text("Recortar margem igual em todos os lados (pontos):") {
                let margin: f64 = value.trim().parse().map_err(|_| "Margem inválida")?;
                with_session(|s| s.crop_current_inset(margin))?;
                render()?;
            }
        }
        23 => shell::activate_tool(ToolMode::Image),
        24 => start_ocr_from_menu(true)?,
        25 => shell::activate_tool(ToolMode::TextField),
        26 => preencher::fill_from_menu(preencher::Wanted::Text)?,
        27 => shell::activate_tool(ToolMode::Checkbox),
        28 => preencher::fill_from_menu(preencher::Wanted::Checkbox)?,
        29 => {
            let fields = with_session(|s| s.list_form_fields())?;
            let content = if fields.is_empty() {
                "Nenhum campo direto encontrado".to_string()
            } else {
                fields
                    .into_iter()
                    .map(|(name, kind, value)| format!("{name}\t{kind}\t{value}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            show_text("Campos do formulário", &content);
        }
        33 => shell::activate_tool(ToolMode::Radio),
        34 => preencher::fill_from_menu(preencher::Wanted::Radio)?,
        35 => shell::activate_tool(ToolMode::Choice),
        36 => preencher::fill_from_menu(preencher::Wanted::Choice)?,
        37 => {
            let runs = with_session(|s| s.list_text_runs())?;
            let content = if runs.is_empty() {
                "Nenhum texto desenhado na página".to_string()
            } else {
                runs.into_iter()
                    .enumerate()
                    .map(|(index, text)| format!("{}. {text}", index + 1))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            show_text("Textos da página", &content);
        }
        38 => {
            let Some(old) = prompt_text("Texto atual a substituir:") else {
                return Ok(());
            };
            let Some(new) = prompt_text("Novo texto:") else {
                return Ok(());
            };
            let replaced = with_session(|s| s.replace_text(old.trim(), &new))?;
            render()?;
            status(&format!("{replaced} substituições de texto"));
        }
        39 => {
            let images = with_session(|s| s.list_images())?;
            let content = if images.is_empty() {
                "Nenhuma imagem na página".to_string()
            } else {
                images
                    .into_iter()
                    .map(|image| {
                        format!(
                            "{} — {}x{} {}",
                            image.name, image.width, image.height, image.filter
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            show_text("Imagens da página", &content);
        }
        40 => {
            let target = match &selected_image {
                Some(selection) => Some((String::new(), Occurrence::Only, selection.anchor)),
                None => ask_image_target("substituir")?
                    .map(|(name, occurrence)| (name, occurrence, (0.0, 0.0))),
            };
            let Some((name, occurrence, anchor)) = target else {
                return Ok(());
            };
            let Some(path) = chooser("Nova imagem PNG ou JPEG", false, FileKind::Image) else {
                return Ok(());
            };
            let outcome = with_session(|s| {
                if name.is_empty() {
                    s.replace_image_at(anchor, &path)
                } else {
                    s.replace_image(&name, &path, occurrence)
                }
            })?;
            render()?;
            status(&image_report("substituída", &outcome));
        }
        41 => {
            let target = match &selected_image {
                Some(_) => None,
                None => match ask_image_target("mover")? {
                    Some(target) => Some(target),
                    None => return Ok(()),
                },
            };
            let Some(delta) = prompt_text_default("Deslocamento DX DY (pontos PDF):", "10 -10")
            else {
                return Ok(());
            };
            let values = delta
                .split_whitespace()
                .map(|part| {
                    part.parse::<f64>()
                        .map_err(|_| "Deslocamento inválido".to_string())
                })
                .collect::<AppResult<Vec<_>>>()?;
            if values.len() != 2 {
                return Err("Informe DX DY".into());
            }
            match (target, &selected_image) {
                (Some((name, occurrence)), _) => {
                    let outcome =
                        with_session(|s| s.move_image(&name, values[0], values[1], occurrence))?;
                    render()?;
                    status(&image_report("movida", &outcome));
                }
                (None, Some(selection)) => {
                    let transform = textmove::translation(values[0], values[1]);
                    with_session(|s| s.transform_selection(selection, &transform))?;
                    render()?;
                    set_selection(Some(selection.transformed(&transform)));
                    status("Imagem movida");
                }
                (None, None) => {}
            }
        }
        42 => {
            let outcome = match &selected_image {
                Some(selection) => {
                    let name = match &selection.target {
                        rustpdf::SelectionTarget::Image { name } => name.as_str(),
                        _ => "",
                    };
                    if !confirm(&format!("Remover esta ocorrência da imagem {name}?")) {
                        return Ok(());
                    }
                    with_session(|s| s.remove_image_at(selection.anchor))?
                }
                None => {
                    let Some((name, occurrence)) = ask_image_target("remover")? else {
                        return Ok(());
                    };
                    if !confirm(&format!("Remover a imagem '{name}'?")) {
                        return Ok(());
                    }
                    with_session(|s| s.remove_image(&name, occurrence))?
                }
            };
            render()?;
            status(&image_report("removida", &outcome));
        }
        30 => {
            let Some(path) = chooser("Inserir imagem PNG ou JPEG", false, FileKind::Image) else {
                return Ok(());
            };
            let (page_width, page_height) = with_session(|s| s.page_dimensions())?;
            let default = format!(
                "{:.0} {:.0} {:.0} {:.0}",
                page_width * 0.1,
                page_height * 0.1,
                page_width * 0.4,
                page_height * 0.4,
            );
            let Some(position) = prompt_text_default("X Y largura altura (pontos PDF):", &default)
            else {
                return Ok(());
            };
            let values = position
                .split_whitespace()
                .map(|part| {
                    part.parse::<f64>()
                        .map_err(|_| "Coordenada inválida".to_string())
                })
                .collect::<AppResult<Vec<_>>>()?;
            if values.len() != 4 {
                return Err("Informe X Y largura altura".into());
            }
            with_session(|s| s.insert_image_at(&path, values[0], values[1], values[2], values[3]))?;
            render()?;
        }
        16 => {
            with_session(|s| s.insert_blank_after())?;
            render()?;
        }
        17 => {
            with_session(|s| s.move_current(-1))?;
            render()?;
        }
        18 => {
            with_session(|s| s.move_current(1))?;
            render()?;
        }
        50 => {
            if !with_session(|s| Ok(s.has_file()))? {
                return dispatch(3);
            }
            with_session(|s| s.save())?;
            render()?;
            status(if with_session(|s| Ok(s.is_protected()))? {
                "Salvo (com a mesma senha e permissões do original)"
            } else {
                "Salvo"
            });
        }
        51 => goto_page(1)?,
        52 => {
            let last = with_session(|s| Ok(s.pages))?;
            goto_page(last)?;
        }
        53 => shell::focus_page_entry(),
        54 => shell::set_zoom(shell::ZoomMode::Custom, Some(1.0))?,
        55 => shell::set_zoom(shell::ZoomMode::FitPage, None)?,
        56 => shell::set_zoom(shell::ZoomMode::FitWidth, None)?,
        57 => {
            with_session(|s| s.rotate_current_by(-90))?;
            render()?;
        }
        58 if selecao_multipla::active() => selecao_multipla::delete_group()?,
        58 => {
            let Some(selection) = current_selection() else {
                return Err("Nada selecionado".into());
            };
            with_session(|s| s.delete_selection(&selection))?;
            set_selection(None);
            render()?;
            status(&format!("{} excluído", selection.label()));
        }
        59 => {
            if has_unsaved() && !confirm("Descartar alterações não salvas?") {
                return Ok(());
            }
            STATE.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    app.session = None;
                    app.page_size = None;
                }
            });
            set_tool(ToolMode::None);
            render()?;
            status("Documento fechado");
        }
        60 => shell::toggle_sidebar(),
        61 => shell::toggle_statusbar(),
        62 => shell::toggle_fullscreen(),
        63 => show_text(
            "Atalhos de teclado",
            "Arquivo\n  Ctrl+N  Novo   ·   Ctrl+O  Abrir   ·   Ctrl+S  Salvar   ·   Ctrl+Shift+S  Salvar como\n  Ctrl+Shift+E  Extrair página   ·   Ctrl+W  Fechar   ·   Ctrl+Q  Sair\n\n\
             Editar\n  Ctrl+Z  Desfazer   ·   Ctrl+Y / Ctrl+Shift+Z  Refazer   ·   Del  Excluir objeto selecionado\n  Esc  Cancelar seleção / voltar a Selecionar\n\n\
             Navegação\n  Page Up / Page Down  Página anterior / seguinte   ·   Ctrl+Home / Ctrl+End  Primeira / última\n  Ctrl+G  Ir para a página\n\n\
             Zoom\n  Ctrl++ / Ctrl+−  Aumentar / diminuir   ·   Ctrl+0  100%   ·   Ctrl+1  Ajustar à página\n  Ctrl+2  Ajustar à largura   ·   Ctrl + roda do mouse  Zoom\n\n\
             Ferramentas\n  Alt+1  Selecionar e editar   ·   Alt+2  Mão   ·   Ctrl+T  Texto   ·   Ctrl+I  Imagem\n  Ctrl+E  Campo de texto   ·   Shift ao redimensionar: proporção livre   ·   Shift ao girar: 15°\n\n\
             Documento\n  Ctrl+Shift+N  Página em branco   ·   Ctrl+Shift+I  Inserir PDF   ·   Alt+Del  Apagar página\n  Ctrl+K  Recortar margens   ·   Ctrl+R / Ctrl+Shift+R  Girar página\n\n\
             Exibir\n  F9  Painel lateral   ·   F11  Tela cheia   ·   F1  Esta ajuda",
        ),
        67 => shell::show_panel("comments"),
        98 => shell::show_about(),
        99 => {
            if has_unsaved() && !confirm("Descartar alterações não salvas e sair?") {
                return Ok(());
            }
            unsafe { gtk_main_quit() };
        }
        _ => {}
    }
    Ok(())
}

unsafe extern "C" fn on_clicked(_widget: *mut c_void, data: *mut c_void) {
    if let Err(error) = dispatch(data as usize) {
        status(&error);
    }
}

fn goto_page(number: usize) -> AppResult<()> {
    with_session(|session| {
        if number == 0 || number > session.pages {
            return Err("Página inválida".into());
        }
        session.page = number;
        Ok(())
    })?;
    render()
}

unsafe extern "C" fn on_page_clicked(_widget: *mut c_void, data: *mut c_void) {
    if let Err(error) = goto_page(data as usize) {
        status(&error);
    }
}

unsafe extern "C" fn on_goto_page(_widget: *mut c_void, _data: *mut c_void) {
    let text = STATE.with(|cell| {
        let borrowed = cell.borrow();
        let app = match borrowed.as_ref() {
            Some(app) => app,
            None => return String::new(),
        };
        let _ = app;
        let pointer = gtk_entry_get_text(shell::PAGE_ENTRY.with(|entry| entry.get()));
        if pointer.is_null() {
            String::new()
        } else {
            CStr::from_ptr(pointer).to_string_lossy().into_owned()
        }
    });
    match text.trim().parse::<usize>() {
        Ok(number) => {
            if let Err(error) = goto_page(number) {
                status(&error);
            }
        }
        Err(_) => status("Informe o número da página"),
    }
}

unsafe extern "C" fn on_destroy(_widget: *mut c_void, _data: *mut c_void) {
    STATE.with(|cell| *cell.borrow_mut() = None);
    gtk_main_quit();
}

unsafe extern "C" fn on_delete_event(
    _widget: *mut c_void,
    _event: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    if has_unsaved() && !confirm("Descartar alterações não salvas e sair?") {
        1
    } else {
        0
    }
}

fn clear_drag() {
    STATE.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.drag = None;
            app.rubber_band = None;
            if app.drag_transform != IDENTITY {
                app.drag_transform = IDENTITY;
            }
            unsafe { gtk_widget_queue_draw(app.image) };
        }
    });
}

fn set_tool(tool: ToolMode) {
    STATE.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.tool = tool;
            app.drag = None;
        }
    });
    set_selection(None);
    shell::sync_tools();
}

/// Define a seleção visível (moldura com alças) e redesenha.
fn set_selection(selection: Option<Selection>) {
    // Seleção simples (ou nenhuma) substitui o grupo.
    selecao_multipla::forget();
    STATE.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.selection = selection;
            app.drag_transform = IDENTITY;
            unsafe { gtk_widget_queue_draw(app.image) };
        }
    });
    shell::update_sensitivity();
}

fn current_selection() -> Option<Selection> {
    STATE.with(|cell| cell.borrow().as_ref().and_then(|app| app.selection.clone()))
}

/// Prévia: transformação aplicada à moldura enquanto o botão está pressionado.
fn set_drag_transform(transform: Matrix) {
    STATE.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.drag_transform = transform;
            unsafe { gtk_widget_queue_draw(app.image) };
        }
    });
}

fn set_rubber_band(rect: Option<[f64; 4]>) {
    STATE.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.rubber_band = rect;
            unsafe { gtk_widget_queue_draw(app.image) };
        }
    });
}

/// Troca o cursor sobre a página (`""` volta ao padrão). Cursores ficam em cache.
fn set_cursor(name: &'static str) {
    thread_local! {
        static CURSORS: RefCell<Vec<(&'static str, usize)>> = const { RefCell::new(Vec::new()) };
    }
    let event_box = STATE.with(|cell| {
        let mut borrowed = cell.borrow_mut();
        let app = borrowed.as_mut()?;
        if app.cursor == name {
            return None;
        }
        app.cursor = name;
        Some(app.event_box)
    });
    let Some(event_box) = event_box else {
        return;
    };
    let cursor = if name.is_empty() {
        ptr::null_mut()
    } else {
        CURSORS.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some((_, pointer)) = cache.iter().find(|(cached, _)| *cached == name) {
                return *pointer as *mut c_void;
            }
            let pointer = unsafe {
                gdk_cursor_new_from_name(gdk_display_get_default(), cstring(name).as_ptr())
            };
            cache.push((name, pointer as usize));
            pointer
        })
    };
    unsafe {
        let window = gtk_widget_get_window(event_box);
        if !window.is_null() {
            gdk_window_set_cursor(window, cursor);
        }
    }
}

/// Conversão entre pontos da página e pixels do widget da prévia.
#[derive(Clone, Copy)]
struct PageMapping {
    offset_x: f64,
    offset_y: f64,
    scale_x: f64,
    scale_y: f64,
    page_height: f64,
}

impl PageMapping {
    /// Mesma margem de centralização descontada em `preview_point`.
    fn of(app: &AppState) -> Option<Self> {
        let (page_width, page_height) = app.page_size?;
        let pixbuf = unsafe { gtk_image_get_pixbuf(app.image) };
        if pixbuf.is_null() || page_width <= 0.0 || page_height <= 0.0 {
            return None;
        }
        let image_width = unsafe { gdk_pixbuf_get_width(pixbuf) } as f64;
        let image_height = unsafe { gdk_pixbuf_get_height(pixbuf) } as f64;
        let allocated_width = unsafe { gtk_widget_get_allocated_width(app.image) } as f64;
        let allocated_height = unsafe { gtk_widget_get_allocated_height(app.image) } as f64;
        Some(Self {
            offset_x: ((allocated_width - image_width) / 2.0).max(0.0),
            offset_y: ((allocated_height - image_height) / 2.0).max(0.0),
            scale_x: image_width / page_width,
            scale_y: image_height / page_height,
            page_height,
        })
    }

    fn to_widget(self, (x, y): (f64, f64)) -> (f64, f64) {
        (
            self.offset_x + x * self.scale_x,
            self.offset_y + (self.page_height - y) * self.scale_y,
        )
    }
}

fn start_drag(drag: DragState) {
    STATE.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.drag = Some(drag);
        }
    });
}

/// Posição do evento em pixels da janela do widget que o recebeu.
fn event_coords(event: *mut c_void) -> Option<(f64, f64)> {
    let (mut x, mut y) = (0.0, 0.0);
    (unsafe { gdk_event_get_coords(event, &mut x, &mut y) } != 0).then_some((x, y))
}

/// Posição do evento em pixels do widget da prévia.
fn event_image_coords(event_box: *mut c_void, event: *mut c_void) -> Option<(f64, f64)> {
    let (mut x, mut y) = (0.0, 0.0);
    if unsafe { gdk_event_get_coords(event, &mut x, &mut y) } == 0 {
        return None;
    }
    let image = STATE.with(|cell| cell.borrow().as_ref().map(|app| app.image))?;
    let (mut image_x, mut image_y) = (0, 0);
    let translated = unsafe {
        gtk_widget_translate_coordinates(
            event_box,
            image,
            x.round() as c_int,
            y.round() as c_int,
            &mut image_x,
            &mut image_y,
        )
    };
    (translated != 0).then_some((image_x as f64, image_y as f64))
}

/// Modo de seleção pelas teclas do evento (Ctrl inclui, Ctrl+Shift retira).
fn selection_mode(event: *mut c_void) -> SelectionMode {
    let mut state = 0u32;
    let known = unsafe { gdk_event_get_state(event, &mut state) } != 0;
    regras_selecao::mode(
        known && state & GDK_CONTROL_MASK != 0,
        known && state & GDK_SHIFT_MASK != 0,
    )
}

fn shift_pressed(event: *mut c_void) -> bool {
    let mut state = 0u32;
    unsafe { gdk_event_get_state(event, &mut state) != 0 && state & GDK_SHIFT_MASK != 0 }
}

/// Alças de redimensionamento disponíveis para a seleção (coordenadas locais).
fn resize_handles(selection: &Selection) -> Vec<(f64, f64)> {
    if !selection.resizable {
        return Vec::new();
    }
    let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let edges = [(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)];
    if selection.uniform {
        corners.to_vec()
    } else {
        corners.iter().chain(edges.iter()).copied().collect()
    }
}

/// Distância (px) da alça de girar acima da borda superior da moldura.
const ROTATE_HANDLE_GAP: f64 = 26.0;

/// Posição em pixels da alça de girar e do meio da borda superior.
fn rotate_handle(frame: &textmove::Frame, mapping: PageMapping) -> ((f64, f64), (f64, f64)) {
    let top = mapping.to_widget(frame.point(0.5, 1.0));
    let center = mapping.to_widget(frame.center());
    let (dx, dy) = (top.0 - center.0, top.1 - center.1);
    let length = dx.hypot(dy);
    let (ux, uy) = if length < 1e-6 {
        (0.0, -1.0)
    } else {
        (dx / length, dy / length)
    };
    (
        (
            top.0 + ux * ROTATE_HANDLE_GAP,
            top.1 + uy * ROTATE_HANDLE_GAP,
        ),
        top,
    )
}

/// Alça da seleção sob o pixel `widget` (raio de 7 px).
fn handle_at(widget: (f64, f64)) -> Option<TransformMode> {
    STATE.with(|cell| {
        let borrowed = cell.borrow();
        let app = borrowed.as_ref()?;
        let selection = app.selection.as_ref()?;
        let mapping = PageMapping::of(app)?;
        let near = |(x, y): (f64, f64)| (x - widget.0).hypot(y - widget.1) <= 7.0;
        if selection.rotatable && near(rotate_handle(&selection.frame, mapping).0) {
            return Some(TransformMode::Rotate);
        }
        resize_handles(selection)
            .into_iter()
            .find(|(hx, hy)| near(mapping.to_widget(selection.frame.point(*hx, *hy))))
            .map(|(hx, hy)| TransformMode::Resize { hx, hy })
    })
}

/// Cursor de redimensionamento conforme a direção da alça na tela.
fn resize_cursor(selection: &Selection, hx: f64, hy: f64) -> &'static str {
    let Some(mapping) = STATE.with(|cell| cell.borrow().as_ref().and_then(PageMapping::of)) else {
        return "move";
    };
    let center = mapping.to_widget(selection.frame.center());
    let handle = mapping.to_widget(selection.frame.point(hx, hy));
    let angle = (handle.1 - center.1)
        .atan2(handle.0 - center.0)
        .to_degrees();
    match ((angle + 180.0 + 22.5) / 45.0) as i32 % 4 {
        0 => "ew-resize",
        1 => "nwse-resize",
        2 => "ns-resize",
        _ => "nesw-resize",
    }
}

/// Transformação (pontos da página) do arraste e texto de status.
fn drag_transform(
    mode: TransformMode,
    selection: &Selection,
    start: (f64, f64),
    current: (f64, f64),
    shift: bool,
) -> (Matrix, String) {
    let frame = selection.frame;
    match mode {
        TransformMode::Move => {
            let (dx, dy) = (current.0 - start.0, current.1 - start.1);
            (
                textmove::translation(dx, dy),
                format!("Mover: {dx:.0} {dy:.0} pt (solte para aplicar)"),
            )
        }
        TransformMode::Resize { hx, hy } => {
            let Some((s, t)) = frame.local(current) else {
                return (IDENTITY, String::new());
            };
            let (ax, ay) = (1.0 - hx, 1.0 - hy);
            let corner = hx != 0.5 && hy != 0.5;
            let mut sx = if hx == 0.5 { 1.0 } else { (s - ax) / (hx - ax) };
            let mut sy = if hy == 0.5 { 1.0 } else { (t - ay) / (hy - ay) };
            if corner && (selection.uniform || (selection.corner_ratio && !shift)) {
                // Projeção na diagonal da alça: escala única.
                let k = ((s - ax) * (hx - ax) + (t - ay) * (hy - ay))
                    / ((hx - ax).powi(2) + (hy - ay).powi(2));
                sx = k;
                sy = k;
            }
            let width = frame.u.0.hypot(frame.u.1).max(1e-9);
            let height = frame.v.0.hypot(frame.v.1).max(1e-9);
            let minimum = (4.0 / width).max(4.0 / height);
            if selection.uniform || (corner && selection.corner_ratio && !shift) {
                sx = sx.max(minimum);
                sy = sx;
            } else {
                sx = sx.max(4.0 / width);
                sy = sy.max(4.0 / height);
            }
            let transform = frame.scaling_about(sx, sy, (ax, ay));
            (
                transform,
                format!(
                    "Tamanho: {:.0} × {:.0} pt ({:.0}%){}",
                    width * sx,
                    height * sy,
                    sx.max(sy) * 100.0,
                    if selection.corner_ratio && !selection.uniform && corner {
                        " — Shift: proporção livre"
                    } else {
                        ""
                    }
                ),
            )
        }
        TransformMode::Rotate => {
            let (cx, cy) = frame.center();
            let before = (start.1 - cy).atan2(start.0 - cx);
            let after = (current.1 - cy).atan2(current.0 - cx);
            let mut angle = after - before;
            if shift {
                let step = 15f64.to_radians();
                angle = (angle / step).round() * step;
            }
            let (sin, cos) = angle.sin_cos();
            let transform = [
                cos,
                sin,
                -sin,
                cos,
                cx - (cx * cos - cy * sin),
                cy - (cx * sin + cy * cos),
            ];
            let current_angle = frame.u.1.atan2(frame.u.0).to_degrees();
            (
                transform,
                format!(
                    "Girar: {:+.0}° (ângulo final {:.0}°) — Shift: passos de 15°",
                    angle.to_degrees(),
                    current_angle + angle.to_degrees()
                ),
            )
        }
    }
}

fn is_identity(matrix: &Matrix) -> bool {
    matrix
        .iter()
        .zip(IDENTITY)
        .all(|(value, identity)| (value - identity).abs() < 1e-3)
}

unsafe fn path_polygon(cr: *mut c_void, points: &[(f64, f64)]) {
    cairo_new_path(cr);
    for (index, (x, y)) in points.iter().enumerate() {
        if index == 0 {
            cairo_move_to(cr, *x, *y);
        } else {
            cairo_line_to(cr, *x, *y);
        }
    }
    cairo_close_path(cr);
}

/// Cor de destaque da seleção e opacidade do preenchimento.
const ACCENT: (f64, f64, f64) = (0.17, 0.47, 0.93);
const ACCENT_FILL: f64 = 0.07;
/// Lado das alças de redimensionar, em pixels.
const HANDLE: f64 = 8.0;

/// Contorno contínuo com halo branco por baixo (visível sobre qualquer
/// fundo) e preenchimento translúcido.
unsafe fn outline(cr: *mut c_void, points: &[(f64, f64)], fill_alpha: f64, width: f64) {
    if fill_alpha > 0.0 {
        path_polygon(cr, points);
        cairo_set_source_rgba(cr, ACCENT.0, ACCENT.1, ACCENT.2, fill_alpha);
        cairo_fill_preserve(cr);
        cairo_new_path(cr);
    }
    path_polygon(cr, points);
    cairo_set_source_rgba(cr, 1.0, 1.0, 1.0, 0.85);
    cairo_set_line_width(cr, width + 2.0);
    cairo_stroke(cr);
    path_polygon(cr, points);
    cairo_set_source_rgba(cr, ACCENT.0, ACCENT.1, ACCENT.2, 1.0);
    cairo_set_line_width(cr, width);
    cairo_stroke(cr);
}

/// Tracejado fino (retângulo de seleção, caixa do grupo, destaque).
unsafe fn dotted(cr: *mut c_void, points: &[(f64, f64)], fill_alpha: f64) {
    if fill_alpha > 0.0 {
        path_polygon(cr, points);
        cairo_set_source_rgba(cr, ACCENT.0, ACCENT.1, ACCENT.2, fill_alpha);
        cairo_fill_preserve(cr);
        cairo_new_path(cr);
    }
    path_polygon(cr, points);
    cairo_set_source_rgba(cr, 1.0, 1.0, 1.0, 0.8);
    cairo_set_line_width(cr, 3.0);
    cairo_stroke(cr);
    let dashes = [4.0, 3.0];
    cairo_set_dash(cr, dashes.as_ptr(), dashes.len() as c_int, 0.0);
    path_polygon(cr, points);
    cairo_set_source_rgba(cr, ACCENT.0, ACCENT.1, ACCENT.2, 1.0);
    cairo_set_line_width(cr, 1.2);
    cairo_stroke(cr);
    cairo_set_dash(cr, ptr::null(), 0, 0.0);
}

/// Retângulo de cantos arredondados (caminho).
unsafe fn rounded_rect(cr: *mut c_void, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    cairo_new_path(cr);
    cairo_arc(cr, x + width - radius, y + radius, radius, -FRAC_PI_2, 0.0);
    cairo_arc(
        cr,
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        FRAC_PI_2,
    );
    cairo_arc(cr, x + radius, y + height - radius, radius, FRAC_PI_2, PI);
    cairo_arc(cr, x + radius, y + radius, radius, PI, PI + FRAC_PI_2);
    cairo_close_path(cr);
}

/// Alça de redimensionar: quadrado branco arredondado, borda de destaque e
/// sombra leve.
unsafe fn resize_handle(cr: *mut c_void, (x, y): (f64, f64)) {
    let (left, top) = (x - HANDLE / 2.0, y - HANDLE / 2.0);
    rounded_rect(cr, left, top + 1.0, HANDLE, HANDLE, 2.0);
    cairo_set_source_rgba(cr, 0.0, 0.0, 0.0, 0.22);
    cairo_fill_preserve(cr);
    cairo_new_path(cr);
    rounded_rect(cr, left, top, HANDLE, HANDLE, 2.0);
    cairo_set_source_rgba(cr, 1.0, 1.0, 1.0, 1.0);
    cairo_fill_preserve(cr);
    cairo_set_source_rgba(cr, ACCENT.0, ACCENT.1, ACCENT.2, 1.0);
    cairo_set_line_width(cr, 1.5);
    cairo_stroke(cr);
}

/// Alça de girar: haste fina até um círculo branco com ponto central.
unsafe fn rotation_handle(cr: *mut c_void, top: (f64, f64), handle: (f64, f64)) {
    cairo_new_path(cr);
    cairo_move_to(cr, top.0, top.1);
    cairo_line_to(cr, handle.0, handle.1);
    cairo_set_source_rgba(cr, ACCENT.0, ACCENT.1, ACCENT.2, 0.9);
    cairo_set_line_width(cr, 1.0);
    cairo_stroke(cr);
    cairo_new_path(cr);
    cairo_arc(
        cr,
        handle.0,
        handle.1 + 1.0,
        6.0,
        0.0,
        std::f64::consts::TAU,
    );
    cairo_set_source_rgba(cr, 0.0, 0.0, 0.0, 0.22);
    cairo_fill_preserve(cr);
    cairo_new_path(cr);
    cairo_arc(cr, handle.0, handle.1, 6.0, 0.0, std::f64::consts::TAU);
    cairo_set_source_rgba(cr, 1.0, 1.0, 1.0, 1.0);
    cairo_fill_preserve(cr);
    cairo_set_source_rgba(cr, ACCENT.0, ACCENT.1, ACCENT.2, 1.0);
    cairo_set_line_width(cr, 1.5);
    cairo_stroke(cr);
    cairo_new_path(cr);
    cairo_arc(cr, handle.0, handle.1, 2.0, 0.0, std::f64::consts::TAU);
    cairo_fill_preserve(cr);
    cairo_new_path(cr);
}

/// Desenha sobre a prévia a seleção (moldura orientada, alças de tamanho e
/// de rotação), o grupo selecionado e os retângulos de seleção e destaque.
unsafe extern "C" fn on_image_draw(
    _widget: *mut c_void,
    cr: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    let snapshot = STATE.with(|cell| {
        let borrowed = cell.try_borrow().ok()?;
        let app = borrowed.as_ref()?;
        Some((
            PageMapping::of(app)?,
            app.selection.clone(),
            app.drag_transform,
            app.rubber_band,
        ))
    });
    let Some((mapping, selection, transform, rubber_band)) = snapshot else {
        return 0;
    };
    cairo_save(cr);
    menu_contexto::draw_page_selection(cr, mapping);
    selecao_multipla::draw(cr, mapping);
    if let Some([x0, y0, x1, y1]) = rubber_band {
        let points: Vec<_> = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            .into_iter()
            .map(|point| mapping.to_widget(point))
            .collect();
        dotted(cr, &points, 0.10);
    }
    if let Some(selection) = selection {
        let frame = selection.frame.transformed(&transform);
        // Moldura com 5 px de folga para fora, nos dois eixos da tela.
        let center = mapping.to_widget(frame.center());
        let points: Vec<_> = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            .into_iter()
            .map(|(s, t)| {
                let (x, y) = mapping.to_widget(frame.point(s, t));
                let (dx, dy) = (x - center.0, y - center.1);
                let length = dx.hypot(dy).max(1e-6);
                (x + dx / length * 5.0, y + dy / length * 5.0)
            })
            .collect();
        outline(cr, &points, ACCENT_FILL, 1.5);
        if selection.rotatable {
            let (handle, top) = rotate_handle(&frame, mapping);
            rotation_handle(cr, top, handle);
        }
        for (hx, hy) in resize_handles(&selection) {
            resize_handle(cr, mapping.to_widget(frame.point(hx, hy)));
        }
    }
    cairo_restore(cr);
    0
}

fn current_tool() -> ToolMode {
    STATE.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|app| app.tool)
            .unwrap_or(ToolMode::None)
    })
}

/// Mensagem sobre forms compartilhados copiados para preservar outras ocorrências.
fn cloned_note(outcome: &TextEditOutcome) -> String {
    match outcome.cloned_forms {
        0 => String::new(),
        _ => {
            " — cópia independente criada (o original continua igual nas outras ocorrências)".into()
        }
    }
}

/// Trecho de texto editável sob o ponto: palavra clicada e texto do trecho.
fn locate_text(point: (f64, f64)) -> AppResult<(String, String)> {
    let Some(hit) = with_session(|session| session.text_at(point.0, point.1))? else {
        return Err("Nenhum texto aqui para editar".into());
    };
    let Some((run, editable)) = with_session(|session| session.text_run_at(point, &hit.word))?
    else {
        return Err(format!(
            "“{}” não foi localizado no conteúdo da página",
            hit.line
        ));
    };
    if !editable {
        return Err(format!(
            "“{}” não pode ser editado (fonte com codificação não suportada)",
            hit.line
        ));
    }
    Ok((hit.word, run))
}

/// Edita só o trecho de texto sob o ponto, no lugar (sobre o próprio texto);
/// texto girado usa a caixa de texto.
fn edit_text_at(point: (f64, f64)) -> AppResult<()> {
    let (word, run) = locate_text(point)?;
    if edicao_inline::begin(point, &word, &run)? {
        status("Editando texto — Enter ou clique fora confirma, Esc cancela");
        return Ok(());
    }
    edit_text_in_dialog(point)
}

/// Edita o trecho sob o ponto numa caixa de texto modal.
fn edit_text_in_dialog(point: (f64, f64)) -> AppResult<()> {
    edicao_inline::commit();
    let (word, run) = locate_text(point)?;
    let Some(new_text) = dialogos::edit_text(&run) else {
        status("Edição de texto cancelada");
        return Ok(());
    };
    apply_text_edit(point, &word, &run, &new_text)
}

/// Grava `new_text` no lugar do trecho `original` sob o ponto.
fn apply_text_edit(point: (f64, f64), word: &str, original: &str, new_text: &str) -> AppResult<()> {
    if new_text == original {
        return Ok(());
    }
    let outcome = with_session(|session| session.replace_text_at(point, word, new_text))?;
    render()?;
    // A largura do texto mudou: recalcula a moldura.
    set_selection(with_session(|session| session.select_at(point))?);
    status(&format!("Texto editado{}", cloned_note(&outcome)));
    Ok(())
}

/// Nomes dos campos de formulário já existentes (para não repetir).
fn field_names() -> AppResult<Vec<String>> {
    Ok(with_session(|s| s.list_form_fields())?
        .into_iter()
        .map(|(name, ..)| name)
        .collect())
}

/// Depois de inserir: volta à ferramenta Selecionar e seleciona o item novo.
/// Sem isso o clique seguinte (para ajustar o item) inseria outro e
/// reabria os diálogos.
fn finish_insertion(select_point: (f64, f64), message: &str) -> AppResult<()> {
    render()?;
    set_tool(ToolMode::None);
    if let Ok(Some(selection)) = with_session(|s| s.select_at(select_point)) {
        set_selection(Some(selection));
    }
    status(&format!("{message} — ferramenta Selecionar ativa"));
    Ok(())
}

/// Insere o elemento do modo atual na posição clicada.
fn place_at(point: (f64, f64)) -> AppResult<()> {
    let (x, y) = point;
    match current_tool() {
        ToolMode::None | ToolMode::Hand => Ok(()),
        ToolMode::Text => {
            let Some(text) = prompt_text("Texto a inserir:") else {
                return Ok(());
            };
            let Some(size) = prompt_text_default("Tamanho da fonte:", "16") else {
                return Ok(());
            };
            let size: f64 = size
                .trim()
                .replace(',', ".")
                .parse()
                .map_err(|_| "Tamanho inválido")?;
            with_session(|s| s.insert_visible_text_at(&text, x, y, size))?;
            finish_insertion((x + size * 0.3, y + size * 0.3), "Texto inserido")
        }
        ToolMode::Note => {
            let Some(text) = prompt_text("Texto da nota:") else {
                return Ok(());
            };
            with_session(|s| s.add_text_note_at(&text, x, y))?;
            finish_insertion(point, "Nota adicionada")
        }
        ToolMode::Image => {
            let path = STATE.with(|cell| {
                cell.borrow()
                    .as_ref()
                    .and_then(|app| app.pending_image.clone())
            });
            let Some(path) = path else {
                return Err("Escolha a imagem antes de clicar na página".into());
            };
            let (page_width, page_height) = with_session(|s| s.page_dimensions())?;
            let width = (page_width * 0.3).max(24.0);
            let height = (page_height * 0.2).max(24.0);
            with_session(|s| s.insert_image_at(&path, x, y, width, height))?;
            finish_insertion((x + width / 2.0, y + height / 2.0), "Imagem inserida")
        }
        ToolMode::TextField => {
            let Some(label) = prompt_text("Nome do campo:") else {
                return Ok(());
            };
            let name = formularios::field_name(&label, "campo", &field_names()?);
            let Some(value) = prompt_text("Valor inicial (pode ficar vazio):") else {
                return Ok(());
            };
            let multiline = confirm("Campo multilinha?");
            let (width, height) = (220.0, if multiline { 60.0 } else { 28.0 });
            with_session(|s| s.insert_text_field(&name, &value, x, y, width, height, multiline))?;
            finish_insertion(
                (x + width / 2.0, y + height / 2.0),
                &format!("Campo de texto “{name}” criado"),
            )
        }
        ToolMode::Checkbox => {
            let Some(label) = prompt_text("Rótulo da caixa (texto ao lado; pode ficar vazio):")
            else {
                return Ok(());
            };
            let name = formularios::field_name(&label, "caixa", &field_names()?);
            let size = 18.0;
            with_session(|s| s.insert_labeled_checkbox(&name, &label, false, x, y, size))?;
            finish_insertion(
                (x + size / 2.0, y + size / 2.0),
                &format!("Caixa de seleção “{name}” criada"),
            )
        }
        ToolMode::Radio => {
            let Some(group) = prompt_text("Nome do grupo (ex.: Forma de pagamento):") else {
                return Ok(());
            };
            let Some(options) = prompt_text("Opções separadas por vírgula (ex.: Sim, Não):")
            else {
                return Ok(());
            };
            let options = formularios::parse_options(&options);
            if options.len() < 2 {
                return Err(
                    "Informe ao menos duas opções diferentes, separadas por vírgula".into(),
                );
            }
            let name = formularios::field_name(&group, "opcoes", &field_names()?);
            let size = 18.0;
            let labels: Vec<(String, f64, f64, f64, f64)> = options
                .into_iter()
                .enumerate()
                .map(|(index, label)| (label, x, y - index as f64 * 28.0, size, size))
                .collect();
            with_session(|s| s.insert_radio_group(&name, &labels, None))?;
            finish_insertion(
                (x + size / 2.0, y + size / 2.0),
                &format!("Grupo de rádio “{name}” criado com {} opções", labels.len()),
            )
        }
        ToolMode::Choice => {
            let Some(label) = prompt_text("Nome do campo de escolha:") else {
                return Ok(());
            };
            let Some(options) = prompt_text("Opções separadas por vírgula:") else {
                return Ok(());
            };
            let options = formularios::parse_options(&options);
            let Some(first) = options.first().cloned() else {
                return Err("Informe ao menos uma opção".into());
            };
            let name = formularios::field_name(&label, "escolha", &field_names()?);
            let combo = confirm("Usar lista suspensa (combo)? Não = lista aberta");
            let (width, height) = (180.0, if combo { 28.0 } else { 60.0 });
            // Nasce com a primeira opção: um campo vazio parecia não ter sido inserido.
            with_session(|s| {
                s.insert_choice_field(&name, &first, &options, x, y, width, height, combo)
            })?;
            finish_insertion(
                (x + width / 2.0, y + height / 2.0),
                &format!("Campo de escolha “{name}” criado"),
            )
        }
        ToolMode::Highlight => Ok(()),
    }
}

unsafe extern "C" fn on_button_press(
    widget: *mut c_void,
    event: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    let _trace = rustpdf::trace(|| "clique (button-press)".into());
    let mut button = 0u32;
    if gdk_event_get_button(event, &mut button) == 0 {
        return 0;
    }
    // Clique fora do campo de edição no lugar confirma o texto.
    if edicao_inline::active() {
        edicao_inline::commit();
        return 1;
    }
    if button == 3 && current_tool() != ToolMode::Hand {
        clear_drag();
        return match preview_point(widget, event, false) {
            Ok(point) => c_int::from(menu_contexto::open(event, point)),
            Err(_) => 0,
        };
    }
    if button != 1 {
        return 0;
    }
    menu_contexto::clear_page_selection();
    // Alças podem ficar um pouco fora da página: o ponto é limitado à borda.
    let Ok(point) = preview_point(widget, event, true) else {
        return 0;
    };
    let inside_page = preview_point(widget, event, false).is_ok();
    drop(rustpdf::trace(|| {
        format!(
            "  ponto do clique: {:.1} {:.1} pt (dentro da página: {inside_page})",
            point.0, point.1
        )
    }));
    if current_tool() == ToolMode::Hand {
        shell::hand_press(event);
        return 1;
    }
    match current_tool() {
        ToolMode::Highlight if inside_page => {
            STATE.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    app.drag = Some(DragState::Highlight { start: point });
                }
            });
            set_rubber_band(Some([point.0, point.1, point.0, point.1]));
            status("Arraste até o canto oposto da área e solte o botão.");
            1
        }
        ToolMode::None if gdk_event_get_event_type(event) == GDK_DOUBLE_BUTTON_PRESS => {
            // Clique duplo: campo de formulário é preenchido; texto, editado.
            clear_drag();
            let result = match with_session(|session| session.form_field_at(point)) {
                Ok(Some(field)) => preencher::fill(field, point),
                Ok(None) => edit_text_at(point),
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                status(&error);
            }
            1
        }
        ToolMode::None if selection_mode(event) != SelectionMode::Replace => {
            // Ctrl/Ctrl+Shift: clique inclui/retira o objeto; arraste, o retângulo.
            if !inside_page {
                return 0;
            }
            start_drag(DragState::Marquee {
                start: point,
                mode: selection_mode(event),
                gate: DragGate::new(event_coords(event).unwrap_or_default()),
            });
            1
        }
        ToolMode::None if selecao_multipla::group_contains(point) => {
            start_drag(DragState::MoveGroup {
                start: point,
                gate: DragGate::new(event_coords(event).unwrap_or_default()),
            });
            1
        }
        ToolMode::None => {
            let handle = event_image_coords(widget, event).and_then(handle_at);
            let current = current_selection();
            let (mode, selection) = match (handle, current) {
                (Some(mode), Some(selection)) => (mode, selection),
                // Clique dentro da seleção atual: mover, sem nova busca.
                (None, Some(selection)) if selection.frame.contains(point, 2.0) => {
                    (TransformMode::Move, selection)
                }
                _ if !inside_page => {
                    set_selection(None);
                    return 0;
                }
                _ => match with_session(|session| session.select_at(point)) {
                    Ok(Some(selection)) => {
                        set_selection(Some(selection.clone()));
                        (TransformMode::Move, selection)
                    }
                    Ok(None) => {
                        // Área vazia: arrastar desenha o retângulo de seleção.
                        set_selection(None);
                        start_drag(DragState::Marquee {
                            start: point,
                            mode: SelectionMode::Replace,
                            gate: DragGate::new(event_coords(event).unwrap_or_default()),
                        });
                        status("Arraste para selecionar vários objetos (Ctrl inclui, Ctrl+Shift retira)");
                        return 1;
                    }
                    Err(error) => {
                        set_selection(None);
                        status(&error);
                        return 1;
                    }
                },
            };
            let mut hints = vec!["arraste para mover"];
            if selection.resizable {
                hints.push("alças redimensionam");
            }
            if selection.rotatable {
                hints.push("alça ○ gira");
            }
            match &selection.target {
                rustpdf::SelectionTarget::Text { .. } => hints.push("clique duplo edita"),
                rustpdf::SelectionTarget::Annotation { kind, .. } if kind == "campo" => {
                    hints.push("clique duplo preenche")
                }
                _ => {}
            }
            status(&format!("{} — {}", selection.label(), hints.join("; ")));
            STATE.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    app.drag = Some(DragState::Transform {
                        mode,
                        start: point,
                        selection,
                        gate: DragGate::new(event_coords(event).unwrap_or_default()),
                    });
                }
            });
            1
        }
        _ if !inside_page => 0,
        _ => {
            // Ferramentas de inserção: posicionam no clique.
            if let Err(error) = place_at(point) {
                status(&error);
            }
            1
        }
    }
}

unsafe extern "C" fn on_motion(
    widget: *mut c_void,
    event: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    if shell::hand_motion(event) {
        return 0;
    }
    let drag = STATE.with(|cell| cell.borrow().as_ref().and_then(|app| app.drag.clone()));
    let Some(drag) = drag else {
        // Sem arraste: cursor indica alça, área de mover ou nada.
        if current_tool() == ToolMode::Hand {
            set_cursor("grab");
            return 0;
        }
        if current_tool() != ToolMode::None {
            // Ferramenta de inserção: a cruz indica que o clique vai inserir.
            set_cursor("crosshair");
            return 0;
        }
        let cursor = match (
            event_image_coords(widget, event).and_then(handle_at),
            current_selection(),
        ) {
            (Some(TransformMode::Rotate), _) => "grab",
            (Some(TransformMode::Resize { hx, hy }), Some(selection)) => {
                resize_cursor(&selection, hx, hy)
            }
            (_, Some(selection))
                if preview_point(widget, event, false)
                    .is_ok_and(|point| selection.frame.contains(point, 2.0)) =>
            {
                "move"
            }
            _ => "",
        };
        set_cursor(cursor);
        return 0;
    };
    let Ok(current) = preview_point(widget, event, true) else {
        return 0;
    };
    match drag {
        DragState::Marquee {
            start,
            mode,
            mut gate,
        } => {
            let was_dragging = gate.dragging();
            if !event_coords(event).is_some_and(|position| gate.update(position)) {
                return 0;
            }
            start_drag(DragState::Marquee { start, mode, gate });
            if !was_dragging {
                if let Err(error) = selecao_multipla::begin_marquee(mode) {
                    status(&error);
                    clear_drag();
                    return 0;
                }
            }
            selecao_multipla::update_marquee(regras_selecao::rect_between(start, current));
        }
        DragState::MoveGroup { start, mut gate } => {
            if !event_coords(event).is_some_and(|position| gate.update(position)) {
                return 0;
            }
            start_drag(DragState::MoveGroup { start, gate });
            let (dx, dy) = (current.0 - start.0, current.1 - start.1);
            selecao_multipla::set_preview(textmove::translation(dx, dy));
            status(&format!(
                "Mover grupo: {dx:.0} {dy:.0} pt (solte para aplicar)"
            ));
        }
        DragState::Highlight { start } => {
            let (x, y, width, height) = rect_from_points(start, current);
            set_rubber_band(Some([x, y, x + width, y + height]));
            status(&format!(
                "Destaque: {x:.0} {y:.0}  {width:.0}×{height:.0} pt (solte para aplicar)"
            ));
        }
        DragState::Transform {
            mode,
            start,
            selection,
            mut gate,
        } => {
            let dragging = event_coords(event).is_some_and(|position| gate.update(position));
            STATE.with(|cell| {
                if let Some(DragState::Transform { gate: stored, .. }) =
                    cell.borrow_mut().as_mut().and_then(|app| app.drag.as_mut())
                {
                    *stored = gate;
                }
            });
            if !dragging {
                return 0;
            }
            if mode == TransformMode::Rotate {
                set_cursor("grabbing");
            }
            let (transform, message) =
                drag_transform(mode, &selection, start, current, shift_pressed(event));
            set_drag_transform(transform);
            status(&message);
        }
    }
    0
}

unsafe extern "C" fn on_button_release(
    widget: *mut c_void,
    event: *mut c_void,
    _data: *mut c_void,
) -> c_int {
    let _trace = rustpdf::trace(|| "soltar (button-release)".into());
    if shell::hand_release() {
        return 1;
    }
    // Clique duplo num combo/lista: a lista abre agora, com o botão solto.
    if preencher::open_pending(event) {
        return 1;
    }
    let mut button = 0u32;
    if gdk_event_get_button(event, &mut button) == 0 || button != 1 {
        return 0;
    }
    let drag = STATE.with(|cell| cell.borrow().as_ref().and_then(|app| app.drag.clone()));
    let Some(drag) = drag else {
        return 0;
    };
    let result = (|| -> AppResult<()> {
        let end = preview_point(widget, event, true)?;
        match drag {
            DragState::Marquee {
                start,
                mode,
                mut gate,
            } => {
                if event_coords(event).is_some_and(|position| gate.update(position))
                    && selecao_multipla::marquee_active()
                {
                    selecao_multipla::update_marquee(regras_selecao::rect_between(start, end));
                    selecao_multipla::finish_marquee();
                } else {
                    selecao_multipla::cancel_marquee();
                    if mode != SelectionMode::Replace {
                        selecao_multipla::toggle_at(start, mode)?;
                    } else {
                        status("Nenhum elemento aqui. Ative uma ferramenta para inserir.");
                    }
                }
                Ok(())
            }
            DragState::MoveGroup { start, mut gate } => {
                if !event_coords(event).is_some_and(|position| gate.update(position)) {
                    selecao_multipla::set_preview(IDENTITY);
                    return Ok(());
                }
                let transform = textmove::translation(end.0 - start.0, end.1 - start.1);
                if let Err(error) = selecao_multipla::transform_group(&transform) {
                    selecao_multipla::set_preview(IDENTITY);
                    return Err(error);
                }
                Ok(())
            }
            DragState::Highlight { start } => {
                let (x, y, width, height) = rect_from_points(start, end);
                if width <= 0.5 || height <= 0.5 {
                    return Err("Arraste uma área maior para destacar".into());
                }
                with_session(|session| session.highlight_at(x, y, width, height))?;
                render()?;
                status("Área destacada. Continue arrastando ou desative o modo.");
                Ok(())
            }
            DragState::Transform {
                mode,
                start,
                selection,
                mut gate,
            } => {
                // Soltar sem passar do limiar é só um clique: nada muda.
                if !event_coords(event).is_some_and(|position| gate.update(position)) {
                    return Ok(());
                }
                let (transform, _) =
                    drag_transform(mode, &selection, start, end, shift_pressed(event));
                if is_identity(&transform) {
                    return Ok(());
                }
                let outcome =
                    with_session(|session| session.transform_selection(&selection, &transform))?;
                render()?;
                set_selection(Some(selection.transformed(&transform)));
                let action = match mode {
                    TransformMode::Move => "movido",
                    TransformMode::Resize { .. } => "redimensionado",
                    TransformMode::Rotate => "girado",
                };
                let mut message = format!("{}: {action}", selection.label());
                if outcome.whole_block {
                    message.push_str(" (bloco de texto inteiro: trechos encadeados)");
                }
                if outcome.cloned_forms > 0 {
                    message.push_str(
                        " — cópia independente criada (o original continua igual nas outras ocorrências)",
                    );
                }
                status(&message);
                Ok(())
            }
        }
    })();
    clear_drag();
    if let Err(error) = result {
        status(&error);
    }
    1
}

fn rect_from_points(first: (f64, f64), second: (f64, f64)) -> (f64, f64, f64, f64) {
    let x = first.0.min(second.0);
    let y = first.1.min(second.1);
    let width = (second.0 - first.0).abs();
    let height = (second.1 - first.1).abs();
    (x, y, width, height)
}

fn main() {
    rustpdf::prefer_bundled_tools();
    unsafe {
        gtk_init(ptr::null_mut(), ptr::null_mut());
        let state = shell::build();
        let window = state.window;
        STATE.with(|cell| *cell.borrow_mut() = Some(state));
        gtk_widget_show_all(window);
        if let Err(error) = render() {
            status(&error);
        }
        shell::sync_tools();
        if let Some(path) = std::env::args_os().nth(1) {
            if let Err(e) = set_session(Path::new(&path)) {
                status(&e);
            }
        }
        gtk_main();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_filters_and_extensions() {
        assert_eq!(case_insensitive_pattern("pdf"), "*.[pP][dD][fF]");
        assert_eq!(case_insensitive_pattern("jpeg"), "*.[jJ][pP][eE][gG]");
        assert_eq!(FileKind::Pdf.filters()[0].1, ["pdf"]);
        assert!(FileKind::Image.filters()[0].1.contains(&"jpg"));
        assert!(FileKind::Image.filters().last().unwrap().1.is_empty());
        let pdf = |name: &str| with_extension(PathBuf::from(name), FileKind::Pdf);
        assert_eq!(pdf("/tmp/relatorio"), PathBuf::from("/tmp/relatorio.pdf"));
        assert_eq!(
            pdf("/tmp/relatorio.PDF"),
            PathBuf::from("/tmp/relatorio.PDF")
        );
        assert_eq!(pdf("/tmp/v1.2"), PathBuf::from("/tmp/v1.2.pdf"));
        assert_eq!(
            with_extension(PathBuf::from("pagina"), FileKind::Png),
            PathBuf::from("pagina.png")
        );
        assert_eq!(
            with_extension(PathBuf::from("foto"), FileKind::Image),
            PathBuf::from("foto")
        );
        for format in ExportFormat::ALL {
            let kind = FileKind::Export(format);
            assert_eq!(kind.filters()[0].1, [format.extension()], "{format:?}");
            let named = with_extension(PathBuf::from("/tmp/saida"), kind);
            assert_eq!(
                named,
                PathBuf::from(format!("/tmp/saida.{}", format.extension()))
            );
        }
    }

    #[test]
    fn export_menu_actions_cover_all_formats_and_match_dispatch_range() {
        for format in ExportFormat::ALL {
            assert!(
                exportacao::MENU.contains(&format),
                "formato {format:?} ausente de exportacao::MENU"
            );
        }
        for (index, &format) in exportacao::MENU.iter().enumerate() {
            let action = exportacao::FIRST_ACTION + index;
            assert!(
                exportacao::export_actions().contains(&action),
                "ação {action} ({format:?}) fora de export_actions()"
            );
            assert_eq!(exportacao::action_format(action), Some(format));
        }
        assert_eq!(
            exportacao::export_actions(),
            exportacao::FIRST_ACTION..exportacao::CANCEL_ACTION
        );
    }
}
