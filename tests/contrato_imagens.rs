// SPDX-License-Identifier: AGPL-3.0-or-later
//! Contratos das portas `ImageSource` e `RulingSource`: interpretador do
//! conteúdo × dublês.
use rustpdf::app::portas::{ImageSource, RulingSource};
use rustpdf::dominio::geometria::Segment;
use rustpdf::infra::imagens::{PdfImages, PdfRulings};
use rustpdf::testkit::{FakeImages, FakeRulings, PdfBuilder, TempDir};
use rustpdf::PdfRect;
use std::path::{Path, PathBuf};

/// Página com a mesma imagem desenhada duas vezes, uma delas em escala.
fn fixture(dir: &TempDir) -> (PathBuf, Vec<PdfRect>) {
    let pdf = dir.join("imagens.pdf");
    let mut builder = PdfBuilder::new();
    let image = builder.image_rgb(2, 1, &[255, 0, 0, 0, 0, 255]);
    builder.page(
        595.0,
        842.0,
        &format!("/XObject << /Im {image} >>"),
        "q 100 0 0 50 10 20 cm /Im Do Q q 1 0 0 1 300 400 cm q 200 0 0 100 0 0 cm /Im Do Q Q",
    );
    builder.write(&pdf);
    let boxes = vec![
        PdfRect::new(10.0, 20.0, 110.0, 70.0),
        PdfRect::new(300.0, 400.0, 500.0, 500.0),
    ];
    (pdf, boxes)
}

fn contract(source: &impl ImageSource, pdf: &Path, expected: &[PdfRect]) {
    let boxes = source.image_boxes(pdf, 1).unwrap();
    assert_eq!(boxes.len(), expected.len());
    for (got, want) in boxes.iter().zip(expected) {
        let close = got
            .to_array()
            .iter()
            .zip(want.to_array())
            .all(|(a, b)| (a - b).abs() < 0.01);
        assert!(close, "{got:?} != {want:?}");
    }
    assert!(source.image_boxes(pdf, 2).is_err(), "página inexistente");
}

#[test]
fn content_interpreter_fulfils_the_contract() {
    let dir = TempDir::new("contrato-imagens");
    let (pdf, boxes) = fixture(&dir);
    contract(&PdfImages, &pdf, &boxes);
}

#[test]
fn fake_fulfils_the_contract() {
    let dir = TempDir::new("contrato-imagens-dublê");
    let (pdf, boxes) = fixture(&dir);
    contract(&FakeImages::new(vec![boxes.clone()]), &pdf, &boxes);
}

/// Página com uma grade de 2 × 1 células (traços) e um retângulo de recorte.
fn ruled(dir: &TempDir) -> (PathBuf, Vec<Segment>) {
    let pdf = dir.join("grade.pdf");
    let mut builder = PdfBuilder::new();
    builder.page(
        595.0,
        842.0,
        "",
        "72 700 m 272 700 l 72 680 m 272 680 l S 72 680 m 72 700 l 172 680 m 172 700 l 272 680 m 272 700 l S 0 0 595 842 re W n",
    );
    builder.write(&pdf);
    let segments = vec![
        ((72.0, 700.0), (272.0, 700.0)),
        ((72.0, 680.0), (272.0, 680.0)),
        ((72.0, 680.0), (72.0, 700.0)),
        ((172.0, 680.0), (172.0, 700.0)),
        ((272.0, 680.0), (272.0, 700.0)),
    ];
    (pdf, segments)
}

fn rulings_contract(source: &impl RulingSource, pdf: &Path, expected: &[Segment]) {
    assert_eq!(source.segments(pdf, 1).unwrap(), expected);
    assert!(source.segments(pdf, 2).is_err(), "página inexistente");
}

#[test]
fn content_interpreter_fulfils_the_rulings_contract() {
    let dir = TempDir::new("contrato-tracos");
    let (pdf, segments) = ruled(&dir);
    rulings_contract(&PdfRulings, &pdf, &segments);
}

#[test]
fn fake_fulfils_the_rulings_contract() {
    let dir = TempDir::new("contrato-tracos-dublê");
    let (pdf, segments) = ruled(&dir);
    rulings_contract(&FakeRulings::new(vec![segments.clone()]), &pdf, &segments);
}
