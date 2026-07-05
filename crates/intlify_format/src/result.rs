// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use ox_mf2_parser::Diagnostic;

use crate::OperationalError;

/// Result returned by APIs that produce formatted text.
pub type FormatResult = Result<FormatSuccess, FormatFailure>;

/// Result returned by APIs that only report whether formatting would change.
pub type FormatCheckResult = Result<FormatCheckSuccess, FormatFailure>;

/// Successful formatting result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatSuccess {
    /// Whether `code` differs byte-for-byte from the supplied source.
    pub changed: bool,
    /// Complete formatted MF2 message text.
    pub code: String,
}

/// Successful formatting check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatCheckSuccess {
    /// Whether formatting would change the supplied source.
    pub changed: bool,
}

/// Failed formatting result.
///
/// Parser diagnostics and operational errors are intentionally separated:
/// parser diagnostics mean the source is invalid MF2, while operational errors
/// mean the formatter could not complete for another reason.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatFailure {
    /// Parser diagnostics emitted before formatting was attempted.
    pub diagnostics: Vec<Diagnostic>,
    /// Non-parser failures such as invalid snapshot capabilities.
    pub errors: Vec<OperationalError>,
}

impl FormatFailure {
    /// Build a failure from parser diagnostics.
    #[must_use]
    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            diagnostics,
            errors: Vec::new(),
        }
    }

    /// Build a failure from one operational error.
    #[must_use]
    pub fn from_error(error: OperationalError) -> Self {
        Self {
            diagnostics: Vec::new(),
            errors: vec![error],
        }
    }

    /// Return whether the failure is empty.
    ///
    /// This is mainly useful for tests and reporter assertions; normal public
    /// APIs should not construct an empty failure.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty() && self.errors.is_empty()
    }
}
