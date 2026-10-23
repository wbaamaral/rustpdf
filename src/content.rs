// SPDX-License-Identifier: AGPL-3.0-or-later
//! Leitura e edição do conteúdo desenhado da página: textos e imagens.
use crate::dominio::fonte::{FontCodec, ToUnicode};
use crate::images;
use crate::infra::qpdf::{dict_keys, ffi::*, path_c, Document};
use crate::textmove::{self, FormXObject, Frame, Interpreter, Matrix, XObject};
use crate::AppResult;
use std::collections::HashMap;
use std::ffi::{c_int, CStr, CString};
use std::path::Path;
use std::ptr;

/// Texto extraído de operadores `Tj`/`TJ`/`'`/`"` no conteúdo da página.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    /// Deslocamento do literal `(` no stream de conteúdo.
    pub offset: usize,
    /// Quantidade de bytes do literal, incluindo os parênteses.
    pub length: usize,
}

/// Imagem XObject da página.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub name: String,
    pub object_id: i64,
    pub width: i64,
    pub height: i64,
    pub filter: String,
}

fn decode_pdf_literal(raw: &[u8]) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let mut index = 0;
    while index < raw.len() {
        let byte = raw[index];
        if byte == b'\\' && index + 1 < raw.len() {
            index += 1;
            match raw[index] {
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0c),
                b'(' => out.push(b'('),
                b')' => out.push(b')'),
                b'\\' => out.push(b'\\'),
                b'\n' => {}
                b'\r' => {
                    if index + 1 < raw.len() && raw[index + 1] == b'\n' {
                        index += 1;
                    }
                }
                digit @ b'0'..=b'7' => {
                    let mut value = (digit - b'0') as u32;
                    let mut consumed = 0;
                    while consumed < 2
                        && index + 1 < raw.len()
                        && (b'0'..=b'7').contains(&raw[index + 1])
                    {
                        index += 1;
                        value = value * 8 + (raw[index] - b'0') as u32;
                        consumed += 1;
                    }
                    out.push(value.min(255) as u8);
                }
                other => out.push(other),
            }
        } else {
            out.push(byte);
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode_pdf_literal(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 2);
    out.push(b'(');
    for byte in text.bytes() {
        match byte {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(byte);
            }
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            0x00..=0x1f | 0x7f..=0xff => {
                out.extend_from_slice(format!("\\{:03o}", byte).as_bytes());
            }
            other => out.push(other),
        }
    }
    out.push(b')');
    out
}

