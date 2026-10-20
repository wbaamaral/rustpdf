// SPDX-License-Identifier: AGPL-3.0-or-later
//! Execução de ferramentas externas (qpdf, poppler, tesseract).

use crate::app::erros::ToolError;
use crate::trace;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

/// Processo de uma ferramenta externa (qpdf, poppler, tesseract). No Windows,
/// sem `CREATE_NO_WINDOW` cada execução abre e fecha uma janela de console,
/// porque o aplicativo gráfico não tem console para os filhos herdarem.
pub fn tool_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub fn tool_output(program: &str, args: &[&OsStr]) -> Result<String, ToolError> {
    let _trace = trace(|| {
        let first = args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
        format!("  processo {program} {first}")
    });
    let output = tool_command(program)
        .args(args)
        .output()
        .map_err(|e| ToolError::Unavailable {
            program: program.into(),
            detail: e.to_string(),
        })?;
    if !accepts_exit(program, output.status.code()) {
        return Err(ToolError::Failed {
            program: program.into(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn run(program: &str, args: &[OsString]) -> Result<String, ToolError> {
    let refs: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
    tool_output(program, &refs)
}

/// Código de saída aceito: sucesso, ou 3 no qpdf (concluiu com avisos — PDF
/// com pequenos defeitos de estrutura; o resultado é válido).
fn accepts_exit(program: &str, code: Option<i32>) -> bool {
    code == Some(0) || (program == "qpdf" && code == Some(3))
}

/// Primeira linha da saída de uma ferramenta (`stdout` ou `stderr`), ou "não encontrado".
pub fn tool_version(program: &str, args: &[&str]) -> String {
    tool_command(program)
        .args(args)
        .output()
        .ok()
        .and_then(|output| {
            let text = if output.stdout.is_empty() {
                output.stderr
            } else {
                output.stdout
            };
            String::from_utf8_lossy(&text)
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string())
        })
        .unwrap_or_else(|| "não encontrado".into())
}

/// Pacotes portáteis (Windows) trazem `pdftoppm`, `qpdf` etc. ao lado do
/// executável: essa pasta entra na frente do `PATH` dos processos filhos, e
/// `../share/tessdata` (se existir) vira o `TESSDATA_PREFIX` do Tesseract.
pub fn prefer_bundled_tools() {
    let Some(directory) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let bundled = ["pdftoppm", "pdftoppm.exe"]
        .iter()
        .any(|tool| directory.join(tool).is_file());
    if !bundled {
        return;
    }
    let mut paths = vec![directory];
    if let Some(current) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current));
    }
    let tessdata = paths[0]
        .parent()
        .map(|prefix| prefix.join("share").join("tessdata"));
    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var("PATH", joined);
    }
    if let Some(tessdata) = tessdata.filter(|path| path.is_dir()) {
        if std::env::var_os("TESSDATA_PREFIX").is_none() {
            std::env::set_var("TESSDATA_PREFIX", tessdata);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn output_returns_stdout_on_success() {
        let version = tool_output("qpdf", &[OsStr::new("--version")]).unwrap();
        assert!(version.starts_with("qpdf version"), "{version}");
    }

    #[test]
    fn failure_reports_program_and_stderr() {
        let error = tool_output("qpdf", &[OsStr::new("--opcao-invalida")]).unwrap_err();
        assert!(
            matches!(&error, ToolError::Failed { program, detail }
                if program == "qpdf" && detail.contains("opcao-invalida")),
            "{error:?}"
        );
        let missing = tool_output("rustpdf-programa-inexistente", &[]).unwrap_err();
        assert!(
            matches!(missing, ToolError::Unavailable { .. }),
            "{missing:?}"
        );
    }

    #[test]
    fn qpdf_warnings_exit_code_is_success() {
        assert!(accepts_exit("qpdf", Some(0)));
        assert!(accepts_exit("qpdf", Some(3)), "qpdf: 3 = só avisos");
        assert!(!accepts_exit("qpdf", Some(2)));
        assert!(!accepts_exit("pdftoppm", Some(3)));
        assert!(!accepts_exit("qpdf", None), "encerrado por sinal");
    }

    #[test]
    fn version_is_first_non_empty_line() {
        assert!(tool_version("qpdf", &["--version"]).starts_with("qpdf version"));
        assert_eq!(
            tool_version("rustpdf-programa-inexistente", &[]),
            "não encontrado"
        );
    }
}
