/// Severidade de um diagnóstico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Diagnóstico emitido pelos analyzers (independente do LSP).
#[derive(Debug, Clone)]
pub struct PawnDiagnostic {
    /// Linha (0-based).
    pub line: u32,
    /// Coluna de início (0-based, bytes UTF-8).
    pub col_start: u32,
    /// Coluna de fim (0-based, bytes UTF-8).
    pub col_end: u32,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    /// Marca diagnósticos de símbolo não usado como "desnecessário" (fade no editor).
    pub unnecessary: bool,
}

impl PawnDiagnostic {
    pub fn error(line: u32, col_start: u32, col_end: u32, code: &'static str, message: impl Into<String>) -> Self {
        Self { line, col_start, col_end, severity: Severity::Error, code, message: message.into(), unnecessary: false }
    }

    pub fn warning(line: u32, col_start: u32, col_end: u32, code: &'static str, message: impl Into<String>) -> Self {
        Self { line, col_start, col_end, severity: Severity::Warning, code, message: message.into(), unnecessary: false }
    }

    pub fn unnecessary_warning(line: u32, col_start: u32, col_end: u32, code: &'static str, message: impl Into<String>) -> Self {
        Self { line, col_start, col_end, severity: Severity::Warning, code, message: message.into(), unnecessary: true }
    }
}