fn parse_literal(data: &[u8], start: usize) -> Option<(String, usize, usize)> {
    if start >= data.len() || data[start] != b'(' {
        return None;
    }
    let mut depth = 1;
    let mut index = start + 1;
    let inner_start = index;
    while index < data.len() {
        match data[index] {
            b'\\' => index += 2,
            b'(' => {
                depth += 1;
                index += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let raw = &data[inner_start..index];
                    return Some((decode_pdf_literal(raw), start, index + 1 - start));
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    None
}

fn is_space(byte: u8) -> bool {
    matches!(byte, 0x00 | 0x09 | 0x0a | 0x0c | 0x0d | 0x20)
}

fn skip_spaces(data: &[u8], mut index: usize) -> usize {
    while index < data.len() && is_space(data[index]) {
        index += 1;
    }
    index
}

fn looks_like_text_operator(data: &[u8], after: usize) -> bool {
    let index = skip_spaces(data, after);
    let rest = &data[index..];
    rest.starts_with(b"Tj")
        || rest.starts_with(b"TJ")
        || rest.starts_with(b"'")
        || rest.starts_with(b"\"")
}

/// Extrai as sequências de texto mostrado na página.
pub fn parse_text_runs(content: &[u8]) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut index = 0;
    while index < content.len() {
        if content[index] == b'(' {
            if let Some((text, offset, length)) = parse_literal(content, index) {
                if looks_like_text_operator(content, index + length) {
                    runs.push(TextRun {
                        text,
                        offset,
                        length,
                    });
                }
                index += length.max(1);
                continue;
            }
        }
        index += 1;
    }
    runs
}

/// Substitui ocorrências de `old` por `new` em operadores de texto.
pub fn replace_text_in_content(
    content: &[u8],
    old: &str,
    new: &str,
) -> AppResult<(Vec<u8>, usize)> {
    if old.is_empty() {
        return Err("Texto a localizar não pode ser vazio".into());
    }
    let runs = parse_text_runs(content);
    let mut out = Vec::with_capacity(content.len());
    let mut cursor = 0;
    let mut replaced = 0;
    for run in runs {
        if run.text != old {
            continue;
        }
        out.extend_from_slice(&content[cursor..run.offset]);
        out.extend_from_slice(&encode_pdf_literal(new));
        cursor = run.offset + run.length;
        replaced += 1;
    }
    if replaced == 0 {
        return Err("Texto não encontrado no conteúdo da página".into());
    }
    out.extend_from_slice(&content[cursor..]);
    Ok((out, replaced))
}

fn read_stream_data(doc: &Document, stream: u32) -> AppResult<Vec<u8>> {
    let mut buffer: *mut u8 = ptr::null_mut();
    let mut length = 0usize;
    let mut filtered = 0;
    doc.check(unsafe {
        qpdf_oh_get_stream_data(
            doc.0,
            stream,
            1, /* qpdf_dl_generalized */
            &mut filtered,
            &mut buffer,
            &mut length,
        )
    })?;
    if buffer.is_null() {
        return Ok(Vec::new());
    }
    let data = unsafe { std::slice::from_raw_parts(buffer, length) }.to_vec();
    unsafe { free(buffer.cast()) };
    Ok(data)
}

fn read_page_content(doc: &Document, page: u32) -> AppResult<Vec<u8>> {
    let mut buffer: *mut u8 = ptr::null_mut();
    let mut length = 0usize;
    doc.check(unsafe { qpdf_oh_get_page_content_data(doc.0, page, &mut buffer, &mut length) })?;
    if buffer.is_null() {
        return Ok(Vec::new());
    }
    let data = unsafe { std::slice::from_raw_parts(buffer, length) }.to_vec();
    unsafe { free(buffer.cast()) };
    Ok(data)
}

fn write_page_content(doc: &Document, page: u32, content: &[u8]) -> AppResult<()> {
    let stream = unsafe { qpdf_oh_new_stream(doc.0) };
    let null = unsafe { qpdf_oh_new_null(doc.0) };
    unsafe {
        qpdf_oh_replace_stream_data(doc.0, stream, content.as_ptr(), content.len(), null, null)
    };
    doc.check(0)?;
    let indirect = unsafe { qpdf_make_indirect_object(doc.0, stream) };
    doc.set(page, c"/Contents", indirect);
    doc.check(0)
}

fn write_stream_content(doc: &Document, stream: u32, content: &[u8]) -> AppResult<()> {
    let null = unsafe { qpdf_oh_new_null(doc.0) };
    unsafe {
        qpdf_oh_replace_stream_data(doc.0, stream, content.as_ptr(), content.len(), null, null)
    };
    doc.check(0)
}

/// Stream de desenho: conteúdo da página ou de um Form XObject.
struct DrawStream {
    /// `"page"` ou o nome do XObject (sem barra).
    label: String,
    stream: u32,
    content: Vec<u8>,
}

fn collect_draw_streams(doc: &Document, page: u32) -> AppResult<Vec<DrawStream>> {
    let mut out = vec![DrawStream {
        label: "page".into(),
        stream: 0,
        content: read_page_content(doc, page)?,
    }];
    let mut visited = Vec::new();
    collect_forms(doc, page, &mut visited, &mut out)?;
    Ok(out)
}

fn collect_forms(
    doc: &Document,
    resources_owner: u32,
    visited: &mut Vec<i64>,
    out: &mut Vec<DrawStream>,
) -> AppResult<()> {
    let resources = unsafe { qpdf_oh_get_key(doc.0, resources_owner, c"/Resources".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, resources) } == 0 {
        return Ok(());
    }
    let xobjects = unsafe { qpdf_oh_get_key(doc.0, resources, c"/XObject".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, xobjects) } == 0 {
        return Ok(());
    }
    let mut keys = Vec::new();
    unsafe { qpdf_oh_begin_dict_key_iter(doc.0, xobjects) };
    while unsafe { qpdf_oh_dict_more_keys(doc.0) } != 0 {
        let key = unsafe { qpdf_oh_dict_next_key(doc.0) };
        if key.is_null() {
            break;
        }
        keys.push(
            unsafe { CStr::from_ptr(key) }
                .to_string_lossy()
                .into_owned(),
        );
    }
    for key in keys {
        let object = unsafe { qpdf_oh_get_key(doc.0, xobjects, key_cstr(&key).as_ptr()) };
        if unsafe { qpdf_oh_is_stream(doc.0, object) } == 0 {
            continue;
        }
        let id = unsafe { qpdf_oh_get_object_id(doc.0, object) } as i64;
        if visited.contains(&id) {
            continue;
        }
        let dict = unsafe { qpdf_oh_get_dict(doc.0, object) };
        let subtype = unsafe { qpdf_oh_get_key(doc.0, dict, c"/Subtype".as_ptr()) };
        if unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Form".as_ptr()) } != 0 {
            visited.push(id);
            let content = read_stream_data(doc, object)?;
            out.push(DrawStream {
                label: key.trim_start_matches('/').to_string(),
                stream: object,
                content,
            });
            collect_forms(doc, dict, visited, out)?;
        }
    }
    Ok(())
}

fn open_page(doc: &Document, input: &Path, page_number: usize) -> AppResult<u32> {
    let _trace = crate::trace(|| format!("  qpdf lê o PDF (página {page_number})"));
    if page_number == 0 {
        return Err("Página inválida".into());
    }
    let input = path_c(input)?;
    doc.check(unsafe { qpdf_read(doc.0, input.as_ptr(), ptr::null()) })?;
    let page = unsafe { qpdf_get_page_n(doc.0, page_number - 1) };
    doc.check(0)?;
    Ok(page)
}

fn finish(doc: &Document, output: &Path) -> AppResult<()> {
    let _trace = crate::trace(|| "  qpdf grava o PDF".into());
    let output = path_c(output)?;
    doc.check(unsafe { qpdf_init_write(doc.0, output.as_ptr()) })?;
    doc.check(unsafe { qpdf_write(doc.0) })
}

/// Lista trechos de texto mostrado na página, em ordem de aparição.
pub fn list_text_runs(input: &Path, page_number: usize) -> AppResult<Vec<String>> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    let mut texts = Vec::new();
    for draw in collect_draw_streams(&doc, page)? {
        for run in parse_text_runs(&draw.content) {
            if !run.text.is_empty() {
                texts.push(run.text);
            }
        }
    }
    Ok(texts)
}

