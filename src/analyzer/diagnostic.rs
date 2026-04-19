/// Severidade de um diagnóstico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Hint,
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
    /// Marca diagnóstico com DiagnosticTag::DEPRECATED (tachado no editor).
    pub deprecated: bool,
}

impl PawnDiagnostic {
    #[allow(clippy::too_many_arguments)]
    fn new(
        line: u32, col_start: u32, col_end: u32,
        severity: Severity, code: &'static str,
        message: impl Into<String>,
        unnecessary: bool, deprecated: bool,
    ) -> Self {
        Self { line, col_start, col_end, severity, code, message: message.into(), unnecessary, deprecated }
    }

    pub fn error(line: u32, col_start: u32, col_end: u32, code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(line, col_start, col_end, Severity::Error, code, msg, false, false)
    }

    pub fn warning(line: u32, col_start: u32, col_end: u32, code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(line, col_start, col_end, Severity::Warning, code, msg, false, false)
    }

    pub fn unnecessary_warning(line: u32, col_start: u32, col_end: u32, code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(line, col_start, col_end, Severity::Warning, code, msg, true, false)
    }

    pub fn hint(line: u32, col_start: u32, col_end: u32, code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(line, col_start, col_end, Severity::Hint, code, msg, true, false)
    }

    pub fn deprecated_decl(line: u32, col_start: u32, col_end: u32, code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(line, col_start, col_end, Severity::Warning, code, msg, false, false)
    }

    pub fn deprecated_warning(line: u32, col_start: u32, col_end: u32, code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(line, col_start, col_end, Severity::Warning, code, msg, false, true)
    }
}
