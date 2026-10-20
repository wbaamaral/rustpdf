// SPDX-License-Identifier: AGPL-3.0-or-later
//! Escritor ZIP mínimo (o pacote dos formatos do Office): deflate ou
//! armazenado, nomes UTF-8, data fixa para saída reprodutível. Sem ZIP64.

use crate::infra::zlib;

/// Versão do formato exigida (2.0: deflate e pastas).
const VERSION: u16 = 20;
/// Bit 11: nome em UTF-8.
const UTF8_NAMES: u16 = 0x0800;
/// 1980-01-01 00:00, a menor data do DOS: saída reprodutível.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;

struct Entry {
    name: String,
    method: u16,
    crc: u32,
    packed: u32,
    size: u32,
    offset: u32,
}

/// Monta o ZIP em memória, na ordem em que as entradas chegam.
#[derive(Default)]
pub struct ZipWriter {
    bytes: Vec<u8>,
    entries: Vec<Entry>,
}

impl ZipWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acrescenta `name` (caminho com `/`) com o conteúdo `data`: deflate
    /// quando reduz o tamanho, senão armazenado.
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        let deflated = zlib::deflate_raw(data)?;
        let (method, stored) = if deflated.len() < data.len() {
            (8, deflated)
        } else {
            (0, data.to_vec())
        };
        self.put_entry(name, method, data, &stored)
    }

    /// Acrescenta `name` sem compressão (método 0, armazenado), obrigatório
    /// para certas partes como o `mimetype` do OpenDocument (ODF).
    pub fn add_stored(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        self.put_entry(name, 0, data, data)
    }

    fn put_entry(
        &mut self,
        name: &str,
        method: u16,
        data: &[u8],
        stored: &[u8],
    ) -> Result<(), String> {
        if self.entries.iter().any(|entry| entry.name == name) {
            return Err(format!("ZIP: entrada repetida {name}"));
        }
        let too_big = |value: usize| u32::try_from(value).map_err(|_| "ZIP: arquivo grande demais");
        let entry = Entry {
            name: name.into(),
            method,
            crc: zlib::crc32(data),
            packed: too_big(stored.len())?,
            size: too_big(data.len())?,
            offset: too_big(self.bytes.len())?,
        };
        let name_len = u16::try_from(name.len()).map_err(|_| "ZIP: nome longo demais")?;
        put32(&mut self.bytes, 0x0403_4b50);
        put16(&mut self.bytes, VERSION);
        self.put_common(&entry, name_len);
        put16(&mut self.bytes, 0); // campo extra
        self.bytes.extend_from_slice(name.as_bytes());
        self.bytes.extend_from_slice(stored);
        self.entries.push(entry);
        Ok(())
    }

    /// Campos repetidos no cabeçalho local e no diretório central.
    fn put_common(&mut self, entry: &Entry, name_len: u16) {
        put16(&mut self.bytes, UTF8_NAMES);
        put16(&mut self.bytes, entry.method);
        put16(&mut self.bytes, DOS_TIME);
        put16(&mut self.bytes, DOS_DATE);
        put32(&mut self.bytes, entry.crc);
        put32(&mut self.bytes, entry.packed);
        put32(&mut self.bytes, entry.size);
        put16(&mut self.bytes, name_len);
    }

    /// Diretório central e registro final; devolve o arquivo completo.
    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        let count = u16::try_from(self.entries.len()).map_err(|_| "ZIP: entradas demais")?;
        let directory =
            u32::try_from(self.bytes.len()).map_err(|_| "ZIP: arquivo grande demais")?;
        let entries = std::mem::take(&mut self.entries);
        for entry in &entries {
            put32(&mut self.bytes, 0x0201_4b50);
            put16(&mut self.bytes, VERSION); // feito por
            put16(&mut self.bytes, VERSION); // necessária
            self.put_common(entry, entry.name.len() as u16);
            put16(&mut self.bytes, 0); // extra
            put16(&mut self.bytes, 0); // comentário
            put16(&mut self.bytes, 0); // disco
            put16(&mut self.bytes, 0); // atributos internos
            put32(&mut self.bytes, 0); // atributos externos
            put32(&mut self.bytes, entry.offset);
            self.bytes.extend_from_slice(entry.name.as_bytes());
        }
        let size = self.bytes.len() as u32 - directory;
        put32(&mut self.bytes, 0x0605_4b50);
        put16(&mut self.bytes, 0);
        put16(&mut self.bytes, 0);
        put16(&mut self.bytes, count);
        put16(&mut self.bytes, count);
        put32(&mut self.bytes, size);
        put32(&mut self.bytes, directory);
        put16(&mut self.bytes, 0); // comentário
        Ok(self.bytes)
    }
}

