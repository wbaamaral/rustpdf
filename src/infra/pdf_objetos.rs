// SPDX-License-Identifier: AGPL-3.0-or-later
//! Serialização mínima de um PDF a partir dos corpos dos objetos.

/// PDF com os objetos `1 0 obj`, `2 0 obj`, … na ordem dada, tabela `xref`
/// e trailer com `/Root 1 0 R`. O primeiro objeto deve ser o catálogo.
pub fn serialize(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0];
    for (index, body) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        bytes.extend_from_slice(body);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    bytes
}

/// Corpo de um objeto stream (`<< /Length n >> stream … endstream`).
pub fn stream(dictionary_entries: &str, data: &[u8]) -> Vec<u8> {
    let mut body =
        format!("<< /Length {}{dictionary_entries} >>\nstream\n", data.len()).into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// Valor numérico da linha que segue `marker` (offsets são em bytes).
    fn number_after(pdf: &[u8], marker: &str) -> usize {
        let at = find(pdf, marker.as_bytes(), 0).unwrap() + marker.len();
        let end = find(pdf, b"\n", at).unwrap();
        text(&pdf[at..end]).parse().unwrap()
    }

    fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        haystack[from..]
            .windows(needle.len())
            .position(|window| window == needle)
            .map(|position| position + from)
    }

    #[test]
    fn xref_points_at_each_object() {
        let pdf = serialize(&[b"<< /Type /Catalog >>".to_vec(), b"42".to_vec()]);
        let header = b"xref\n0 3\n0000000000 65535 f \n";
        let table = find(&pdf, header, 0).unwrap() + header.len();
        let entry = |index: usize| -> usize {
            let start = table + index * 20;
            text(&pdf[start..start + 10]).parse().unwrap()
        };
        assert!(pdf[entry(0)..].starts_with(b"1 0 obj\n<< /Type /Catalog >>"));
        assert!(pdf[entry(1)..].starts_with(b"2 0 obj\n42\nendobj"));
    }

    #[test]
    fn trailer_and_startxref_are_consistent() {
        let pdf = serialize(&[b"<< /Type /Catalog >>".to_vec()]);
        assert!(pdf.starts_with(b"%PDF-1.4\n"));
        assert!(find(&pdf, b"trailer\n<< /Size 2 /Root 1 0 R >>", 0).is_some());
        let start = number_after(&pdf, "startxref\n");
        assert!(pdf[start..].starts_with(b"xref\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn stream_declares_exact_length() {
        let body = stream(" /Filter /FlateDecode", b"abc");
        assert_eq!(
            text(&body),
            "<< /Length 3 /Filter /FlateDecode >>\nstream\nabc\nendstream"
        );
    }
}
