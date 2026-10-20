// SPDX-License-Identifier: AGPL-3.0-or-later
//! Escrita de anotações PDF pela API C de qpdf, chamada diretamente de Rust.
use crate::AppResult;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;

#[link(name = "qpdf")]
extern "C" {
    fn qpdf_init() -> *mut c_void;
    fn qpdf_cleanup(qpdf: *mut *mut c_void);
    fn qpdf_read(qpdf: *mut c_void, filename: *const c_char, password: *const c_char) -> c_int;
    fn qpdf_init_write(qpdf: *mut c_void, filename: *const c_char) -> c_int;
    fn qpdf_write(qpdf: *mut c_void) -> c_int;
    fn qpdf_has_error(qpdf: *mut c_void) -> c_int;
    fn qpdf_get_error(qpdf: *mut c_void) -> *mut c_void;
    fn qpdf_get_error_full_text(qpdf: *mut c_void, error: *mut c_void) -> *const c_char;
    fn qpdf_get_page_n(qpdf: *mut c_void, index: usize) -> u32;
    fn qpdf_get_root(qpdf: *mut c_void) -> u32;
    fn qpdf_push_inherited_attributes_to_page(qpdf: *mut c_void) -> c_int;
    fn qpdf_oh_get_key(qpdf: *mut c_void, object: u32, key: *const c_char) -> u32;
    fn qpdf_oh_is_array(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_is_dictionary(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_is_stream(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_is_string(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_is_name(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_is_name_and_equals(qpdf: *mut c_void, object: u32, name: *const c_char) -> c_int;
    fn qpdf_oh_get_dict(qpdf: *mut c_void, object: u32) -> u32;
    fn qpdf_oh_get_utf8_value(qpdf: *mut c_void, object: u32) -> *const c_char;
    fn qpdf_oh_get_name(qpdf: *mut c_void, object: u32) -> *const c_char;
    fn qpdf_oh_get_array_n_items(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_get_array_item(qpdf: *mut c_void, object: u32, index: c_int) -> u32;
    fn qpdf_oh_get_numeric_value(qpdf: *mut c_void, object: u32) -> f64;
    fn qpdf_oh_new_array(qpdf: *mut c_void) -> u32;
    fn qpdf_oh_new_dictionary(qpdf: *mut c_void) -> u32;
    fn qpdf_oh_new_null(qpdf: *mut c_void) -> u32;
    fn qpdf_oh_new_name(qpdf: *mut c_void, name: *const c_char) -> u32;
    fn qpdf_oh_new_string(qpdf: *mut c_void, value: *const c_char) -> u32;
    fn qpdf_oh_new_stream(qpdf: *mut c_void) -> u32;
    fn qpdf_oh_new_integer(qpdf: *mut c_void, value: i64) -> u32;
    fn qpdf_oh_new_real_from_double(qpdf: *mut c_void, value: f64, decimals: c_int) -> u32;
    fn qpdf_oh_new_unicode_string(qpdf: *mut c_void, value: *const c_char) -> u32;
    fn qpdf_oh_append_item(qpdf: *mut c_void, array: u32, item: u32);
    fn qpdf_oh_get_object_id(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_replace_key(qpdf: *mut c_void, object: u32, key: *const c_char, item: u32);
    fn qpdf_make_indirect_object(qpdf: *mut c_void, object: u32) -> u32;
    fn qpdf_oh_replace_stream_data(
        qpdf: *mut c_void,
        stream: u32,
        data: *const u8,
        len: usize,
        filter: u32,
        decode_parms: u32,
    );
}

struct Document(*mut c_void);

impl Document {
    fn new() -> AppResult<Self> {
        let ptr = unsafe { qpdf_init() };
        if ptr.is_null() {
            Err("qpdf_init falhou".into())
        } else {
            Ok(Self(ptr))
        }
    }

    fn check(&self, code: c_int) -> AppResult<()> {
        if code & 2 == 0 && unsafe { qpdf_has_error(self.0) } == 0 {
            return Ok(());
        }
        unsafe {
            let error = qpdf_get_error(self.0);
            if error.is_null() {
                return Err("Erro interno de qpdf".into());
            }
            let detail = qpdf_get_error_full_text(self.0, error);
            if detail.is_null() {
                return Err("Erro interno de qpdf".into());
            }
            Err(CStr::from_ptr(detail).to_string_lossy().into_owned())
        }
    }

    fn set(&self, object: u32, key: &CStr, value: u32) {
        unsafe { qpdf_oh_replace_key(self.0, object, key.as_ptr(), value) }
    }

    fn name(&self, name: &CStr) -> u32 {
        unsafe { qpdf_oh_new_name(self.0, name.as_ptr()) }
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        unsafe { qpdf_cleanup(&mut self.0) }
    }
}

pub fn add_text_note_at(
    input: &Path,
    output: &Path,
    page_number: usize,
    text: &str,
    coordinates: [f64; 4],
) -> AppResult<()> {
    if page_number == 0 {
        return Err("Número de página inválido".into());
    }
    if coordinates.iter().any(|value| !value.is_finite())
        || coordinates[2] <= coordinates[0]
        || coordinates[3] <= coordinates[1]
    {
        return Err("Posição da nota inválida".into());
    }
    if text.trim().is_empty() {
        return Err("A anotação está vazia".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let text = CString::new(text).map_err(|_| "A anotação contém byte NUL")?;
    let author = c"RustPDF";
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let mut annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0 {
        annots = unsafe { qpdf_oh_new_array(doc.0) };
        doc.set(page, c"/Annots", annots);
    }
    let annotation = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(annotation, c"/Type", doc.name(c"/Annot"));
    doc.set(annotation, c"/Subtype", doc.name(c"/Text"));
    doc.set(annotation, c"/Name", doc.name(c"/Comment"));
    doc.set(annotation, c"/Contents", unsafe {
        qpdf_oh_new_unicode_string(doc.0, text.as_ptr())
    });
    doc.set(annotation, c"/T", unsafe {
        qpdf_oh_new_unicode_string(doc.0, author.as_ptr())
    });
    doc.set(annotation, c"/F", unsafe { qpdf_oh_new_integer(doc.0, 4) });
    let rect = unsafe { qpdf_oh_new_array(doc.0) };
    for value in coordinates {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, value, 3) };
        unsafe { qpdf_oh_append_item(doc.0, rect, number) };
    }
    doc.set(annotation, c"/Rect", rect);
    let indirect = unsafe { qpdf_make_indirect_object(doc.0, annotation) };
    unsafe { qpdf_oh_append_item(doc.0, annots, indirect) };
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

pub fn add_highlight_at(
    input: &Path,
    output: &Path,
    page_number: usize,
    coordinates: [f64; 4],
) -> AppResult<()> {
    if page_number == 0
        || coordinates.iter().any(|value| !value.is_finite())
        || coordinates[2] <= coordinates[0]
        || coordinates[3] <= coordinates[1]
    {
        return Err("Área de destaque inválida".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let mut annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0 {
        annots = unsafe { qpdf_oh_new_array(doc.0) };
        doc.set(page, c"/Annots", annots);
    }
    let annotation = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(annotation, c"/Type", doc.name(c"/Annot"));
    doc.set(annotation, c"/Subtype", doc.name(c"/Highlight"));
    doc.set(annotation, c"/F", unsafe { qpdf_oh_new_integer(doc.0, 4) });
    let rect = unsafe { qpdf_oh_new_array(doc.0) };
    for value in coordinates {
        unsafe {
            qpdf_oh_append_item(doc.0, rect, qpdf_oh_new_real_from_double(doc.0, value, 3));
        }
    }
    doc.set(annotation, c"/Rect", rect);
    let [left, bottom, right, top] = coordinates;
    let quad = unsafe { qpdf_oh_new_array(doc.0) };
    for value in [left, top, right, top, left, bottom, right, bottom] {
        unsafe {
            qpdf_oh_append_item(doc.0, quad, qpdf_oh_new_real_from_double(doc.0, value, 3));
        }
    }
    doc.set(annotation, c"/QuadPoints", quad);
    let color = unsafe { qpdf_oh_new_array(doc.0) };
    for value in [1.0, 1.0, 0.0] {
        unsafe {
            qpdf_oh_append_item(doc.0, color, qpdf_oh_new_real_from_double(doc.0, value, 3));
        }
    }
    doc.set(annotation, c"/C", color);
    doc.set(annotation, c"/CA", unsafe {
        qpdf_oh_new_real_from_double(doc.0, 0.4, 3)
    });
    let indirect = unsafe { qpdf_make_indirect_object(doc.0, annotation) };
    unsafe { qpdf_oh_append_item(doc.0, annots, indirect) };
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

pub fn crop_page_inset(
    input: &Path,
    output: &Path,
    page_number: usize,
    margin: f64,
) -> AppResult<()> {
    if page_number == 0 || !margin.is_finite() || margin < 0.0 {
        return Err("Página ou margem inválida".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    doc.check(unsafe { qpdf_push_inherited_attributes_to_page(doc.0) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let mut box_array = unsafe { qpdf_oh_get_key(doc.0, page, c"/CropBox".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, box_array) } == 0 {
        box_array = unsafe { qpdf_oh_get_key(doc.0, page, c"/MediaBox".as_ptr()) };
    }
    if unsafe { qpdf_oh_is_array(doc.0, box_array) } == 0
        || unsafe { qpdf_oh_get_array_n_items(doc.0, box_array) } != 4
    {
        return Err("A página não tem caixa retangular válida".into());
    }
    let mut edges = [0.0; 4];
    for (index, edge) in edges.iter_mut().enumerate() {
        let item = unsafe { qpdf_oh_get_array_item(doc.0, box_array, index as c_int) };
        *edge = unsafe { qpdf_oh_get_numeric_value(doc.0, item) };
    }
    doc.check(0)?;
    if edges.iter().any(|value| !value.is_finite())
        || edges[2] - edges[0] <= 2.0 * margin
        || edges[3] - edges[1] <= 2.0 * margin
    {
        return Err("Margem maior que a página".into());
    }
    let crop = unsafe { qpdf_oh_new_array(doc.0) };
    for value in [
        edges[0] + margin,
        edges[1] + margin,
        edges[2] - margin,
        edges[3] - margin,
    ] {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, value, 3) };
        unsafe { qpdf_oh_append_item(doc.0, crop, number) };
    }
    doc.set(page, c"/CropBox", crop);
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

fn ensure_form_font(doc: &Document, form: u32) -> u32 {
    let font = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(font, c"/Type", doc.name(c"/Font"));
    doc.set(font, c"/Subtype", doc.name(c"/Type1"));
    doc.set(font, c"/BaseFont", doc.name(c"/Helvetica"));
    doc.set(font, c"/Encoding", doc.name(c"/WinAnsiEncoding"));
    let mut resources = unsafe { qpdf_oh_get_key(doc.0, form, c"/DR".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, resources) } == 0 {
        resources = unsafe { qpdf_oh_new_dictionary(doc.0) };
        doc.set(form, c"/DR", resources);
    }
    let mut fonts = unsafe { qpdf_oh_get_key(doc.0, resources, c"/Font".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, fonts) } == 0 {
        fonts = unsafe { qpdf_oh_new_dictionary(doc.0) };
        doc.set(resources, c"/Font", fonts);
    }
    doc.set(fonts, c"/RustPDFHelv", font);
    resources
}

fn set_text_appearance(
    doc: &Document,
    widget: u32,
    resources: u32,
    rect: [f64; 4],
    value: &CStr,
) -> AppResult<()> {
    doc.set(widget, c"/V", unsafe {
        qpdf_oh_new_unicode_string(doc.0, value.as_ptr())
    });
    doc.set(widget, c"/DA", unsafe {
        qpdf_oh_new_string(doc.0, c"/RustPDFHelv 12 Tf 0 g".as_ptr())
    });
    let appearance = unsafe { qpdf_oh_new_stream(doc.0) };
    let ap_dict = unsafe { qpdf_oh_get_dict(doc.0, appearance) };
    doc.set(ap_dict, c"/Type", doc.name(c"/XObject"));
    doc.set(ap_dict, c"/Subtype", doc.name(c"/Form"));
    doc.set(ap_dict, c"/Resources", resources);
    let width = rect[2] - rect[0];
    let height = rect[3] - rect[1];
    let bbox = unsafe { qpdf_oh_new_array(doc.0) };
    for coordinate in [0.0, 0.0, width, height] {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, coordinate, 3) };
        unsafe { qpdf_oh_append_item(doc.0, bbox, number) };
    }
    doc.set(ap_dict, c"/BBox", bbox);
    let text = value.to_string_lossy();
    let mut content = format!(
        "q 0 0 {width:.3} {height:.3} re W n 1 1 1 rg 0 0 {width:.3} {height:.3} re f 0 0 0 RG 0.8 w 0.4 0.4 {:.3} {:.3} re S BT /RustPDFHelv 12 Tf 0 g",
        width - 0.8,
        height - 0.8
    )
    .into_bytes();
    let line_height = 14.0;
    let lines: Vec<&str> = text.split('\n').collect();
    let total = line_height * lines.len() as f64;
    let start_y = if total < height {
        (height - total) / 2.0 + total - 12.0
    } else {
        height - 12.0
    };
    for (index, line) in lines.iter().enumerate() {
        let y = start_y - index as f64 * line_height;
        content.extend_from_slice(format!(" 1 0 0 1 4 {y:.3} Tm (").as_bytes());
        content.extend_from_slice(&crate::ocr::pdf_text(line));
        content.extend_from_slice(b") Tj");
    }
    content.extend_from_slice(b" ET Q\n");
    let null = unsafe { qpdf_oh_new_null(doc.0) };
    unsafe {
        qpdf_oh_replace_stream_data(
            doc.0,
            appearance,
            content.as_ptr(),
            content.len(),
            null,
            null,
        )
    };
    doc.check(0)?;
    let ap = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(ap, c"/N", appearance);
    doc.set(widget, c"/AP", ap);
    doc.check(0)
}

pub fn add_text_field(
    input: &Path,
    output: &Path,
    page_number: usize,
    name: &str,
    value: &str,
    rect: [f64; 4],
    multiline: bool,
) -> AppResult<()> {
    if page_number == 0 || name.trim().is_empty() || name.contains('.') {
        return Err("Nome de campo inválido".into());
    }
    if rect.iter().any(|coordinate| !coordinate.is_finite())
        || rect[2] - rect[0] < 30.0
        || rect[3] - rect[1] < 16.0
    {
        return Err("Retângulo de campo inválido".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let name = CString::new(name).map_err(|_| "Nome de campo inválido")?;
    let value = CString::new(value).map_err(|_| "Valor de campo inválido")?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let root = unsafe { qpdf_get_root(doc.0) };
    let mut form = unsafe { qpdf_oh_get_key(doc.0, root, c"/AcroForm".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, form) } == 0 {
        form = unsafe { qpdf_make_indirect_object(doc.0, qpdf_oh_new_dictionary(doc.0)) };
        doc.set(root, c"/AcroForm", form);
    }
    let mut fields = unsafe { qpdf_oh_get_key(doc.0, form, c"/Fields".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, fields) } == 0 {
        fields = unsafe { qpdf_oh_new_array(doc.0) };
        doc.set(form, c"/Fields", fields);
    }
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, fields) };
    for index in 0..count {
        let field = unsafe { qpdf_oh_get_array_item(doc.0, fields, index) };
        let title = unsafe { qpdf_oh_get_key(doc.0, field, c"/T".as_ptr()) };
        let existing = unsafe { qpdf_oh_get_utf8_value(doc.0, title) };
        if !existing.is_null() && unsafe { CStr::from_ptr(existing) } == name.as_c_str() {
            return Err("Já existe um campo com esse nome".into());
        }
    }

    let resources = ensure_form_font(&doc, form);

    let widget = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(widget, c"/Type", doc.name(c"/Annot"));
    doc.set(widget, c"/Subtype", doc.name(c"/Widget"));
    doc.set(widget, c"/FT", doc.name(c"/Tx"));
    doc.set(widget, c"/T", unsafe {
        qpdf_oh_new_unicode_string(doc.0, name.as_ptr())
    });
    doc.set(widget, c"/F", unsafe { qpdf_oh_new_integer(doc.0, 4) });
    // Bit 13 (value 4096): campo de texto multilinha.
    doc.set(widget, c"/Ff", unsafe {
        qpdf_oh_new_integer(doc.0, if multiline { 4096 } else { 0 })
    });
    doc.set(widget, c"/P", page);
    let rectangle = unsafe { qpdf_oh_new_array(doc.0) };
    for coordinate in rect {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, coordinate, 3) };
        unsafe { qpdf_oh_append_item(doc.0, rectangle, number) };
    }
    doc.set(widget, c"/Rect", rectangle);

    set_text_appearance(&doc, widget, resources, rect, &value)?;

    let indirect = unsafe { qpdf_make_indirect_object(doc.0, widget) };
    unsafe { qpdf_oh_append_item(doc.0, fields, indirect) };
    let mut annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0 {
        annots = unsafe { qpdf_oh_new_array(doc.0) };
        doc.set(page, c"/Annots", annots);
    }
    unsafe { qpdf_oh_append_item(doc.0, annots, indirect) };
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

pub fn set_text_field_value(input: &Path, output: &Path, name: &str, value: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err("Nome de campo inválido".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let name = CString::new(name).map_err(|_| "Nome de campo inválido")?;
    let value = CString::new(value).map_err(|_| "Valor de campo inválido")?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let root = unsafe { qpdf_get_root(doc.0) };
    let form = unsafe { qpdf_oh_get_key(doc.0, root, c"/AcroForm".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, form) } == 0 {
        return Err("O PDF não contém formulário AcroForm".into());
    }
    let fields = unsafe { qpdf_oh_get_key(doc.0, form, c"/Fields".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, fields) } == 0 {
        return Err("O PDF não contém campos de formulário".into());
    }
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, fields) };
    let mut target = None;
    for index in 0..count {
        let field = unsafe { qpdf_oh_get_array_item(doc.0, fields, index) };
        let title = unsafe { qpdf_oh_get_key(doc.0, field, c"/T".as_ptr()) };
        let existing = unsafe { qpdf_oh_get_utf8_value(doc.0, title) };
        if !existing.is_null() && unsafe { CStr::from_ptr(existing) } == name.as_c_str() {
            target = Some(field);
            break;
        }
    }
    let field = target.ok_or("Campo de formulário não encontrado")?;
    let field_type = unsafe { qpdf_oh_get_key(doc.0, field, c"/FT".as_ptr()) };
    if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Tx".as_ptr()) } == 0 {
        return Err("O campo encontrado não é de texto".into());
    }
    let rectangle = unsafe { qpdf_oh_get_key(doc.0, field, c"/Rect".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, rectangle) } == 0
        || unsafe { qpdf_oh_get_array_n_items(doc.0, rectangle) } != 4
    {
        return Err("O campo não possui widget direto editável".into());
    }
    let mut rect = [0.0; 4];
    for (index, coordinate) in rect.iter_mut().enumerate() {
        let item = unsafe { qpdf_oh_get_array_item(doc.0, rectangle, index as c_int) };
        *coordinate = unsafe { qpdf_oh_get_numeric_value(doc.0, item) };
    }
    if rect.iter().any(|coordinate| !coordinate.is_finite())
        || rect[2] - rect[0] < 1.0
        || rect[3] - rect[1] < 1.0
    {
        return Err("Retângulo do campo inválido".into());
    }
    let resources = ensure_form_font(&doc, form);
    set_text_appearance(&doc, field, resources, rect, &value)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

fn checkbox_appearance(doc: &Document, width: f64, height: f64, checked: bool) -> AppResult<u32> {
    let stream = unsafe { qpdf_oh_new_stream(doc.0) };
    let dictionary = unsafe { qpdf_oh_get_dict(doc.0, stream) };
    doc.set(dictionary, c"/Type", doc.name(c"/XObject"));
    doc.set(dictionary, c"/Subtype", doc.name(c"/Form"));
    let bbox = unsafe { qpdf_oh_new_array(doc.0) };
    for coordinate in [0.0, 0.0, width, height] {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, coordinate, 3) };
        unsafe { qpdf_oh_append_item(doc.0, bbox, number) };
    }
    doc.set(dictionary, c"/BBox", bbox);
    let mut content = format!(
        "q 1 1 1 rg 0 0 {width:.3} {height:.3} re f 0 0 0 RG 1 w 0.5 0.5 {:.3} {:.3} re S\n",
        width - 1.0,
        height - 1.0
    );
    if checked {
        content.push_str(&format!(
            "2 w 3 {:.3} m {:.3} 3 l {:.3} {:.3} l S\n",
            height * 0.5,
            width * 0.4,
            width - 3.0,
            height - 3.0
        ));
    }
    content.push_str("Q\n");
    let null = unsafe { qpdf_oh_new_null(doc.0) };
    unsafe {
        qpdf_oh_replace_stream_data(doc.0, stream, content.as_ptr(), content.len(), null, null)
    };
    doc.check(0)?;
    Ok(stream)
}

pub fn add_checkbox(
    input: &Path,
    output: &Path,
    page_number: usize,
    name: &str,
    checked: bool,
    rect: [f64; 4],
) -> AppResult<()> {
    if page_number == 0 || name.trim().is_empty() || name.contains('.') {
        return Err("Nome de campo inválido".into());
    }
    if rect.iter().any(|coordinate| !coordinate.is_finite())
        || rect[2] - rect[0] < 16.0
        || rect[3] - rect[1] < 16.0
    {
        return Err("Retângulo da caixa de seleção inválido".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let name = CString::new(name).map_err(|_| "Nome de campo inválido")?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let root = unsafe { qpdf_get_root(doc.0) };
    let mut form = unsafe { qpdf_oh_get_key(doc.0, root, c"/AcroForm".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, form) } == 0 {
        form = unsafe { qpdf_make_indirect_object(doc.0, qpdf_oh_new_dictionary(doc.0)) };
        doc.set(root, c"/AcroForm", form);
    }
    let mut fields = unsafe { qpdf_oh_get_key(doc.0, form, c"/Fields".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, fields) } == 0 {
        fields = unsafe { qpdf_oh_new_array(doc.0) };
        doc.set(form, c"/Fields", fields);
    }
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, fields) };
    for index in 0..count {
        let field = unsafe { qpdf_oh_get_array_item(doc.0, fields, index) };
        let title = unsafe { qpdf_oh_get_key(doc.0, field, c"/T".as_ptr()) };
        let existing = unsafe { qpdf_oh_get_utf8_value(doc.0, title) };
        if !existing.is_null() && unsafe { CStr::from_ptr(existing) } == name.as_c_str() {
            return Err("Já existe um campo com esse nome".into());
        }
    }
    let widget = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(widget, c"/Type", doc.name(c"/Annot"));
    doc.set(widget, c"/Subtype", doc.name(c"/Widget"));
    doc.set(widget, c"/FT", doc.name(c"/Btn"));
    doc.set(widget, c"/T", unsafe {
        qpdf_oh_new_unicode_string(doc.0, name.as_ptr())
    });
    doc.set(widget, c"/F", unsafe { qpdf_oh_new_integer(doc.0, 4) });
    doc.set(widget, c"/P", page);
    let rectangle = unsafe { qpdf_oh_new_array(doc.0) };
    for coordinate in rect {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, coordinate, 3) };
        unsafe { qpdf_oh_append_item(doc.0, rectangle, number) };
    }
    doc.set(widget, c"/Rect", rectangle);
    let normal = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(
        normal,
        c"/Off",
        checkbox_appearance(&doc, rect[2] - rect[0], rect[3] - rect[1], false)?,
    );
    doc.set(
        normal,
        c"/Yes",
        checkbox_appearance(&doc, rect[2] - rect[0], rect[3] - rect[1], true)?,
    );
    let ap = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(ap, c"/N", normal);
    doc.set(widget, c"/AP", ap);
    let state = if checked { c"/Yes" } else { c"/Off" };
    doc.set(widget, c"/V", doc.name(state));
    doc.set(widget, c"/AS", doc.name(state));
    let indirect = unsafe { qpdf_make_indirect_object(doc.0, widget) };
    unsafe { qpdf_oh_append_item(doc.0, fields, indirect) };
    let mut annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0 {
        annots = unsafe { qpdf_oh_new_array(doc.0) };
        doc.set(page, c"/Annots", annots);
    }
    unsafe { qpdf_oh_append_item(doc.0, annots, indirect) };
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

pub fn set_checkbox_value(input: &Path, output: &Path, name: &str, checked: bool) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err("Nome de campo inválido".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let name = CString::new(name).map_err(|_| "Nome de campo inválido")?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let root = unsafe { qpdf_get_root(doc.0) };
    let form = unsafe { qpdf_oh_get_key(doc.0, root, c"/AcroForm".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, form) } == 0 {
        return Err("O PDF não contém formulário AcroForm".into());
    }
    let fields = unsafe { qpdf_oh_get_key(doc.0, form, c"/Fields".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, fields) } == 0 {
        return Err("O PDF não contém campos de formulário".into());
    }
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, fields) };
    let mut target = None;
    for index in 0..count {
        let field = unsafe { qpdf_oh_get_array_item(doc.0, fields, index) };
        let title = unsafe { qpdf_oh_get_key(doc.0, field, c"/T".as_ptr()) };
        let existing = unsafe { qpdf_oh_get_utf8_value(doc.0, title) };
        if !existing.is_null() && unsafe { CStr::from_ptr(existing) } == name.as_c_str() {
            target = Some(field);
            break;
        }
    }
    let field = target.ok_or("Campo de formulário não encontrado")?;
    let field_type = unsafe { qpdf_oh_get_key(doc.0, field, c"/FT".as_ptr()) };
    if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Btn".as_ptr()) } == 0 {
        return Err("O campo encontrado não é uma caixa de seleção".into());
    }
    let ap = unsafe { qpdf_oh_get_key(doc.0, field, c"/AP".as_ptr()) };
    let normal = unsafe { qpdf_oh_get_key(doc.0, ap, c"/N".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, normal) } == 0 {
        return Err("A caixa de seleção não possui aparência editável".into());
    }
    let yes = unsafe { qpdf_oh_get_key(doc.0, normal, c"/Yes".as_ptr()) };
    let off = unsafe { qpdf_oh_get_key(doc.0, normal, c"/Off".as_ptr()) };
    if unsafe { qpdf_oh_is_stream(doc.0, yes) } == 0
        || unsafe { qpdf_oh_is_stream(doc.0, off) } == 0
    {
        return Err("A aparência da caixa de seleção é inválida".into());
    }
    let state = if checked { c"/Yes" } else { c"/Off" };
    doc.set(field, c"/V", doc.name(state));
    doc.set(field, c"/AS", doc.name(state));
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

fn radio_appearance(doc: &Document, width: f64, height: f64, selected: bool) -> AppResult<u32> {
    let stream = unsafe { qpdf_oh_new_stream(doc.0) };
    let dictionary = unsafe { qpdf_oh_get_dict(doc.0, stream) };
    doc.set(dictionary, c"/Type", doc.name(c"/XObject"));
    doc.set(dictionary, c"/Subtype", doc.name(c"/Form"));
    let bbox = unsafe { qpdf_oh_new_array(doc.0) };
    for coordinate in [0.0, 0.0, width, height] {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, coordinate, 3) };
        unsafe { qpdf_oh_append_item(doc.0, bbox, number) };
    }
    doc.set(dictionary, c"/BBox", bbox);
    let radius = (width.min(height) / 2.0) - 1.0;
    let cx = width / 2.0;
    let cy = height / 2.0;
    // Aproximação de círculo com quatro curvas de Bézier (k ≈ 0.5523).
    let k = radius * 0.552_284_749_831;
    let mut content = format!(
        "q 1 1 1 rg 0 0 {width:.3} {height:.3} re f 0 0 0 RG 1 w \
         {:.3} {:.3} m {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c \
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c \
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c \
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c S\n",
        cx,
        cy + radius,
        cx + k,
        cy + radius,
        cx + radius,
        cy + k,
        cx + radius,
        cy,
        cx + radius,
        cy - k,
        cx + k,
        cy - radius,
        cx,
        cy - radius,
        cx - k,
        cy - radius,
        cx - radius,
        cy - k,
        cx - radius,
        cy,
        cx - radius,
        cy + k,
        cx - k,
        cy + radius,
        cx,
        cy + radius,
    );
    if selected {
        let inner = radius * 0.45;
        content.push_str(&format!(
            "0 0 0 rg {:.3} {:.3} {:.3} 0 360 arc f\n",
            cx, cy, inner
        ));
    }
    content.push_str("Q\n");
    let null = unsafe { qpdf_oh_new_null(doc.0) };
    unsafe {
        qpdf_oh_replace_stream_data(doc.0, stream, content.as_ptr(), content.len(), null, null)
    };
    doc.check(0)?;
    Ok(stream)
}

fn radio_export_name(label: &str) -> CString {
    let mut name = String::from("Opt");
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character);
        }
    }
    if name == "Opt" {
        name.push('0');
    }
    CString::new(format!("/{name}")).expect("nome de estado de rádio")
}

