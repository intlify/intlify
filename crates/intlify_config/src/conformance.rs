// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Project Profile Resolver Conformance Suite: its Suite Index.
//!
//! The suite root is the directory that holds the Suite Index, and every path
//! the index names is relative to it (D15:2858). The index pins every
//! artifact of the suite, and its schema is the trust root of the runner that
//! opens a suite (D15:3029). This crate writes both ([`index`]).
//!
//! This crate checks in its suite below `conformance/`, with the index at
//! `suite-index-v0.json`. Only tests and the developer examples build this
//! module.

pub(crate) mod index;

/// The developer entry point: write the committed Suite Index and its schema.
#[cfg(feature = "conformance")]
pub mod support {
    pub use super::index::{artifacts, schema_digest};
}
