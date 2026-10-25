// SPDX-License-Identifier: AGPL-3.0-or-later
use rustpdf::apresentacao::cli::{parse_export, parse_ocr, parse_ocr_searchable, ExportFormat};
use rustpdf::apresentacao::exportacao::cli_progress_message;
use rustpdf::apresentacao::ocr as ocr_texts;
use rustpdf::infra::exportacao::export_to_files;
use rustpdf::{
    create_blank_pdf, parse_checkbox_state, AppResult, ImageEditOutcome, Matrix, Occurrence,
    PageNumber, PdfSession, Selection, SelectionTarget,
};
use std::cell::Cell;
use std::io::IsTerminal;
use std::path::Path;

fn usage() -> &'static str {
    concat!("Uso: rustpdf-cli [--password-stdin] [--allow-unencrypted-output] COMANDO ...\n  rustpdf-cli info ENTRADA.pdf\n  rustpdf-cli new SAIDA.pdf\n  rustpdf-cli rotate ENTRADA.pdf SAIDA.pdf PAGINA\n  rustpdf-cli delete ENTRADA.pdf SAIDA.pdf PAGINA\n  rustpdf-cli append ENTRADA.pdf SAIDA.pdf OUTRO.pdf\n  rustpdf-cli blank-after ENTRADA.pdf SAIDA.pdf PAGINA\n  rustpdf-cli extract ENTRADA.pdf SAIDA.pdf PAGINA\n  rustpdf-cli png ENTRADA.pdf SAIDA.png PAGINA\n  rustpdf-cli text ENTRADA.pdf PAGINA\n  rustpdf-cli export ENTRADA.pdf SAIDA.{md,docx,pptx,odt,odg,csv,xlsx} [--paginas 1-3,7] [--para-ia] [--sem-imagens] [--quebrar-paginas] [--fiel] [--ocr IDIOMA] [--separador ;] [--sem-bom]\n  rustpdf-cli ocr ENTRADA.pdf PAGINAS [IDIOMA]\n  rustpdf-cli ocr-searchable ENTRADA.pdf SAIDA.pdf PAGINAS [IDIOMA] [--refazer]\n  rustpdf-cli fields ENTRADA.pdf\n  rustpdf-cli text-field ENTRADA.pdf SAIDA.pdf PAGINA NOME VALOR X Y LARGURA ALTURA [multiline]\n  rustpdf-cli set-field ENTRADA.pdf SAIDA.pdf NOME NOVO_VALOR\n  rustpdf-cli checkbox ENTRADA.pdf SAIDA.pdf PAGINA NOME SIM_OU_NAO X Y TAMANHO\n  rustpdf-cli set-checkbox ENTRADA.pdf SAIDA.pdf NOME SIM_OU_NAO\n  rustpdf-cli radio ENTRADA.pdf SAIDA.pdf PAGINA NOME SELECAO_OU_- \"op1,op2\" X Y TAMANHO ESPACO\n  rustpdf-cli set-radio ENTRADA.pdf SAIDA.pdf NOME SELECAO\n  rustpdf-cli choice ENTRADA.pdf SAIDA.pdf PAGINA NOME VALOR \"op1,op2\" X Y LARGURA ALTURA [combo|lista]\n  rustpdf-cli set-choice ENTRADA.pdf SAIDA.pdf NOME VALOR\n  rustpdf-cli text-runs ENTRADA.pdf PAGINA\n  rustpdf-cli replace-text ENTRADA.pdf SAIDA.pdf PAGINA ANTIGO NOVO\n  rustpdf-cli images ENTRADA.pdf PAGINA\n  rustpdf-cli image-draws ENTRADA.pdf PAGINA\n  rustpdf-cli replace-image ENTRADA.pdf SAIDA.pdf PAGINA NOME IMAGEM.png [N|todas]\n  rustpdf-cli replace-image-at ENTRADA.pdf SAIDA.pdf PAGINA X Y IMAGEM.png\n  rustpdf-cli move-image ENTRADA.pdf SAIDA.pdf PAGINA NOME DX DY [N|todas]\n  rustpdf-cli remove-image ENTRADA.pdf SAIDA.pdf PAGINA NOME [N|todas]\n  rustpdf-cli remove-image-at ENTRADA.pdf SAIDA.pdf PAGINA X Y\n  rustpdf-cli move-text ENTRADA.pdf SAIDA.pdf PAGINA X Y DX DY\n  rustpdf-cli edit-text-at ENTRADA.pdf SAIDA.pdf PAGINA X Y NOVO_TEXTO\n  rustpdf-cli select ENTRADA.pdf PAGINA X Y\n  rustpdf-cli move-object ENTRADA.pdf SAIDA.pdf PAGINA X Y DX DY\n  rustpdf-cli resize-object ENTRADA.pdf SAIDA.pdf PAGINA X Y FATOR_X FATOR_Y\n  rustpdf-cli rotate-object ENTRADA.pdf SAIDA.pdf PAGINA X Y GRAUS\n  rustpdf-cli transform-object ENTRADA.pdf SAIDA.pdf PAGINA X Y A B C D E F\n  rustpdf-cli note ENTRADA.pdf SAIDA.pdf PAGINA TEXTO\n  rustpdf-cli note-at ENTRADA.pdf SAIDA.pdf PAGINA TEXTO X Y\n  rustpdf-cli highlight-at ENTRADA.pdf SAIDA.pdf PAGINA X Y LARGURA ALTURA\n  rustpdf-cli insert-text ENTRADA.pdf SAIDA.pdf PAGINA TEXTO\n  rustpdf-cli insert-text-at ENTRADA.pdf SAIDA.pdf PAGINA TEXTO X Y TAMANHO_FONTE\n  rustpdf-cli insert-image ENTRADA.pdf SAIDA.pdf PAGINA IMAGEM.png\n  rustpdf-cli insert-image-at ENTRADA.pdf SAIDA.pdf PAGINA IMAGEM.png X Y LARGURA ALTURA\n  rustpdf-cli crop ENTRADA.pdf SAIDA.pdf PAGINA MARGEM_EM_PONTOS\n\nRustPDF ", env!("CARGO_PKG_VERSION"), " — Copyright (C) 2026 Welyqrson Bastos Amaral. Licença AGPL-3.0-or-later; sem garantia.\nPAGINAS: 3, 1-3,7 ou todas. O andamento sai na saída de erro.\nCódigo-fonte: https://github.com/wbaamaral/rustpdf")
}

