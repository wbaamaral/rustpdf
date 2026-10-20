// SPDX-License-Identifier: AGPL-3.0-or-later
//! Testes da CLI executando o binário `rustpdf-cli` em PDFs reais.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustpdf::testkit::{PdfBuilder, TempDir, PNG_2X2 as PNG};

/// Executa a CLI; devolve (sucesso, stdout, stderr).
fn cli(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_rustpdf-cli"))
        .args(args)
        .output()
        .expect("executar rustpdf-cli");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn ok(args: &[&str]) -> String {
    let (success, stdout, stderr) = cli(args);
    assert!(success, "{args:?} falhou: {stderr}");
    stdout
}

/// Valor de uma propriedade (`chave<TAB>valor`) da saída de `select`.
fn property(output: &str, key: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}\t")))
        .unwrap_or_else(|| panic!("sem {key} em {output}"))
        .to_string()
}

fn numbers(text: &str) -> Vec<f64> {
    text.split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect()
}

fn assert_close(got: &[f64], want: &[f64], tolerance: f64) {
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() <= tolerance, "{got:?} != {want:?}");
    }
}

fn workspace(name: &str) -> TempDir {
    TempDir::new(&format!("cli-{name}"))
}

fn path(root: &Path, name: &str) -> String {
    root.join(name).to_string_lossy().into_owned()
}

#[test]
fn selects_moves_resizes_and_rotates_objects() {
    let dir = workspace("objects");
    let root = dir.path();
    let (blank, image_pdf, text_pdf, base) = (
        path(root, "blank.pdf"),
        path(root, "image.pdf"),
        path(root, "text.pdf"),
        path(root, "base.pdf"),
    );
    let png = path(root, "pixels.png");
    fs::write(&png, PNG).unwrap();
    ok(&["new", &blank]);
    ok(&[
        "insert-image-at",
        &blank,
        &image_pdf,
        "1",
        &png,
        "100",
        "500",
        "150",
        "120",
    ]);
    ok(&[
        "insert-text-at",
        &image_pdf,
        &text_pdf,
        "1",
        "Girar",
        "100",
        "300",
        "20",
    ]);
    ok(&["note-at", &text_pdf, &base, "1", "Nota", "400", "700"]);

    let image = ok(&["select", &base, "1", "175", "560"]);
    assert_eq!(property(&image, "tipo"), "imagem:Im1");
    assert_close(
        &numbers(&property(&image, "caixa")),
        &[100.0, 500.0, 250.0, 620.0],
        0.5,
    );
    assert_eq!(property(&image, "giravel"), "sim");

    // Redimensionar em torno do centro (175, 560) e girar 90°.
    let resized = path(root, "resized.pdf");
    ok(&[
        "resize-object",
        &base,
        &resized,
        "1",
        "175",
        "560",
        "2",
        "0.5",
    ]);
    let after = ok(&["select", &resized, "1", "175", "560"]);
    assert_close(
        &numbers(&property(&after, "caixa")),
        &[25.0, 530.0, 325.0, 590.0],
        0.5,
    );
    let rotated = path(root, "rotated.pdf");
    ok(&["rotate-object", &resized, &rotated, "1", "175", "560", "90"]);
    let after = ok(&["select", &rotated, "1", "175", "560"]);
    assert_close(
        &numbers(&property(&after, "caixa")),
        &[145.0, 410.0, 205.0, 710.0],
        0.5,
    );
    assert_close(&numbers(&property(&after, "angulo")), &[90.0], 0.1);

    // Texto: mover e girar; escala desproporcional é recusada.
    let text = ok(&["select", &rotated, "1", "115", "306"]);
    assert_eq!(property(&text, "tipo"), "texto");
    assert_eq!(property(&text, "proporcional"), "sim");
    let (success, _, stderr) = cli(&[
        "resize-object",
        &rotated,
        &path(root, "x.pdf"),
        "1",
        "115",
        "306",
        "2",
        "1",
    ]);
    assert!(!success && stderr.contains("proporcionalmente"), "{stderr}");
    let moved = path(root, "moved.pdf");
    ok(&[
        "move-object",
        &rotated,
        &moved,
        "1",
        "115",
        "306",
        "0",
        "-100",
    ]);
    let text = ok(&["select", &moved, "1", "115", "206"]);
    assert_eq!(property(&text, "rotulo"), "Texto “Girar”");
    let turned = path(root, "turned.pdf");
    ok(&["rotate-object", &moved, &turned, "1", "115", "206", "30"]);
    let center = numbers(&property(&text, "caixa"));
    let (cx, cy) = ((center[0] + center[2]) / 2.0, (center[1] + center[3]) / 2.0);
    let text = ok(&["select", &turned, "1", &cx.to_string(), &cy.to_string()]);
    assert_close(&numbers(&property(&text, "angulo")), &[30.0], 0.5);

    // Nota: só move.
    let (success, _, stderr) = cli(&[
        "rotate-object",
        &turned,
        &path(root, "x.pdf"),
        "1",
        "410",
        "710",
        "45",
    ]);
    assert!(
        !success && stderr.contains("não pode ser girado"),
        "{stderr}"
    );
    let final_pdf = path(root, "final.pdf");
    ok(&[
        "move-object",
        &turned,
        &final_pdf,
        "1",
        "410",
        "710",
        "-50",
        "0",
    ]);
    let note = ok(&["select", &final_pdf, "1", "360", "710"]);
    assert!(property(&note, "tipo").starts_with("anotacao:nota"));

    // Matriz genérica e saída válida.
    let matrix = path(root, "matrix.pdf");
    ok(&[
        "transform-object",
        &final_pdf,
        &matrix,
        "1",
        "175",
        "560",
        "1",
        "0",
        "0",
        "1",
        "10",
        "0",
    ]);
    let check = Command::new("qpdf")
        .args(["--check", &matrix])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );

    let (success, _, stderr) = cli(&["select", &final_pdf, "1", "500", "100"]);
    assert!(!success && stderr.contains("Nenhum objeto"), "{stderr}");
}

