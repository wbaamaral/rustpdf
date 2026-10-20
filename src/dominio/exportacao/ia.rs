// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pré-processamento e otimização do documento para consumo por agentes de IA.

use super::modelo::{Block, Document};

/// Opções de pré-processamento para consumo por agentes de IA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiOptimizationOptions {
    /// Remove cabeçalhos e rodapés repetitivos entre páginas.
    pub remove_headers_footers: bool,
    /// Remove números de páginas isolados ou marcações de paginação.
    pub remove_page_numbers: bool,
    /// Une parágrafos fragmentados pela quebra de página física.
    pub merge_cross_page_paragraphs: bool,
    /// Descarta blocos de imagens binárias.
    pub remove_images: bool,
    /// Remove tabelas vazias (sem texto real nas células).
    pub remove_empty_tables: bool,
    /// Condensa sequências repetitivas de caracteres de preenchimento decorativo.
    pub clean_decorative_noise: bool,
}

impl Default for AiOptimizationOptions {
    fn default() -> Self {
        Self {
            remove_headers_footers: true,
            remove_page_numbers: true,
            merge_cross_page_paragraphs: true,
            remove_images: true,
            remove_empty_tables: true,
            clean_decorative_noise: true,
        }
    }
}

/// Aplica as regras de otimização para agentes de IA sobre o documento.
pub fn optimize_for_ai(document: &Document, options: &AiOptimizationOptions) -> Document {
    let mut doc = document.clone();
    if options.remove_images {
        strip_images(&mut doc);
    }
    if options.remove_empty_tables {
        strip_empty_tables(&mut doc);
    }
    split_inline_bullet_paragraphs(&mut doc);
    strip_orphan_bullet_blocks(&mut doc);
    if options.remove_page_numbers {
        strip_page_numbers(&mut doc);
    }
    if options.remove_headers_footers {
        strip_running_headers_footers(&mut doc);
    }
    if options.clean_decorative_noise {
        clean_decorative_noise(&mut doc);
    }
    strip_pua_characters(&mut doc);
    if options.merge_cross_page_paragraphs {
        merge_cross_page_paragraphs(&mut doc);
    }
    // Remove páginas que tenham ficado completamente vazias após a limpeza
    doc.pages.retain(|p| !p.blocks.is_empty());
    doc
}

/// Remove blocos de imagens binárias.
fn strip_images(doc: &mut Document) {
    for page in &mut doc.pages {
        page.blocks
            .retain(|block| !matches!(block, Block::Image { .. }));
    }
}

/// Remove tabelas vazias (onde todas as células estão em branco).
fn strip_empty_tables(doc: &mut Document) {
    for page in &mut doc.pages {
        page.blocks.retain(|block| match block {
            Block::Table { rows } => rows
                .iter()
                .any(|row| row.iter().any(|cell| !cell.trim().is_empty())),
            _ => true,
        });
    }
}

/// Remove marcações de numeração de página nos extremos (primeiro ou último bloco).
fn strip_page_numbers(doc: &mut Document) {
    for page in &mut doc.pages {
        // Verifica o primeiro bloco
        if let Some(block) = page.blocks.first() {
            if is_page_number_block(block) {
                page.blocks.remove(0);
            }
        }
        // Verifica o último bloco
        if let Some(block) = page.blocks.last() {
            if is_page_number_block(block) {
                page.blocks.pop();
            }
        }
    }
}

fn is_page_number_block(block: &Block) -> bool {
    let text = match block {
        Block::Paragraph { text } | Block::Heading { text, .. } => text.trim(),
        _ => return false,
    };
    is_page_number_text(text)
}

