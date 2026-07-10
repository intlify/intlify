// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fatal parser API failures.
//!
//! Recoverable MF2 syntax errors are not represented here: they remain
//! successful parse results with diagnostics. These errors only cover cases
//! where the parser cannot produce a trustworthy result.

use crate::error::OxMf2ErrorCode;
use crate::span::SourceId;

/// Parser API failure code (`4000..4999`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ParseErrorCode {
    SourceTooLarge = 4000,
    InvalidSourceId = 4001,
    TooManySources = 4002,
    TooManyNodes = 4003,
    TooManyEdges = 4004,
    TooManyTokens = 4005,
    TooManyTrivia = 4006,
    TooManyDiagnostics = 4007,
    MissingRoot = 4008,
}

impl ParseErrorCode {
    #[inline]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    #[inline]
    pub const fn as_ox_mf2_error_code(self) -> OxMf2ErrorCode {
        self.as_u32()
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::SourceTooLarge => "ParseSourceTooLarge",
            Self::InvalidSourceId => "ParseInvalidSourceId",
            Self::TooManySources => "ParseTooManySources",
            Self::TooManyNodes => "ParseTooManyNodes",
            Self::TooManyEdges => "ParseTooManyEdges",
            Self::TooManyTokens => "ParseTooManyTokens",
            Self::TooManyTrivia => "ParseTooManyTrivia",
            Self::TooManyDiagnostics => "ParseTooManyDiagnostics",
            Self::MissingRoot => "ParseMissingRoot",
        }
    }
}

/// Resource whose `u32`-indexed parser representation was exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseResource {
    Sources,
    Nodes,
    Edges,
    Tokens,
    Trivia,
    Diagnostics,
}

impl ParseResource {
    const fn description(self) -> &'static str {
        match self {
            Self::Sources => "source count",
            Self::Nodes => "node count",
            Self::Edges => "edge count",
            Self::Tokens => "token count",
            Self::Trivia => "trivia count",
            Self::Diagnostics => "diagnostic count",
        }
    }
}

/// Fatal failure from a single-source parser entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Source length cannot be represented by the parser's `u32` spans.
    SourceTooLarge,
    /// The requested source is not present in the supplied source store.
    InvalidSourceId { source_id: SourceId },
    /// A `u32`-indexed parser table or collection exhausted its domain.
    ResourceLimit { resource: ParseResource },
    /// Parsing completed without producing the required CST root.
    MissingRoot,
}

impl ParseError {
    #[inline]
    pub const fn code(self) -> ParseErrorCode {
        match self {
            Self::SourceTooLarge => ParseErrorCode::SourceTooLarge,
            Self::InvalidSourceId { .. } => ParseErrorCode::InvalidSourceId,
            Self::ResourceLimit {
                resource: ParseResource::Sources,
            } => ParseErrorCode::TooManySources,
            Self::ResourceLimit {
                resource: ParseResource::Nodes,
            } => ParseErrorCode::TooManyNodes,
            Self::ResourceLimit {
                resource: ParseResource::Edges,
            } => ParseErrorCode::TooManyEdges,
            Self::ResourceLimit {
                resource: ParseResource::Tokens,
            } => ParseErrorCode::TooManyTokens,
            Self::ResourceLimit {
                resource: ParseResource::Trivia,
            } => ParseErrorCode::TooManyTrivia,
            Self::ResourceLimit {
                resource: ParseResource::Diagnostics,
            } => ParseErrorCode::TooManyDiagnostics,
            Self::MissingRoot => ParseErrorCode::MissingRoot,
        }
    }

    #[inline]
    pub const fn as_ox_mf2_error_code(self) -> OxMf2ErrorCode {
        self.code().as_ox_mf2_error_code()
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SourceTooLarge => f.write_str("source length exceeds u32::MAX byte offsets"),
            Self::InvalidSourceId { source_id } => {
                write!(
                    f,
                    "source id {} is not present in SourceStore",
                    source_id.raw()
                )
            }
            Self::ResourceLimit { resource } => {
                write!(f, "{} exceeds u32::MAX", resource.description())
            }
            Self::MissingRoot => f.write_str("parser produced no CST root"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Fatal batch failure with the input position that caused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchParseError {
    pub input_index: usize,
    pub error: ParseError,
}

impl core::fmt::Display for BatchParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "batch input {}: {}", self.input_index, self.error)
    }
}

impl std::error::Error for BatchParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
