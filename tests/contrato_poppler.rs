// SPDX-License-Identifier: AGPL-3.0-or-later
//! Contrato das portas `LayoutSource` e `Renderer`: a mesma bateria roda no
//! adaptador real (poppler) e no dublê do testkit, para o dublê não mentir.
use rustpdf::app::portas::{LayoutSource, Renderer};
use rustpdf::infra::poppler::Poppler;
use rustpdf::testkit::{FakePoppler, PdfBuilder, TempDir, TextLine};
use rustpdf::LayoutRect;
use std::path::{Path, PathBuf};

const LINES: &[TextLine] = &[(72.0, 700.0, 14.0, "Ola mundo"), (72.0, 600.0, 14.0, "Fim")];

/// PDF de uma página com `LINES` e o dublê equivalente.
fn fixture(dir: &TempDir) -> (PathBuf, FakePoppler) {
    let pdf = dir.join("contrato.pdf");
    let mut builder = PdfBuilder::new();
    builder.text_page(LINES);
    builder.write(&pdf);
    (pdf, FakePoppler::new(vec![LINES.to_vec()]))
}

fn layout_contract(source: &impl LayoutSource, pdf: &Path) {
    let page = source.layout(pdf, 1).unwrap();
    assert_eq!((page.width, page.height), (595.0, 842.0));
    let lines: Vec<String> = page.lines().map(|line| line.text()).collect();
    assert_eq!(lines, ["Ola mundo", "Fim"]);
    let first = page.words().next().unwrap();
    assert!((first.rect.left() - 72.0).abs() < 1.0, "{:?}", first.rect);
    // Linha de base em y = 700 (de baixo): topo ≈ 142 − ascendente, base ≈ 142 + descendente.
    assert!(
        (125.0..142.0).contains(&first.rect.top()),
        "{:?}",
        first.rect
    );
    assert!(
        (140.0..150.0).contains(&first.rect.bottom()),
        "{:?}",
        first.rect
    );
    assert!(source.layout(pdf, 2).is_err(), "página inexistente");
    assert!(source.text(pdf, 1).unwrap().contains("Ola mundo"));
    assert!(source.text(pdf, 2).is_err());
}

fn renderer_contract(renderer: &impl Renderer, pdf: &Path, dir: &TempDir) {
    let png = dir.join("pagina.png");
    renderer.render_page(pdf, 1, 120, &png).unwrap();
    assert!(std::fs::read(&png)
        .unwrap()
        .starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(renderer
        .render_page(pdf, 2, 120, &dir.join("x.png"))
        .is_err());
    let region = renderer
        .render_region(pdf, 1, LayoutRect::new(72.0, 100.0, 216.0, 172.0), 144)
        .unwrap();
    assert!(region.starts_with(b"\x89PNG\r\n\x1a\n"));
    let outside = renderer.render_region(pdf, 2, LayoutRect::new(0.0, 0.0, 10.0, 10.0), 72);
    assert!(outside.is_err());
    let thumbs = dir.join("miniaturas");
    std::fs::create_dir(&thumbs).unwrap();
    let pages = renderer.render_pages(pdf, 1..=1, 60, &thumbs).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].0, 1);
    assert_eq!(pages[0].1, thumbs.join("p-00001.png"));
    assert!(pages[0].1.is_file());
}

#[test]
fn poppler_fulfils_the_contract() {
    let dir = TempDir::new("contrato-poppler");
    let (pdf, _) = fixture(&dir);
    layout_contract(&Poppler, &pdf);
    renderer_contract(&Poppler, &pdf, &dir);
}

#[test]
fn fake_fulfils_the_contract() {
    let dir = TempDir::new("contrato-dublê");
    let (pdf, fake) = fixture(&dir);
    layout_contract(&fake, &pdf);
    renderer_contract(&fake, &pdf, &dir);
}

#[test]
fn poppler_renders_the_requested_width() {
    let dir = TempDir::new("contrato-largura");
    let (pdf, _) = fixture(&dir);
    let png = dir.join("largura.png");
    Poppler.render_page(&pdf, 1, 120, &png).unwrap();
    let header = std::fs::read(&png).unwrap();
    assert_eq!(u32::from_be_bytes(header[16..20].try_into().unwrap()), 120);
}

#[test]
fn poppler_renders_regions_at_the_requested_resolution() {
    let dir = TempDir::new("contrato-regiao");
    let (pdf, _) = fixture(&dir);
    // 144 × 72 pt a 144 dpi = 288 × 144 pixels.
    let png = Poppler
        .render_region(&pdf, 1, LayoutRect::new(72.0, 100.0, 216.0, 172.0), 144)
        .unwrap();
    assert_eq!(rustpdf::infra::png::dimensions(&png), Some((288, 144)));
}
