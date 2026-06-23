// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Binding-only API error codes (N-API / WASM).
//!
//! These codes are defined ahead of binding implementation so the numeric
//! namespace stays stable. Rust core does not emit them yet.

use super::OxMf2ErrorCode;

/// Binding runtime initialization failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum InitializationErrorCode {
    WasmNotInitialized = 10_000,
    NativeBindingUnavailable = 10_001,
}

impl InitializationErrorCode {
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
            Self::WasmNotInitialized => "InitializationWasmNotInitialized",
            Self::NativeBindingUnavailable => "InitializationNativeBindingUnavailable",
        }
    }
}

impl core::fmt::Display for InitializationErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// Binding input validation failures not better expressed as `TypeError` /
/// `RangeError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum BindingValidationErrorCode {
    InvalidOptions = 11_000,
}

impl BindingValidationErrorCode {
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
            Self::InvalidOptions => "BindingValidationInvalidOptions",
        }
    }
}

impl core::fmt::Display for BindingValidationErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}