#[test]
fn image_commands_act_on_chosen_occurrences() {
    let dir = workspace("image-occurrences");
    let root = dir.path();
    let input = path(root, "shared.pdf");
    let mut pdf = PdfBuilder::new();
    let image = pdf.image_rgb(1, 1, b"abc");
    let resources = format!("/XObject << /Im1 {image} >>");
    pdf.page(
        595.0,
        842.0,
        &resources,
        "q 100 0 0 50 10 20 cm /Im1 Do Q q 100 0 0 50 10 200 cm /Im1 Do Q",
    );
    pdf.page(595.0, 842.0, &resources, "q 100 0 0 50 10 20 cm /Im1 Do Q");
    pdf.write(Path::new(&input));
    let png = path(root, "pixels.png");
    fs::write(&png, PNG).unwrap();
    let draws = |pdf: &str, page: &str| -> Vec<String> {
        ok(&["image-draws", pdf, page])
            .lines()
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        draws(&input, "1"),
        [
            "Im1\t1\t10.00 20.00 110.00 70.00\t0.00",
            "Im1\t2\t10.00 200.00 110.00 250.00\t0.00"
        ]
    );

    // Sem ocorrência: recusa e orienta; com ocorrência: só ela muda.
    let moved = path(root, "moved.pdf");
    let (success, _, stderr) = cli(&["move-image", &input, &moved, "1", "Im1", "5", "0"]);
    assert!(
        !success && stderr.contains("informe a ocorrência"),
        "{stderr}"
    );
    ok(&["move-image", &input, &moved, "1", "Im1", "5", "0", "2"]);
    assert_eq!(
        draws(&moved, "1")[1],
        "Im1\t2\t15.00 200.00 115.00 250.00\t0.00"
    );
    assert!(draws(&moved, "1")[0].starts_with("Im1\t1\t10.00 20.00"));

    // Trocar pelo ponto: só a 1ª ocorrência aponta para a imagem nova.
    let replaced = path(root, "replaced.pdf");
    ok(&["replace-image-at", &moved, &replaced, "1", "60", "45", &png]);
    let now = draws(&replaced, "1");
    assert!(
        now[0].starts_with("Im1_c1\t1\t") && now[1].starts_with("Im1\t1\t"),
        "{now:?}"
    );
    assert_eq!(
        draws(&replaced, "2"),
        ["Im1\t1\t10.00 20.00 110.00 70.00\t0.00"]
    );

    // Remover pelo ponto e depois todas as ocorrências de um nome.
    let removed = path(root, "removed.pdf");
    ok(&["remove-image-at", &replaced, &removed, "1", "60", "225"]);
    assert_eq!(draws(&removed, "1").len(), 1);
    let empty = path(root, "empty.pdf");
    ok(&["remove-image", &removed, &empty, "1", "Im1_c1", "todas"]);
    assert!(draws(&empty, "1").is_empty());
    assert_eq!(draws(&empty, "2").len(), 1, "a página 2 mantém a imagem");
    let check = Command::new("qpdf")
        .args(["--check", &empty])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
}

