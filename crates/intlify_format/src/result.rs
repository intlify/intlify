// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use ox_mf2_parser::Diagnostic;

use crate::OperationalError;

pub type FormatResult = Result<FormatSuccess, FormatFailure>;

pub type FormatCheckResult = Result<FormatCheckSuccess, FormatFailure>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatSuccess {
    pub changed: bool,
    pub code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatCheckSuccess {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatFailure {
    pub diagnostics: Vec<Diagnostic>,
    pub errors: Vec<OperationalError>,
}

impl FormatFailure {
    #[must_use]
    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            diagnostics,
            errors: Vec::new(),
        }
    }

    #[must_use]
    pub fn from_error(error: OperationalError) -> Self {
        Self {
            diagnostics: Vec::new(),
            errors: vec![error],
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty() && self.errors.is_empty()
    }
}