fn put16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::zlib;

    fn u16_at(bytes: &[u8], at: usize) -> usize {
        u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize
    }

    fn u32_at(bytes: &[u8], at: usize) -> usize {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    }

    /// Entrada lida pelo diretório central.
    struct Entry {
        name: String,
        method: usize,
        crc: u32,
        size: usize,
        stored: Vec<u8>,
    }

    /// Leitor mínimo: diretório central → cabeçalho local → bytes guardados.
    fn read(zip: &[u8]) -> Vec<Entry> {
        let end = zip.len() - 22;
        assert_eq!(u32_at(zip, end), 0x0605_4b50, "fim do diretório central");
        let count = u16_at(zip, end + 10);
        let mut at = u32_at(zip, end + 16);
        let mut entries = Vec::new();
        for _ in 0..count {
            assert_eq!(u32_at(zip, at), 0x0201_4b50);
            assert_eq!(u16_at(zip, at + 8) & 0x0800, 0x0800, "nome em UTF-8");
            let name_len = u16_at(zip, at + 28);
            let local = u32_at(zip, at + 42);
            assert_eq!(u32_at(zip, local), 0x0403_4b50);
            let data_at = local + 30 + u16_at(zip, local + 26) + u16_at(zip, local + 28);
            let packed = u32_at(zip, at + 20);
            entries.push(Entry {
                name: String::from_utf8(zip[at + 46..at + 46 + name_len].to_vec()).unwrap(),
                method: u16_at(zip, at + 10),
                crc: u32_at(zip, at + 16) as u32,
                size: u32_at(zip, at + 24),
                stored: zip[data_at..data_at + packed].to_vec(),
            });
            at += 46 + name_len + u16_at(zip, at + 30) + u16_at(zip, at + 32);
        }
        entries
    }

    /// A entrada guarda exatamente `data` (deflate é determinístico).
    fn holds(entry: &Entry, data: &[u8]) -> bool {
        let stored = match entry.method {
            8 => zlib::deflate_raw(data).unwrap(),
            _ => data.to_vec(),
        };
        entry.stored == stored && entry.crc == zlib::crc32(data) && entry.size == data.len()
    }

    fn sample() -> Vec<u8> {
        let mut zip = ZipWriter::new();
        zip.add("[Content_Types].xml", "<Types/>".repeat(50).as_bytes())
            .unwrap();
        zip.add("pasta/ação.txt", "acentuação".as_bytes()).unwrap();
        zip.add("vazio", b"").unwrap();
        zip.finish().unwrap()
    }

    #[test]
    fn entries_round_trip_with_crc_and_utf8_names() {
        let entries = read(&sample());
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["[Content_Types].xml", "pasta/ação.txt", "vazio"]);
        assert_eq!(entries[0].method, 8, "repetitivo: deflate");
        assert!(holds(&entries[0], "<Types/>".repeat(50).as_bytes()));
        assert!(holds(&entries[1], "acentuação".as_bytes()));
        assert_eq!(entries[2].method, 0, "vazio: armazenado");
        assert!(holds(&entries[2], b""));
    }

    #[test]
    fn output_is_reproducible() {
        assert_eq!(sample(), sample());
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let mut zip = ZipWriter::new();
        zip.add("a.xml", b"1").unwrap();
        assert!(zip.add("a.xml", b"2").is_err());
    }

    #[test]
    fn add_stored_forces_uncompressed_method_zero() {
        let mut zip = ZipWriter::new();
        let payload = "<Types/>".repeat(50).into_bytes();
        zip.add_stored("mimetype", &payload).unwrap();
        let bytes = zip.finish().unwrap();
        let entries = read(&bytes);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mimetype");
        assert_eq!(entries[0].method, 0, "deve ser armazenado sem compressão");
        assert_eq!(entries[0].size, payload.len());
        assert_eq!(entries[0].stored, payload);
    }

    #[test]
    fn unzip_accepts_the_archive_when_available() {
        let dir = crate::testkit::TempDir::new("zip");
        let path = dir.join("teste.zip");
        std::fs::write(&path, sample()).unwrap();
        let Ok(output) = std::process::Command::new("unzip")
            .arg("-t")
            .arg(&path)
            .output()
        else {
            return; // sem unzip neste sistema
        };
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