/// Substitui texto mostrado na página. Retorna a quantidade de substituições.
pub fn replace_page_text(
    input: &Path,
    output: &Path,
    page_number: usize,
    old: &str,
    new: &str,
) -> AppResult<usize> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    let draws = collect_draw_streams(&doc, page)?;
    let mut total = 0;
    for draw in draws {
        let Ok((updated, replaced)) = replace_text_in_content(&draw.content, old, new) else {
            continue;
        };
        if replaced == 0 {
            continue;
        }
        if draw.label == "page" {
            write_page_content(&doc, page, &updated)?;
        } else {
            write_stream_content(&doc, draw.stream, &updated)?;
        }
        total += replaced;
    }
    if total == 0 {
        return Err("Texto não encontrado no conteúdo da página".into());
    }
    finish(&doc, output)?;
    Ok(total)
}

fn xobject_dictionary(doc: &Document, owner: u32) -> AppResult<u32> {
    let resources = unsafe { qpdf_oh_get_key(doc.0, owner, c"/Resources".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, resources) } == 0 {
        return Err("A página não possui recursos".into());
    }
    let xobjects = unsafe { qpdf_oh_get_key(doc.0, resources, c"/XObject".as_ptr()) };
    if unsafe { qpdf_oh_is_dictionary(doc.0, xobjects) } == 0 {
        return Err("A página não possui imagens XObject".into());
    }
    Ok(xobjects)
}

/// Imagens XObject, incluindo aninhadas em formulários.
fn collect_images(
    doc: &Document,
    owner: u32,
    visited: &mut Vec<i64>,
    out: &mut Vec<(String, u32)>,
) -> AppResult<()> {
    let xobjects = match xobject_dictionary(doc, owner) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    for key in dict_keys(doc, xobjects) {
        let object = unsafe { qpdf_oh_get_key(doc.0, xobjects, key_cstr(&key).as_ptr()) };
        if unsafe { qpdf_oh_is_stream(doc.0, object) } == 0 {
            continue;
        }
        let id = unsafe { qpdf_oh_get_object_id(doc.0, object) } as i64;
        if visited.contains(&id) {
            continue;
        }
        let dict = unsafe { qpdf_oh_get_dict(doc.0, object) };
        let subtype = unsafe { qpdf_oh_get_key(doc.0, dict, c"/Subtype".as_ptr()) };
        if unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Image".as_ptr()) } != 0 {
            visited.push(id);
            out.push((key.trim_start_matches('/').to_string(), object));
        } else if unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Form".as_ptr()) } != 0 {
            visited.push(id);
            collect_images(doc, dict, visited, out)?;
        }
    }
    Ok(())
}

/// Lista imagens raster presentes na página.
pub fn list_images(input: &Path, page_number: usize) -> AppResult<Vec<ImageInfo>> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    let mut visited = Vec::new();
    let mut found = Vec::new();
    collect_images(&doc, page, &mut visited, &mut found)?;
    let mut images = Vec::new();
    for (name, object) in found {
        let dict = unsafe { qpdf_oh_get_dict(doc.0, object) };
        let width = unsafe {
            qpdf_oh_get_numeric_value(doc.0, qpdf_oh_get_key(doc.0, dict, c"/Width".as_ptr()))
        } as i64;
        let height = unsafe {
            qpdf_oh_get_numeric_value(doc.0, qpdf_oh_get_key(doc.0, dict, c"/Height".as_ptr()))
        } as i64;
        let filter = unsafe { qpdf_oh_get_key(doc.0, dict, c"/Filter".as_ptr()) };
        let filter = if filter != 0 && unsafe { qpdf_oh_is_dictionary(doc.0, filter) } == 0 {
            let pointer = unsafe { qpdf_oh_get_name(doc.0, filter) };
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
        images.push(ImageInfo {
            name,
            object_id: unsafe { qpdf_oh_get_object_id(doc.0, object) } as i64,
            width,
            height,
            filter,
        });
    }
    Ok(images)
}

fn key_cstr(key: &str) -> CString {
    if key.starts_with('/') {
        CString::new(key.as_bytes()).expect("chave sem NUL")
    } else {
        CString::new(format!("/{key}")).expect("chave sem NUL")
    }
}

/// Resolve `/Nome` nos recursos de `owner` (página ou form): Form XObject,
/// imagem ou outro.
fn resolve_xobject(doc: &Document, page: u32, owner: u32, name: &str) -> XObject {
    let lookup = |holder: u32| -> Option<(u32, u32)> {
        let dict = if unsafe { qpdf_oh_is_stream(doc.0, holder) } != 0 {
            unsafe { qpdf_oh_get_dict(doc.0, holder) }
        } else {
            holder
        };
        let xobjects = xobject_dictionary(doc, dict).ok()?;
        let object = unsafe { qpdf_oh_get_key(doc.0, xobjects, key_cstr(name).as_ptr()) };
        (unsafe { qpdf_oh_is_stream(doc.0, object) } != 0).then_some((object, xobjects))
    };
    // Form sem /Resources próprio herda os da página.
    let Some((object, resources)) = lookup(owner).or_else(|| lookup(page)) else {
        return XObject::Other;
    };
    let dict = unsafe { qpdf_oh_get_dict(doc.0, object) };
    let subtype = unsafe { qpdf_oh_get_key(doc.0, dict, c"/Subtype".as_ptr()) };
    if unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Image".as_ptr()) } != 0 {
        return XObject::Image { resources };
    }
    if unsafe { qpdf_oh_is_name_and_equals(doc.0, subtype, c"/Form".as_ptr()) } == 0 {
        return XObject::Other;
    }
    let mut matrix = textmove::IDENTITY;
    let array = unsafe { qpdf_oh_get_key(doc.0, dict, c"/Matrix".as_ptr()) };
    if unsafe { qpdf_oh_is_array(doc.0, array) } != 0
        && unsafe { qpdf_oh_get_array_n_items(doc.0, array) } == 6
    {
        for (index, slot) in matrix.iter_mut().enumerate() {
            let item = unsafe { qpdf_oh_get_array_item(doc.0, array, index as c_int) };
            *slot = unsafe { qpdf_oh_get_numeric_value(doc.0, item) };
        }
    }
    match read_stream_data(doc, object) {
        Ok(content) => XObject::Form(FormXObject {
            key: object,
            content,
            matrix,
            resources,
        }),
        Err(_) => XObject::Other,
    }
}

