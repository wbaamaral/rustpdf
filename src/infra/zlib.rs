// SPDX-License-Identifier: AGPL-3.0-or-later
//! zlib (`libz`) por FFI: compressão, descompressão e CRC-32.

use std::ffi::{c_int, c_ulong};

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
    #[link_name = "crc32"]
    fn zlib_crc32(crc: c_ulong, bytes: *const u8, len: u32) -> c_ulong;
    #[link_name = "adler32"]
    fn zlib_adler32(adler: c_ulong, bytes: *const u8, len: u32) -> c_ulong;
}

/// Dados no formato zlib (o `/FlateDecode` do PDF), nível 6.
pub fn compress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut size = unsafe { compressBound(data.len() as c_ulong) };
    let mut compressed = vec![0; size as usize];
    let status = unsafe {
        compress2(
            compressed.as_mut_ptr(),
            &mut size,
            data.as_ptr(),
            data.len() as c_ulong,
            6,
        )
    };
    if status != 0 {
        return Err(format!("Compressão falhou: {status}"));
    }
    compressed.truncate(size as usize);
    Ok(compressed)
}

/// Descomprime dados zlib cujo tamanho final é conhecido; tamanho diferente é erro.
pub fn decompress(compressed: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut data = vec![0; expected_len];
    let mut size = expected_len as c_ulong;
    let status = unsafe {
        uncompress(
            data.as_mut_ptr(),
            &mut size,
            compressed.as_ptr(),
            compressed.len() as c_ulong,
        )
    };
    if status != 0 || size as usize != expected_len {
        return Err("Dados comprimidos inválidos".into());
    }
    Ok(data)
}

/// Deflate cru (sem cabeçalho nem soma), o do ZIP: a saída do `compress2`
/// é cabeçalho zlib (2 bytes) + deflate cru + Adler-32 (4 bytes).
pub fn deflate_raw(data: &[u8]) -> Result<Vec<u8>, String> {
    let wrapped = compress(data)?;
    if wrapped.len() < 6 {
        return Err("Compressão falhou: saída curta".into());
    }
    Ok(wrapped[2..wrapped.len() - 4].to_vec())
}

/// Adler-32 (a soma do formato zlib).
pub fn adler32(bytes: &[u8]) -> u32 {
    let mut sum: c_ulong = 1;
    for chunk in bytes.chunks(u32::MAX as usize) {
        sum = unsafe { zlib_adler32(sum, chunk.as_ptr(), chunk.len() as u32) };
    }
    sum as u32
}

/// CRC-32 (ISO-HDLC, o do PNG e do ZIP).
pub fn crc32(bytes: &[u8]) -> u32 {
    crc32_update(0, bytes)
}

/// Continua um CRC-32 com mais bytes.
pub fn crc32_update(crc: u32, bytes: &[u8]) -> u32 {
    let mut crc = crc as c_ulong;
    for chunk in bytes.chunks(u32::MAX as usize) {
        crc = unsafe { zlib_crc32(crc, chunk.as_ptr(), chunk.len() as u32) };
    }
    crc as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32_update(crc32(b"1234"), b"56789"), 0xCBF4_3926);
    }

    #[test]
    fn compress_and_decompress_round_trip() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let compressed = compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        assert_eq!(decompress(&compressed, data.len()).unwrap(), data);
        assert_eq!(
            compress(&[]).map(|c| decompress(&c, 0).unwrap()),
            Ok(vec![])
        );
    }

    #[test]
    fn decompress_rejects_garbage_and_wrong_size() {
        assert!(decompress(b"isto nao e zlib", 100).is_err());
        let compressed = compress(b"abcdef").unwrap();
        assert!(
            decompress(&compressed, 3).is_err(),
            "saída maior que o esperado"
        );
        assert!(
            decompress(&compressed, 10).is_err(),
            "saída menor que o esperado"
        );
    }

    #[test]
    fn raw_deflate_round_trips_through_the_zlib_wrapper() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 17) as u8).collect();
        let raw = deflate_raw(&data).unwrap();
        assert!(raw.len() < data.len());
        // Recoloca cabeçalho e Adler-32 do zlib e descomprime.
        let mut wrapped = vec![0x78, 0x9c];
        wrapped.extend_from_slice(&raw);
        wrapped.extend_from_slice(&adler32(&data).to_be_bytes());
        assert_eq!(decompress(&wrapped, data.len()).unwrap(), data);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }
}
