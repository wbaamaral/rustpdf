// SPDX-License-Identifier: AGPL-3.0-or-later
//! Testes da CLI executando o binário `rustpdf-cli` em PDFs reais.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0,
    0, 0, 114, 182, 13, 36, 0, 0, 0, 18, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 12,
    129, 52, 24, 0, 0, 73, 200, 9, 247, 249, 171, 182, 13, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
    130,
];

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

fn workspace(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rustpdf-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir(&root).unwrap();
    root
}

fn path(root: &Path, name: &str) -> String {
    root.join(name).to_string_lossy().into_owned()
}

#[test]
fn selects_moves_resizes_and_rotates_objects() {
    let root = workspace("objects");
    let (blank, image_pdf, text_pdf, base) = (
        path(&root, "blank.pdf"),
        path(&root, "image.pdf"),
        path(&root, "text.pdf"),
        path(&root, "base.pdf"),
    );
    let png = path(&root, "pixels.png");
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
    let resized = path(&root, "resized.pdf");
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
    let rotated = path(&root, "rotated.pdf");
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
        &path(&root, "x.pdf"),
        "1",
        "115",
        "306",
        "2",
        "1",
    ]);
    assert!(!success && stderr.contains("proporcionalmente"), "{stderr}");
    let moved = path(&root, "moved.pdf");
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
    let turned = path(&root, "turned.pdf");
    ok(&["rotate-object", &moved, &turned, "1", "115", "206", "30"]);
    let center = numbers(&property(&text, "caixa"));
    let (cx, cy) = ((center[0] + center[2]) / 2.0, (center[1] + center[3]) / 2.0);
    let text = ok(&["select", &turned, "1", &cx.to_string(), &cy.to_string()]);
    assert_close(&numbers(&property(&text, "angulo")), &[30.0], 0.5);

    // Nota: só move.
    let (success, _, stderr) = cli(&[
        "rotate-object",
        &turned,
        &path(&root, "x.pdf"),
        "1",
        "410",
        "710",
        "45",
    ]);
    assert!(
        !success && stderr.contains("não pode ser girado"),
        "{stderr}"
    );
    let final_pdf = path(&root, "final.pdf");
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
    let matrix = path(&root, "matrix.pdf");
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
    let _ = fs::remove_dir_all(root);
}

/// PDF mínimo com xref correta a partir dos corpos dos objetos (1, 2, …).
fn write_pdf(path: &str, objects: &[String]) {
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    fs::write(path, bytes).unwrap();
}

fn stream(dictionary: &str, data: &str) -> String {
    format!(
        "<< {dictionary} /Length {} >>\nstream\n{data}\nendstream",
        data.len()
    )
}

#[test]
fn image_commands_act_on_chosen_occurrences() {
    let root = workspace("image-occurrences");
    let input = path(&root, "shared.pdf");
    let resources = "/Resources << /XObject << /Im1 5 0 R >> >>";
    write_pdf(
        &input,
        &[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".into(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] {resources} /Contents 4 0 R >>"),
            stream("", "q 100 0 0 50 10 20 cm /Im1 Do Q q 100 0 0 50 10 200 cm /Im1 Do Q"),
            stream(
                "/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8",
                "abc",
            ),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] {resources} /Contents 7 0 R >>"),
            stream("", "q 100 0 0 50 10 20 cm /Im1 Do Q"),
        ],
    );
    let png = path(&root, "pixels.png");
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
    let moved = path(&root, "moved.pdf");
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
    let replaced = path(&root, "replaced.pdf");
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
    let removed = path(&root, "removed.pdf");
    ok(&["remove-image-at", &replaced, &removed, "1", "60", "225"]);
    assert_eq!(draws(&removed, "1").len(), 1);
    let empty = path(&root, "empty.pdf");
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
    let _ = fs::remove_dir_all(root);
}