/// O Form XObject `target` aparece nos recursos (diretos ou aninhados) de
/// outra página do documento.
fn form_on_other_pages(doc: &Document, current_page: usize, target: i64) -> bool {
    let pages = unsafe { qpdf_get_num_pages(doc.0) }.max(0) as usize;
    let mut visited = Vec::new();
    for index in (0..pages).filter(|index| *index != current_page) {
        let page = unsafe { qpdf_get_page_n(doc.0, index) };
        let mut stack = vec![page];
        while let Some(owner) = stack.pop() {
            let Ok(xobjects) = xobject_dictionary(doc, owner) else {
                continue;
            };
            for key in dict_keys(doc, xobjects) {
                let child = unsafe { qpdf_oh_get_key(doc.0, xobjects, key_cstr(&key).as_ptr()) };
                if unsafe { qpdf_oh_is_stream(doc.0, child) } == 0 {
                    continue;
                }
                let id = unsafe { qpdf_oh_get_object_id(doc.0, child) } as i64;
                if id == target {
                    return true;
                }
                if !visited.contains(&id) {
                    visited.push(id);
                    stack.push(unsafe { qpdf_oh_get_dict(doc.0, child) });
                }
            }
        }
    }
    false
}

/// Cria uma cópia independente do Form XObject com o conteúdo decodificado.
fn clone_form(doc: &Document, original: u32, content: &[u8]) -> AppResult<u32> {
    let stream = unsafe { qpdf_oh_new_stream(doc.0) };
    let null = unsafe { qpdf_oh_new_null(doc.0) };
    unsafe {
        qpdf_oh_replace_stream_data(doc.0, stream, content.as_ptr(), content.len(), null, null)
    };
    doc.check(0)?;
    let clone = unsafe { qpdf_make_indirect_object(doc.0, stream) };
    let source = unsafe { qpdf_oh_get_dict(doc.0, original) };
    let target = unsafe { qpdf_oh_get_dict(doc.0, clone) };
    for key in dict_keys(doc, source) {
        if matches!(key.as_str(), "/Length" | "/Filter" | "/DecodeParms" | "/DL") {
            continue;
        }
        let value = unsafe { qpdf_oh_get_key(doc.0, source, key_cstr(&key).as_ptr()) };
        doc.set(target, &key_cstr(&key), value);
    }
    doc.check(0)?;
    Ok(clone)
}

/// Nome livre no dicionário `/XObject`, derivado de `base`.
fn unique_xobject_name(doc: &Document, xobjects: u32, base: &str) -> String {
    let existing = dict_keys(doc, xobjects);
    (1..)
        .map(|index| format!("{base}_c{index}"))
        .find(|name| {
            !existing
                .iter()
                .any(|key| key.trim_start_matches('/') == name)
        })
        .expect("sequência infinita")
}

/// Alteração aplicada ao trecho de texto sob o clique.
pub enum TextEdit<'a> {
    Move {
        dx: f64,
        dy: f64,
    },
    /// Transformação afim no espaço da página (escala, rotação, translação).
    Transform(Matrix),
    Replace(&'a str),
}

/// Resultado da edição de um trecho de texto.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEditOutcome {
    /// Texto do trecho editado (antes da edição).
    pub text: String,
    /// O bloco `BT … ET` inteiro foi movido (trechos encadeados).
    pub whole_block: bool,
    /// Form XObjects compartilhados que foram copiados para não alterar outras ocorrências.
    pub cloned_forms: usize,
}

/// Abre a página e interpreta o conteúdo, com os Form XObjects resolvidos.
fn interpret_page<'d>(
    doc: &'d Document,
    page: u32,
) -> AppResult<Interpreter<impl FnMut(u32, &str) -> XObject + 'd>> {
    let content = read_page_content(doc, page)?;
    let mut interpreter =
        Interpreter::new(move |owner, name| resolve_xobject(doc, page, owner, name));
    interpreter.run(page, content, textmove::IDENTITY);
    Ok(interpreter)
}

/// Dicionário da fonte `/name` nos recursos de `owner` (página ou form); um
/// form sem `/Resources` próprio herda os da página.
fn font_dictionary(doc: &Document, page: u32, owner: u32, name: &str) -> Option<u32> {
    let lookup = |holder: u32| -> Option<u32> {
        let dict = if unsafe { qpdf_oh_is_stream(doc.0, holder) } != 0 {
            unsafe { qpdf_oh_get_dict(doc.0, holder) }
        } else {
            holder
        };
        let resources = unsafe { qpdf_oh_get_key(doc.0, dict, c"/Resources".as_ptr()) };
        let fonts = unsafe { qpdf_oh_get_key(doc.0, resources, c"/Font".as_ptr()) };
        if unsafe { qpdf_oh_is_dictionary(doc.0, fonts) } == 0 {
            return None;
        }
        let font = unsafe { qpdf_oh_get_key(doc.0, fonts, key_cstr(name).as_ptr()) };
        (unsafe { qpdf_oh_is_dictionary(doc.0, font) } != 0).then_some(font)
    };
    lookup(owner).or_else(|| lookup(page))
}