#[test]
fn export_markdown_keeps_headings_paragraphs_and_lists() {
    let dir = workspace("export-md");
    let root = dir.path();
    let input = path(root, "relatorio.pdf");
    let mut pdf = PdfBuilder::new();
    pdf.text_page(&[
        (72.0, 760.0, 28.0, "Relatorio anual"),
        (72.0, 700.0, 12.0, "O ano teve bons resultados."),
        (72.0, 660.0, 12.0, "* Receita maior"),
        (72.0, 645.0, 12.0, "* Custos menores"),
    ]);
    pdf.text_page(&[(72.0, 760.0, 12.0, "Segunda pagina.")]);
    pdf.write(Path::new(&input));

    let output = path(root, "relatorio.md");
    ok(&["export", &input, &output]);
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "# Relatorio anual\n\nO ano teve bons resultados.\n\n- Receita maior\n- Custos menores\n\nSegunda pagina.\n"
    );

    let second = path(root, "segunda.md");
    ok(&["export", &input, &second, "--paginas", "2"]);
    assert_eq!(fs::read_to_string(&second).unwrap(), "Segunda pagina.\n");

    let (success, _, stderr) = cli(&["export", &input, &path(root, "x.odt")]);
    assert!(!success && stderr.contains("não suportado"), "{stderr}");
    let (success, _, stderr) = cli(&["export", &input, &output, "--paginas", "9"]);
    assert!(
        !success && stderr.contains("Página fora de 1..=2"),
        "{stderr}"
    );
}

#[test]
fn export_markdown_with_ai_optimization() {
    let dir = workspace("export-md-ai");
    let root = dir.path();
    let input = path(root, "documento.pdf");
    let mut pdf = PdfBuilder::new();
    pdf.text_page(&[
        (72.0, 780.0, 12.0, "Manual do Sistema"),
        (72.0, 700.0, 12.0, "O processo garante alta disponi-"),
        (72.0, 50.0, 12.0, "Pagina 1 de 2"),
    ]);
    pdf.text_page(&[
        (72.0, 780.0, 12.0, "Manual do Sistema"),
        (72.0, 700.0, 12.0, "bilidade e baixa latencia."),
        (72.0, 50.0, 12.0, "Pagina 2 de 2"),
    ]);
    pdf.write(Path::new(&input));

    let output = path(root, "otimizado.md");
    ok(&["export", &input, &output, "--para-ia"]);

    let content = fs::read_to_string(&output).unwrap();
    // Cabeçalho da primeira página é mantido (título), mas não é repetido na segunda
    assert!(content.contains("Manual do Sistema"));
    assert_eq!(
        content.matches("Manual do Sistema").count(),
        1,
        "cabeçalho repetido não deve aparecer na segunda página"
    );
    // Números de página devem ter sido removidos
    assert!(!content.contains("Pagina 1 de 2"));
    assert!(!content.contains("Pagina 2 de 2"));
    // Parágrafo partido por hífen deve ser unificado
    assert!(content.contains("O processo garante alta disponibilidade e baixa latencia."));
    // Nenhuma pasta de imagens foi criada
    assert!(!Path::new(&path(root, "otimizado_arquivos")).exists());
}

