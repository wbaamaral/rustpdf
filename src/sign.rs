// SPDX-License-Identifier: AGPL-3.0-or-later
//! Assinatura digital de PDF (PKCS#7 detached, Adobe.PPKLite).
//!
//! O fluxo é: gerar o PDF com o campo `/Sig` e um `/Contents` reservado via qpdf,
//! recalcular `/ByteRange` sobre os bytes finais, assinar o intervalo com
//! `openssl cms` e preencher o espaço reservado com o hex do CMS, preenchido
//! com zeros à direita (como manda a especificação PDF).
use crate::AppResult;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fs;
use std::path::Path;
use std::process::Command;

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
    fn qpdf_oh_get_key(qpdf: *mut c_void, object: u32, key: *const c_char) -> u32;
    fn qpdf_oh_is_array(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_is_dictionary(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_is_string(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_is_name_and_equals(qpdf: *mut c_void, object: u32, name: *const c_char) -> c_int;
    fn qpdf_oh_get_utf8_value(qpdf: *mut c_void, object: u32) -> *const c_char;
    fn qpdf_oh_get_array_n_items(qpdf: *mut c_void, object: u32) -> c_int;
    fn qpdf_oh_get_array_item(qpdf: *mut c_void, object: u32, index: c_int) -> u32;
    fn qpdf_oh_get_numeric_value(qpdf: *mut c_void, object: u32) -> f64;
    fn qpdf_oh_new_array(qpdf: *mut c_void) -> u32;
    fn qpdf_oh_new_dictionary(qpdf: *mut c_void) -> u32;
    fn qpdf_oh_new_name(qpdf: *mut c_void, name: *const c_char) -> u32;
    fn qpdf_oh_new_string(qpdf: *mut c_void, value: *const c_char) -> u32;
    fn qpdf_oh_new_stream(qpdf: *mut c_void) -> u32;
    fn qpdf_oh_new_integer(qpdf: *mut c_void, value: i64) -> u32;
    fn qpdf_oh_new_real_from_double(qpdf: *mut c_void, value: f64, decimals: c_int) -> u32;
    fn qpdf_oh_new_unicode_string(qpdf: *mut c_void, value: *const c_char) -> u32;
    fn qpdf_oh_append_item(qpdf: *mut c_void, array: u32, item: u32);
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

/// Capacidade do CMS DER embutido no `/Contents` (em bytes).
const DER_CAPACITY: usize = 8192;
/// Comprimento do hex de `/Contents` — o espaço reservado tem este tamanho.
const HEX_CAPACITY: usize = DER_CAPACITY * 2;
/// Prefixo único do espaço reservado, para localizá-lo no PDF escrito.
const MARKER: &[u8] = b"RSPDFSIGN";
/// Largura fixa dos quatro inteiros de `/ByteRange` (para remendar sem mudar o tamanho).
const BYTE_RANGE_DIGITS: usize = 10;

#[derive(Debug, Clone)]
pub struct SignatureInfo {
    pub field_name: String,
    pub signer: String,
    pub reason: String,
    pub location: String,
    pub date: String,
    pub byte_range: Vec<i64>,
    pub contents_len: usize,
}

struct Document(*mut c_void);

impl Document {
    fn new() -> AppResult<Self> {
        let pointer = unsafe { qpdf_init() };
        if pointer.is_null() {
            return Err("Não foi possível iniciar o qpdf".into());
        }
        Ok(Self(pointer))
    }

    fn check(&self, code: c_int) -> AppResult<()> {
        if code & 2 == 0 && unsafe { qpdf_has_error(self.0) } == 0 {
            return Ok(());
        }
        let error = unsafe { qpdf_get_error(self.0) };
        let text = unsafe { qpdf_get_error_full_text(self.0, error) };
        let message = if text.is_null() {
            format!("qpdf: código {code}")
        } else {
            unsafe { CStr::from_ptr(text) }
                .to_string_lossy()
                .into_owned()
        };
        Err(message)
    }

    fn set(&self, object: u32, key: &CStr, value: u32) {
        unsafe { qpdf_oh_replace_key(self.0, object, key.as_ptr(), value) };
    }

    fn name(&self, name: &CStr) -> u32 {
        unsafe { qpdf_oh_new_name(self.0, name.as_ptr()) }
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        unsafe {
            let mut pointer = self.0;
            qpdf_cleanup(&mut pointer);
        }
    }
}

fn path_c(path: &Path) -> AppResult<CString> {
    crate::path_to_cstring(path)
}

fn cstring(value: &str, what: &str) -> AppResult<CString> {
    CString::new(value).map_err(|_| format!("{what} contém byte NUL"))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn pdf_date_now() -> String {
    // Formato PDF: D:AAAAMMDDHHmmSSZ (UTC). Usa a data do sistema via `date` do GNU.
    let out = Command::new("date")
        .args(["-u", "+%Y%m%d%H%M%SZ"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| s.len() == 15)
        .unwrap_or_else(|| "19700101000000Z".into());
    format!("D:{out}")
}

fn placeholder_bytes() -> Vec<u8> {
    let mut data = Vec::with_capacity(HEX_CAPACITY);
    data.extend_from_slice(MARKER);
    data.extend(std::iter::repeat(b'0').take(HEX_CAPACITY - MARKER.len()));
    data
}

fn byte_range_token() -> String {
    format!(
        "/ByteRange [{} {} {} {}]",
        "0".repeat(BYTE_RANGE_DIGITS),
        "0".repeat(BYTE_RANGE_DIGITS),
        "0".repeat(BYTE_RANGE_DIGITS),
        "0".repeat(BYTE_RANGE_DIGITS)
    )
}

fn format_byte_range(values: [usize; 4]) -> String {
    format!(
        "/ByteRange [{} {} {} {}]",
        format!("{:0width$}", values[0], width = BYTE_RANGE_DIGITS),
        format!("{:0width$}", values[1], width = BYTE_RANGE_DIGITS),
        format!("{:0width$}", values[2], width = BYTE_RANGE_DIGITS),
        format!("{:0width$}", values[3], width = BYTE_RANGE_DIGITS),
    )
}

fn hex_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for byte in data {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

fn hex_decode(text: &str) -> AppResult<Vec<u8>> {
    let clean: Vec<u8> = text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if clean.len() % 2 != 0 {
        return Err("Hex de assinatura com comprimento ímpar".into());
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for pair in clean.chunks(2) {
        let hi = (pair[0] as char)
            .to_digit(16)
            .ok_or("Hex de assinatura inválido")?;
        let lo = (pair[1] as char)
            .to_digit(16)
            .ok_or("Hex de assinatura inválido")?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

/// Monta o PDF com campo de assinatura e `/Contents` reservado, depois assina.
pub fn sign_pdf(
    input: &Path,
    output: &Path,
    page_number: usize,
    rect: [f64; 4],
    cert: &Path,
    key: &Path,
    reason: &str,
    signer_name: &str,
    location: &str,
) -> AppResult<()> {
    if page_number == 0 {
        return Err("Número de página inválido".into());
    }
    if rect.iter().any(|value| !value.is_finite()) || rect[2] <= rect[0] || rect[3] <= rect[1] {
        return Err("Retângulo da assinatura inválido".into());
    }
    if !cert.is_file() || !key.is_file() {
        return Err("Certificado ou chave privada não encontrado".into());
    }
    let signer = signer_name.trim();
    if signer.is_empty() {
        return Err("Nome do assinante inválido".into());
    }
    let reason = reason.trim();

    let field_name = "Assinatura";
    let date = pdf_date_now();
    let placeholder = placeholder_bytes();
    let placeholder_c = CString::new(placeholder.clone()).map_err(|_| "Reserva inválida")?;
    let signer_c = cstring(signer, "Nome do assinante")?;
    let reason_c = cstring(reason, "Motivo")?;
    let location_c = cstring(location, "Local")?;
    let date_c = cstring(&date, "Data")?;
    let field_c = cstring(field_name, "Nome do campo")?;

    let unsigned = output.with_extension("unsigned.pdf");
    {
        let input_c = path_c(input)?;
        let unsigned_c = path_c(&unsigned)?;
        let doc = Document::new()?;
        doc.check(unsafe { qpdf_read(doc.0, input_c.as_ptr(), std::ptr::null()) })?;
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

        // Dicionário da assinatura (/V) com reserva de /Contents e /ByteRange.
        let sig = unsafe { qpdf_make_indirect_object(doc.0, qpdf_oh_new_dictionary(doc.0)) };
        doc.set(sig, c"/Type", doc.name(c"/Sig"));
        doc.set(sig, c"/Filter", doc.name(c"/Adobe.PPKLite"));
        doc.set(sig, c"/SubFilter", doc.name(c"/adbe.pkcs7.detached"));
        doc.set(sig, c"/Name", unsafe {
            qpdf_oh_new_unicode_string(doc.0, signer_c.as_ptr())
        });
        doc.set(sig, c"/M", unsafe {
            qpdf_oh_new_string(doc.0, date_c.as_ptr())
        });
        doc.set(sig, c"/Reason", unsafe {
            qpdf_oh_new_unicode_string(doc.0, reason_c.as_ptr())
        });
        doc.set(sig, c"/Location", unsafe {
            qpdf_oh_new_unicode_string(doc.0, location_c.as_ptr())
        });
        doc.set(sig, c"/Contents", unsafe {
            qpdf_oh_new_string(doc.0, placeholder_c.as_ptr())
        });
        doc.set(sig, c"/ByteRange", unsafe {
            qpdf_oh_new_string(doc.0, cstring(&byte_range_token(), "ByteRange")?.as_ptr())
        });

        // Widget do campo de assinatura.
        let resources = ensure_signature_font(&doc, form);
        let widget = unsafe { qpdf_oh_new_dictionary(doc.0) };
        doc.set(widget, c"/Type", doc.name(c"/Annot"));
        doc.set(widget, c"/Subtype", doc.name(c"/Widget"));
        doc.set(widget, c"/FT", doc.name(c"/Sig"));
        doc.set(widget, c"/T", unsafe {
            qpdf_oh_new_unicode_string(doc.0, field_c.as_ptr())
        });
        doc.set(widget, c"/F", unsafe { qpdf_oh_new_integer(doc.0, 4) });
        doc.set(widget, c"/P", page);
        doc.set(widget, c"/V", sig);
        let rectangle = unsafe { qpdf_oh_new_array(doc.0) };
        for coordinate in rect {
            let number = unsafe { qpdf_oh_new_real_from_double(doc.0, coordinate, 3) };
            unsafe { qpdf_oh_append_item(doc.0, rectangle, number) };
        }
        doc.set(widget, c"/Rect", rectangle);
        set_signature_appearance(&doc, widget, resources, rect, signer, &date, reason)?;

        let indirect = unsafe { qpdf_make_indirect_object(doc.0, widget) };
        unsafe { qpdf_oh_append_item(doc.0, fields, indirect) };
        let mut annots = unsafe { qpdf_oh_get_key(doc.0, page, c"/Annots".as_ptr()) };
        if unsafe { qpdf_oh_is_array(doc.0, annots) } == 0 {
            annots = unsafe { qpdf_oh_new_array(doc.0) };
            doc.set(page, c"/Annots", annots);
        }
        unsafe { qpdf_oh_append_item(doc.0, annots, indirect) };
        doc.check(0)?;
        doc.check(unsafe { qpdf_init_write(doc.0, unsigned_c.as_ptr()) })?;
        doc.check(unsafe { qpdf_write(doc.0) })?;
    }

    let mut bytes = fs::read(&unsigned).map_err(|e| format!("Leitura do PDF: {e}"))?;
    let _ = fs::remove_file(&unsigned);

    // Localiza o espaço reservado de /Contents e o token de /ByteRange.
    let placeholder_start = find_subslice(&bytes, &placeholder)
        .ok_or("Não foi possível localizar o espaço reservado da assinatura")?;
    let contents_start = placeholder_start + 1; // primeiro hex (após '<')
    let contents_end = contents_start + HEX_CAPACITY; // '>' fica aqui
    if placeholder_start == 0
        || bytes.get(placeholder_start - 1) != Some(&b'<')
        || bytes.get(contents_end) != Some(&b'>')
    {
        return Err("Estrutura do espaço reservado de assinatura inválida".into());
    }

    let range_token = byte_range_token();
    let range_pos = find_subslice(&bytes, range_token.as_bytes())
        .ok_or("Não foi possível localizar o /ByteRange da assinatura")?;
    let file_len = bytes.len();
    let values = [0usize, contents_start, contents_end, file_len - contents_end];
    let new_token = format_byte_range(values);
    if new_token.len() != range_token.len() {
        return Err("Tamanho do /ByteRange inconsistente".into());
    }
    bytes[range_pos..range_pos + range_token.len()].copy_from_slice(new_token.as_bytes());

    // Dados assinados: tudo menos os dígitos hex de /Contents.
    let mut signed = Vec::with_capacity(file_len - HEX_CAPACITY);
    signed.extend_from_slice(&bytes[0..contents_start]);
    signed.extend_from_slice(&bytes[contents_end..]);

    let work_dir = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(std::env::temp_dir);
    let signed_path = work_dir.join(format!("rustpdf-signed-{}.bin", std::process::id()));
    let sig_path = work_dir.join(format!("rustpdf-sig-{}.der", std::process::id()));
    fs::write(&signed_path, &signed).map_err(|e| format!("Escrita do conteúdo a assinar: {e}"))?;

    let sign_result = Command::new("openssl")
        .args([
            "cms",
            "-sign",
            "-binary",
            "-in",
        ])
        .arg(&signed_path)
        .args(["-signer"])
        .arg(cert)
        .args(["-inkey"])
        .arg(key)
        .args([
            "-outform",
            "DER",
            "-out",
        ])
        .arg(&sig_path)
        .args(["-md", "sha256"])
        .output();
    let _ = fs::remove_file(&signed_path);
    let sign_output = match sign_result {
        Ok(output) => output,
        Err(e) => {
            let _ = fs::remove_file(&sig_path);
            return Err(format!("Não foi possível executar openssl: {e}"));
        }
    };
    if !sign_output.status.success() {
        let _ = fs::remove_file(&sig_path);
        let detail = String::from_utf8_lossy(&sign_output.stderr);
        return Err(format!("openssl falhou ao assinar: {}", detail.trim()));
    }
    let signature = fs::read(&sig_path).map_err(|e| {
        let _ = fs::remove_file(&sig_path);
        format!("Leitura da assinatura: {e}")
    })?;
    let _ = fs::remove_file(&sig_path);
    if signature.is_empty() {
        return Err("openssl gerou assinatura vazia".into());
    }
    if signature.len() > DER_CAPACITY {
        return Err(format!(
            "Assinatura grande demais para o espaço reservado ({} > {DER_CAPACITY} bytes)",
            signature.len()
        ));
    }

    // Preenche o hex da assinatura com zeros à direita até HEX_CAPACITY.
    let mut hex = hex_encode(&signature);
    hex.push_str(&"0".repeat(HEX_CAPACITY - hex.len()));
    bytes[contents_start..contents_end].copy_from_slice(hex.as_bytes());

    fs::write(output, &bytes).map_err(|e| format!("Escrita do PDF assinado: {e}"))
}

fn ensure_signature_font(doc: &Document, form: u32) -> u32 {
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

fn set_signature_appearance(
    doc: &Document,
    widget: u32,
    resources: u32,
    rect: [f64; 4],
    signer: &str,
    date: &str,
    reason: &str,
) -> AppResult<()> {
    doc.set(widget, c"/DA", unsafe {
        qpdf_oh_new_string(doc.0, c"/RustPDFHelv 9 Tf 0 g".as_ptr())
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

    let mut content = format!(
        "q 0.95 0.95 1 rg 0 0 {width:.3} {height:.3} re f 0.15 0.25 0.55 RG 1.2 w 0.6 0.6 {:.3} {:.3} re S BT /RustPDFHelv 9 Tf 0 g",
        width - 1.2,
        height - 1.2
    )
    .into_bytes();
    let lines = [
        "Assinado digitalmente por:".to_string(),
        signer.to_string(),
        format!("Data: {}", date.trim_start_matches("D:")),
        if reason.is_empty() {
            String::new()
        } else {
            format!("Motivo: {reason}")
        },
    ];
    let mut y = height - 12.0;
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        content.extend_from_slice(format!(" 1 0 0 1 5 {y:.3} Tm (").as_bytes());
        content.extend_from_slice(&crate::dominio::ocr::pdf_text(line));
        content.extend_from_slice(b") Tj");
        y -= 11.0;
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

/// Lista campos de assinatura presentes no PDF.
pub fn list_signatures(input: &Path) -> AppResult<Vec<SignatureInfo>> {
    let input_c = path_c(input)?;
    let doc = Document::new()?;
    doc.check(unsafe { qpdf_read(doc.0, input_c.as_ptr(), std::ptr::null()) })?;
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
        let field_type = unsafe { qpdf_oh_get_key(doc.0, field, c"/FT".as_ptr()) };
        if unsafe { qpdf_oh_is_name_and_equals(doc.0, field_type, c"/Sig".as_ptr()) } == 0 {
            continue;
        }
        let title = unsafe { qpdf_oh_get_key(doc.0, field, c"/T".as_ptr()) };
        let field_name = if unsafe { qpdf_oh_is_string(doc.0, title) } != 0 {
            let pointer = unsafe { qpdf_oh_get_utf8_value(doc.0, title) };
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
        let value = unsafe { qpdf_oh_get_key(doc.0, field, c"/V".as_ptr()) };
        if unsafe { qpdf_oh_is_dictionary(doc.0, value) } == 0 {
            result.push(SignatureInfo {
                field_name,
                signer: String::new(),
                reason: String::new(),
                location: String::new(),
                date: String::new(),
                byte_range: Vec::new(),
                contents_len: 0,
            });
            continue;
        }
        let signer = read_text(&doc, value, c"/Name");
        let reason = read_text(&doc, value, c"/Reason");
        let location = read_text(&doc, value, c"/Location");
        let date = read_text(&doc, value, c"/M");
        let mut byte_range = Vec::new();
        let range = unsafe { qpdf_oh_get_key(doc.0, value, c"/ByteRange".as_ptr()) };
        if unsafe { qpdf_oh_is_array(doc.0, range) } != 0 {
            let n = unsafe { qpdf_oh_get_array_n_items(doc.0, range) };
            for i in 0..n {
                let item = unsafe { qpdf_oh_get_array_item(doc.0, range, i) };
                byte_range.push(unsafe { qpdf_oh_get_numeric_value(doc.0, item) } as i64);
            }
        }
        let contents_len = {
            // O /Contents é string (hex ou literal); medimos pelo valor textual.
            let contents = unsafe { qpdf_oh_get_key(doc.0, value, c"/Contents".as_ptr()) };
            if unsafe { qpdf_oh_is_string(doc.0, contents) } != 0 {
                let pointer = unsafe { qpdf_oh_get_utf8_value(doc.0, contents) };
                if pointer.is_null() {
                    0
                } else {
                    unsafe { CStr::from_ptr(pointer) }.to_bytes().len()
                }
            } else {
                0
            }
        };
        result.push(SignatureInfo {
            field_name,
            signer,
            reason,
            location,
            date,
            byte_range,
            contents_len,
        });
    }
    doc.check(0)?;
    Ok(result)
}

fn read_text(doc: &Document, dict: u32, key: &CStr) -> String {
    let value = unsafe { qpdf_oh_get_key(doc.0, dict, key.as_ptr()) };
    if unsafe { qpdf_oh_is_string(doc.0, value) } == 0 {
        return String::new();
    }
    let pointer = unsafe { qpdf_oh_get_utf8_value(doc.0, value) };
    if pointer.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}

/// Extrai o par (dados assinados, CMS DER) da primeira assinatura do PDF.
fn extract_signature_payload(input: &Path) -> AppResult<(Vec<u8>, Vec<u8>, SignatureInfo)> {
    let bytes = fs::read(input).map_err(|e| format!("Leitura do PDF: {e}"))?;
    let signatures = list_signatures(input)?;
    let info = signatures
        .into_iter()
        .find(|s| s.byte_range.len() == 4 && s.contents_len > 0)
        .ok_or("O PDF não contém assinatura válida")?;
    let ranges = [
        info.byte_range[0] as usize,
        info.byte_range[1] as usize,
        info.byte_range[2] as usize,
        info.byte_range[3] as usize,
    ];
    if ranges[0] != 0
        || ranges[1] > bytes.len()
        || ranges[2] > bytes.len()
        || ranges[2] + ranges[3] > bytes.len()
        || ranges[1] > ranges[2]
    {
        return Err("ByteRange da assinatura inválido".into());
    }
    // Localiza o hex de /Contents: começa logo após o primeiro intervalo.
    let contents_start = ranges[1];
    let contents_end = ranges[2];
    if contents_end <= contents_start || bytes.get(contents_end) != Some(&b'>') {
        return Err("Estrutura de /Contents inválida".into());
    }
    let hex = &bytes[contents_start + 1..contents_end];
    // Tira o preenchimento de zeros à direita para recuperar o DER.
    let der = hex_decode(&String::from_utf8_lossy(hex))?;
    let der = match der.iter().rposition(|b| *b != 0) {
        Some(last) => der[..=last].to_vec(),
        None => return Err("Assinatura CMS vazia".into()),
    };
    let mut signed = Vec::with_capacity(ranges[1] + ranges[3]);
    signed.extend_from_slice(&bytes[0..ranges[1]]);
    signed.extend_from_slice(&bytes[contents_end..]);
    Ok((signed, der, info))
}

/// Verifica a primeira assinatura do PDF com `openssl cms`.
///
/// Retorna `(válido, informações)`. Quando `cert_file` é informado, o certificado
/// precisa confiar na cadeia da assinatura; sem ele, valida apenas a integridade
/// do CMS sobre o ByteRange (certos embutidos no próprio PKCS#7).
pub fn verify_signature(
    input: &Path,
    cert_file: Option<&Path>,
) -> AppResult<(bool, SignatureInfo)> {
    let (signed, der, info) = extract_signature_payload(input)?;
    let work_dir = input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(std::env::temp_dir);
    let data_path = work_dir.join(format!("rustpdf-vdata-{}.bin", std::process::id()));
    let sig_path = work_dir.join(format!("rustpdf-vsig-{}.der", std::process::id()));
    fs::write(&data_path, &signed).map_err(|e| format!("Escrita temporária: {e}"))?;
    fs::write(&sig_path, &der).map_err(|e| format!("Escrita da assinatura: {e}"))?;

    let mut cmd = Command::new("openssl");
    cmd.args([
        "cms",
        "-verify",
        "-binary",
        "-inform",
        "DER",
        "-in",
    ])
    .arg(&sig_path)
    .args(["-content"])
    .arg(&data_path)
    .args(["-out"]);
    let sink = work_dir.join(format!("rustpdf-vout-{}.bin", std::process::id()));
    cmd.arg(&sink);
    if let Some(cert) = cert_file {
        cmd.args(["-certfile"]).arg(cert);
    } else {
        cmd.arg("-noverify");
    }
    let result = cmd.output();
    let _ = fs::remove_file(&data_path);
    let _ = fs::remove_file(&sig_path);
    let _ = fs::remove_file(&sink);
    match result {
        Ok(output) => Ok((output.status.success(), info)),
        Err(e) => Err(format!("Não foi possível executar openssl: {e}")),
    }
}
