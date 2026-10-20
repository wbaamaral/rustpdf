// SPDX-License-Identifier: AGPL-3.0-or-later
//! PNG: dimensões, decodificação para RGB e codificação de RGB.

use crate::infra::zlib;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Largura e altura em pixels, do bloco `IHDR` (que segue a assinatura).
/// `None` se não for PNG ou se uma dimensão for zero.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(SIGNATURE) || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

/// Dimensões lendo só o cabeçalho do arquivo.
pub fn file_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let mut header = [0u8; 24];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|e| format!("Imagem PNG inacessível: {e}"))?;
    dimensions(&header).ok_or_else(|| "Arquivo não é um PNG válido".to_string())
}

/// Pixels de uma imagem `width` × `height` (PNG ou JPEG), se não vazia e
/// dentro do limite — conferido antes de alocar.
pub fn pixel_count(width: usize, height: usize, max_pixels: usize) -> Result<usize, String> {
    let pixels = width.checked_mul(height).ok_or("Imagem grande demais")?;
    if width == 0 || height == 0 || pixels > max_pixels {
        return Err("Imagem grande demais ou vazia".into());
    }
    Ok(pixels)
}

fn read_be_u32(bytes: &[u8]) -> usize {
    u32::from_be_bytes(bytes.try_into().expect("quatro bytes")) as usize
}

/// PNG de 8 bits (cinza, RGB, paleta, com ou sem alfa; sem entrelaçamento)
/// para RGB, com a transparência composta sobre branco. Recusa imagens com
/// mais de `max_pixels` pixels antes de alocar.
pub fn decode(input: &[u8], max_pixels: usize) -> Result<(usize, usize, Vec<u8>), String> {
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
        let checksum = zlib::crc32_update(zlib::crc32(kind), data);
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
                pixel_count(width, height, max_pixels)?;
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
            // Os dados comprimidos são parte da entrada, já limitada por quem chama.
            b"IDAT" => compressed.extend_from_slice(data),
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
    let raw = zlib::decompress(&compressed, raw_size).map_err(|_| "Dados PNG inválidos")?;
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

/// PNG RGB de 8 bits sem filtro (`rgb` = largura × altura × 3 bytes).
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Result<Vec<u8>, String> {
    let row = width as usize * 3;
    if width == 0 || height == 0 || rgb.len() != row * height as usize {
        return Err("Pixels RGB incompletos ou imagem vazia".into());
    }
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgb.chunks(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut png = SIGNATURE.to_vec();
    for (kind, data) in [
        (b"IHDR", header),
        (b"IDAT", zlib::compress(&raw)?),
        (b"IEND", Vec::new()),
    ] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(kind);
        png.extend_from_slice(&data);
        png.extend_from_slice(&zlib::crc32_update(zlib::crc32(kind), &data).to_be_bytes());
    }
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{TempDir, PNG_2X2};

    #[test]
    fn dimensions_come_from_the_ihdr_chunk() {
        assert_eq!(dimensions(PNG_2X2), Some((2, 2)));
        let mut wide = PNG_2X2.to_vec();
        wide[16..20].copy_from_slice(&1200u32.to_be_bytes());
        wide[20..24].copy_from_slice(&1697u32.to_be_bytes());
        assert_eq!(dimensions(&wide), Some((1200, 1697)));
    }

    #[test]
    fn rejects_non_png_or_empty_images() {
        assert_eq!(dimensions(b"GIF89a"), None);
        assert_eq!(dimensions(&PNG_2X2[..20]), None);
        let mut empty = PNG_2X2.to_vec();
        empty[16..20].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(dimensions(&empty), None);
    }

    #[test]
    fn reads_dimensions_from_file() {
        let dir = TempDir::new("png");
        let path = dir.join("a.png");
        std::fs::write(&path, PNG_2X2).unwrap();
        assert_eq!(file_dimensions(&path).unwrap(), (2, 2));
        assert!(file_dimensions(&dir.join("ausente.png")).is_err());
    }

    #[test]
    fn encode_and_decode_round_trip() {
        let rgb: Vec<u8> = (0..4 * 3 * 3).map(|i| (i * 7) as u8).collect();
        let png = encode_rgb(4, 3, &rgb).unwrap();
        assert_eq!(dimensions(&png), Some((4, 3)));
        assert_eq!(decode(&png, 1000).unwrap(), (4, 3, rgb));
        assert!(encode_rgb(2, 2, &[0; 11]).is_err(), "pixels incompletos");
        assert!(encode_rgb(0, 2, &[]).is_err(), "imagem vazia");
    }

    #[test]
    fn decodes_rgba_over_white() {
        let (width, height, rgb) = decode(PNG_2X2, 1000).unwrap();
        assert_eq!((width, height, rgb.len()), (2, 2, 12));
    }

    #[test]
    fn decode_rejects_corrupt_or_oversized_images() {
        let mut corrupt = PNG_2X2.to_vec();
        corrupt[29] ^= 0xff;
        assert_eq!(decode(&corrupt, 1000).unwrap_err(), "CRC PNG inválido");
        assert!(decode(&PNG_2X2[..40], 1000).is_err(), "truncado");
        assert!(decode(b"GIF89a", 1000).is_err());
        assert!(decode(PNG_2X2, 3).is_err(), "acima do limite de pixels");
    }
}