fn is_page_number_text(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    // Apenas dígitos
    if text.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // Algarismos romanos isolados (ex.: i, iv, viii)
    let is_roman = (1..=6).contains(&text.len())
        && text
            .chars()
            .all(|c| matches!(c, 'i' | 'v' | 'x' | 'I' | 'V' | 'X'));
    if is_roman {
        return true;
    }
    // Padrão "- N -" ou "-- N --"
    let stripped_dashes = text.trim_matches(['-', '—', '–', ' ']);
    if !stripped_dashes.is_empty() && stripped_dashes.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // Padrões como "Página 1", "Pág. 1", "Página 1 de 10", "Page 1 of 5", "1 / 10"
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    match words.as_slice() {
        ["página" | "pagina" | "pág." | "pag." | "page", n]
            if n.chars().all(|c| c.is_ascii_digit()) =>
        {
            true
        }
        ["página" | "pagina" | "pág." | "pag." | "page", n, "de" | "of" | "/", total]
            if n.chars().all(|c| c.is_ascii_digit())
                && total.chars().all(|c| c.is_ascii_digit()) =>
        {
            true
        }
        [n, "/" | "de" | "of", total]
            if n.chars().all(|c| c.is_ascii_digit())
                && total.chars().all(|c| c.is_ascii_digit()) =>
        {
            true
        }
        _ => false,
    }
}

/// Remove cabeçalhos e rodapés repetitivos entre páginas.
fn strip_running_headers_footers(doc: &mut Document) {
    if doc.pages.len() < 2 {
        return;
    }

    // 1. Detectar cabeçalho repetido: texto do primeiro bloco que se repete em múltiplas páginas
    let first_texts: Vec<Option<String>> = doc
        .pages
        .iter()
        .map(|p| p.blocks.first().and_then(block_text_trimmed))
        .collect();

    let mut repeated_header = None;
    for i in 0..first_texts.len().saturating_sub(1) {
        if let (Some(a), Some(b)) = (&first_texts[i], &first_texts[i + 1]) {
            if a == b && a.len() <= 120 {
                repeated_header = Some(a.clone());
                break;
            }
        }
    }

    // Se encontrou cabeçalho repetido, remove a partir da página 2
    // (preserva na página 1 se for o título inicial do documento)
    if let Some(header) = repeated_header {
        for (idx, page) in doc.pages.iter_mut().enumerate() {
            if idx > 0 {
                if let Some(block) = page.blocks.first() {
                    if block_text_trimmed(block).as_deref() == Some(&header) {
                        page.blocks.remove(0);
                    }
                }
            }
        }
    }

    // 2. Detectar rodapé repetido: texto do último bloco que se repete em múltiplas páginas
    let last_texts: Vec<Option<String>> = doc
        .pages
        .iter()
        .map(|p| p.blocks.last().and_then(block_text_trimmed))
        .collect();

    let mut repeated_footer = None;
    for i in 0..last_texts.len().saturating_sub(1) {
        if let (Some(a), Some(b)) = (&last_texts[i], &last_texts[i + 1]) {
            if a == b && a.len() <= 120 {
                repeated_footer = Some(a.clone());
                break;
            }
        }
    }

    // Se encontrou rodapé repetido, remove de todas as páginas
    if let Some(footer) = repeated_footer {
        for page in &mut doc.pages {
            if let Some(block) = page.blocks.last() {
                if block_text_trimmed(block).as_deref() == Some(&footer) {
                    page.blocks.pop();
                }
            }
        }
    }
}

