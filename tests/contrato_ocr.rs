// SPDX-License-Identifier: AGPL-3.0-or-later
//! Contrato da porta `OcrEngine`: Tesseract real e dublê do testkit.
use rustpdf::app::portas::{OcrEngine, Renderer};
use rustpdf::infra::png;
use rustpdf::infra::poppler::Poppler;
use rustpdf::infra::tesseract::Tesseract;
use rustpdf::testkit::{FakeOcr, PdfBuilder, TempDir};
use std::path::{Path, PathBuf};

/// Página com texto grande renderizada em PNG (o que o OCR recebe na prática).
fn scanned_page(dir: &TempDir) -> PathBuf {
    let pdf = dir.join("digitalizada.pdf");
    let mut builder = PdfBuilder::new();
    builder.text_page(&[
        (72.0, 700.0, 40.0, "RUSTPDF testa"),
        (72.0, 600.0, 40.0, "OCR aqui"),
    ]);
    builder.write(&pdf);
    let png = dir.join("digitalizada.png");
    Poppler.render_page(&pdf, 1, 1200, &png).unwrap();
    png
}

/// Confere palavras comuns: o OCR pode errar letras de nomes em maiúsculas
/// (o Tesseract do Ubuntu 24.04 lê "RUSTPDF" como "RUSTPDE").
fn contract(ocr: &impl OcrEngine, image: &Path) {
    let (width, height) = png::file_dimensions(image).unwrap();
    let image = &std::fs::read(image).unwrap();
    let page = ocr.words(image, "eng").unwrap();
    assert_eq!(
        (page.width, page.height),
        (f64::from(width), f64::from(height))
    );
    let word = page
        .words()
        .find(|word| word.text == "testa")
        .expect("palavra testa");
    assert!(word.rect.left() >= 0.0 && word.rect.right() <= page.width);
    assert!(word.rect.top() >= 0.0 && word.rect.bottom() <= page.height);
    assert!(
        word.rect.top() < page.height / 2.0,
        "a primeira linha fica no alto"
    );
    assert!(ocr.text(image, Some("eng")).unwrap().contains("testa"));
    assert!(ocr.words(b"isto nao e um PNG", "eng").is_err());
}

#[test]
fn tesseract_fulfils_the_contract() {
    let dir = TempDir::new("contrato-tesseract");
    let image = scanned_page(&dir);
    contract(&Tesseract, &image);
}

#[test]
fn fake_fulfils_the_contract() {
    let dir = TempDir::new("contrato-ocr-dublê");
    let image = scanned_page(&dir);
    contract(
        &FakeOcr::new(&[&["RUSTPDF", "testa"], &["OCR", "aqui"]]),
        &image,
    );
}