#[test]
fn export_csv_writes_each_table() {
    let dir = workspace("export-csv");
    let root = dir.path();
    let input = path(root, "tabela.pdf");
    let mut pdf = PdfBuilder::new();
    pdf.text_page(&[
        (72.0, 760.0, 12.0, "Estoque do mes"),
        (72.0, 700.0, 12.0, "Produto"),
        (250.0, 700.0, 12.0, "Qtd"),
        (72.0, 684.0, 12.0, "Caneta azul"),
        (250.0, 684.0, 12.0, "10"),
        (72.0, 668.0, 12.0, "Lapis"),
        (250.0, 668.0, 12.0, "3"),
    ]);
    pdf.text_page(&[(72.0, 760.0, 12.0, "Sem tabela aqui.")]);
    pdf.write(Path::new(&input));

    let output = path(root, "estoque.csv");
    ok(&["export", &input, &output, "--paginas", "1"]);
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "\u{feff}Produto;Qtd\r\nCaneta azul;10\r\nLapis;3\r\n"
    );
    let comma = path(root, "virgula.csv");
    ok(&["export", &input, &comma, "--separador", ",", "--sem-bom"]);
    assert_eq!(
        fs::read_to_string(&comma).unwrap(),
        "Produto,Qtd\r\nCaneta azul,10\r\nLapis,3\r\n"
    );
    let markdown = path(root, "estoque.md");
    ok(&["export", &input, &markdown, "--paginas", "1"]);
    assert!(fs::read_to_string(&markdown)
        .unwrap()
        .contains("| Produto     | Qtd |\n| ----------- | --- |\n| Caneta azul | 10  |"));

    let (success, _, stderr) = cli(&["export", &input, &path(root, "nada.csv"), "--paginas", "2"]);
    assert!(!success && stderr.contains("Nenhuma tabela"), "{stderr}");
}

#[test]
fn export_xlsx_opens_in_libreoffice() {
    let dir = workspace("export-xlsx");
    let root = dir.path();
    let input = path(root, "tabela.pdf");
    let mut pdf = PdfBuilder::new();
    pdf.text_page(&[
        (72.0, 700.0, 12.0, "Produto"),
        (250.0, 700.0, 12.0, "Valor"),
        (72.0, 684.0, 12.0, "Caneta azul"),
        (250.0, 684.0, 12.0, "1.234,50"),
        (72.0, 668.0, 12.0, "Lapis"),
        (250.0, 668.0, 12.0, "3"),
    ]);
    pdf.write(Path::new(&input));
    let output = path(root, "tabela.xlsx");
    ok(&["export", &input, &output]);
    assert!(fs::read(&output).unwrap().starts_with(b"PK\x03\x04"));

    // Abre de fato num programa de planilhas, quando houver um.
    let profile = format!(
        "-env:UserInstallation=file://{}",
        root.join("perfil").display()
    );
    let Ok(converted) = Command::new("soffice")
        .args([&profile, "--headless", "--convert-to", "csv", "--outdir"])
        .arg(root)
        .arg(&output)
        .output()
    else {
        return; // sem LibreOffice neste sistema
    };
    assert!(
        converted.status.success(),
        "{}",
        String::from_utf8_lossy(&converted.stderr)
    );
    let csv = fs::read_to_string(root.join("tabela.csv")).unwrap();
    // O valor chega como número (formatado pela localidade), não como o
    // texto "1.234,50" do PDF.
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "Produto,Valor");
    assert!(
        ["Caneta azul,1234.5", "Caneta azul,\"1234,5\""].contains(&lines[1]),
        "{csv}"
    );
    assert_eq!(lines[2], "Lapis,3");
}

#[test]
fn export_docx_opens_in_libreoffice() {
    let dir = workspace("export-docx");
    let root = dir.path();
    let input = path(root, "relatorio.pdf");
    let mut pdf = PdfBuilder::new();
    pdf.text_page(&[
        (72.0, 760.0, 28.0, "Relatorio anual"),
        (72.0, 700.0, 12.0, "O ano teve bons resultados."),
        (72.0, 660.0, 12.0, "* Receita maior"),
        (72.0, 620.0, 12.0, "Produto"),
        (250.0, 620.0, 12.0, "Qtd"),
        (72.0, 604.0, 12.0, "Caneta"),
        (250.0, 604.0, 12.0, "10"),
    ]);
    pdf.write(Path::new(&input));
    let output = path(root, "relatorio.docx");
    ok(&["export", &input, &output]);
    assert!(fs::read(&output).unwrap().starts_with(b"PK\x03\x04"));

    let profile = format!(
        "-env:UserInstallation=file://{}",
        root.join("perfil").display()
    );
    let Ok(converted) = Command::new("soffice")
        .args([
            &profile,
            "--headless",
            "--convert-to",
            "txt:Text (encoded):UTF8",
            "--outdir",
        ])
        .arg(root)
        .arg(&output)
        .output()
    else {
        return; // sem LibreOffice neste sistema
    };
    assert!(
        converted.status.success(),
        "{}",
        String::from_utf8_lossy(&converted.stderr)
    );
    let text = fs::read_to_string(root.join("relatorio.txt")).unwrap();
    let order = [
        "Relatorio anual",
        "O ano teve bons resultados.",
        "Receita maior",
        "Produto",
        "Qtd",
        "Caneta",
        "10",
    ];
    let mut from = 0;
    for expected in order {
        let at = text[from..]
            .find(expected)
            .unwrap_or_else(|| panic!("{expected:?} fora de ordem em:\n{text}"));
        from += at + expected.len();
    }
}

