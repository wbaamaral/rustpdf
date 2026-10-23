// SPDX-License-Identifier: AGPL-3.0-or-later
//! Menu de contexto da página (botão direito): quais ações valem para o
//! objeto sob o ponteiro e como o texto colado vira texto da página.

use super::dialogos::single_line;

/// O que está sob o ponteiro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Nothing,
    Text,
    /// Imagem, anotação ou campo.
    Other,
}

/// Situação no momento do clique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub target: Target,
    /// "Selecionar tudo" está ativo (todo o texto da página destacado).
    pub page_text_selected: bool,
    pub clipboard_has_text: bool,
    pub can_undo: bool,
    /// Vários objetos selecionados (o clique foi dentro do grupo).
    pub group: bool,
}

/// Itens habilitados do menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Actions {
    pub edit_inline: bool,
    pub edit_dialog: bool,
    pub cut: bool,
    pub copy: bool,
    pub paste: bool,
    pub delete: bool,
    pub select_all: bool,
    pub undo: bool,
}

pub fn actions(context: Context) -> Actions {
    let text = context.target == Target::Text;
    // Editar é de um trecho só; num grupo, só as ações de conjunto.
    let single_text = text && !context.group;
    Actions {
        edit_inline: single_text,
        edit_dialog: single_text,
        cut: text,
        copy: text || context.page_text_selected,
        paste: context.clipboard_has_text,
        delete: context.target != Target::Nothing,
        select_all: true,
        undo: context.can_undo,
    }
}

/// Texto da área de transferência pronto para inserir na página: uma linha,
/// sem espaços nas pontas; `None` se não sobrar nada.
pub fn paste_text(clipboard: &str) -> Option<String> {
    let line = single_line(clipboard.trim());
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(target: Target) -> Context {
        Context {
            target,
            page_text_selected: false,
            clipboard_has_text: false,
            can_undo: false,
            group: false,
        }
    }

    #[test]
    fn groups_hide_text_editing_but_keep_clipboard_and_delete() {
        let with_text = actions(Context {
            group: true,
            ..context(Target::Text)
        });
        assert!(!with_text.edit_inline && !with_text.edit_dialog);
        assert!(with_text.cut && with_text.copy && with_text.delete);
        let images = actions(Context {
            group: true,
            ..context(Target::Other)
        });
        assert!(!images.cut && !images.copy && images.delete);
    }

    #[test]
    fn text_enables_editing_and_clipboard_actions() {
        let text = actions(context(Target::Text));
        assert!(text.edit_inline && text.edit_dialog && text.cut && text.copy && text.delete);
        assert!(!text.paste && !text.undo);
        assert!(text.select_all);
    }

    #[test]
    fn other_objects_can_only_be_deleted() {
        let image = actions(context(Target::Other));
        assert!(image.delete);
        assert!(!image.edit_inline && !image.edit_dialog && !image.cut && !image.copy);
        let empty = actions(context(Target::Nothing));
        assert!(!empty.delete && !empty.copy);
        assert!(empty.select_all);
    }

    #[test]
    fn page_selection_clipboard_and_history_drive_the_rest() {
        let all = actions(Context {
            target: Target::Nothing,
            page_text_selected: true,
            clipboard_has_text: true,
            can_undo: true,
            group: false,
        });
        assert!(all.copy && all.paste && all.undo);
        assert!(!all.cut && !all.delete);
    }

    #[test]
    fn pasted_text_becomes_one_clean_line() {
        assert_eq!(
            paste_text("  linha 1\nlinha\t2  \n"),
            Some("linha 1 linha 2".into())
        );
        assert_eq!(paste_text(" \n\t "), None);
        assert_eq!(paste_text("R$ 10,00"), Some("R$ 10,00".into()));
    }
}