fn acroform_fields(doc: &Document, create: bool) -> AppResult<(u32, u32)> {
    let root = unsafe { qpdf_get_root(doc.0) };
    let mut form = unsafe { qpdf_oh_get_key(doc.0, root, c"/AcroForm".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, form) } == 0 {
        if !create {
            return Err("O PDF não contém formulário AcroForm".into());
        }
        form = unsafe { qpdf_make_indirect_object(doc.0, qpdf_oh_new_dictionary(doc.0)) };
        doc.set(root, c"/AcroForm", form);
    }
    let mut fields = unsafe { qpdf_oh_get_key(doc.0, form, c"/Fields".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, fields) } == 0 {
        if !create {
            return Err("O PDF não contém campos de formulário".into());
        }
        fields = unsafe { qpdf_oh_new_array(doc.0) };
        doc.set(form, c"/Fields", fields);
    }
    Ok((form, fields))
}

fn find_field_by_name(doc: &Document, fields: u32, name: &CStr) -> Option<u32> {
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, fields) };
    for index in 0..count {
        let field = unsafe { qpdf_oh_get_array_item(doc.0, fields, index) };
        let title = unsafe { qpdf_oh_get_key(doc.0, field, c"/T".as_ptr()) };
        let existing = unsafe { qpdf_oh_get_utf8_value(doc.0, title) };
        if !existing.is_null() && unsafe { CStr::from_ptr(existing) } == name {
            return Some(field);
        }
    }
    None
}