/// Codificação da fonte: o `/ToUnicode` quando existe; senão WinAnsi para
/// fontes simples com `/WinAnsiEncoding`. `None` nos demais casos (fonte
/// composta sem `/ToUnicode`, `/Differences`, Type 3, StandardEncoding…).
fn font_codec(doc: &Document, font: u32) -> Option<FontCodec> {
    let is_name = |object: u32, name: &CStr| unsafe {
        qpdf_oh_is_name_and_equals(doc.0, object, name.as_ptr()) != 0
    };
    let to_unicode = unsafe { qpdf_oh_get_key(doc.0, font, c"/ToUnicode".as_ptr()) };
    if unsafe { qpdf_oh_is_stream(doc.0, to_unicode) } != 0 {
        if let Some(map) = read_stream_data(doc, to_unicode)
            .ok()
            .and_then(|cmap| ToUnicode::parse(&cmap))
        {
            return Some(FontCodec::ToUnicode(map));
        }
    }
    let subtype = unsafe { qpdf_oh_get_key(doc.0, font, c"/Subtype".as_ptr()) };
    if is_name(subtype, c"/Type0") || is_name(subtype, c"/Type3") {
        return None;
    }
    let encoding = unsafe { qpdf_oh_get_key(doc.0, font, c"/Encoding".as_ptr()) };
    let encoding = if unsafe { qpdf_oh_is_dictionary(doc.0, encoding) } != 0 {
        let differences = unsafe { qpdf_oh_get_key(doc.0, encoding, c"/Differences".as_ptr()) };
        if unsafe { qpdf_oh_is_null(doc.0, differences) } == 0 {
            return None;
        }
        unsafe { qpdf_oh_get_key(doc.0, encoding, c"/BaseEncoding".as_ptr()) }
    } else {
        encoding
    };
    // StandardEncoding, a codificação própria de fontes embutidas e as
    // simbólicas não são Latin-1 acima de 0x7F: ficam no caminho antigo.
    is_name(encoding, c"/WinAnsiEncoding").then_some(FontCodec::WinAnsi)
}

/// Codificações das fontes usadas pelos trechos, por (stream, fonte).
struct Fonts(HashMap<(usize, String), Option<FontCodec>>);

impl Fonts {
    fn of(&self, run: &textmove::TextRunPlacement) -> Option<&FontCodec> {
        self.0
            .get(&(run.stream, run.font.clone()))
            .and_then(Option::as_ref)
    }
}

/// Resolve a fonte de cada trecho e guarda o texto decodificado por ela.
fn decode_runs<F>(doc: &Document, page: u32, interpreter: &mut Interpreter<F>) -> Fonts
where
    F: FnMut(u32, &str) -> XObject,
{
    let mut fonts = Fonts(HashMap::new());
    for run in &mut interpreter.runs {
        let owner = interpreter.streams[run.stream].0;
        let codec = fonts
            .0
            .entry((run.stream, run.font.clone()))
            .or_insert_with(|| {
                font_dictionary(doc, page, owner, &run.font).and_then(|font| font_codec(doc, font))
            });
        if let Some(codec) = codec {
            run.decoded = Some(codec.decode(&run.shown_bytes()));
        }
    }
    fonts
}

/// Texto do trecho sob (`x`, `y`) e se ele pode ser reescrito.
pub fn page_text_run(
    input: &Path,
    page_number: usize,
    x: f64,
    y: f64,
    clicked_text: &str,
) -> AppResult<Option<(String, bool)>> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    doc.check(unsafe { qpdf_push_inherited_attributes_to_page(doc.0) })?;
    let mut interpreter = interpret_page(&doc, page)?;
    let fonts = decode_runs(&doc, page, &mut interpreter);
    Ok(
        textmove::nearest_unit(&interpreter.runs, x, y, clicked_text).map(|unit| {
            // Bloco letra a letra: reescrever o texto não é suportado.
            let editable = !unit.block
                && textmove::run_is_editable(
                    &interpreter.streams[unit.run.stream].1,
                    &unit.run,
                    fonts.of(&unit.run),
                );
            (unit.text, editable)
        }),
    )
}

/// Moldura e texto do trecho sob (`x`, `y`). `words` são as palavras que o
/// poppler mede na página (caixas no espaço padrão): a largura do trecho é a
/// projeção, no eixo do texto, das palavras dele que estão na mesma faixa da
/// linha de base — vale para texto girado, que o poppler quebra em pedaços.
pub fn page_text_frame(
    input: &Path,
    page_number: usize,
    x: f64,
    y: f64,
    clicked_text: &str,
    words: &[(String, [f64; 4])],
) -> AppResult<Option<(Frame, String, bool)>> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    doc.check(unsafe { qpdf_push_inherited_attributes_to_page(doc.0) })?;
    let mut interpreter = interpret_page(&doc, page)?;
    decode_runs(&doc, page, &mut interpreter);
    let Some(unit) = textmove::nearest_unit(&interpreter.runs, x, y, clicked_text) else {
        return Ok(None);
    };
    let width = unit
        .width
        .unwrap_or_else(|| textmove::measure_run_width(&unit.run, words));
    let mut labelled = unit.run.clone();
    labelled.decoded = Some(unit.text.clone());
    let text = textmove::readable_text(&labelled, clicked_text);
    Ok(Some((
        textmove::run_frame(&unit.run, width),
        text,
        unit.block,
    )))
}

/// Moldura e texto de cada trecho de texto da página (espaço padrão), na
/// ordem do conteúdo. `words` como em [`page_text_frame`].
pub fn page_text_frames(
    input: &Path,
    page_number: usize,
    words: &[(String, [f64; 4])],
) -> AppResult<Vec<(Frame, String)>> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    doc.check(unsafe { qpdf_push_inherited_attributes_to_page(doc.0) })?;
    let mut interpreter = interpret_page(&doc, page)?;
    decode_runs(&doc, page, &mut interpreter);
    Ok(textmove::text_units(&interpreter.runs)
        .into_iter()
        .filter(|unit| !unit.text.trim().is_empty())
        .map(|unit| {
            let width = unit
                .width
                .unwrap_or_else(|| textmove::measure_run_width(&unit.run, words));
            let text = textmove::readable_text(&unit.run, "");
            (textmove::run_frame(&unit.run, width), text)
        })
        .collect())
}