#[test]
fn export_includes_images_in_markdown_and_word() {
    let dir = workspace("export-imagens");
    let root = dir.path();
    let input = path(root, "figura.pdf");
    let mut pdf = PdfBuilder::new();
    let font = pdf.helvetica();
    let image = pdf.image_rgb(2, 2, &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]);
    pdf.page(
        595.0,
        842.0,
        &format!("/Font << /F1 {font} >> /XObject << /Im {image} >>"),
        "BT /F1 12 Tf 1 0 0 1 72 760 Tm (Legenda acima.) Tj ET q 200 0 0 100 72 600 cm /Im Do Q",
    );
    pdf.write(Path::new(&input));

    let markdown = path(root, "figura.md");
    ok(&["export", &input, &markdown]);
    assert_eq!(
        fs::read_to_string(&markdown).unwrap(),
        "Legenda acima.\n\n![Imagem da página 1](figura_arquivos/p1-i1.png)\n"
    );
    let png = fs::read(root.join("figura_arquivos/p1-i1.png")).unwrap();
    // 200 × 100 pt a 150 dpi.
    assert_eq!(rustpdf::infra::png::dimensions(&png), Some((417, 208)));

    let plain = path(root, "sem.md");
    ok(&["export", &input, &plain, "--sem-imagens"]);
    assert_eq!(fs::read_to_string(&plain).unwrap(), "Legenda acima.\n");

    let word = path(root, "figura.docx");
    ok(&["export", &input, &word]);
    let profile = format!(
        "-env:UserInstallation=file://{}",
        root.join("perfil").display()
    );
    let Ok(converted) = Command::new("soffice")
        .args([&profile, "--headless", "--convert-to", "pdf", "--outdir"])
        .arg(root.join("impresso"))
        .arg(&word)
        .output()
    else {
        return; // sem LibreOffice neste sistema
    };
    assert!(converted.status.success());
    let printed = root.join("impresso/figura.pdf");
    let list = Command::new("pdfimages")
        .arg("-list")
        .arg(&printed)
        .output()
        .unwrap();
    let images = String::from_utf8_lossy(&list.stdout)
        .lines()
        .count()
        .saturating_sub(2);
    assert_eq!(images, 1, "{}", String::from_utf8_lossy(&list.stdout));
}

#[test]
fn export_pptx_editable_and_faithful() {
    let dir = workspace("export-pptx");
    let root = dir.path();
    let input = path(root, "slides.pdf");
    let mut pdf = PdfBuilder::new();
    pdf.text_page(&[
        (72.0, 760.0, 28.0, "Primeiro slide"),
        (72.0, 700.0, 12.0, "Texto do corpo."),
    ]);
    pdf.text_page(&[(72.0, 760.0, 28.0, "Segundo slide")]);
    pdf.write(Path::new(&input));

    let editable = path(root, "editavel.pptx");
    ok(&["export", &input, &editable]);
    let faithful = path(root, "fiel.pptx");
    ok(&["export", &input, &faithful, "--fiel"]);
    assert!(fs::read(&editable).unwrap().starts_with(b"PK\x03\x04"));

    let profile = format!(
        "-env:UserInstallation=file://{}",
        root.join("perfil").display()
    );
    let print = |file: &str| -> Option<PathBuf> {
        let converted = Command::new("soffice")
            .args([&profile, "--headless", "--convert-to", "pdf", "--outdir"])
            .arg(root.join("impresso"))
            .arg(file)
            .output()
            .ok()?;
        assert!(converted.status.success());
        let name = Path::new(file).with_extension("pdf");
        Some(root.join("impresso").join(name.file_name().unwrap()))
    };
    let Some(printed) = print(&editable) else {
        return; // sem LibreOffice neste sistema
    };
    let text = Command::new("pdftotext")
        .arg(&printed)
        .arg("-")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&text.stdout);
    for expected in ["Primeiro slide", "Texto do corpo.", "Segundo slide"] {
        assert!(text.contains(expected), "{expected:?} em:\n{text}");
    }
    let printed = print(&faithful).unwrap();
    let list = Command::new("pdfimages")
        .arg("-list")
        .arg(&printed)
        .output()
        .unwrap();
    let images = String::from_utf8_lossy(&list.stdout)
        .lines()
        .count()
        .saturating_sub(2);
    assert_eq!(images, 2, "uma imagem por slide");
}