fn parse_page(value: &str, count: usize) -> AppResult<usize> {
    PageNumber::parse(value, count)
        .map(PageNumber::get)
        .map_err(|error| error.to_string())
}

fn parse_number(value: &str, label: &str) -> AppResult<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("{label} inválido: {value}"))
}

/// Objeto em (`x`, `y`) — pontos visíveis da página — ou erro.
fn select_object(session: &PdfSession, x: &str, y: &str) -> AppResult<(Selection, (f64, f64))> {
    let point = (parse_number(x, "X")?, parse_number(y, "Y")?);
    let selection = session
        .select_at(point)?
        .ok_or_else(|| format!("Nenhum objeto em {} {}", point.0, point.1))?;
    Ok((selection, point))
}

fn format_pair((x, y): (f64, f64)) -> String {
    format!("{x:.2} {y:.2}")
}

/// Descrição da seleção, uma propriedade por linha (`chave<TAB>valor`).
fn describe(selection: &Selection) -> String {
    let frame = selection.frame;
    let kind = match &selection.target {
        SelectionTarget::Annotation { kind, index } => format!("anotacao:{kind}#{index}"),
        SelectionTarget::Image { name } => format!("imagem:{name}"),
        SelectionTarget::Text { .. } => "texto".into(),
    };
    let [x0, y0, x1, y1] = frame.bounds();
    let yes_no = |value: bool| if value { "sim" } else { "nao" };
    [
        format!("tipo\t{kind}"),
        format!("rotulo\t{}", selection.label()),
        format!("origem\t{}", format_pair(frame.origin)),
        format!("eixo_u\t{}", format_pair(frame.u)),
        format!("eixo_v\t{}", format_pair(frame.v)),
        format!("caixa\t{x0:.2} {y0:.2} {x1:.2} {y1:.2}"),
        format!("largura\t{:.2}", frame.u.0.hypot(frame.u.1)),
        format!("altura\t{:.2}", frame.v.0.hypot(frame.v.1)),
        format!("angulo\t{:.2}", frame.u.1.atan2(frame.u.0).to_degrees()),
        format!("redimensionavel\t{}", yes_no(selection.resizable)),
        format!("giravel\t{}", yes_no(selection.rotatable)),
        format!("proporcional\t{}", yes_no(selection.uniform)),
    ]
    .join("\n")
}

