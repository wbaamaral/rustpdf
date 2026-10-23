// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conteúdo dos diálogos: título, texto de apoio e rótulos dos botões a
//! partir das mensagens usadas pela interface. Os botões dizem a ação
//! ("Excluir", "Descartar") em vez de "Sim"/"Não", e ações destrutivas são
//! marcadas para ganhar destaque de perigo.

/// Verbos que iniciam uma confirmação e viram o rótulo do botão; os
/// destrutivos ganham destaque de perigo.
const VERBS: [(&str, bool); 6] = [
    ("Descartar", true),
    ("Excluir", true),
    ("Remover", true),
    ("Apagar", true),
    ("Substituir", true),
    ("Sair", false),
];

/// Confirmação: título, explicação opcional e botões.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirmation {
    pub heading: String,
    pub body: Option<String>,
    pub confirm: String,
    pub cancel: String,
    pub destructive: bool,
}

/// Separa o título (primeira linha ou primeira frase) do restante.
fn split_heading(message: &str) -> (String, Option<String>) {
    let message = message.trim();
    let (heading, body) = match message.split_once('\n') {
        Some((first, rest)) => (first, rest),
        None => match message
            .char_indices()
            .find(|(index, character)| {
                matches!(character, '?' | '.' | '!')
                    && message[index + character.len_utf8()..].starts_with(' ')
            })
            .map(|(index, character)| index + character.len_utf8())
        {
            Some(end) => (&message[..end], &message[end..]),
            None => (message, ""),
        },
    };
    let body = body.trim();
    (
        heading.trim().to_string(),
        (!body.is_empty()).then(|| body.to_string()),
    )
}

/// Monta a confirmação a partir da pergunta.
pub fn confirmation(message: &str) -> Confirmation {
    let (heading, body) = split_heading(message);
    let first_word = heading
        .split(|character: char| !character.is_alphanumeric())
        .next()
        .unwrap_or_default();
    if let Some((verb, destructive)) = VERBS.iter().find(|(verb, _)| *verb == first_word) {
        return Confirmation {
            heading,
            body,
            confirm: (*verb).to_string(),
            cancel: "Cancelar".into(),
            destructive: *destructive,
        };
    }
    let asks_to_continue = message.trim_end().ends_with("Continuar?");
    Confirmation {
        heading,
        body,
        confirm: if asks_to_continue { "Continuar" } else { "Sim" }.into(),
        cancel: if asks_to_continue { "Cancelar" } else { "Não" }.into(),
        destructive: false,
    }
}

/// Pedido de texto: título e dica (o que estava entre parênteses).
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub heading: String,
    pub hint: Option<String>,
}

/// Monta o pedido a partir do rótulo (`"Páginas (ex.: 1-3):"`).
pub fn prompt(message: &str) -> Prompt {
    let message = message.trim().trim_end_matches(':').trim_end();
    let (heading, hint) = match (message.find('('), message.rfind(')')) {
        (Some(open), Some(close)) if open < close && close + 1 == message.len() => (
            message[..open].trim_end(),
            Some(message[open + 1..close].trim().to_string()),
        ),
        _ => (message, None),
    };
    Prompt {
        heading: heading.to_string(),
        hint: hint.filter(|hint| !hint.is_empty()),
    }
}

/// Texto de uma linha: quebras e tabulações viram espaço (um trecho de texto
/// do PDF é desenhado numa linha só).
pub fn single_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if matches!(character, '\n' | '\r' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_end_matches(' ')
        .to_string()
}

/// O botão de aplicar a edição fica ativo só quando o texto mudou.
pub fn can_apply(original: &str, edited: &str) -> bool {
    single_line(edited) != original
}

/// Contagem exibida sob a caixa de edição.
pub fn character_count(text: &str) -> String {
    match text.chars().count() {
        1 => "1 caractere".into(),
        count => format!("{count} caracteres"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_verbs_label_the_button() {
        let discard = confirmation("Descartar alterações não salvas e sair?");
        assert_eq!(discard.heading, "Descartar alterações não salvas e sair?");
        assert_eq!(discard.body, None);
        assert_eq!(
            (discard.confirm.as_str(), discard.cancel.as_str()),
            ("Descartar", "Cancelar")
        );
        assert!(discard.destructive);
        let remove = confirmation(
            "Remover a proteção? Qualquer pessoa poderá abrir o arquivo salvo sem senha.",
        );
        assert_eq!(remove.heading, "Remover a proteção?");
        assert_eq!(
            remove.body.as_deref(),
            Some("Qualquer pessoa poderá abrir o arquivo salvo sem senha.")
        );
        assert_eq!(remove.confirm, "Remover");
        assert!(remove.destructive);
        assert!(!confirmation("Sair agora?").destructive);
    }

    #[test]
    fn first_line_is_the_heading_and_continue_keeps_its_verb() {
        let export = confirmation(
            "Este PDF está protegido por senha.\nO arquivo exportado não terá proteção. Continuar?",
        );
        assert_eq!(export.heading, "Este PDF está protegido por senha.");
        assert_eq!(
            export.body.as_deref(),
            Some("O arquivo exportado não terá proteção. Continuar?")
        );
        assert_eq!(
            (export.confirm.as_str(), export.cancel.as_str()),
            ("Continuar", "Cancelar")
        );
        assert!(!export.destructive);
    }

    #[test]
    fn plain_questions_are_yes_or_no() {
        let question = confirmation("Campo multilinha?");
        assert_eq!(question.heading, "Campo multilinha?");
        assert_eq!(
            (question.confirm.as_str(), question.cancel.as_str()),
            ("Sim", "Não")
        );
        // Ponto decimal não quebra o título.
        assert_eq!(confirmation("Versão 1.5?").heading, "Versão 1.5?");
    }

    #[test]
    fn prompt_moves_the_parenthetical_into_a_hint() {
        assert_eq!(
            prompt("Páginas a exportar (vazio = todas; ex.: 1-3,7):"),
            Prompt {
                heading: "Páginas a exportar".into(),
                hint: Some("vazio = todas; ex.: 1-3,7".into()),
            }
        );
        assert_eq!(
            prompt("Deslocamento DX DY (pontos PDF):"),
            Prompt {
                heading: "Deslocamento DX DY".into(),
                hint: Some("pontos PDF".into()),
            }
        );
        assert_eq!(
            prompt("Nome do campo:"),
            Prompt {
                heading: "Nome do campo".into(),
                hint: None,
            }
        );
        // Parênteses no meio continuam no título.
        assert_eq!(prompt("Valor (R$) do item").heading, "Valor (R$) do item");
        assert_eq!(prompt("Algo ():").hint, None);
    }

    #[test]
    fn edited_text_is_a_single_line_and_must_change() {
        assert_eq!(single_line("a\nb\tc\r\n"), "a b c");
        assert!(!can_apply("Valor", "Valor"));
        assert!(!can_apply("Valor", "Valor\n"));
        assert!(can_apply("Valor", "Valor 2"));
        assert!(can_apply("Valor", ""));
        assert_eq!(character_count("é"), "1 caractere");
        assert_eq!(character_count("abc"), "3 caracteres");
    }
}