#[test]
fn export_reads_scanned_pages_with_ocr() {
    let dir = workspace("export-ocr");
    let root = dir.path();
    // Página com texto, renderizada e embutida só como imagem (digitalizada).
    let original = path(root, "original.pdf");
    let mut pdf = PdfBuilder::new();
    pdf.text_page(&[
        (72.0, 700.0, 40.0, "RUSTPDF testa"),
        (72.0, 600.0, 40.0, "OCR aqui"),
    ]);
    pdf.write(Path::new(&original));
    ok(&["png", &original, &path(root, "pagina.png"), "1"]);
    let png = fs::read(root.join("pagina.png")).unwrap();
    let (width, height, rgb) = rustpdf::infra::png::decode(&png, 50_000_000).unwrap();
    let scanned = path(root, "digitalizado.pdf");
    let mut pdf = PdfBuilder::new();
    let image = pdf.image_rgb(width, height, &rgb);
    pdf.page(
        595.0,
        842.0,
        &format!("/XObject << /Im {image} >>"),
        "q 595 0 0 842 0 0 cm /Im Do Q",
    );
    pdf.write(Path::new(&scanned));

    let output = path(root, "lido.md");
    let (success, _, stderr) = cli(&["export", &scanned, &output, "--sem-imagens"]);
    assert!(
        !success && stderr.contains("OCR"),
        "sem texto: orienta o OCR ({stderr})"
    );
    ok(&["export", &scanned, &output, "--sem-imagens", "--ocr", "eng"]);
    let text = fs::read_to_string(&output).unwrap();
    // O OCR pode errar letras de nomes em maiúsculas (o Tesseract do Ubuntu
    // 24.04 lê "RUSTPDE"); palavras comuns saem estáveis.
    assert!(text.contains("testa") && text.contains("aqui"), "{text}");
}

#[test]
fn ruled_table_with_long_cells_exports_to_csv() {
    let dir = workspace("export-grade");
    let root = dir.path();
    let input = path(root, "grade.pdf");
    let mut pdf = PdfBuilder::new();
    let font = pdf.helvetica();
    // Grade 2 × 2 (linhas em 700, 680, 660; colunas em 70, 150, 500) e texto longo.
    let content = concat!(
        "70 700 m 500 700 l 70 680 m 500 680 l 70 660 m 500 660 l ",
        "70 660 m 70 700 l 150 660 m 150 700 l 500 660 m 500 700 l S ",
        "BT /F1 10 Tf 1 0 0 1 75 686 Tm (Codigo) Tj ET ",
        "BT /F1 10 Tf 1 0 0 1 155 686 Tm (Descricao do item) Tj ET ",
        "BT /F1 10 Tf 1 0 0 1 75 666 Tm (A-10) Tj ET ",
        "BT /F1 10 Tf 1 0 0 1 155 666 Tm (Caneta azul de ponta fina para escrita diaria) Tj ET"
    );
    pdf.page(595.0, 842.0, &format!("/Font << /F1 {font} >>"), content);
    pdf.write(Path::new(&input));
    let output = path(root, "grade.csv");
    ok(&["export", &input, &output, "--sem-bom"]);
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "Codigo;Descricao do item\r\nA-10;Caneta azul de ponta fina para escrita diaria\r\n"
    );
}
