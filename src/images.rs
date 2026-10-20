// SPDX-License-Identifier: AGPL-3.0-or-later
//! PNG/JPEG -> imagem RGB embutida em PDF, com limites antes da alocação.
use crate::{write_pdf_objects, AppResult};
use std::ffi::{c_int, c_ulong, c_void, CStr};
use std::fs;
use std::path::Path;

const MAX_SOURCE_BYTES: usize = 100 * 1024 * 1024;
const MAX_PIXELS: usize = 20_000_000;

#[link(name = "z")]
extern "C" {
    fn compressBound(source_len: c_ulong) -> c_ulong;
    fn compress2(
        destination: *mut u8,
        destination_len: *mut c_ulong,
        source: *const u8,
        source_len: c_ulong,
        level: c_int,
    ) -> c_int;
    fn uncompress(
        destination: *mut u8,
        destination_len: *mut c_ulong,
        source: *const u8,
        source_len: c_ulong,
    ) -> c_int;
    fn crc32(crc: c_ulong, bytes: *const u8, len: u32) -> c_ulong;
}

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
    let pixels = width.checked_mul(height).ok_or("Imagem grande demais")?;
    if width == 0 || height == 0 || pixels > MAX_PIXELS {
        return Err("Imagem grande demais ou vazia".into());
    }
    Ok(pixels)
}

fn read_be_u32(bytes: &[u8]) -> usize {
    u32::from_be_bytes(bytes.try_into().expect("quatro bytes")) as usize
}

fn decode_png(input: &[u8]) -> AppResult<(usize, usize, Vec<u8>)> {
    if !input.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("PNG inválido".into());
    }
    let (mut width, mut height, mut color) = (0, 0, 0);
    let mut palette = Vec::new();
    let mut transparency = Vec::new();
    let mut compressed = Vec::new();
    let mut found_end = false;
    let mut offset = 8;
    while offset + 12 <= input.len() {
        let size = read_be_u32(&input[offset..offset + 4]);
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(size))
            .ok_or("Chunk PNG inválido")?;
        if end > input.len() {
            return Err("Chunk PNG truncado".into());
        }
        let kind = &input[offset + 4..offset + 8];
        let data = &input[offset + 8..offset + 8 + size];
        let expected_crc = read_be_u32(&input[offset + 8 + size..end]);
        let checksum = unsafe { crc32(crc32(0, kind.as_ptr(), 4), data.as_ptr(), size as u32) };
        if checksum as usize != expected_crc {
            return Err("CRC PNG inválido".into());
        }
        match kind {
            b"IHDR" => {
                if size != 13 || width != 0 {
                    return Err("Cabeçalho PNG inválido".into());
                }
                width = read_be_u32(&data[0..4]);
                height = read_be_u32(&data[4..8]);
                dimensions(width, height)?;
                if data[8] != 8
                    || data[10..13] != [0, 0, 0]
                    || !matches!(data[9], 0 | 2 | 3 | 4 | 6)
                {
                    return Err("PNG exige 8 bits por canal e sem entrelaçamento".into());
                }
                color = data[9];
            }
            b"PLTE" => palette = data.to_vec(),
            b"tRNS" => transparency = data.to_vec(),
            b"IDAT" => {
                if compressed.len().saturating_add(size) > MAX_SOURCE_BYTES {
                    return Err("PNG comprimido grande demais".into());
                }
                compressed.extend_from_slice(data);
            }
            b"IEND" => {
                found_end = true;
                break;
            }
            _ => {}
        }
        offset = end;
    }
    if width == 0 || !found_end || compressed.is_empty() {
        return Err("PNG incompleto".into());
    }
    let channels = match color {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => unreachable!(),
    };
    let row_bytes = width.checked_mul(channels).ok_or("Imagem grande demais")?;
    let raw_size = row_bytes
        .checked_add(1)
        .and_then(|n| n.checked_mul(height))
        .ok_or("Imagem grande demais")?;
    let mut raw = vec![0; raw_size];
    let mut actual = raw_size as c_ulong;
    let result = unsafe {
        uncompress(
            raw.as_mut_ptr(),
            &mut actual,
            compressed.as_ptr(),
            compressed.len() as c_ulong,
        )
    };
    if result != 0 || actual as usize != raw_size {
        return Err("Dados PNG inválidos".into());
    }
    if color == 3 && (palette.is_empty() || palette.len() % 3 != 0) {
        return Err("Paleta PNG inválida".into());
    }
    let mut previous = vec![0u8; row_bytes];
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in 0..height {
        let base = row * (row_bytes + 1);
        let filter = raw[base];
        let mut current = vec![0u8; row_bytes];
        for index in 0..row_bytes {
            let left = if index >= channels {
                current[index - channels]
            } else {
                0
            };
            let above = previous[index];
            let upper_left = if index >= channels {
                previous[index - channels]
            } else {
                0
            };
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => above,
                3 => ((left as u16 + above as u16) / 2) as u8,
                4 => paeth(left, above, upper_left),
                _ => return Err("Filtro PNG inválido".into()),
            };
            current[index] = raw[base + 1 + index].wrapping_add(predictor);
        }
        for pixel in current.chunks_exact(channels) {
            let (r, g, b, alpha) = match color {
                0 => (pixel[0], pixel[0], pixel[0], 255),
                2 => (pixel[0], pixel[1], pixel[2], 255),
                3 => {
                    let index = pixel[0] as usize;
                    let start = index * 3;
                    if start + 3 > palette.len() {
                        return Err("Índice de paleta PNG inválido".into());
                    }
                    (
                        palette[start],
                        palette[start + 1],
                        palette[start + 2],
                        transparency.get(index).copied().unwrap_or(255),
                    )
                }
                4 => (pixel[0], pixel[0], pixel[0], pixel[1]),
                6 => (pixel[0], pixel[1], pixel[2], pixel[3]),
                _ => unreachable!(),
            };
            for value in [r, g, b] {
                rgb.push(
                    ((value as u16 * alpha as u16 + 255 * (255 - alpha as u16) + 127) / 255) as u8,
                );
            }
        }
        previous = current;
    }
    Ok((width, height, rgb))
}

fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
    let estimate = left as i32 + above as i32 - upper_left as i32;
    let distances = [
        (estimate - left as i32).abs(),
        (estimate - above as i32).abs(),
        (estimate - upper_left as i32).abs(),
    ];
    if distances[0] <= distances[1] && distances[0] <= distances[2] {
        left
    } else if distances[1] <= distances[2] {
        above
    } else {
        upper_left
    }
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
        decode_png(&bytes)
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        decode_jpeg(&bytes)
    } else {
        Err("A imagem precisa ser PNG ou JPEG".into())
    }
}

pub(crate) fn compress(data: &[u8]) -> AppResult<Vec<u8>> {
    let mut bound = unsafe { compressBound(data.len() as c_ulong) };
    let mut compressed = vec![0; bound as usize];
    let status = unsafe {
        compress2(
            compressed.as_mut_ptr(),
            &mut bound,
            data.as_ptr(),
            data.len() as c_ulong,
            6,
        )
    };
    if status != 0 {
        return Err(format!("Compressão da imagem falhou: {status}"));
    }
    compressed.truncate(bound as usize);
    Ok(compressed)
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
    let mut image_stream = format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", compressed.len()).into_bytes();
    image_stream.extend_from_slice(&compressed);
    image_stream.extend_from_slice(b"\nendstream");
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_width:.3} {page_height:.3}] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>").into_bytes(),
        format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content).into_bytes(),
        image_stream,
    ];
    write_pdf_objects(destination, &objects)
}