/// Imagem desenhada sob (`x`, `y`): nome e moldura da ocorrência mais acima.
pub fn page_image_at(
    input: &Path,
    page_number: usize,
    x: f64,
    y: f64,
) -> AppResult<Option<(String, Frame)>> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    let interpreter = interpret_page(&doc, page)?;
    Ok(interpreter
        .images
        .iter()
        .rev()
        .find(|draw| draw.frame().contains((x, y), 2.0))
        .map(|draw| (draw.name.clone(), draw.frame())))
}

/// Copia (copy-on-write) os Form XObjects compartilhados no caminho até
/// `stream` e aplica `edit` ao conteúdo dele. Devolve quantos forms foram copiados.
fn edit_stream<F>(
    doc: &Document,
    page: u32,
    page_number: usize,
    interpreter: &Interpreter<F>,
    stream: usize,
    edit: impl FnOnce(&[u8]) -> AppResult<Vec<u8>>,
) -> AppResult<usize>
where
    F: FnMut(u32, &str) -> XObject,
{
    let object_id =
        |index: usize| unsafe { qpdf_oh_get_object_id(doc.0, interpreter.streams[index].0) as i64 };
    // Conteúdo atualizado e destino de escrita de cada stream alterado.
    let mut updated: Vec<(usize, u32, Vec<u8>)> = Vec::new();
    let mut targets: Vec<u32> = interpreter.streams.iter().map(|(key, _)| *key).collect();
    let mut cloned_forms = 0;
    let mut clone_below = false;
    for current in interpreter.invocation_path(stream) {
        let invocation = interpreter.invocations[current].as_ref().unwrap();
        let id = object_id(current);
        let uses_on_page = (1..interpreter.streams.len())
            .filter(|other| object_id(*other) == id)
            .count();
        let shared =
            clone_below || uses_on_page > 1 || form_on_other_pages(doc, page_number - 1, id);
        if !shared {
            continue;
        }
        // Cópia do form, registrada com nome novo; só este `Do` passa a usá-la.
        let clone = clone_form(
            doc,
            interpreter.streams[current].0,
            &interpreter.streams[current].1,
        )?;
        let name = unique_xobject_name(doc, invocation.resources, &invocation.name);
        doc.set(invocation.resources, &key_cstr(&name), clone);
        doc.check(0)?;
        let parent = invocation.parent;
        let parent_content = updated
            .iter()
            .position(|(index, _, _)| *index == parent)
            .map(|position| updated.remove(position).2)
            .unwrap_or_else(|| interpreter.streams[parent].1.clone());
        let (start, end) = invocation.name_span;
        let mut renamed = parent_content[..start].to_vec();
        renamed.extend_from_slice(format!("/{}", textmove::encode_name(&name)).as_bytes());
        renamed.extend_from_slice(&parent_content[end..]);
        updated.push((parent, targets[parent], renamed));
        targets[current] = clone;
        cloned_forms += 1;
        clone_below = true;
    }
    let edited = edit(&interpreter.streams[stream].1)?;
    updated.push((stream, targets[stream], edited));
    for (index, target, content) in &updated {
        if *index == 0 {
            write_page_content(doc, page, content)?;
        } else {
            write_stream_content(doc, *target, content)?;
        }
    }
    Ok(cloned_forms)
}

/// Edita somente o trecho de texto desenhado perto de (`x`, `y`) — pontos no
/// espaço padrão da página.
///
/// Se o trecho está em um Form XObject desenhado mais de uma vez (nesta ou em
/// outra página), o form é copiado (copy-on-write) e só o `Do` desta
/// ocorrência passa a apontar para a cópia; o mesmo vale para os forms
/// aninhados no caminho até o texto.
pub fn edit_page_text(
    input: &Path,
    output: &Path,
    page_number: usize,
    x: f64,
    y: f64,
    clicked_text: &str,
    edit: TextEdit,
) -> AppResult<TextEditOutcome> {
    let transform = match edit {
        TextEdit::Move { dx, dy } => Some(textmove::translation(dx, dy)),
        TextEdit::Transform(matrix) => Some(matrix),
        TextEdit::Replace(_) => None,
    };
    if transform.is_some_and(|matrix| matrix.iter().any(|value| !value.is_finite())) {
        return Err("Transformação inválida".into());
    }
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    // Recursos herdados de /Pages passam para cada página antes da análise.
    doc.check(unsafe { qpdf_push_inherited_attributes_to_page(doc.0) })?;
    let mut interpreter = interpret_page(&doc, page)?;
    let fonts = decode_runs(&doc, page, &mut interpreter);
    let unit = textmove::nearest_unit(&interpreter.runs, x, y, clicked_text)
        .ok_or("Texto não localizado no conteúdo da página")?;
    if unit.block && matches!(edit, TextEdit::Replace(_)) {
        return Err(
            "Este texto é desenhado letra a letra (ex.: PDF do Google Docs): mover, \
             redimensionar e girar funcionam, mas reescrever o texto não é suportado"
                .into(),
        );
    }
    let run = unit.run;
    let mut whole_block = false;
    let cloned_forms = edit_stream(
        &doc,
        page,
        page_number,
        &interpreter,
        run.stream,
        |source| match (&edit, transform) {
            (TextEdit::Replace(text), _) => {
                textmove::replace_run_text(source, &run, text, fonts.of(&run))
            }
            (_, Some(matrix)) => {
                let (edited, whole) = textmove::transform_run(source, &run, &matrix)?;
                // Numa palavra desenhada letra a letra o bloco é a própria unidade.
                whole_block = whole && !unit.block;
                Ok(edited)
            }
            _ => unreachable!(),
        },
    )?;
    drop(interpreter);
    finish(&doc, output)?;
    Ok(TextEditOutcome {
        text: unit.text,
        whole_block,
        cloned_forms,
    })
}

