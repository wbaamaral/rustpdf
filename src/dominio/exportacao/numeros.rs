// SPDX-License-Identifier: AGPL-3.0-or-later
//! Valores de célula: números no formato brasileiro (e o inglês quando não
//! há ambiguidade), moeda e porcentagem; o resto continua texto.

#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Number(f64),
    /// Valor em reais (`R$`).
    Currency(f64),
    /// Fração: `15%` vale 0,15.
    Percent(f64),
    Text(String),
}

/// Valor de uma célula de tabela exportada.
pub fn cell_value(text: &str) -> CellValue {
    let trimmed = text.trim();
    if let Some(amount) = trimmed.strip_prefix("R$") {
        if let Some(value) = number(amount.trim()) {
            return CellValue::Currency(value);
        }
    }
    if let Some(amount) = trimmed.strip_suffix('%') {
        if let Some(value) = number(amount.trim()) {
            return CellValue::Percent(value / 100.0);
        }
    }
    match number(trimmed) {
        Some(value) => CellValue::Number(value),
        None => CellValue::Text(text.to_string()),
    }
}

/// `1.234,56`, `1234,5`, `1.000` (milhar) ou `3.14` (decimal com ponto,
/// quando não pode ser milhar). Zeros à esquerda (`007`) são código, não número.
fn number(text: &str) -> Option<f64> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text),
    };
    let first = digits.chars().next()?;
    let second = digits.chars().nth(1);
    if !first.is_ascii_digit()
        || (first == '0' && second.is_some_and(|c| c.is_ascii_digit()))
        || !digits
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return None;
    }
    let normalized = match digits.split_once(',') {
        Some((whole, fraction)) => {
            let whole_ok = plain(whole) || thousands(whole);
            if !whole_ok || !plain(fraction) {
                return None;
            }
            format!("{}.{fraction}", whole.replace('.', ""))
        }
        None if thousands(digits) => digits.replace('.', ""),
        None => match digits.split_once('.') {
            Some((whole, fraction)) if plain(whole) && plain(fraction) => digits.to_string(),
            Some(_) => return None,
            None => digits.to_string(),
        },
    };
    // Acima de 15 dígitos significativos o f64 (e a planilha) perde dígitos:
    // chave de NF-e, código de barras e afins ficam como texto.
    let significant = normalized
        .chars()
        .filter(char::is_ascii_digit)
        .skip_while(|digit| *digit == '0')
        .count();
    if significant > 15 {
        return None;
    }
    format!("{sign}{normalized}")
        .parse()
        .ok()
        .filter(|value: &f64| value.is_finite())
}

fn plain(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

/// `1.000`, `12.345.678`: grupos de três dígitos separados por ponto.
fn thousands(text: &str) -> bool {
    let mut groups = text.split('.');
    let head = groups.next().unwrap_or("");
    let rest: Vec<&str> = groups.collect();
    plain(head)
        && head.len() <= 3
        && !rest.is_empty()
        && rest.iter().all(|g| g.len() == 3 && plain(g))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brazilian_numbers() {
        assert_eq!(cell_value("1.234,56"), CellValue::Number(1234.56));
        assert_eq!(cell_value("1234,5"), CellValue::Number(1234.5));
        assert_eq!(
            cell_value("1.000"),
            CellValue::Number(1000.0),
            "ponto de milhar"
        );
        assert_eq!(cell_value("-12,5"), CellValue::Number(-12.5));
        assert_eq!(cell_value("42"), CellValue::Number(42.0));
        assert_eq!(cell_value(" 0,5 "), CellValue::Number(0.5));
    }

    #[test]
    fn unambiguous_english_decimals() {
        assert_eq!(cell_value("2.75"), CellValue::Number(2.75));
        assert_eq!(cell_value("0.5"), CellValue::Number(0.5));
    }

    #[test]
    fn currency_and_percent() {
        assert_eq!(cell_value("R$ 2.500,00"), CellValue::Currency(2500.0));
        assert_eq!(cell_value("R$-3,10"), CellValue::Currency(-3.1));
        assert_eq!(cell_value("15%"), CellValue::Percent(0.15));
        assert_eq!(cell_value("12,5 %"), CellValue::Percent(0.125));
    }

    #[test]
    fn long_digit_runs_stay_text() {
        // Chave de NF-e, código de barras: o f64 perderia dígitos.
        let key = "35190612345678000123550010000012341000012345";
        assert_eq!(cell_value(key), CellValue::Text(key.into()));
        assert_eq!(
            cell_value(&"9".repeat(400)),
            CellValue::Text("9".repeat(400))
        );
        assert_eq!(
            cell_value("123456789012345"),
            CellValue::Number(123456789012345.0)
        );
    }

    #[test]
    fn everything_else_stays_text() {
        for text in [
            "",
            "abc",
            "007",
            "12/03/2024",
            "1.2.3",
            "1,2,3",
            "R$",
            "%",
            "1.23.456",
            "10-20",
            "1e5",
        ] {
            assert_eq!(cell_value(text), CellValue::Text(text.into()), "{text:?}");
        }
    }
}
