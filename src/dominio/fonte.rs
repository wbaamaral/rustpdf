// SPDX-License-Identifier: AGPL-3.0-or-later
//! Codificação de fontes: converte os bytes de uma string mostrada (`Tj`/`TJ`)
//! em texto e o texto de volta nos códigos da fonte.
//!
//! - [`ToUnicode`] lê o CMap `/ToUnicode` (`codespacerange`, `bfchar`,
//!   `bfrange`), comum em fontes embutidas como subconjunto — os códigos são
//!   arbitrários e só o CMap diz qual caractere cada um desenha;
//! - [`FontCodec::WinAnsi`] cobre as fontes simples com `/WinAnsiEncoding`;
//! - [`FontCodec::Latin1`] é a suposição para fontes simples sem informação.

use std::collections::BTreeMap;

/// Maior faixa aceita num `bfrange` (evita alocações enormes com CMaps ruins).
const MAX_RANGE: u32 = 0x1_0000;

/// Código de caractere: comprimento em bytes e valor (big-endian).
type Code = (usize, u32);

/// Mapa `/ToUnicode` de uma fonte.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToUnicode {
    /// Faixas de códigos válidos: `(bytes, menor, maior)`.
    codespace: Vec<(usize, u32, u32)>,
    codes: BTreeMap<Code, String>,
}

#[derive(Debug, PartialEq)]
enum CMapToken {
    Hex(Vec<u8>),
    ArrayStart,
    ArrayEnd,
    Word(String),
}

fn cmap_tokens(data: &[u8]) -> Vec<CMapToken> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < data.len() {
        let byte = data[index];
        match byte {
            b'%' => {
                while index < data.len() && !matches!(data[index], b'\n' | b'\r') {
                    index += 1;
                }
            }
            b'<' if data.get(index + 1) == Some(&b'<') => index += 2,
            b'>' if data.get(index + 1) == Some(&b'>') => index += 2,
            b'<' => {
                let end = data[index..]
                    .iter()
                    .position(|byte| *byte == b'>')
                    .map_or(data.len(), |offset| index + offset);
                tokens.push(CMapToken::Hex(hex_bytes(&data[index + 1..end])));
                index = end + 1;
            }
            b'(' => {
                // Strings literais (nomes do CMap): só precisam ser puladas.
                let mut depth = 0;
                while index < data.len() {
                    match data[index] {
                        b'\\' => index += 1,
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    index += 1;
                }
                index += 1;
            }
            b'[' => {
                tokens.push(CMapToken::ArrayStart);
                index += 1;
            }
            b']' => {
                tokens.push(CMapToken::ArrayEnd);
                index += 1;
            }
            _ if byte.is_ascii_whitespace() || byte == 0 => index += 1,
            _ => {
                let start = index;
                while index < data.len()
                    && !data[index].is_ascii_whitespace()
                    && !b"<>[]()%{}".contains(&data[index])
                {
                    index += 1;
                }
                if index == start {
                    index += 1;
                    continue;
                }
                tokens.push(CMapToken::Word(
                    String::from_utf8_lossy(&data[start..index]).into_owned(),
                ));
            }
        }
    }
    tokens
}

fn hex_bytes(digits: &[u8]) -> Vec<u8> {
    let values: Vec<u8> = digits
        .iter()
        .filter_map(|digit| (*digit as char).to_digit(16).map(|value| value as u8))
        .collect();
    values
        .chunks(2)
        .map(|pair| pair[0] << 4 | pair.get(1).copied().unwrap_or(0))
        .collect()
}

/// Código a partir dos bytes; `None` acima de 4 bytes ou vazio.
fn code_of(bytes: &[u8]) -> Option<Code> {
    if bytes.is_empty() || bytes.len() > 4 {
        return None;
    }
    let value = bytes
        .iter()
        .fold(0u32, |value, byte| value << 8 | u32::from(*byte));
    Some((bytes.len(), value))
}