/// Uma ocorrência (desenho `/Nome Do`) de imagem na página.
#[derive(Debug, Clone)]
pub struct ImagePlacement {
    pub name: String,
    /// Posição entre os desenhos do mesmo nome na página (1, 2, …).
    pub occurrence: usize,
    /// Caixa alinhada em pontos (espaço padrão): canto inferior esquerdo e tamanho.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Moldura orientada (acompanha rotação e escala).
    pub frame: Frame,
}

/// Quais desenhos de imagem uma operação atinge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Occurrence {
    /// A única ocorrência do nome; erro se houver mais de uma.
    Only,
    /// A ocorrência `n` (1, 2, …) na ordem de desenho.
    Index(usize),
    /// Todas as ocorrências do nome nesta página.
    All,
}

/// Alvo de uma operação de imagem: ponto na página ou nome + ocorrência.
#[derive(Debug, Clone, Copy)]
pub enum ImageTarget<'a> {
    At {
        x: f64,
        y: f64,
    },
    Named {
        name: &'a str,
        occurrence: Occurrence,
    },
}

/// Resultado de uma operação sobre desenhos de imagem.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageEditOutcome {
    /// Quantos desenhos foram alterados.
    pub draws: usize,
    /// Form XObjects compartilhados copiados (copy-on-write).
    pub cloned_forms: usize,
}

/// Ocorrências de imagem da página na ordem de desenho, com caixa e moldura.
/// Segmentos retos pintados na página (espaço padrão, origem embaixo).
pub fn page_segments(input: &Path, page_number: usize) -> AppResult<Vec<textmove::Segment>> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    let segments = interpret_page(&doc, page)?.segments;
    Ok(segments)
}

pub fn image_placements(input: &Path, page_number: usize) -> AppResult<Vec<ImagePlacement>> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    let interpreter = interpret_page(&doc, page)?;
    let mut counts: Vec<(String, usize)> = Vec::new();
    Ok(interpreter
        .images
        .iter()
        .map(|draw| {
            let occurrence = match counts.iter_mut().find(|(name, _)| *name == draw.name) {
                Some((_, count)) => {
                    *count += 1;
                    *count
                }
                None => {
                    counts.push((draw.name.clone(), 1));
                    1
                }
            };
            let frame = draw.frame();
            let [x0, y0, x1, y1] = frame.bounds();
            ImagePlacement {
                name: draw.name.clone(),
                occurrence,
                x: x0,
                y: y0,
                width: x1 - x0,
                height: y1 - y0,
                frame,
            }
        })
        .collect())
}

/// Índices (em `interpreter.images`) dos desenhos atingidos por `target`.
fn image_targets<F>(interpreter: &Interpreter<F>, target: ImageTarget) -> AppResult<Vec<usize>>
where
    F: FnMut(u32, &str) -> XObject,
{
    match target {
        ImageTarget::At { x, y } => interpreter
            .images
            .iter()
            .rposition(|draw| draw.frame().contains((x, y), 2.0))
            .map(|index| vec![index])
            .ok_or_else(|| "Nenhuma imagem neste ponto".into()),
        ImageTarget::Named { name, occurrence } => {
            let name = name.trim_start_matches('/');
            let draws: Vec<usize> = interpreter
                .images
                .iter()
                .enumerate()
                .filter(|(_, draw)| draw.name == name)
                .map(|(index, _)| index)
                .collect();
            if draws.is_empty() {
                return Err(format!("Imagem '{name}' não desenhada nesta página"));
            }
            match occurrence {
                Occurrence::All => Ok(draws),
                Occurrence::Only if draws.len() == 1 => Ok(draws),
                Occurrence::Only => Err(format!(
                    "Imagem '{name}' é desenhada {} vezes nesta página: informe a ocorrência (1..={}) ou 'todas'",
                    draws.len(),
                    draws.len()
                )),
                Occurrence::Index(n) => draws
                    .get(n.wrapping_sub(1))
                    .map(|index| vec![*index])
                    .ok_or_else(|| {
                        format!(
                            "Ocorrência {n} inválida: '{name}' é desenhada {} vez(es)",
                            draws.len()
                        )
                    }),
            }
        }
    }
}

/// Abre a página, localiza os desenhos de `target` e aplica `edit` a cada um,
/// com copy-on-write dos forms do caminho. Os desenhos são tratados do último
/// para o primeiro, reinterpretando a página a cada passo (remoções não
/// deslocam os índices dos que faltam).
fn edit_image_draws(
    input: &Path,
    output: &Path,
    page_number: usize,
    target: ImageTarget,
    mut edit: impl FnMut(&Document, &textmove::ImageDraw, &[u8]) -> AppResult<Vec<u8>>,
    finish_edit: impl FnOnce(&Document, u32, usize, &[textmove::ImageDraw]) -> AppResult<()>,
) -> AppResult<ImageEditOutcome> {
    let doc = Document::new()?;
    let page = open_page(&doc, input, page_number)?;
    doc.check(unsafe { qpdf_push_inherited_attributes_to_page(doc.0) })?;
    let indices = image_targets(&interpret_page(&doc, page)?, target)?;
    let mut outcome = ImageEditOutcome::default();
    let mut edited = Vec::new();
    for index in indices.into_iter().rev() {
        let interpreter = interpret_page(&doc, page)?;
        let draw = interpreter.images[index].clone();
        outcome.cloned_forms += edit_stream(
            &doc,
            page,
            page_number,
            &interpreter,
            draw.stream,
            |source| edit(&doc, &draw, source),
        )?;
        outcome.draws += 1;
        edited.push(draw);
    }
    finish_edit(&doc, page, page_number - 1, &edited)?;
    finish(&doc, output)?;
    Ok(outcome)
}

