// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeMap;

pub type ErrorDetails = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    Input,
    Config,
    Execution,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormatErrorCode {
    SourceSnapshotMismatch,
    UnsupportedInputFile,
    InvalidIgnorePattern,
    IgnoreFileReadFailed,
    UnmatchedInput,
    InvalidOptions,
    InvalidSnapshot,
    InputReadFailed,
    OutputWriteFailed,
    ConfigReadFailed,
    ConfigParseFailed,
    ConfigValidationFailed,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalError {
    pub kind: ErrorKind,
    pub code: FormatErrorCode,
    pub message: String,
    pub path: Option<String>,
    pub details: ErrorDetails,
}

impl OperationalError {
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

    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn missing_snapshot_capability(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Input, FormatErrorCode::InvalidSnapshot, message)
            .with_detail("reason", "missing_capability")
    }

    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, FormatErrorCode::InternalError, message)
    }
}
