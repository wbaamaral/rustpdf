// SPDX-License-Identifier: AGPL-3.0-or-later
//! Inserção de campos de formulário pela interface: lista de opções digitada,
//! e nome interno a partir do rótulo visível.

/// Opções digitadas separadas por vírgula, sem vazias nem repetidas.
pub fn parse_options(text: &str) -> Vec<String> {
    let mut options: Vec<String> = Vec::new();
    for option in text
        .split(',')
        .map(str::trim)
        .filter(|option| !option.is_empty())
    {
        if !options.iter().any(|existing| existing == option) {
            options.push(option.to_string());
        }
    }
    options
}

/// Nome interno (`/T`) a partir do rótulo: minúsculas ASCII, dígitos e `_`,
/// sem acentos, único entre `existing` (sufixo `_2`, `_3`…). Rótulo vazio
/// usa `fallback`.
pub fn field_name(label: &str, fallback: &str, existing: &[String]) -> String {
    let mut base = String::new();
    for character in label.chars().flat_map(fold_accent) {
        if character.is_ascii_alphanumeric() {
            base.push(character.to_ascii_lowercase());
        } else if !base.ends_with('_') && !base.is_empty() {
            base.push('_');
        }
    }
    let base = base.trim_end_matches('_');
    let base = if base.is_empty() { fallback } else { base };
    let base: String = base.chars().take(40).collect();
    if !existing.contains(&base) {
        return base;
    }
    (2..)
        .map(|index| format!("{base}_{index}"))
        .find(|candidate| !existing.contains(candidate))
        .expect("sequência infinita")
}

/// Letra sem acento (á → a, ç → c); outros caracteres ficam como estão.
fn fold_accent(character: char) -> Vec<char> {
    let folded = match character {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'A',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'ç' => 'c',
        'Ç' => 'C',
        'ñ' => 'n',
        'Ñ' => 'N',
        other => other,
    };
    vec![folded]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_trimmed_without_blanks_or_repeats() {
        assert_eq!(
            parse_options(" Pix, Boleto ,,Cartão,Pix "),
            ["Pix", "Boleto", "Cartão"]
        );
        assert!(parse_options(" , ").is_empty());
    }

    #[test]
    fn names_come_from_labels_and_stay_unique() {
        let existing = vec![
            "aceito_os_termos".to_string(),
            "aceito_os_termos_2".to_string(),
        ];
        assert_eq!(
            field_name("Forma de pagamento", "radio", &[]),
            "forma_de_pagamento"
        );
        assert_eq!(field_name("Ação & Opção!", "campo", &[]), "acao_opcao");
        assert_eq!(
            field_name("Aceito os termos", "caixa", &existing),
            "aceito_os_termos_3"
        );
        assert_eq!(field_name("  ", "caixa", &[]), "caixa");
        assert_eq!(field_name("—", "caixa", &["caixa".into()]), "caixa_2");
        assert_eq!(field_name(&"x".repeat(80), "c", &[]).len(), 40);
    }
}