fn append_widget_to_page(doc: &Document, page: u32, widget: u32) {
    let mut annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0 {
        annots = unsafe { qpdf_oh_new_array(doc.0) };
        doc.set(page, c"/Annots", annots);
    }
    unsafe { qpdf_oh_append_item(doc.0, annots, widget) };
}

/// Cria um grupo de botões de opção (`/Btn` com a bandeira Radio).
/// Cada entrada é `(valor_exportado, retângulo)`. `selected` é o índice inicial.
pub fn add_radio_group(
    input: &Path,
    output: &Path,
    page_number: usize,
    name: &str,
    options: &[(String, [f64; 4])],
    selected: Option<usize>,
) -> AppResult<()> {
    if page_number == 0 || name.trim().is_empty() || name.contains('.') {
        return Err("Nome de campo inválido".into());
    }
    if options.len() < 2 {
        return Err("O grupo de rádio precisa de ao menos duas opções".into());
    }
    if let Some(index) = selected {
        if index >= options.len() {
            return Err("Opção de rádio selecionada fora do intervalo".into());
        }
    }
    for (_, rect) in options {
        if rect.iter().any(|coordinate| !coordinate.is_finite())
            || rect[2] - rect[0] < 12.0
            || rect[3] - rect[1] < 12.0
        {
            return Err("Retângulo de rádio inválido".into());
        }
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let name = CString::new(name).map_err(|_| "Nome de campo inválido")?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let (_form, fields) = acroform_fields(&doc, true)?;
    if find_field_by_name(&doc, fields, &name).is_some() {
        return Err("Já existe um campo com esse nome".into());
    }

    let parent = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(parent, c"/FT", doc.name(c"/Btn"));
    // NoToggleToOff | Radio
    doc.set(parent, c"/Ff", unsafe { qpdf_oh_new_integer(doc.0, 3) });
    doc.set(parent, c"/T", unsafe {
        qpdf_oh_new_unicode_string(doc.0, name.as_ptr())
    });
    doc.set(parent, c"/F", unsafe { qpdf_oh_new_integer(doc.0, 4) });
    doc.set(parent, c"/P", page);
    let kids = unsafe { qpdf_oh_new_array(doc.0) };
    doc.set(parent, c"/Kids", kids);
    let parent_indirect = unsafe { qpdf_make_indirect_object(doc.0, parent) };

    let mut states = Vec::new();
    for (label, rect) in options {
        let state = radio_export_name(label);
        states.push(state.clone());
        let width = rect[2] - rect[0];
        let height = rect[3] - rect[1];
        let widget = unsafe { qpdf_oh_new_dictionary(doc.0) };
        doc.set(widget, c"/Type", doc.name(c"/Annot"));
        doc.set(widget, c"/Subtype", doc.name(c"/Widget"));
        doc.set(widget, c"/Parent", parent_indirect);
        let rectangle = unsafe { qpdf_oh_new_array(doc.0) };
        for coordinate in *rect {
            let number = unsafe { qpdf_oh_new_real_from_double(doc.0, coordinate, 3) };
            unsafe { qpdf_oh_append_item(doc.0, rectangle, number) };
        }
        doc.set(widget, c"/Rect", rectangle);
        doc.set(widget, c"/P", page);
        let normal = unsafe { qpdf_oh_new_dictionary(doc.0) };
        doc.set(
            normal,
            c"/Off",
            radio_appearance(&doc, width, height, false)?,
        );
        doc.set(normal, &state, radio_appearance(&doc, width, height, true)?);
        let ap = unsafe { qpdf_oh_new_dictionary(doc.0) };
        doc.set(ap, c"/N", normal);
        doc.set(widget, c"/AP", ap);
        doc.set(widget, c"/AS", doc.name(c"/Off"));
        let indirect = unsafe { qpdf_make_indirect_object(doc.0, widget) };
        unsafe { qpdf_oh_append_item(doc.0, kids, indirect) };
        append_widget_to_page(&doc, page, indirect);
    }
    if let Some(index) = selected {
        doc.set(parent_indirect, c"/V", doc.name(&states[index]));
        for (position, state) in states.iter().enumerate() {
            let kid = unsafe { qpdf_oh_get_array_item(doc.0, kids, position as c_int) };
            let active = if position == index {
                doc.name(state)
            } else {
                doc.name(c"/Off")
            };
            doc.set(kid, c"/AS", active);
        }
    } else {
        doc.set(parent_indirect, c"/V", doc.name(c"/Off"));
    }
    unsafe { qpdf_oh_append_item(doc.0, fields, parent_indirect) };
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

pub fn set_radio_value(input: &Path, output: &Path, name: &str, selection: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err("Nome de campo inválido".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let name = CString::new(name).map_err(|_| "Nome de campo inválido")?;
    let selection = CString::new(selection).map_err(|_| "Seleção inválida")?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let (_form, fields) = acroform_fields(&doc, false)?;
    let field =
        find_field_by_name(&doc, fields, &name).ok_or("Campo de formulário não encontrado")?;
    let field_type = unsafe { qpdf_oh_get_key(doc.0, field, c"/FT".as_ptr()) };
    if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Btn".as_ptr()) } == 0 {
        return Err("O campo encontrado não é um botão".into());
    }
    let flags = unsafe { qpdf_oh_get_key(doc.0, field, c"/Ff".as_ptr()) };
    let flags = unsafe { qpdf_oh_get_numeric_value(doc.0, flags) } as i64;
    if flags & 2 == 0 {
        return Err("O campo encontrado não é um grupo de rádio".into());
    }
    let kids = unsafe { qpdf_oh_get_key(doc.0, field, c"/Kids".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, kids) } == 0 {
        return Err("O rádio não possui widgets em /Kids".into());
    }
    let wanted = radio_export_name(&selection.to_string_lossy());
    let wanted_name = unsafe { qpdf_oh_new_name(doc.0, wanted.as_ptr()) };
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, kids) };
    let mut found = false;
    for index in 0..count {
        let kid = unsafe { qpdf_oh_get_array_item(doc.0, kids, index) };
        let normal = unsafe {
            qpdf_oh_get_key(
                doc.0,
                qpdf_oh_get_key(doc.0, kid, c"/AP".as_ptr()),
                c"/N".as_ptr(),
            )
        };
        let has_state = unsafe { qpdf_oh_get_key(doc.0, normal, wanted.as_ptr()) };
        if unsafe { qpdf_oh_is_stream(doc.0, has_state) } != 0 {
            doc.set(kid, c"/AS", wanted_name);
            found = true;
        } else {
            doc.set(kid, c"/AS", doc.name(c"/Off"));
        }
    }
    if !found {
        return Err("Opção de rádio inexistente neste grupo".into());
    }
    doc.set(field, c"/V", wanted_name);
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

/// Cria campo de escolha (`/Ch`): combo quando `combo` for verdadeiro, senão lista.
#[allow(clippy::too_many_arguments)]
pub fn add_choice_field(
    input: &Path,
    output: &Path,
    page_number: usize,
    name: &str,
    value: &str,
    options: &[String],
    rect: [f64; 4],
    combo: bool,
) -> AppResult<()> {
    if page_number == 0 || name.trim().is_empty() || name.contains('.') {
        return Err("Nome de campo inválido".into());
    }
    if options.is_empty() {
        return Err("O campo de escolha precisa de opções".into());
    }
    if !value.is_empty() && !options.iter().any(|option| option == value) {
        return Err("O valor inicial precisa estar entre as opções".into());
    }
    if rect.iter().any(|coordinate| !coordinate.is_finite())
        || rect[2] - rect[0] < 30.0
        || rect[3] - rect[1] < 16.0
    {
        return Err("Retângulo de campo inválido".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let name = CString::new(name).map_err(|_| "Nome de campo inválido")?;
    let value = CString::new(value).map_err(|_| "Valor de campo inválido")?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let (form, fields) = acroform_fields(&doc, true)?;
    if find_field_by_name(&doc, fields, &name).is_some() {
        return Err("Já existe um campo com esse nome".into());
    }
    let resources = ensure_form_font(&doc, form);

    let widget = unsafe { qpdf_oh_new_dictionary(doc.0) };
    doc.set(widget, c"/Type", doc.name(c"/Annot"));
    doc.set(widget, c"/Subtype", doc.name(c"/Widget"));
    doc.set(widget, c"/FT", doc.name(c"/Ch"));
    doc.set(widget, c"/T", unsafe {
        qpdf_oh_new_unicode_string(doc.0, name.as_ptr())
    });
    doc.set(widget, c"/F", unsafe { qpdf_oh_new_integer(doc.0, 4) });
    // Bit 18 (131072): combo. Sem a bit, é lista.
    doc.set(widget, c"/Ff", unsafe {
        qpdf_oh_new_integer(doc.0, if combo { 131072 } else { 0 })
    });
    doc.set(widget, c"/P", page);
    let rectangle = unsafe { qpdf_oh_new_array(doc.0) };
    for coordinate in rect {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, coordinate, 3) };
        unsafe { qpdf_oh_append_item(doc.0, rectangle, number) };
    }
    doc.set(widget, c"/Rect", rectangle);
    let opt = unsafe { qpdf_oh_new_array(doc.0) };
    for option in options {
        let option = CString::new(option.as_str()).map_err(|_| "Opção inválida")?;
        unsafe { qpdf_oh_append_item(doc.0, opt, qpdf_oh_new_string(doc.0, option.as_ptr())) };
    }
    doc.set(widget, c"/Opt", opt);
    set_text_appearance(&doc, widget, resources, rect, &value)?;
    let indirect = unsafe { qpdf_make_indirect_object(doc.0, widget) };
    unsafe { qpdf_oh_append_item(doc.0, fields, indirect) };
    append_widget_to_page(&doc, page, indirect);
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

pub fn set_choice_value(input: &Path, output: &Path, name: &str, value: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err("Nome de campo inválido".into());
    }
    let input = crate::path_to_cstring(input)?;
    let output = crate::path_to_cstring(output)?;
    let name = CString::new(name).map_err(|_| "Nome de campo inválido")?;
    let value = CString::new(value).map_err(|_| "Valor de campo inválido")?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let (_form, fields) = acroform_fields(&doc, false)?;
    let field =
        find_field_by_name(&doc, fields, &name).ok_or("Campo de formulário não encontrado")?;
    let field_type = unsafe { qpdf_oh_get_key(doc.0, field, c"/FT".as_ptr()) };
    if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Ch".as_ptr()) } == 0 {
        return Err("O campo encontrado não é de escolha".into());
    }
    let rectangle = unsafe { qpdf_oh_get_key(doc.0, field, c"/Rect".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, rectangle) } == 0
        || unsafe { qpdf_oh_get_array_n_items(doc.0, rectangle) } != 4
    {
        return Err("O campo não possui widget direto editável".into());
    }
    let mut rect = [0.0; 4];
    for (index, coordinate) in rect.iter_mut().enumerate() {
        let item = unsafe { qpdf_oh_get_array_item(doc.0, rectangle, index as c_int) };
        *coordinate = unsafe { qpdf_oh_get_numeric_value(doc.0, item) };
    }
    let opt = unsafe { qpdf_oh_get_key(doc.0, field, c"/Opt".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, opt) } != 0 {
        let count = unsafe { qpdf_oh_get_array_n_items(doc.0, opt) };
        let mut allowed = false;
        for index in 0..count {
            let item = unsafe { qpdf_oh_get_array_item(doc.0, opt, index) };
            let pointer = unsafe { qpdf_oh_get_utf8_value(doc.0, item) };
            if !pointer.is_null() && unsafe { CStr::from_ptr(pointer) } == value.as_c_str() {
                allowed = true;
                break;
            }
        }
        if !value.to_string_lossy().is_empty() && !allowed {
            return Err("O valor precisa estar entre as opções do campo".into());
        }
    }
    let form = unsafe { qpdf_oh_get_key(doc.0, qpdf_get_root(doc.0), c"/AcroForm".as_ptr()) };
    let resources = ensure_form_font(&doc, form);
    set_text_appearance(&doc, field, resources, rect, &value)?;
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

pub fn list_form_fields(input: &Path) -> AppResult<Vec<(String, String, String)>> {
    let input = crate::path_to_cstring(input)?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let root = unsafe { qpdf_get_root(doc.0) };
    let form = unsafe { qpdf_oh_get_key(doc.0, root, c"/AcroForm".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, form) } == 0 {
        return Ok(Vec::new());
    }
    let fields = unsafe { qpdf_oh_get_key(doc.0, form, c"/Fields".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, fields) } == 0 {
        return Ok(Vec::new());
    }
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, fields) };
    let mut result = Vec::new();
    for index in 0..count {
        let field = unsafe { qpdf_oh_get_array_item(doc.0, fields, index) };
        let title = unsafe { qpdf_oh_get_key(doc.0, field, c"/T".as_ptr()) };
        if unsafe { qpdf_oh_is_string(doc.0, title) } == 0 {
            continue;
        }
        let name = unsafe { qpdf_oh_get_utf8_value(doc.0, title) };
        if name.is_null() {
            continue;
        }
        let name = unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned();
        let field_type = unsafe { qpdf_oh_get_key(doc.0, field, c"/FT".as_ptr()) };
        let flags = unsafe { qpdf_oh_get_key(doc.0, field, c"/Ff".as_ptr()) };
        let flags = unsafe { qpdf_oh_get_numeric_value(doc.0, flags) } as i64;
        let kind = if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Tx".as_ptr()) } != 0
        {
            if flags & 4096 != 0 {
                "texto-multilinha"
            } else {
                "texto"
            }
        } else if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Btn".as_ptr()) } != 0 {
            if flags & 2 != 0 {
                "rádio"
            } else if flags & 4 != 0 {
                "botão"
            } else {
                "seleção"
            }
        } else if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Ch".as_ptr()) } != 0 {
            if flags & 131072 != 0 {
                "combo"
            } else {
                "lista"
            }
        } else if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Sig".as_ptr()) } != 0 {
            "assinatura"
        } else {
            "outro"
        };
        let value = unsafe { qpdf_oh_get_key(doc.0, field, c"/V".as_ptr()) };
        let value = if unsafe { qpdf_oh_is_string(doc.0, value) } != 0 {
            let pointer = unsafe { qpdf_oh_get_utf8_value(doc.0, value) };
            if pointer.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(pointer) }
                    .to_string_lossy()
                    .into_owned()
            }
        } else if unsafe { qpdf_oh_is_name(doc.0, value) } != 0 {
            let pointer = unsafe { qpdf_oh_get_name(doc.0, value) };
            if pointer.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(pointer) }
                    .to_string_lossy()
                    .into_owned()
            }
        } else {
            String::new()
        };
        result.push((name, kind.into(), value));
    }
    doc.check(0)?;
    Ok(result)
}