/// Destino UTF-16BE de um `bfchar`/`bfrange`.
fn utf16_units(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks(2)
        .map(|pair| u16::from(pair[0]) << 8 | u16::from(pair.get(1).copied().unwrap_or(0)))
        .collect()
}

fn utf16_text(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

impl ToUnicode {
    /// Lê o CMap; `None` se ele não mapeia nenhum código.
    pub fn parse(cmap: &[u8]) -> Option<Self> {
        let mut map = Self::default();
        let tokens = cmap_tokens(cmap);
        let mut index = 0;
        let hex = |index: usize| match tokens.get(index) {
            Some(CMapToken::Hex(bytes)) => Some(bytes.as_slice()),
            _ => None,
        };
        while index < tokens.len() {
            let CMapToken::Word(word) = &tokens[index] else {
                index += 1;
                continue;
            };
            index += 1;
            match word.as_str() {
                "begincodespacerange" => {
                    while let (Some(low), Some(high)) = (hex(index), hex(index + 1)) {
                        if let (Some((size, low)), Some((_, high))) = (code_of(low), code_of(high))
                        {
                            map.codespace.push((size, low, high));
                        }
                        index += 2;
                    }
                }
                "beginbfchar" => {
                    while let Some(source) = hex(index) {
                        // Destino não hexadecimal (ex.: nome de glifo) é ignorado.
                        if let (Some(code), Some(target)) = (code_of(source), hex(index + 1)) {
                            map.codes.insert(code, utf16_text(&utf16_units(target)));
                        }
                        index += 2;
                    }
                }
                "beginbfrange" => {
                    while let (Some(low), Some(high)) = (hex(index), hex(index + 1)) {
                        index += 2;
                        let (Some((size, low)), Some((_, high))) = (code_of(low), code_of(high))
                        else {
                            continue;
                        };
                        let count = high.saturating_sub(low).min(MAX_RANGE - 1);
                        match tokens.get(index) {
                            Some(CMapToken::Hex(target)) => {
                                let base = utf16_units(target);
                                for offset in 0..=count {
                                    let mut units = base.clone();
                                    if let Some(last) = units.last_mut() {
                                        *last = last.wrapping_add(offset as u16);
                                    }
                                    map.codes.insert((size, low + offset), utf16_text(&units));
                                }
                                index += 1;
                            }
                            Some(CMapToken::ArrayStart) => {
                                index += 1;
                                let mut offset = 0;
                                while let Some(target) = hex(index) {
                                    if offset <= count {
                                        map.codes.insert(
                                            (size, low + offset),
                                            utf16_text(&utf16_units(target)),
                                        );
                                    }
                                    offset += 1;
                                    index += 1;
                                }
                                if tokens.get(index) == Some(&CMapToken::ArrayEnd) {
                                    index += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if map.codes.is_empty() {
            return None;
        }
        if map.codespace.is_empty() {
            // Sem codespace: os comprimentos dos códigos mapeados o definem.
            let mut sizes: Vec<usize> = map.codes.keys().map(|(size, _)| *size).collect();
            sizes.dedup();
            map.codespace = sizes
                .into_iter()
                .map(|size| (size, 0, u32::MAX >> (32 - 8 * size as u32)))
                .collect();
        }
        map.codespace.sort_by_key(|(size, ..)| *size);
        Some(map)
    }

    /// Comprimento do código que começa em `bytes`.
    fn code_size(&self, bytes: &[u8]) -> usize {
        self.codespace
            .iter()
            .find(|(size, low, high)| {
                bytes.len() >= *size
                    && code_of(&bytes[..*size])
                        .is_some_and(|(_, value)| (*low..=*high).contains(&value))
            })
            .map_or(self.codespace[0].0, |(size, ..)| *size)
            .min(bytes.len())
    }

    /// Texto desenhado pelos bytes; código sem mapa vira `U+FFFD`.
    pub fn decode(&self, bytes: &[u8]) -> String {
        let mut text = String::new();
        let mut index = 0;
        while index < bytes.len() {
            let size = self.code_size(&bytes[index..]);
            match code_of(&bytes[index..index + size]).and_then(|code| self.codes.get(&code)) {
                Some(mapped) => text.push_str(mapped),
                None => text.push(char::REPLACEMENT_CHARACTER),
            }
            index += size;
        }
        text
    }

    /// Códigos que desenham `text`, ou o primeiro caractere sem código na fonte.
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, char> {
        // Reverso: texto → menor código que o desenha.
        let mut reverse: BTreeMap<&str, Code> = BTreeMap::new();
        for (code, mapped) in &self.codes {
            if !mapped.is_empty() && !mapped.contains(char::REPLACEMENT_CHARACTER) {
                reverse.entry(mapped.as_str()).or_insert(*code);
            }
        }
        let longest = reverse
            .keys()
            .map(|mapped| mapped.chars().count())
            .max()
            .unwrap_or(1);
        let characters: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut index = 0;
        while index < characters.len() {
            // Prefere a sequência mais longa (ligaduras como “fi”).
            let found = (1..=longest.min(characters.len() - index))
                .rev()
                .find_map(|count| {
                    let piece: String = characters[index..index + count].iter().collect();
                    reverse.get(piece.as_str()).map(|code| (count, *code))
                });
            let Some((count, (size, value))) = found else {
                return Err(characters[index]);
            };
            out.extend_from_slice(&value.to_be_bytes()[4 - size..]);
            index += count;
        }
        Ok(out)
    }
}

/// Caracteres da faixa 0x80–0x9F da WinAnsiEncoding (`None`: sem glifo).
const WIN_ANSI_HIGH: [Option<char>; 32] = [
    Some('\u{20ac}'),
    None,
    Some('\u{201a}'),
    Some('\u{0192}'),
    Some('\u{201e}'),
    Some('\u{2026}'),
    Some('\u{2020}'),
    Some('\u{2021}'),
    Some('\u{02c6}'),
    Some('\u{2030}'),
    Some('\u{0160}'),
    Some('\u{2039}'),
    Some('\u{0152}'),
    None,
    Some('\u{017d}'),
    None,
    None,
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('\u{201c}'),
    Some('\u{201d}'),
    Some('\u{2022}'),
    Some('\u{2013}'),
    Some('\u{2014}'),
    Some('\u{02dc}'),
    Some('\u{2122}'),
    Some('\u{0161}'),
    Some('\u{203a}'),
    Some('\u{0153}'),
    None,
    Some('\u{017e}'),
    Some('\u{0178}'),
];

/// Como os bytes de uma string mostrada viram texto numa fonte.
#[derive(Debug, Clone, PartialEq)]
pub enum FontCodec {
    /// CMap `/ToUnicode` da fonte (inclui fontes compostas, de 2 bytes).
    ToUnicode(ToUnicode),
    /// Fonte simples com `/WinAnsiEncoding`.
    WinAnsi,
    /// Fonte simples sem informação de codificação: um byte por caractere.
    Latin1,
}

impl FontCodec {
    pub fn decode(&self, bytes: &[u8]) -> String {
        match self {
            Self::ToUnicode(map) => map.decode(bytes),
            Self::WinAnsi => bytes
                .iter()
                .map(|byte| match byte {
                    0x80..=0x9f => WIN_ANSI_HIGH[usize::from(byte - 0x80)]
                        .unwrap_or(char::REPLACEMENT_CHARACTER),
                    _ => char::from(*byte),
                })
                .collect(),
            Self::Latin1 => bytes.iter().map(|byte| char::from(*byte)).collect(),
        }
    }

    /// Códigos de `text` nesta fonte, ou o primeiro caractere que ela não tem.
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, char> {
        match self {
            Self::ToUnicode(map) => map.encode(text),
            Self::WinAnsi => text
                .chars()
                .map(|character| {
                    let code = character as u32;
                    if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
                        return Ok(code as u8);
                    }
                    WIN_ANSI_HIGH
                        .iter()
                        .position(|high| *high == Some(character))
                        .map(|offset| 0x80 + offset as u8)
                        .ok_or(character)
                })
                .collect(),
            Self::Latin1 => text
                .chars()
                .map(|character| u8::try_from(character as u32).map_err(|_| character))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUBSET: &[u8] = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
        /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
        /CMapName /Adobe-Identity-UCS def /CMapType 2 def\n\
        1 begincodespacerange <00> <FF> endcodespacerange\n\
        3 beginbfchar <01> <0041> <02> <00E9> <03> <0020> endbfchar\n\
        1 beginbfrange <04> <06> <0061> endbfrange\n\
        1 beginbfrange <07> <08> [<0066006C> <2013>] endbfrange\n\
        endcmap CMapName currentdict /CMap defineresource pop end end";

    #[test]
    fn decodes_subset_codes_through_bfchar_and_bfrange() {
        let map = ToUnicode::parse(SUBSET).unwrap();
        assert_eq!(map.decode(&[1, 4, 5, 6, 3, 2, 7, 8]), "Aabc éfl\u{2013}");
        assert_eq!(map.decode(&[0x42]), "\u{fffd}");
    }

    #[test]
    fn encodes_text_back_into_subset_codes() {
        let map = ToUnicode::parse(SUBSET).unwrap();
        assert_eq!(map.encode("Abc é"), Ok(vec![1, 5, 6, 3, 2]));
        // Ligadura: a sequência mais longa vence.
        assert_eq!(map.encode("fl\u{2013}"), Ok(vec![7, 8]));
        assert_eq!(map.encode("Az"), Err('z'));
        assert_eq!(map.encode(""), Ok(vec![]));
    }

    #[test]
    fn handles_two_byte_codes_of_composite_fonts() {
        let cmap = b"1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
            2 beginbfchar <0003> <0020> <0024> <0041> endbfchar\n\
            1 beginbfrange <0044> <0046> <0061> endbfrange";
        let map = ToUnicode::parse(cmap).unwrap();
        assert_eq!(map.decode(&[0, 0x24, 0, 3, 0, 0x45]), "A b");
        assert_eq!(
            map.encode("ab A"),
            Ok(vec![0, 0x44, 0, 0x45, 0, 3, 0, 0x24])
        );
    }

    #[test]
    fn infers_code_size_without_codespace_and_skips_garbage() {
        let cmap = b"% comentario <00>\n2 beginbfchar <0001> <0041> <0002> /B endbfchar";
        let map = ToUnicode::parse(cmap).unwrap();
        assert_eq!(map.decode(&[0, 1, 0, 2]), "A\u{fffd}");
        assert!(ToUnicode::parse(b"begincmap endcmap").is_none());
        assert!(ToUnicode::parse(b"").is_none());
        // Faixa absurda é limitada, sem alocar 4 bilhões de entradas.
        let huge = b"1 beginbfrange <00000000> <FFFFFFFF> <0041> endbfrange";
        assert!(ToUnicode::parse(huge).unwrap().codes.len() <= MAX_RANGE as usize);
    }

    #[test]
    fn win_ansi_maps_typographic_characters() {
        let codec = FontCodec::WinAnsi;
        assert_eq!(
            codec.encode("“a” – 5€"),
            Ok(vec![0x93, b'a', 0x94, b' ', 0x96, b' ', b'5', 0x80])
        );
        assert_eq!(codec.decode(&[0x93, b'x', 0x94, 0x81]), "“x”\u{fffd}");
        assert_eq!(codec.encode("ação"), Ok(vec![b'a', 0xe7, 0xe3, b'o']));
        assert_eq!(codec.encode("中"), Err('中'));
        assert_eq!(codec.encode("\u{1}"), Err('\u{1}'));
    }

    #[test]
    fn latin1_is_one_byte_per_character() {
        let codec = FontCodec::Latin1;
        assert_eq!(codec.encode("é!"), Ok(vec![0xe9, b'!']));
        assert_eq!(codec.encode("€"), Err('€'));
        assert_eq!(codec.decode(&[0xe9]), "é");
    }
}