/// Aplica `transform` ao objeto selecionado, salva e informa a nova caixa.
fn transform_object(
    session: &mut PdfSession,
    selection: &Selection,
    transform: &Matrix,
    destination: &Path,
    allow_unencrypted: bool,
) -> AppResult<()> {
    let outcome = session.transform_selection(selection, transform)?;
    save_session(session, destination, allow_unencrypted)?;
    let [x0, y0, x1, y1] = selection.transformed(transform).frame.bounds();
    println!("caixa\t{x0:.2} {y0:.2} {x1:.2} {y1:.2}");
    if outcome.whole_block {
        println!("Bloco de texto inteiro transformado (linhas encadeadas)");
    }
    if outcome.cloned_forms > 0 {
        println!(
            "{} Form XObject(s) compartilhado(s) copiado(s)",
            outcome.cloned_forms
        );
    }
    Ok(())
}

/// Ocorrência opcional de imagem: ausente → única; `N` → N-ésima; `todas`.
fn parse_occurrence(value: Option<&String>) -> AppResult<Occurrence> {
    match value.map(String::as_str) {
        None => Ok(Occurrence::Only),
        Some("todas") => Ok(Occurrence::All),
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|n| *n >= 1)
            .map(Occurrence::Index)
            .ok_or_else(|| format!("Ocorrência inválida: {text} (use 1, 2, … ou 'todas')")),
    }
}

fn report_images(outcome: &ImageEditOutcome, action: &str) {
    println!("{} desenho(s) {action}", outcome.draws);
    if outcome.cloned_forms > 0 {
        println!(
            "{} Form XObject(s) compartilhado(s) copiado(s)",
            outcome.cloned_forms
        );
    }
}

fn save_session(
    session: &mut PdfSession,
    destination: &Path,
    allow_unencrypted: bool,
) -> AppResult<()> {
    if allow_unencrypted {
        session.save_as_unencrypted(destination)
    } else {
        session.save_as(destination)
    }
}