/// Anotação da página com retângulo em pontos PDF.
#[derive(Debug, Clone)]
pub struct AnnotationInfo {
    pub index: usize,
    pub kind: String,
    pub text: String,
    pub rect: [f64; 4],
}

fn read_rect(doc: &Document, object: u32) -> Option<[f64; 4]> {
    let rectangle = unsafe { qpdf_oh_get_key(doc.0, object, c"/Rect".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, rectangle) } == 0
        || unsafe { qpdf_oh_get_array_n_items(doc.0, rectangle) } != 4
    {
        return None;
    }
    let mut rect = [0.0; 4];
    for (index, value) in rect.iter_mut().enumerate() {
        let item = unsafe { qpdf_oh_get_array_item(doc.0, rectangle, index as c_int) };
        *value = unsafe { qpdf_oh_get_numeric_value(doc.0, item) };
    }
    if rect.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(rect)
}

fn write_rect(doc: &Document, object: u32, rect: [f64; 4]) {
    let array = unsafe { qpdf_oh_new_array(doc.0) };
    for value in rect {
        let number = unsafe { qpdf_oh_new_real_from_double(doc.0, value, 3) };
        unsafe { qpdf_oh_append_item(doc.0, array, number) };
    }
    doc.set(object, c"/Rect", array);
}

