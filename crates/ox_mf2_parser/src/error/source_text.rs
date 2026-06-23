// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Source text attachment and slicing API error codes.

use super::OxMf2ErrorCode;

/// Source text access failures after parsing or decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SourceTextErrorCode {
    SourceTextNotIncluded = 3000,
    SourceTextSpanOutOfBounds = 3001,
    SourceTextTooLarge = 3002,
    /// Reserved: batch source attachment count mismatch.
    SourceTextCountMismatch = 3003,
    /// Reserved: UTF-16 unpaired surrogate rejection at binding boundary.
    SourceTextUnpairedSurrogate = 3004,
}

impl SourceTextErrorCode {
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
            Self::SourceTextNotIncluded => "SourceTextNotIncluded",
            Self::SourceTextSpanOutOfBounds => "SourceTextSpanOutOfBounds",
            Self::SourceTextTooLarge => "SourceTextTooLarge",
            Self::SourceTextCountMismatch => "SourceTextCountMismatch",
            Self::SourceTextUnpairedSurrogate => "SourceTextUnpairedSurrogate",
        }
    }
}

impl core::fmt::Display for SourceTextErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}
