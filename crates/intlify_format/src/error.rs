// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeMap;

/// Stable key/value details attached to an operational formatter error.
pub type ErrorDetails = BTreeMap<String, String>;

/// Broad error class used by CLI and binding reporters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Input selection, input decoding, snapshot input, or source mismatch.
    Input,
    /// Config discovery, parsing, or validation.
    Config,
    /// Formatting execution outside parser diagnostics, such as file output.
    Execution,
    /// Formatter invariant violation after supposedly valid parser input.
    Internal,
}

impl ErrorKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Config => "config",
            Self::Execution => "execution",
            Self::Internal => "internal",
        }
    }
}

/// Stable formatter operational error codes.
///
/// Parser diagnostics are carried separately in [`crate::FormatFailure`].
/// These codes describe I/O, config, binding, snapshot, and invariant errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormatErrorCode {
    /// Snapshot-backed formatting received source text that does not match.
    SourceSnapshotMismatch,
    /// CLI file discovery found an input kind the formatter does not support.
    UnsupportedInputFile,
    /// A CLI ignore pattern could not be parsed.
    InvalidIgnorePattern,
    /// A CLI ignore file could not be read.
    IgnoreFileReadFailed,
    /// CLI input patterns did not match any supported files.
    UnmatchedInput,
    /// Raw binding or CLI options could not be converted into typed options.
    InvalidOptions,
    /// Snapshot bytes or snapshot capabilities are not usable for formatting.
    InvalidSnapshot,
    /// Source input could not be read or decoded.
    InputReadFailed,
    /// Formatted output could not be written.
    OutputWriteFailed,
    /// Config file contents could not be read.
    ConfigReadFailed,
    /// Config file contents could not be parsed.
    ConfigParseFailed,
    /// Parsed config failed schema or semantic validation.
    ConfigValidationFailed,
    /// Formatter invariant violation that should not be user-actionable.
    InternalError,
}

impl FormatErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceSnapshotMismatch => "source_snapshot_mismatch",
            Self::UnsupportedInputFile => "unsupported_input_file",
            Self::InvalidIgnorePattern => "invalid_ignore_pattern",
            Self::IgnoreFileReadFailed => "ignore_file_read_failed",
            Self::UnmatchedInput => "unmatched_input",
            Self::InvalidOptions => "invalid_options",
            Self::InvalidSnapshot => "invalid_snapshot",
            Self::InputReadFailed => "input_read_failed",
            Self::OutputWriteFailed => "output_write_failed",
            Self::ConfigReadFailed => "config_read_failed",
            Self::ConfigParseFailed => "config_parse_failed",
            Self::ConfigValidationFailed => "config_validation_failed",
            Self::InternalError => "internal_error",
        }
    }
}

/// Operational failure that is not a parser diagnostic.
///
/// The public result model keeps parser diagnostics and operational errors
/// separate so callers can distinguish invalid MF2 source from formatter
/// execution failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalError {
    /// Broad error class for reporter grouping.
    pub kind: ErrorKind,
    /// Stable machine-readable formatter code.
    pub code: FormatErrorCode,
    /// Human-readable message suitable for text reporters.
    pub message: String,
    /// Optional file path or external input identifier.
    pub path: Option<String>,
    /// Stable structured metadata for JSON reporters.
    pub details: ErrorDetails,
}

impl OperationalError {
    /// Create an operational error without path or details.
    #[must_use]
    pub fn new(kind: ErrorKind, code: FormatErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind,
            code,
            message: message.into(),
            path: None,
            details: ErrorDetails::new(),
        }
    }

    /// Attach the file path or external input identifier associated with this error.
    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Attach one stable detail entry for JSON reporters.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    /// Build the standard missing snapshot capability error.
    #[must_use]
    pub fn missing_snapshot_capability(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Input, FormatErrorCode::InvalidSnapshot, message)
            .with_detail("reason", "missing_capability")
    }

    /// Build an invalid snapshot input error with a stable reason.
    #[must_use]
    pub fn invalid_snapshot(message: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(ErrorKind::Input, FormatErrorCode::InvalidSnapshot, message)
            .with_detail("reason", reason)
    }

    /// Build a source/snapshot mismatch error.
    #[must_use]
    pub fn source_snapshot_mismatch(message: impl Into<String>) -> Self {
        Self::new(
            ErrorKind::Input,
            FormatErrorCode::SourceSnapshotMismatch,
            message,
        )
    }

    /// Build an internal formatter invariant error.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, FormatErrorCode::InternalError, message)
    }
}