fn path_c(path: &Path) -> AppResult<CString> {
    crate::path_to_cstring(path)
}

/// Lista anotações e widgets de formulário com retângulos.
pub fn list_annotations(input: &Path, page_number: usize) -> AppResult<Vec<AnnotationInfo>> {
    let _trace = crate::trace(|| "  qpdf lista anotações".into());
    if page_number == 0 {
        return Err("Número de página inválido".into());
    }
    let input = path_c(input)?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0 {
        return Ok(Vec::new());
    }
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, annots) };
    let mut result = Vec::new();
    for index in 0..count {
        let annot = unsafe { qpdf_oh_get_array_item(doc.0, annots, index) };
        let Some(rect) = read_rect(&doc, annot) else {
            continue;
        };
        let subtype = unsafe { qpdf_oh_get_key(doc.0, annot, c"/Subtype".as_ptr()) };
        let kind = if unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Text".as_ptr()) } != 0
        {
            "nota"
        } else if unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Highlight".as_ptr()) } != 0
        {
            "destaque"
        } else if unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Widget".as_ptr()) } != 0 {
            "campo"
        } else {
            "outro"
        };
        let contents = unsafe { qpdf_oh_get_key(doc.0, annot, c"/Contents".as_ptr()) };
        let text = if unsafe { qpdf_oh_is_string(doc.0, contents) } != 0 {
            let pointer = unsafe { qpdf_oh_get_utf8_value(doc.0, contents) };
            if pointer.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(pointer) }
                    .to_string_lossy()
                    .into_owned()
            }
        } else {
            String::new()
        };
        result.push(AnnotationInfo {
            index: index as usize,
            kind: kind.into(),
            text,
            rect,
        });
    }
    doc.check(0)?;
    Ok(result)
}

