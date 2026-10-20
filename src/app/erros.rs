// SPDX-License-Identifier: AGPL-3.0-or-later
//! Erros tipados dos casos de uso e das portas. A mensagem em pt-BR vem do
//! `Display`; os testes conferem a variante.
use std::fmt;

/// Falha de uma ferramenta externa (qpdf, poppler, tesseract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// O programa não pôde ser iniciado (ausente, sem permissão…).
    Unavailable { program: String, detail: String },
    /// O programa terminou com erro; `detail` é o stderr.
    Failed { program: String, detail: String },
    /// O programa respondeu algo que não pôde ser interpretado.
    InvalidOutput { program: String, detail: String },
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { program, detail } => {
                write!(f, "Não foi possível executar {program}: {detail}")
            }
            Self::Failed { program, detail } => write!(f, "{program} falhou: {detail}"),
            Self::InvalidOutput { program, detail } => {
                write!(f, "Resposta inválida de {program}: {detail}")
            }
        }
    }
}

impl std::error::Error for ToolError {}

impl From<ToolError> for String {
    fn from(error: ToolError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_errors_read_in_portuguese() {
        let missing = ToolError::Unavailable {
            program: "qpdf".into(),
            detail: "No such file".into(),
        };
        assert_eq!(
            missing.to_string(),
            "Não foi possível executar qpdf: No such file"
        );
        let failed = ToolError::Failed {
            program: "pdftoppm".into(),
            detail: "Syntax Error".into(),
        };
        assert_eq!(failed.to_string(), "pdftoppm falhou: Syntax Error");
        let invalid = ToolError::InvalidOutput {
            program: "qpdf".into(),
            detail: "número de páginas".into(),
        };
        assert_eq!(
            invalid.to_string(),
            "Resposta inválida de qpdf: número de páginas"
        );
        // Compatível com o restante do código, que ainda usa String.
        let message: String = failed.into();
        assert_eq!(message, "pdftoppm falhou: Syntax Error");
    }
}
