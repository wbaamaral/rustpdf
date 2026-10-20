// SPDX-License-Identifier: AGPL-3.0-or-later
//! Regras de dependência entre as camadas do crate.
//!
//! `dominio` é puro; `app` conhece só o domínio; `apresentacao` não fala com a
//! infraestrutura. As regras valem para todos os arquivos de cada camada.
use std::fs;
use std::path::{Path, PathBuf};

/// Camadas que podem ser citadas por `crate::<camada>` a partir de cada camada.
/// `None` = qualquer módulo, exceto os listados em `FORBIDDEN_MODULES`.
struct Layer {
    name: &'static str,
    allowed_modules: Option<&'static [&'static str]>,
    forbidden_modules: &'static [&'static str],
    forbidden_text: &'static [&'static str],
    forbids_unsafe: bool,
}

/// `unsafe` não entra aqui: `#![forbid(unsafe_code)]` faz o compilador barrar.
const IMPURE: &[&str] = &[
    "extern \"C\"",
    "std::fs",
    "std::process",
    "std::thread",
    "std::net",
    "std::env",
];

const LAYERS: &[Layer] = &[
    Layer {
        name: "dominio",
        allowed_modules: Some(&["dominio"]),
        forbidden_modules: &[],
        forbidden_text: IMPURE,
        forbids_unsafe: true,
    },
    Layer {
        name: "app",
        allowed_modules: Some(&["dominio", "app"]),
        forbidden_modules: &[],
        forbidden_text: &["extern \"C\"", "std::process"],
        forbids_unsafe: true,
    },
    Layer {
        name: "apresentacao",
        allowed_modules: Some(&["dominio", "app", "apresentacao"]),
        forbidden_modules: &[],
        forbidden_text: &["extern \"C\"", "std::process"],
        forbids_unsafe: true,
    },
    Layer {
        name: "infra",
        allowed_modules: None,
        forbidden_modules: &["apresentacao"],
        forbidden_text: &[],
        forbids_unsafe: false,
    },
];

/// Código de produção sem comentários de linha (`//`, `///`, `//!`). O
/// módulo de testes (`#[cfg(test)]`, no fim do arquivo) não entra no binário
/// e pode usar o `testkit`, que monta dublês com a infraestrutura.
fn code_only(source: &str) -> String {
    let production = source.split("#[cfg(test)]").next().unwrap_or("");
    production
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Módulos citados como `crate::<módulo>`.
fn crate_modules(code: &str) -> Vec<String> {
    code.match_indices("crate::")
        .map(|(start, key)| {
            code[start + key.len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

/// Violações de `layer` no código `source`.
fn violations(layer: &Layer, source: &str) -> Vec<String> {
    let code = code_only(source);
    let mut found = Vec::new();
    for module in crate_modules(&code) {
        let allowed = match layer.allowed_modules {
            Some(allowed) => allowed.contains(&module.as_str()),
            None => !layer.forbidden_modules.contains(&module.as_str()),
        };
        if !allowed {
            found.push(format!("usa crate::{module}"));
        }
    }
    for text in layer.forbidden_text {
        if code.contains(text) {
            found.push(format!("contém `{text}`"));
        }
    }
    found
}

fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(rust_files(&path));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

#[test]
fn layers_respect_dependency_rules() {
    let mut problems = Vec::new();
    for layer in LAYERS {
        let root = source_dir().join(layer.name);
        assert!(
            root.join("mod.rs").is_file(),
            "camada {} ausente",
            layer.name
        );
        for file in rust_files(&root) {
            let source = fs::read_to_string(&file).unwrap();
            for problem in violations(layer, &source) {
                problems.push(format!("{}: {problem}", file.display()));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "violações de camada:\n{}",
        problems.join("\n")
    );
}

#[test]
fn pure_layers_forbid_unsafe_code() {
    for layer in LAYERS.iter().filter(|layer| layer.forbids_unsafe) {
        let module = source_dir().join(layer.name).join("mod.rs");
        let source = fs::read_to_string(&module).unwrap();
        assert!(
            source.contains("#![forbid(unsafe_code)]"),
            "{} sem #![forbid(unsafe_code)]",
            module.display()
        );
    }
}

#[test]
fn layers_are_declared_in_the_library() {
    let lib = fs::read_to_string(source_dir().join("lib.rs")).unwrap();
    for layer in LAYERS {
        assert!(
            lib.contains(&format!("pub mod {};", layer.name)),
            "lib.rs não declara a camada {}",
            layer.name
        );
    }
}

#[test]
fn checker_detects_each_kind_of_violation() {
    let dominio = &LAYERS[0];
    assert!(violations(dominio, "use crate::dominio::geometria::PdfRect;").is_empty());
    assert_eq!(
        violations(dominio, "use crate::infra::qpdf;"),
        ["usa crate::infra"]
    );
    assert_eq!(
        violations(dominio, "use crate::content::x;"),
        ["usa crate::content"]
    );
    assert_eq!(
        violations(dominio, "let _ = std::fs::read(p);"),
        ["contém `std::fs`"]
    );
    assert!(violations(dominio, "// usa std::fs só no comentário").is_empty());
    assert!(violations(dominio, "#![forbid(unsafe_code)]").is_empty());
    assert_eq!(
        violations(dominio, "extern \"C\" { fn f(); }"),
        ["contém `extern \"C\"`"]
    );

    let app = &LAYERS[1];
    assert!(violations(app, "use crate::dominio::layout;").is_empty());
    assert_eq!(
        violations(app, "use crate::infra::poppler;"),
        ["usa crate::infra"]
    );

    let tested =
        "use crate::dominio::x;\n#[cfg(test)]\nmod tests { use crate::testkit::FakePoppler; }";
    assert!(
        violations(app, tested).is_empty(),
        "testes podem usar o testkit"
    );
    assert_eq!(
        violations(app, "use crate::testkit::FakePoppler;"),
        ["usa crate::testkit"],
        "fora dos testes, não"
    );

    let infra = &LAYERS[3];
    assert!(violations(infra, "use crate::app::portas; unsafe { f() }").is_empty());
    assert_eq!(
        violations(infra, "use crate::apresentacao::cli;"),
        ["usa crate::apresentacao"]
    );
}