/// Move uma anotação/widget pelo índice em `/Annots`, deslocando o retângulo.
pub fn move_annotation(
    input: &Path,
    output: &Path,
    page_number: usize,
    index: usize,
    dx: f64,
    dy: f64,
) -> AppResult<()> {
    transform_annotation(
        input,
        output,
        page_number,
        index,
        &[1.0, 0.0, 0.0, 1.0, dx, dy],
    )
}

/// Aplica a matriz afim `transform` (espaço da página) à anotação: o `/Rect`
/// passa a ser a caixa dos cantos transformados e os `/QuadPoints` (destaques)
/// são transformados ponto a ponto. A aparência é ajustada pelo leitor ao novo
/// `/Rect`.
pub fn transform_annotation(
    input: &Path,
    output: &Path,
    page_number: usize,
    index: usize,
    transform: &[f64; 6],
) -> AppResult<()> {
    if page_number == 0 || transform.iter().any(|value| !value.is_finite()) {
        return Err("Página ou transformação inválida".into());
    }
    let apply = |x: f64, y: f64| {
        (
            x * transform[0] + y * transform[2] + transform[4],
            x * transform[1] + y * transform[3] + transform[5],
        )
    };
    let input_c = path_c(input)?;
    let output_c = path_c(output)?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input_c.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0 {
        return Err("A página não tem anotações".into());
    }
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, annots) };
    if index >= count as usize {
        return Err("Anotação não encontrada".into());
    }
    let annot = unsafe { qpdf_oh_get_array_item(doc.0, annots, index as c_int) };
    let [x0, y0, x1, y1] = read_rect(&doc, annot).ok_or("Anotação sem retângulo válido")?;
    let mut rect = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
        let (x, y) = apply(x, y);
        rect = [
            rect[0].min(x),
            rect[1].min(y),
            rect[2].max(x),
            rect[3].max(y),
        ];
    }
    if rect[2] - rect[0] < 1.0 || rect[3] - rect[1] < 1.0 {
        return Err("Tamanho mínimo da anotação é 1 pt".into());
    }
    write_rect(&doc, annot, rect);
    // Highlights também guardam QuadPoints absolutos.
    let quad = unsafe { qpdf_oh_get_key(doc.0, annot, c"/QuadPoints".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, quad) } != 0 {
        let n = unsafe { qpdf_oh_get_array_n_items(doc.0, quad) };
        let values: Vec<f64> = (0..n)
            .map(|i| unsafe {
                qpdf_oh_get_numeric_value(doc.0, qpdf_oh_get_array_item(doc.0, quad, i))
            })
            .collect();
        let array = unsafe { qpdf_oh_new_array(doc.0) };
        for pair in values.chunks(2) {
            let (x, y) = apply(pair[0], *pair.get(1).unwrap_or(&0.0));
            for value in [x, y] {
                let number = unsafe { qpdf_oh_new_real_from_double(doc.0, value, 3) };
                unsafe { qpdf_oh_append_item(doc.0, array, number) };
            }
        }
        doc.set(annot, c"/QuadPoints", array);
    }
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output_c.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

