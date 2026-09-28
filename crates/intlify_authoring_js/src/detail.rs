// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The details this crate reports.
//!
//! As in `intlify_authoring`, a detail names which cause inside a reason
//! family one record reports, so that a fixture asserts the cause it means
//! rather than any cause sharing the family. Details are not a stable public
//! code registry; nothing outside this workspace should branch on one.

use intlify_authoring::Detail;

/// The unit's bytes are exactly what its snapshot names, and are not UTF-8.
#[must_use]
pub fn unit_not_text() -> Detail {
    Detail::literal("unit-not-text")
}

/// The host rejected the unit under the grammar its snapshot names.
#[must_use]
pub fn host_syntax_invalid() -> Detail {
    Detail::literal("host-syntax-invalid")
}
