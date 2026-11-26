// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What a binding read where a boundary expects text: a typed input slot's
//! Safe Origin Kind candidate, or the Resolver Construction Identity
//! reference (D15:3156, 3162).
//!
//! A binding reads a host value and reports its text, or that it is not a
//! string, so the boundary, not the binding, refuses a value that is no text:
//! a candidate as `unknown-safe-origin-kind`, a reference as
//! `malformed-construction-identity`.

/// A value a binding read where a boundary expects text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextInput<'a> {
    /// The host value's text.
    Text(&'a str),
    /// A host value that is not a string.
    NotText,
}

impl<'a> From<&'a str> for TextInput<'a> {
    fn from(text: &'a str) -> Self {
        Self::Text(text)
    }
}