/// Aplica `transform` (espaço padrão da página) aos desenhos de `target`.
pub fn transform_page_images(
    input: &Path,
    output: &Path,
    page_number: usize,
    target: ImageTarget,
    transform: &Matrix,
) -> AppResult<ImageEditOutcome> {
    if transform.iter().any(|value| !value.is_finite()) {
        return Err("Transformação inválida".into());
    }
    edit_image_draws(
        input,
        output,
        page_number,
        target,
        |_, draw, source| textmove::transform_draw(source, draw, transform),
        |_, _, _, _| Ok(()),
    )
}

/// Cria um XObject de imagem novo a partir de um PNG/JPEG.
fn new_image_object(doc: &Document, image_path: &Path) -> AppResult<u32> {
    let (width, height, rgb) = images::load_rgb(image_path)?;
    let compressed = images::compress(&rgb)?;
    let stream = unsafe { qpdf_oh_new_stream(doc.0) };
    let object = unsafe { qpdf_make_indirect_object(doc.0, stream) };
    let dict = unsafe { qpdf_oh_get_dict(doc.0, object) };
    doc.set(dict, c"/Type", doc.name(c"/XObject"));
    doc.set(dict, c"/Subtype", doc.name(c"/Image"));
    doc.set(dict, c"/Width", unsafe {
        qpdf_oh_new_integer(doc.0, width as i64)
    });
    doc.set(dict, c"/Height", unsafe {
        qpdf_oh_new_integer(doc.0, height as i64)
    });
    doc.set(dict, c"/ColorSpace", doc.name(c"/DeviceRGB"));
    doc.set(dict, c"/BitsPerComponent", unsafe {
        qpdf_oh_new_integer(doc.0, 8)
    });
    doc.set(dict, c"/Filter", doc.name(c"/FlateDecode"));
    write_stream_content(doc, object, &compressed)?;
    doc.check(0)?;
    Ok(object)
}

/// Troca a imagem dos desenhos de `target` por um PNG/JPEG novo. Cria um
/// XObject novo (o original continua valendo para as outras ocorrências e
/// páginas) e só os desenhos atingidos passam a apontar para ele.
pub fn replace_page_images(
    input: &Path,
    output: &Path,
    page_number: usize,
    target: ImageTarget,
    image_path: &Path,
) -> AppResult<ImageEditOutcome> {
    let mut object = None;
    // Nome registrado em cada dicionário /XObject (reaproveitado entre desenhos).
    let mut registered: Vec<(u32, String)> = Vec::new();
    edit_image_draws(
        input,
        output,
        page_number,
        target,
        |doc, draw, source| {
            let image = match object {
                Some(image) => image,
                None => *object.insert(new_image_object(doc, image_path)?),
            };
            let name = match registered.iter().find(|(dict, _)| *dict == draw.resources) {
                Some((_, name)) => name.clone(),
                None => {
                    let name = unique_xobject_name(doc, draw.resources, &draw.name);
                    doc.set(draw.resources, &key_cstr(&name), image);
                    doc.check(0)?;
                    registered.push((draw.resources, name.clone()));
                    name
                }
            };
            Ok(textmove::rename_draw(source, draw, &name))
        },
        remove_unused_images,
    )
}

/// Remove os desenhos de `target`. O recurso da imagem só sai do dicionário
/// quando nada mais na página o desenha e nenhuma outra página o referencia.
pub fn remove_page_images(
    input: &Path,
    output: &Path,
    page_number: usize,
    target: ImageTarget,
) -> AppResult<ImageEditOutcome> {
    edit_image_draws(
        input,
        output,
        page_number,
        target,
        |_, draw, source| Ok(textmove::remove_draw(source, draw)),
        remove_unused_images,
    )
}

/// Tira do dicionário de recursos as imagens dos desenhos editados que não
/// são mais desenhadas na página nem referenciadas por outra página.
fn remove_unused_images(
    doc: &Document,
    page: u32,
    page_index: usize,
    edited: &[textmove::ImageDraw],
) -> AppResult<()> {
    let interpreter = interpret_page(doc, page)?;
    for draw in edited {
        let key = key_cstr(&draw.name);
        let object = unsafe { qpdf_oh_get_key(doc.0, draw.resources, key.as_ptr()) };
        if unsafe { qpdf_oh_is_stream(doc.0, object) } == 0 {
            continue;
        }
        let id = unsafe { qpdf_oh_get_object_id(doc.0, object) } as i64;
        let still_drawn = interpreter.images.iter().any(|other| {
            let other_object =
                unsafe { qpdf_oh_get_key(doc.0, other.resources, key_cstr(&other.name).as_ptr()) };
            let other_id = unsafe { qpdf_oh_get_object_id(doc.0, other_object) } as i64;
            other_id == id
        });
        if !still_drawn && !form_on_other_pages(doc, page_index, id) {
            unsafe { qpdf_oh_remove_key(doc.0, draw.resources, key.as_ptr()) };
            doc.check(0)?;
        }
    }
    Ok(())
}