fn block_text_trimmed(block: &Block) -> Option<String> {
    match block {
        Block::Heading { text, .. } | Block::Paragraph { text } => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

/// Condensa sequências longas de preenchimento decorativo (ex.: `.......`, `-------`).
fn clean_decorative_noise(doc: &mut Document) {
    for page in &mut doc.pages {
        for block in &mut page.blocks {
            match block {
                Block::Paragraph { text } | Block::Heading { text, .. } => {
                    *text = clean_text_content(text);
                }
                Block::ListItem { text, .. } => {
                    *text = clean_text_content(text);
                }
                Block::Table { rows } => {
                    for row in rows {
                        for cell in row {
                            *cell = clean_text_content(cell);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Caracteres gráficos usados como marcadores de lista.
fn is_bullet_char(c: char) -> bool {
    matches!(
        c,
        '•' | '◦'
            | '▪'
            | '▫'
            | '‣'
            | '●'
            | '○'
            | '■'
            | '·'
            | '\u{f0b7}'
            | '\u{f0a7}'
            | '\u{f0d8}'
            | '\u{f0fc}'
    ) || (0xF000..=0xF0FF).contains(&(c as u32))
}

/// Remove blocos que consistem apenas de marcadores isolados (ex.: coluna de bullets desemparelhada).
fn strip_orphan_bullet_blocks(doc: &mut Document) {
    for page in &mut doc.pages {
        page.blocks.retain(|block| match block {
            Block::Paragraph { text } | Block::Heading { text, .. } => {
                let trimmed = text.trim();
                let all_bullets = !trimmed.is_empty()
                    && trimmed.chars().all(|c| {
                        c.is_whitespace() || is_bullet_char(c) || matches!(c, '-' | '–' | '—' | '*')
                    });
                !all_bullets
            }
            Block::ListItem { text, .. } => !text.trim().is_empty(),
            _ => true,
        });
    }
}

/// Se um parágrafo contém múltiplos marcadores de lista no meio do texto,
/// separa-o em blocos ListItem individuais.
fn split_inline_bullet_paragraphs(doc: &mut Document) {
    for page in &mut doc.pages {
        let mut new_blocks = Vec::with_capacity(page.blocks.len());
        for block in page.blocks.drain(..) {
            match block {
                Block::Paragraph { text } if contains_multiple_inline_bullets(&text) => {
                    for item in split_by_bullets(&text) {
                        let trimmed = item.trim();
                        if !trimmed.is_empty() {
                            new_blocks.push(Block::ListItem {
                                marker: String::new(),
                                level: 0,
                                text: trimmed.to_string(),
                            });
                        }
                    }
                }
                other => new_blocks.push(other),
            }
        }
        page.blocks = new_blocks;
    }
}

fn contains_multiple_inline_bullets(text: &str) -> bool {
    text.chars().filter(|&c| is_bullet_char(c)).count() >= 2
}

fn split_by_bullets(text: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if is_bullet_char(c) {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                items.push(trimmed.to_string());
            }
            current.clear();
        } else {
            current.push(c);
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        items.push(trimmed.to_string());
    }
    items
}

/// Remove caracteres da área de uso privado (PUA) de todos os blocos do documento.
fn strip_pua_characters(doc: &mut Document) {
    for page in &mut doc.pages {
        for block in &mut page.blocks {
            match block {
                Block::Paragraph { text } | Block::Heading { text, .. } => {
                    *text = filter_pua(text);
                }
                Block::ListItem { text, .. } => {
                    *text = filter_pua(text);
                }
                Block::Table { rows } => {
                    for row in rows {
                        for cell in row {
                            *cell = filter_pua(cell);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn filter_pua(s: &str) -> String {
    s.chars()
        .filter(|&c| !(0xE000..=0xF8FF).contains(&(c as u32)))
        .collect()
}

/// Remove caracteres da área de uso privado (PUA) e condensa preenchimentos decorativos.
fn clean_text_content(s: &str) -> String {
    let without_pua = filter_pua(s);
    condense_repeated_chars(&without_pua)
}

fn condense_repeated_chars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_char = None;
    let mut run_len = 0;

    for c in s.chars() {
        if matches!(c, '.' | '-' | '_' | ' ' | '~' | '=') {
            if Some(c) == last_char {
                run_len += 1;
                // Mantém no máximo 3 repetições de caracteres de preenchimento
                if run_len <= 3 {
                    out.push(c);
                }
            } else {
                last_char = Some(c);
                run_len = 1;
                out.push(c);
            }
        } else {
            last_char = Some(c);
            run_len = 1;
            out.push(c);
        }
    }
    out
}

/// Une parágrafos fragmentados que cruzam a divisa entre páginas consecutivas.
fn merge_cross_page_paragraphs(doc: &mut Document) {
    if doc.pages.len() < 2 {
        return;
    }

    for i in 0..doc.pages.len().saturating_sub(1) {
        let (left, right) = doc.pages.split_at_mut(i + 1);
        let prev_page = &mut left[i];
        let next_page = &mut right[0];

        let Some(last_block) = prev_page.blocks.last_mut() else {
            continue;
        };
        let Some(first_block) = next_page.blocks.first() else {
            continue;
        };

        if let (Block::Paragraph { text: prev_text }, Block::Paragraph { text: next_text }) =
            (last_block, first_block)
        {
            let prev_trimmed = prev_text.trim_end();
            let next_trimmed = next_text.trim_start();

            if next_trimmed.is_empty() {
                continue;
            }

            let first_char = next_trimmed.chars().next().unwrap_or(' ');

            // Caso 1: Palavra partida com hífen na margem da página ("interrup-" + "ção")
            let has_hyphen = prev_trimmed.ends_with('-')
                && prev_trimmed
                    .strip_suffix('-')
                    .and_then(|s| s.chars().last())
                    .is_some_and(char::is_alphabetic);

            if has_hyphen && first_char.is_lowercase() {
                // Remove o hífen e junta diretamente a próxima palavra
                let stem = prev_trimmed.strip_suffix('-').unwrap_or(prev_trimmed);
                *prev_text = format!("{stem}{next_trimmed}");
                next_page.blocks.remove(0);
                continue;
            }

            // Caso 2: Frase incompleta (sem pontuação terminal) e próxima página inicia com minúscula
            let ends_with_terminal = prev_trimmed
                .chars()
                .last()
                .is_some_and(|c| matches!(c, '.' | '!' | '?' | ':' | ';' | '…'));

            if !ends_with_terminal && first_char.is_lowercase() {
                *prev_text = format!("{prev_trimmed} {next_trimmed}");
                next_page.blocks.remove(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dominio::exportacao::modelo::{Block, Document, PageContent, A4};

    fn page(number: usize, blocks: Vec<Block>) -> PageContent {
        PageContent {
            number,
            size: A4,
            blocks,
        }
    }

    fn paragraph(text: &str) -> Block {
        Block::Paragraph { text: text.into() }
    }

    fn heading(level: u8, text: &str) -> Block {
        Block::Heading {
            level,
            text: text.into(),
        }
    }

    #[test]
    fn removes_running_headers_and_footers_from_subsequent_pages() {
        let doc = Document {
            pages: vec![
                page(
                    1,
                    vec![
                        heading(1, "Manual do Usuário"),
                        paragraph("Texto da página um."),
                        paragraph("Confidencial 2026"),
                    ],
                ),
                page(
                    2,
                    vec![
                        heading(1, "Manual do Usuário"),
                        paragraph("Texto da página dois."),
                        paragraph("Confidencial 2026"),
                    ],
                ),
                page(
                    3,
                    vec![
                        heading(1, "Manual do Usuário"),
                        paragraph("Texto da página três."),
                        paragraph("Confidencial 2026"),
                    ],
                ),
            ],
        };

        let opt = optimize_for_ai(&doc, &AiOptimizationOptions::default());
        // Na pág 1 o cabeçalho é mantido (título do documento), mas o rodapé repetido é removido
        assert_eq!(
            opt.pages[0].blocks,
            [
                heading(1, "Manual do Usuário"),
                paragraph("Texto da página um.")
            ]
        );
        // Nas páginas 2 e 3 tanto o cabeçalho quanto o rodapé são removidos
        assert_eq!(opt.pages[1].blocks, [paragraph("Texto da página dois.")]);
        assert_eq!(opt.pages[2].blocks, [paragraph("Texto da página três.")]);
    }

    #[test]
    fn removes_page_number_markers() {
        let doc = Document {
            pages: vec![
                page(
                    1,
                    vec![
                        paragraph("Página 1 de 3"),
                        paragraph("Conteúdo 1."),
                        paragraph("1"),
                    ],
                ),
                page(
                    2,
                    vec![
                        paragraph("- 2 -"),
                        paragraph("Conteúdo 2."),
                        paragraph("Page 2 of 3"),
                    ],
                ),
                page(
                    3,
                    vec![
                        paragraph("Pág. 3"),
                        paragraph("Conteúdo 3."),
                        paragraph("iii"),
                    ],
                ),
            ],
        };

        let opt = optimize_for_ai(&doc, &AiOptimizationOptions::default());
        assert_eq!(opt.pages[0].blocks, [paragraph("Conteúdo 1.")]);
        assert_eq!(opt.pages[1].blocks, [paragraph("Conteúdo 2.")]);
        assert_eq!(opt.pages[2].blocks, [paragraph("Conteúdo 3.")]);
    }

    #[test]
    fn merges_hyphenated_cross_page_paragraphs() {
        let doc = Document {
            pages: vec![
                page(1, vec![paragraph("O sistema oferece alta disponi-")]),
                page(2, vec![paragraph("bilidade e resiliência em nuvem.")]),
            ],
        };

        let opt = optimize_for_ai(&doc, &AiOptimizationOptions::default());
        assert_eq!(opt.pages.len(), 1);
        assert_eq!(
            opt.pages[0].blocks,
            [paragraph(
                "O sistema oferece alta disponibilidade e resiliência em nuvem."
            )]
        );
    }

    #[test]
    fn merges_unpunctuated_cross_page_paragraphs() {
        let doc = Document {
            pages: vec![
                page(
                    1,
                    vec![paragraph(
                        "A inteligência artificial aplicada ao processamento de",
                    )],
                ),
                page(2, vec![paragraph("documentos técnicos acelera a análise.")]),
            ],
        };

        let opt = optimize_for_ai(&doc, &AiOptimizationOptions::default());
        assert_eq!(opt.pages.len(), 1);
        assert_eq!(
            opt.pages[0].blocks,
            [paragraph(
                "A inteligência artificial aplicada ao processamento de documentos técnicos acelera a análise."
            )]
        );
    }

    #[test]
    fn does_not_merge_when_previous_ends_with_terminal_punctuation() {
        let doc = Document {
            pages: vec![
                page(1, vec![paragraph("Primeiro parágrafo completo.")]),
                page(2, vec![paragraph("Segundo parágrafo independente.")]),
            ],
        };

        let opt = optimize_for_ai(&doc, &AiOptimizationOptions::default());
        assert_eq!(opt.pages.len(), 2);
        assert_eq!(
            opt.pages[0].blocks,
            [paragraph("Primeiro parágrafo completo.")]
        );
        assert_eq!(
            opt.pages[1].blocks,
            [paragraph("Segundo parágrafo independente.")]
        );
    }

    #[test]
    fn strips_images_and_empty_tables() {
        let doc = Document {
            pages: vec![page(
                1,
                vec![
                    Block::Image {
                        png: vec![1, 2, 3],
                        size: (100.0, 50.0),
                    },
                    Block::Table {
                        rows: vec![vec!["  ".into(), "".into()], vec!["\t".into(), "".into()]],
                    },
                    paragraph("Texto relevante."),
                    Block::Table {
                        rows: vec![vec!["Coluna 1".into(), "Coluna 2".into()]],
                    },
                ],
            )],
        };

        let opt = optimize_for_ai(&doc, &AiOptimizationOptions::default());
        assert_eq!(
            opt.pages[0].blocks,
            [
                paragraph("Texto relevante."),
                Block::Table {
                    rows: vec![vec!["Coluna 1".into(), "Coluna 2".into()]],
                }
            ]
        );
    }

    #[test]
    fn condenses_decorative_filler_characters() {
        let doc = Document {
            pages: vec![page(
                1,
                vec![paragraph(
                    "Nome: ........................................ Data: ----------------",
                )],
            )],
        };

        let opt = optimize_for_ai(&doc, &AiOptimizationOptions::default());
        assert_eq!(opt.pages[0].blocks, [paragraph("Nome: ... Data: ---")]);
    }

    #[test]
    fn strips_orphan_bullet_blocks_and_pua_chars() {
        let doc = Document {
            pages: vec![page(
                1,
                vec![
                    paragraph("• • • • •"),
                    paragraph("\u{f0b7} \u{f0b7}"),
                    paragraph("Texto com glifo Wingdings \u{f0b7} embutido."),
                ],
            )],
        };

        let opt = optimize_for_ai(
            &doc,
            &AiOptimizationOptions {
                clean_decorative_noise: false,
                ..Default::default()
            },
        );
        assert_eq!(
            opt.pages[0].blocks,
            [paragraph("Texto com glifo Wingdings  embutido.")]
        );
    }

    #[test]
    fn splits_inline_bullet_paragraphs_into_list_items() {
        let doc = Document {
            pages: vec![page(
                1,
                vec![paragraph("• Primeiro item • Segundo item • Terceiro item")],
            )],
        };

        let opt = optimize_for_ai(&doc, &AiOptimizationOptions::default());
        let item = |text: &str| Block::ListItem {
            marker: "".into(),
            level: 0,
            text: text.into(),
        };
        assert_eq!(
            opt.pages[0].blocks,
            [
                item("Primeiro item"),
                item("Segundo item"),
                item("Terceiro item")
            ]
        );
    }
}
