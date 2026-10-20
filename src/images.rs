// SPDX-License-Identifier: AGPL-3.0-or-later
//! PNG/JPEG -> imagem RGB embutida em PDF, com limites antes da alocação.
use crate::infra::pdf_objetos::stream;
use crate::{write_pdf_objects, AppResult};
use std::ffi::{c_int, c_ulong, c_void, CStr};
use std::fs;
use std::path::Path;

const MAX_SOURCE_BYTES: usize = 100 * 1024 * 1024;
const MAX_PIXELS: usize = 20_000_000;

#[link(name = "turbojpeg")]
extern "C" {
    fn tjInitDecompress() -> *mut c_void;
    fn tjDestroy(handle: *mut c_void) -> c_int;
    fn tjDecompressHeader3(
        handle: *mut c_void,
        input: *const u8,
        size: c_ulong,
        width: *mut c_int,
        height: *mut c_int,
        subsamp: *mut c_int,
        colorspace: *mut c_int,
    ) -> c_int;
    fn tjDecompress2(
        handle: *mut c_void,
        input: *const u8,
        size: c_ulong,
        output: *mut u8,
        width: c_int,
        pitch: c_int,
        height: c_int,
        pixel_format: c_int,
        flags: c_int,
    ) -> c_int;
    fn tjGetErrorStr2(handle: *mut c_void) -> *const i8;
}

fn dimensions(width: usize, height: usize) -> AppResult<usize> {
    crate::infra::png::pixel_count(width, height, MAX_PIXELS)
}

struct JpegHandle(*mut c_void);
impl Drop for JpegHandle {
    fn drop(&mut self) {
        unsafe {
            tjDestroy(self.0);
        }
    }
}

fn decode_jpeg(input: &[u8]) -> AppResult<(usize, usize, Vec<u8>)> {
    let handle = JpegHandle(unsafe { tjInitDecompress() });
    if handle.0.is_null() {
        return Err("TurboJPEG indisponível".into());
    }
    let (mut width, mut height, mut subsamp, mut colorspace) = (0, 0, 0, 0);
    let result = unsafe {
        tjDecompressHeader3(
            handle.0,
            input.as_ptr(),
            input.len() as c_ulong,
            &mut width,
            &mut height,
            &mut subsamp,
            &mut colorspace,
        )
    };
    if result != 0 || width <= 0 || height <= 0 {
        return Err(jpeg_error(handle.0));
    }
    let pixels = dimensions(width as usize, height as usize)?;
    let mut rgb = vec![0; pixels * 3];
    let result = unsafe {
        tjDecompress2(
            handle.0,
            input.as_ptr(),
            input.len() as c_ulong,
            rgb.as_mut_ptr(),
            width,
            width * 3,
            height,
            0,
            0,
        )
    };
    if result != 0 {
        return Err(jpeg_error(handle.0));
    }
    Ok((width as usize, height as usize, rgb))
}

fn jpeg_error(handle: *mut c_void) -> String {
    let message = unsafe { tjGetErrorStr2(handle) };
    if message.is_null() {
        "JPEG inválido".into()
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }
}

pub(crate) fn load_rgb(path: &Path) -> AppResult<(usize, usize, Vec<u8>)> {
    let metadata = fs::metadata(path).map_err(|e| format!("Imagem inacessível: {e}"))?;
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err("Arquivo de imagem grande demais".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("Imagem inacessível: {e}"))?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err("Arquivo de imagem grande demais".into());
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        crate::infra::png::decode(&bytes, MAX_PIXELS)
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        decode_jpeg(&bytes)
    } else {
        Err("A imagem precisa ser PNG ou JPEG".into())
    }
}

pub(crate) fn compress(data: &[u8]) -> AppResult<Vec<u8>> {
    crate::infra::zlib::compress(data).map_err(|e| format!("Imagem: {e}"))
}

pub fn create_image_overlay(
    image: &Path,
    destination: &Path,
    page_width: f64,
    page_height: f64,
) -> AppResult<()> {
    create_image_overlay_impl(image, destination, page_width, page_height, None)
}

pub fn create_image_overlay_at(
    image: &Path,
    destination: &Path,
    page_width: f64,
    page_height: f64,
    placement: (f64, f64, f64, f64),
) -> AppResult<()> {
    create_image_overlay_impl(image, destination, page_width, page_height, Some(placement))
}

fn create_image_overlay_impl(
    image: &Path,
    destination: &Path,
    page_width: f64,
    page_height: f64,
    placement: Option<(f64, f64, f64, f64)>,
) -> AppResult<()> {
    if !page_width.is_finite()
        || !page_height.is_finite()
        || page_width <= 0.0
        || page_height <= 0.0
    {
        return Err("Dimensões da página inválidas".into());
    }
    let (width, height, rgb) = load_rgb(image)?;
    let compressed = compress(&rgb)?;
    let (x, y, drawn_width, drawn_height) = placement.unwrap_or_else(|| {
        let scale = (page_width * 0.7 / width as f64).min(page_height * 0.7 / height as f64);
        let drawn_width = width as f64 * scale;
        let drawn_height = height as f64 * scale;
        (
            (page_width - drawn_width) / 2.0,
            (page_height - drawn_height) / 2.0,
            drawn_width,
            drawn_height,
        )
    });
    if [x, y, drawn_width, drawn_height]
        .iter()
        .any(|number| !number.is_finite())
        || x < 0.0
        || y < 0.0
        || drawn_width <= 0.0
        || drawn_height <= 0.0
        || x + drawn_width > page_width
        || y + drawn_height > page_height
    {
        return Err("Posição ou tamanho da imagem inválido".into());
    }
    let content =
        format!("q\n{drawn_width:.3} 0 0 {drawn_height:.3} {x:.3} {y:.3} cm\n/Im1 Do\nQ\n");
    let image_stream = stream(
        &format!(" /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode"),
        &compressed,
    );
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_width:.3} {page_height:.3}] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>").into_bytes(),
        stream("", content.as_bytes()),
        image_stream,
    ];
    write_pdf_objects(destination, &objects)
}