/// Cópia de `array` sem os itens cujo objeto tem um dos `ids`.
fn array_without(doc: &Document, array: u32, ids: &[c_int]) -> (u32, usize) {
    let fresh = unsafe { qpdf_oh_new_array(doc.0) };
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, array) };
    let mut removed = 0;
    for index in 0..count {
        let item = unsafe { qpdf_oh_get_array_item(doc.0, array, index) };
        let id = unsafe { qpdf_oh_get_object_id(doc.0, item) };
        if id != 0 && ids.contains(&id) {
            removed += 1;
        } else {
            unsafe { qpdf_oh_append_item(doc.0, fresh, item) };
        }
    }
    (fresh, removed)
}

/// Remove a anotação/widget pelo índice em `/Annots`. Campos de formulário
/// também saem de `/AcroForm /Fields` (ou dos `/Kids` do campo pai; o pai
/// sem filhos também sai).
pub fn remove_annotation(
    input: &Path,
    output: &Path,
    page_number: usize,
    index: usize,
) -> AppResult<()> {
    if page_number == 0 {
        return Err("Número de página inválido".into());
    }
    let input_c = path_c(input)?;
    let output_c = path_c(output)?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input_c.as_ptr(), std::ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    let annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0
        || index >= unsafe { qpdf_oh_get_array_n_items(doc.0, annots) } as usize
    {
        return Err("Anotação não encontrada".into());
    }
    let annot = unsafe { qpdf_oh_get_array_item(doc.0, annots, index as c_int) };
    let annot_id = unsafe { qpdf_oh_get_object_id(doc.0, annot) };
    // /Annots sem o item (por posição: anotações diretas não têm id).
    let fresh = unsafe { qpdf_oh_new_array(doc.0) };
    let count = unsafe { qpdf_oh_get_array_n_items(doc.0, annots) };
    for position in (0..count).filter(|position| *position as usize != index) {
        let item = unsafe { qpdf_oh_get_array_item(doc.0, annots, position) };
        unsafe { qpdf_oh_append_item(doc.0, fresh, item) };
    }
    doc.set(page, c"/Annots", fresh);
    let subtype = unsafe { qpdf_oh_get_key(doc.0, annot, c"/Subtype".as_ptr()) };
    let is_widget = unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Widget".as_ptr()) } != 0;
    if is_widget && annot_id != 0 {
        if let Ok((_, fields)) = acroform_fields(&doc, false) {
            let parent = unsafe { qpdf_oh_get_key(doc.0, annot, c"/Parent".as_ptr()) };
            let parent_id = unsafe { qpdf_oh_get_object_id(doc.0, parent) };
            let kids = unsafe { qpdf_oh_get_key(doc.0, parent, c"/Kids".as_ptr()) };
            let mut remove_from_fields = vec![annot_id];
            if parent_id != 0 && unsafe { qpdf_oh_is_array(doc.0, kids) } != 0 {
                let (remaining, _) = array_without(&doc, kids, &[annot_id]);
                if unsafe { qpdf_oh_get_array_n_items(doc.0, remaining) } == 0 {
                    remove_from_fields.push(parent_id);
                } else {
                    doc.set(parent, c"/Kids", remaining);
                }
            }
            let (remaining, _) = array_without(&doc, fields, &remove_from_fields);
            let (form, _) = acroform_fields(&doc, false)?;
            doc.set(form, c"/Fields", remaining);
        }
    }
    doc.check(0)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output_c.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}