fn execute(args: &[String], password: Option<&str>, allow_unencrypted: bool) -> AppResult<()> {
    if args.len() < 3 {
        return Err(usage().into());
    }
    let operation = args[1].as_str();
    let input = Path::new(&args[2]);
    if operation == "info" && args.len() == 3 {
        let session = if let Some(password) = password {
            PdfSession::open_with_password(input, password)?
        } else {
            PdfSession::open(input)?
        };
        println!("Páginas: {}", session.pages);
        return Ok(());
    }
    if operation == "new" && args.len() == 3 {
        return create_blank_pdf(input);
    }
    if operation == "export" {
        return export(&args[2..], password);
    }
    if operation == "ocr" {
        return ocr(&args[2..], password);
    }
    if operation == "ocr-searchable" {
        return ocr_searchable(&args[2..], password, allow_unencrypted);
    }
    let mut session = if let Some(password) = password {
        PdfSession::open_with_password(input, password)?
    } else {
        PdfSession::open(input)?
    };
    match operation {
        "fields" if args.len() == 3 => {
            for (name, kind, value) in session.list_form_fields()? {
                println!("{name}\t{kind}\t{value}");
            }
        }
        "rotate" | "delete" | "blank-after" | "extract" | "png" | "note" | "insert-text"
        | "insert-image" | "crop" => {
            let expected = if operation == "note"
                || operation == "insert-text"
                || operation == "insert-image"
                || operation == "crop"
            {
                6
            } else {
                5
            };
            if args.len() != expected {
                return Err(usage().into());
            }
            session.page = parse_page(&args[4], session.pages)?;
            let destination = Path::new(&args[3]);
            match operation {
                "rotate" => session.rotate_current()?,
                "delete" => session.delete_current()?,
                "blank-after" => session.insert_blank_after()?,
                "extract" => {
                    return if allow_unencrypted {
                        session.extract_current_unencrypted(destination)
                    } else {
                        session.extract_current(destination)
                    };
                }
                "png" => return session.export_png(destination),
                "note" => session.add_text_note(&args[5])?,
                "insert-text" => session.insert_visible_text(&args[5])?,
                "insert-image" => session.insert_image(Path::new(&args[5]))?,
                "crop" => {
                    let margin: f64 = args[5].parse().map_err(|_| "Margem inválida")?;
                    session.crop_current_inset(margin)?;
                }
                _ => unreachable!(),
            }
            save_session(&mut session, destination, allow_unencrypted)?;
        }
        "append" if args.len() == 5 => {
            session.append_pdf(Path::new(&args[4]))?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "text-field" if args.len() == 11 || args.len() == 12 => {
            session.page = parse_page(&args[4], session.pages)?;
            let coordinates = args[7..=10]
                .iter()
                .map(|value| {
                    value
                        .parse::<f64>()
                        .map_err(|_| "Coordenada inválida".to_string())
                })
                .collect::<AppResult<Vec<_>>>()?;
            let multiline = matches!(
                args.get(11).map(String::as_str),
                Some("multiline") | Some("multi")
            );
            session.insert_text_field(
                &args[5],
                &args[6],
                coordinates[0],
                coordinates[1],
                coordinates[2],
                coordinates[3],
                multiline,
            )?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "insert-text-at" if args.len() == 9 => {
            session.page = parse_page(&args[4], session.pages)?;
            let x = args[6].parse::<f64>().map_err(|_| "X inválido")?;
            let y = args[7].parse::<f64>().map_err(|_| "Y inválido")?;
            let size = args[8].parse::<f64>().map_err(|_| "Tamanho inválido")?;
            session.insert_visible_text_at(&args[5], x, y, size)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "note-at" if args.len() == 8 => {
            session.page = parse_page(&args[4], session.pages)?;
            let x = args[6].parse::<f64>().map_err(|_| "X inválido")?;
            let y = args[7].parse::<f64>().map_err(|_| "Y inválido")?;
            session.add_text_note_at(&args[5], x, y)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "highlight-at" if args.len() == 9 => {
            session.page = parse_page(&args[4], session.pages)?;
            let coordinates = args[5..=8]
                .iter()
                .map(|value| {
                    value
                        .parse::<f64>()
                        .map_err(|_| "Coordenada inválida".to_string())
                })
                .collect::<AppResult<Vec<_>>>()?;
            session.highlight_at(
                coordinates[0],
                coordinates[1],
                coordinates[2],
                coordinates[3],
            )?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "insert-image-at" if args.len() == 10 => {
            session.page = parse_page(&args[4], session.pages)?;
            let x = args[6].parse::<f64>().map_err(|_| "X inválido")?;
            let y = args[7].parse::<f64>().map_err(|_| "Y inválido")?;
            let width = args[8].parse::<f64>().map_err(|_| "Largura inválida")?;
            let height = args[9].parse::<f64>().map_err(|_| "Altura inválida")?;
            session.insert_image_at(Path::new(&args[5]), x, y, width, height)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "set-field" if args.len() == 6 => {
            session.set_text_field_value(&args[4], &args[5])?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "radio" if args.len() == 12 => {
            session.page = parse_page(&args[4], session.pages)?;
            let labels: Vec<String> = args[7].split(',').map(str::to_string).collect();
            if labels.len() < 2 {
                return Err("Informe ao menos duas opções separadas por vírgula".into());
            }
            let x = args[8].parse::<f64>().map_err(|_| "X inválido")?;
            let mut y = args[9].parse::<f64>().map_err(|_| "Y inválido")?;
            let size = args[10].parse::<f64>().map_err(|_| "Tamanho inválido")?;
            let step = args[11].parse::<f64>().map_err(|_| "Espaço inválido")?;
            let selected = if args[6].is_empty() || args[6] == "-" {
                None
            } else {
                Some(
                    labels
                        .iter()
                        .position(|label| *label == args[6])
                        .ok_or("Opção selecionada não está na lista")?,
                )
            };
            let mut options = Vec::new();
            for label in labels {
                options.push((label, x, y, size, size));
                y -= step;
            }
            session.insert_radio_group(&args[5], &options, selected)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "set-radio" if args.len() == 6 => {
            session.set_radio_value(&args[4], &args[5])?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "choice" if args.len() == 12 || args.len() == 13 => {
            session.page = parse_page(&args[4], session.pages)?;
            let options: Vec<String> = args[7].split(',').map(str::to_string).collect();
            if options.is_empty() {
                return Err("Informe ao menos uma opção".into());
            }
            let value = args[6].clone();
            if !value.is_empty() && !options.contains(&value) {
                return Err("O valor inicial precisa estar entre as opções".into());
            }
            let x = args[8].parse::<f64>().map_err(|_| "X inválido")?;
            let y = args[9].parse::<f64>().map_err(|_| "Y inválido")?;
            let width = args[10].parse::<f64>().map_err(|_| "Largura inválida")?;
            let height = args[11].parse::<f64>().map_err(|_| "Altura inválida")?;
            let combo = match args.get(12).map(String::as_str) {
                None | Some("combo") => true,
                Some("lista") | Some("list") => false,
                Some(_) => return Err("Informe combo ou lista".into()),
            };
            session.insert_choice_field(&args[5], &value, &options, x, y, width, height, combo)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "text-runs" if args.len() == 4 => {
            session.page = parse_page(&args[3], session.pages)?;
            for text in session.list_text_runs()? {
                println!("{text}");
            }
        }
        "replace-text" if args.len() == 7 => {
            session.page = parse_page(&args[4], session.pages)?;
            let replaced = session.replace_text(&args[5], &args[6])?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
            println!("{replaced} substituições");
        }
        "images" if args.len() == 4 => {
            session.page = parse_page(&args[3], session.pages)?;
            for image in session.list_images()? {
                println!(
                    "{}\t{}x{}\t{}\tobj {}",
                    image.name, image.width, image.height, image.filter, image.object_id
                );
            }
        }
        "image-draws" if args.len() == 4 => {
            session.page = parse_page(&args[3], session.pages)?;
            for draw in session.image_placements()? {
                let [x0, y0, x1, y1] = draw.frame.bounds();
                let angle = draw.frame.u.1.atan2(draw.frame.u.0).to_degrees();
                println!(
                    "{}\t{}\t{x0:.2} {y0:.2} {x1:.2} {y1:.2}\t{angle:.2}",
                    draw.name, draw.occurrence
                );
            }
        }
        "replace-image" if (7..=8).contains(&args.len()) => {
            session.page = parse_page(&args[4], session.pages)?;
            let occurrence = parse_occurrence(args.get(7))?;
            let outcome = session.replace_image(&args[5], Path::new(&args[6]), occurrence)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
            report_images(&outcome, "substituído(s)");
        }
        "move-image" if (8..=9).contains(&args.len()) => {
            session.page = parse_page(&args[4], session.pages)?;
            let dx = parse_number(&args[6], "DX")?;
            let dy = parse_number(&args[7], "DY")?;
            let occurrence = parse_occurrence(args.get(8))?;
            let outcome = session.move_image(&args[5], dx, dy, occurrence)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
            report_images(&outcome, "movido(s)");
        }
        "replace-image-at" if args.len() == 8 => {
            session.page = parse_page(&args[4], session.pages)?;
            let point = (parse_number(&args[5], "X")?, parse_number(&args[6], "Y")?);
            let outcome = session.replace_image_at(point, Path::new(&args[7]))?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
            report_images(&outcome, "substituído(s)");
        }
        "remove-image-at" if args.len() == 7 => {
            session.page = parse_page(&args[4], session.pages)?;
            let point = (parse_number(&args[5], "X")?, parse_number(&args[6], "Y")?);
            let outcome = session.remove_image_at(point)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
            report_images(&outcome, "removido(s)");
        }
        "move-text" if args.len() == 9 => {
            session.page = parse_page(&args[4], session.pages)?;
            let number = |index: usize, label: &str| {
                args[index]
                    .parse::<f64>()
                    .map_err(|_| format!("{label} inválido"))
            };
            let (x, y) = (number(5, "X")?, number(6, "Y")?);
            let (dx, dy) = (number(7, "DX")?, number(8, "DY")?);
            let outcome = session.move_text((x, y), (x + dx, y + dy), "")?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
            if outcome.whole_block {
                println!("Bloco de texto inteiro movido (linhas encadeadas)");
            }
            if outcome.cloned_forms > 0 {
                println!(
                    "{} Form XObject(s) compartilhado(s) copiado(s)",
                    outcome.cloned_forms
                );
            }
        }
        "edit-text-at" if args.len() == 8 => {
            session.page = parse_page(&args[4], session.pages)?;
            let x = args[5].parse::<f64>().map_err(|_| "X inválido")?;
            let y = args[6].parse::<f64>().map_err(|_| "Y inválido")?;
            let outcome = session.replace_text_at((x, y), "", &args[7])?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
            println!("Editado: {}", outcome.text);
        }
        "select" if args.len() == 6 => {
            session.page = parse_page(&args[3], session.pages)?;
            let (selection, _) = select_object(&session, &args[4], &args[5])?;
            println!("{}", describe(&selection));
        }
        "move-object" | "resize-object" | "rotate-object" | "transform-object" => {
            let expected = match operation {
                "rotate-object" => 8,
                "transform-object" => 13,
                _ => 9,
            };
            if args.len() != expected {
                return Err(usage().into());
            }
            session.page = parse_page(&args[4], session.pages)?;
            let (selection, _) = select_object(&session, &args[5], &args[6])?;
            let number = |index: usize, label: &str| parse_number(&args[index], label);
            let transform = match operation {
                "move-object" => {
                    let (dx, dy) = (number(7, "DX")?, number(8, "DY")?);
                    [1.0, 0.0, 0.0, 1.0, dx, dy]
                }
                "resize-object" => {
                    let (sx, sy) = (number(7, "FATOR_X")?, number(8, "FATOR_Y")?);
                    if sx <= 0.0 || sy <= 0.0 {
                        return Err("Fatores de escala devem ser positivos".into());
                    }
                    selection.scaling(sx, sy)
                }
                "rotate-object" => selection.rotation(number(7, "GRAUS")?),
                _ => {
                    let mut matrix = [0.0; 6];
                    for (offset, slot) in matrix.iter_mut().enumerate() {
                        *slot = number(7 + offset, "Matriz")?;
                    }
                    matrix
                }
            };
            transform_object(
                &mut session,
                &selection,
                &transform,
                Path::new(&args[3]),
                allow_unencrypted,
            )?;
        }
        "remove-image" if (6..=7).contains(&args.len()) => {
            session.page = parse_page(&args[4], session.pages)?;
            let occurrence = parse_occurrence(args.get(6))?;
            let outcome = session.remove_image(&args[5], occurrence)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
            report_images(&outcome, "removido(s)");
        }
        "set-choice" if args.len() == 6 => {
            session.set_choice_value(&args[4], &args[5])?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "checkbox" if args.len() == 10 => {
            session.page = parse_page(&args[4], session.pages)?;
            let checked = parse_checkbox_state(&args[6])?;
            let coordinates = args[7..=9]
                .iter()
                .map(|value| {
                    value
                        .parse::<f64>()
                        .map_err(|_| "Coordenada inválida".to_string())
                })
                .collect::<AppResult<Vec<_>>>()?;
            session.insert_checkbox(
                &args[5],
                checked,
                coordinates[0],
                coordinates[1],
                coordinates[2],
            )?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "set-checkbox" if args.len() == 6 => {
            session.set_checkbox_value(&args[4], parse_checkbox_state(&args[5])?)?;
            save_session(&mut session, Path::new(&args[3]), allow_unencrypted)?;
        }
        "text" if args.len() == 4 => {
            session.page = parse_page(&args[3], session.pages)?;
            print!("{}", session.extract_text()?);
        }
        _ => return Err(usage().into()),
    }
    Ok(())
}

/// `export ENTRADA SAIDA [opções]`: estrutura do texto → arquivo(s).
fn export(args: &[String], password: Option<&str>) -> AppResult<()> {
    let command = parse_export(args)?;
    let session = match password {
        Some(password) => PdfSession::open_with_password(&command.input, password)?,
        None => PdfSession::open(&command.input)?,
    };
    let progress = StderrProgress::new();
    let format = command.request.format;
    let written = export_to_files(
        session.work_path(),
        session.pages,
        &command.request,
        &|done, total| {
            progress.show(&cli_progress_message(format, done, total));
            true
        },
    )?;
    progress.finish();
    if command.request.format == ExportFormat::Csv {
        for path in written {
            println!("{}", path.display());
        }
    }
    Ok(())
}

/// Andamento na saída de erro: no terminal, uma linha reescrita; em arquivo
/// ou pipe, uma linha por passo. A saída padrão fica só com o resultado.
struct StderrProgress {
    terminal: bool,
    open: Cell<bool>,
}

impl StderrProgress {
    fn new() -> Self {
        Self {
            terminal: std::io::stderr().is_terminal(),
            open: Cell::new(false),
        }
    }

    fn show(&self, message: &str) {
        if self.terminal {
            eprint!("\r{message}\x1b[K");
            self.open.set(true);
        } else {
            eprintln!("{message}");
        }
    }

    /// Termina a linha reescrita (sem efeito fora do terminal).
    fn finish(&self) {
        if self.open.replace(false) {
            eprintln!();
        }
    }
}

impl Drop for StderrProgress {
    fn drop(&mut self) {
        self.finish();
    }
}

fn open_session(input: &Path, password: Option<&str>) -> AppResult<PdfSession> {
    match password {
        Some(password) => PdfSession::open_with_password(input, password),
        None => PdfSession::open(input),
    }
}

/// `ocr ENTRADA PAGINAS [IDIOMA]`: texto de cada página, separado por
/// quebra de página (form feed) como no pdftotext.
fn ocr(args: &[String], password: Option<&str>) -> AppResult<()> {
    let command = parse_ocr(args)?;
    let session = open_session(&command.input, password)?;
    let pages = ocr_texts::pages(&command.pages, session.pages)?;
    let progress = StderrProgress::new();
    let texts = session.ocr_text(&pages, command.language.as_deref(), &|done, total| {
        if done < total {
            progress.show(&ocr_texts::progress_message(done, total));
        }
        true
    })?;
    progress.finish();
    print!("{}", texts.join("\x0c"));
    Ok(())
}

/// `ocr-searchable ENTRADA SAIDA PAGINAS [IDIOMA] [--refazer]`.
fn ocr_searchable(
    args: &[String],
    password: Option<&str>,
    allow_unencrypted: bool,
) -> AppResult<()> {
    let command = parse_ocr_searchable(args)?;
    let mut session = open_session(&command.input, password)?;
    let pages = ocr_texts::pages(&command.pages, session.pages)?;
    let progress = StderrProgress::new();
    let summary =
        session.ocr_searchable(&pages, &command.language, command.redo, &|done, total| {
            progress.show(&ocr_texts::progress_message(done, total));
            true
        })?;
    progress.finish();
    save_session(&mut session, &command.output, allow_unencrypted)?;
    println!("{}", ocr_texts::summary_message(&summary));
    Ok(())
}

fn main() {
    rustpdf::prefer_bundled_tools();
    let mut args: Vec<String> = std::env::args().collect();
    let mut password = None;
    let mut allow_unencrypted = false;
    loop {
        match args.get(1).map(String::as_str) {
            Some("--password-stdin") => {
                let mut line = String::new();
                match std::io::stdin().read_line(&mut line) {
                    Ok(0) | Err(_) => {
                        eprintln!("Não foi possível ler a senha da entrada padrão");
                        std::process::exit(1);
                    }
                    Ok(_) => password = Some(line.trim_end_matches(['\r', '\n']).to_string()),
                }
                args.remove(1);
            }
            Some("--allow-unencrypted-output") => {
                allow_unencrypted = true;
                args.remove(1);
            }
            _ => break,
        }
    }
    if let Err(error) = execute(&args, password.as_deref(), allow_unencrypted) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
