// SPDX-License-Identifier: AGPL-3.0-or-later
//! Escape de texto XML (leitura do poppler e escrita de OOXML).

/// Texto de um nó XML com as entidades resolvidas (nomeadas e numéricas).
/// Entidades desconhecidas ou malformadas ficam como estão.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let decoded = tail
            .find(';')
            .filter(|&end| end <= 12)
            .and_then(|end| entity(&tail[1..end]).map(|c| (c, end)));
        match decoded {
            Some((character, end)) => {
                out.push(character);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Texto pronto para nó ou atributo XML; remove controles proibidos no XML 1.0.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(character),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_named_and_numeric_entities() {
        assert_eq!(
            unescape("a &amp; b &lt;c&gt; &quot;d&quot; &apos;e&apos;"),
            "a & b <c> \"d\" 'e'"
        );
        assert_eq!(unescape("&#233;t&#xE9; &#X41;"), "été A");
        // &amp;lt; é o texto "&lt;", não "<".
        assert_eq!(unescape("&amp;lt;"), "&lt;");
        // Entidade desconhecida ou malformada fica como está.
        assert_eq!(unescape("&nbsp; & &#xZZ; &#;"), "&nbsp; & &#xZZ; &#;");
    }

    #[test]
    fn escape_produces_valid_xml_text() {
        assert_eq!(
            escape("a & b <c> \"d\" 'e'"),
            "a &amp; b &lt;c&gt; &quot;d&quot; &apos;e&apos;"
        );
        // Controles proibidos no XML 1.0 somem; tab e quebras ficam.
        assert_eq!(escape("x\u{0}y\u{1b}z\t\n\r"), "xyz\t\n\r");
        assert_eq!(unescape(&escape("<&'\">ç")), "<&'\">ç");
    }
}
