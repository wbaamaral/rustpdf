// SPDX-License-Identifier: AGPL-3.0-or-later
//! qpdf pela API C (`libqpdf`): declarações únicas e o documento aberto.
use crate::AppResult;
use std::ffi::{c_int, c_void, CStr, CString};
use std::path::Path;

/// Bit de erro no retorno das funções do qpdf (`QPDF_ERRORS`). O outro bit,
/// `QPDF_WARNINGS` (1), só avisa (PDF com pequenos defeitos): não é falha.
const QPDF_ERRORS: c_int = 1 << 1;

/// Declarações da API C usadas pelo projeto (uma só lista).
pub(crate) mod ffi {
    use std::ffi::{c_char, c_int, c_void};

    #[link(name = "qpdf")]
    extern "C" {
        pub(crate) fn qpdf_cleanup(qpdf: *mut *mut c_void);
        pub(crate) fn qpdf_get_error(qpdf: *mut c_void) -> *mut c_void;
        pub(crate) fn qpdf_get_error_full_text(
            qpdf: *mut c_void,
            error: *mut c_void,
        ) -> *const c_char;
        pub(crate) fn qpdf_get_num_pages(qpdf: *mut c_void) -> c_int;
        pub(crate) fn qpdf_get_page_n(qpdf: *mut c_void, index: usize) -> u32;
        pub(crate) fn qpdf_get_root(qpdf: *mut c_void) -> u32;
        pub(crate) fn qpdf_has_error(qpdf: *mut c_void) -> c_int;
        pub(crate) fn qpdf_init() -> *mut c_void;
        pub(crate) fn qpdf_init_write(qpdf: *mut c_void, filename: *const c_char) -> c_int;
        pub(crate) fn qpdf_make_indirect_object(qpdf: *mut c_void, object: u32) -> u32;
        pub(crate) fn qpdf_oh_append_item(qpdf: *mut c_void, array: u32, item: u32);
        pub(crate) fn qpdf_oh_begin_dict_key_iter(qpdf: *mut c_void, dict: u32);
        pub(crate) fn qpdf_oh_dict_more_keys(qpdf: *mut c_void) -> c_int;
        pub(crate) fn qpdf_oh_dict_next_key(qpdf: *mut c_void) -> *const c_char;
        pub(crate) fn qpdf_oh_get_array_item(qpdf: *mut c_void, object: u32, index: c_int) -> u32;
        pub(crate) fn qpdf_oh_get_array_n_items(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_get_dict(qpdf: *mut c_void, object: u32) -> u32;
        pub(crate) fn qpdf_oh_get_key(qpdf: *mut c_void, object: u32, key: *const c_char) -> u32;
        pub(crate) fn qpdf_oh_get_name(qpdf: *mut c_void, object: u32) -> *const c_char;
        pub(crate) fn qpdf_oh_get_numeric_value(qpdf: *mut c_void, object: u32) -> f64;
        pub(crate) fn qpdf_oh_get_object_id(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_get_page_content_data(
            qpdf: *mut c_void,
            page: u32,
            bufp: *mut *mut u8,
            len: *mut usize,
        ) -> c_int;
        pub(crate) fn qpdf_oh_get_stream_data(
            qpdf: *mut c_void,
            stream_oh: u32,
            decode_level: c_int,
            filtered: *mut c_int,
            bufp: *mut *mut u8,
            len: *mut usize,
        ) -> c_int;
        pub(crate) fn qpdf_oh_get_utf8_value(qpdf: *mut c_void, object: u32) -> *const c_char;
        pub(crate) fn qpdf_oh_is_array(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_is_dictionary(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_is_name(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_is_null(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_is_number(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_is_name_and_equals(
            qpdf: *mut c_void,
            object: u32,
            name: *const c_char,
        ) -> c_int;
        pub(crate) fn qpdf_oh_is_stream(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_is_string(qpdf: *mut c_void, object: u32) -> c_int;
        pub(crate) fn qpdf_oh_new_array(qpdf: *mut c_void) -> u32;
        pub(crate) fn qpdf_oh_new_dictionary(qpdf: *mut c_void) -> u32;
        pub(crate) fn qpdf_oh_new_integer(qpdf: *mut c_void, value: i64) -> u32;
        pub(crate) fn qpdf_oh_new_name(qpdf: *mut c_void, name: *const c_char) -> u32;
        pub(crate) fn qpdf_oh_new_null(qpdf: *mut c_void) -> u32;
        pub(crate) fn qpdf_oh_new_real_from_double(
            qpdf: *mut c_void,
            value: f64,
            decimals: c_int,
        ) -> u32;
        pub(crate) fn qpdf_oh_new_stream(qpdf: *mut c_void) -> u32;
        pub(crate) fn qpdf_oh_new_string(qpdf: *mut c_void, value: *const c_char) -> u32;
        pub(crate) fn qpdf_oh_new_unicode_string(qpdf: *mut c_void, value: *const c_char) -> u32;
        pub(crate) fn qpdf_oh_remove_key(qpdf: *mut c_void, object: u32, key: *const c_char);
        pub(crate) fn qpdf_oh_replace_key(
            qpdf: *mut c_void,
            object: u32,
            key: *const c_char,
            item: u32,
        );
        pub(crate) fn qpdf_oh_replace_stream_data(
            qpdf: *mut c_void,
            stream: u32,
            data: *const u8,
            len: usize,
            filter: u32,
            decode_parms: u32,
        );
        pub(crate) fn qpdf_push_inherited_attributes_to_page(qpdf: *mut c_void) -> c_int;
        pub(crate) fn qpdf_read(
            qpdf: *mut c_void,
            filename: *const c_char,
            password: *const c_char,
        ) -> c_int;
        pub(crate) fn qpdf_write(qpdf: *mut c_void) -> c_int;
        pub(crate) fn qpdf_get_qpdf_version() -> *const c_char;
    }

    // Buffers de `qpdf_oh_get_stream_data`/`qpdf_oh_get_page_content_data` são
    // alocados com `malloc` e liberados com `free` (o `qpdf_oh_free_buffer` só
    // existe no qpdf 12; o Ubuntu 24.04 traz o 11.9).
    extern "C" {
        pub(crate) fn free(pointer: *mut c_void);
    }
}

use ffi::*;

/// Versão da `libqpdf` ligada ao programa.
pub fn version() -> String {
    let pointer = unsafe { qpdf_get_qpdf_version() };
    if pointer.is_null() {
        return "?".into();
    }
    // SAFETY: string estática do qpdf, terminada em NUL.
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}

/// Caminho como string C para as funções do qpdf.
pub(crate) fn path_c(path: &Path) -> AppResult<CString> {
    crate::path_to_cstring(path)
}

/// Documento aberto pelo qpdf; liberado ao sair de escopo.
pub(crate) struct Document(pub(crate) *mut c_void);

impl Document {
    pub(crate) fn new() -> AppResult<Self> {
        let pointer = unsafe { qpdf_init() };
        if pointer.is_null() {
            return Err("Não foi possível iniciar o qpdf".into());
        }
        Ok(Self(pointer))
    }

    /// Converte o retorno de uma função do qpdf em resultado: só o bit
    /// `QPDF_ERRORS` é falha.
    pub(crate) fn check(&self, code: c_int) -> AppResult<()> {
        if code & QPDF_ERRORS == 0 {
            return Ok(());
        }
        if unsafe { qpdf_has_error(self.0) } == 0 {
            return Err(format!("qpdf: código {code}"));
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

    pub(crate) fn set(&self, object: u32, key: &CStr, value: u32) {
        unsafe { qpdf_oh_replace_key(self.0, object, key.as_ptr(), value) };
    }

    pub(crate) fn name(&self, name: &CStr) -> u32 {
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

/// Chaves (com a barra) de um dicionário do qpdf.
pub(crate) fn dict_keys(doc: &Document, dict: u32) -> Vec<String> {
    let mut keys = Vec::new();
    unsafe { ffi::qpdf_oh_begin_dict_key_iter(doc.0, dict) };
    while unsafe { ffi::qpdf_oh_dict_more_keys(doc.0) } != 0 {
        let key = unsafe { ffi::qpdf_oh_dict_next_key(doc.0) };
        if key.is_null() {
            break;
        }
        keys.push(
            unsafe { CStr::from_ptr(key) }
                .to_string_lossy()
                .into_owned(),
        );
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        let version = version();
        assert!(
            version.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "{version}"
        );
    }

    #[test]
    fn check_treats_warnings_as_success_and_errors_as_failure() {
        let doc = Document::new().unwrap();
        assert!(doc.check(0).is_ok());
        assert!(doc.check(1).is_ok(), "QPDF_WARNINGS não é erro");
        let error = doc.check(2).unwrap_err();
        assert_eq!(error, "qpdf: código 2");
        let missing = path_c(Path::new("/rustpdf/arquivo-inexistente.pdf")).unwrap();
        let code = unsafe { qpdf_read(doc.0, missing.as_ptr(), std::ptr::null()) };
        let error = doc.check(code).unwrap_err();
        assert!(error.contains("arquivo-inexistente"), "{error}");
    }
}
