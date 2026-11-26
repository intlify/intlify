// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Project Profile Resolver Conformance Suite: its Suite Index,
//! and the runner that opens a suite from it.
//!
//! The suite root is the directory that holds the Suite Index, and the runner
//! is handed the index's path: it never searches for an index or assumes
//! where one is (D15:2858). Its trust root is the revision-`"0"` Suite Index
//! Schema, whose identity and file digest it pins ([`index`]). It checks that
//! schema, validates the index against it, compares what the index pins with
//! the artifacts this crate is compiled from, and verifies every body the
//! index names by its file digest ([`runner`]). Whatever stops a suite from
//! opening is a Conformance harness failure, never a construction failure, an
//! input-admission failure, a Finding, or a resolver outcome.
//!
//! This crate checks in its suite below `conformance/`, with the index at
//! `suite-index-v0.json`. The runner reads files, so like the benchmark it is
//! no part of a resolution: only tests and the developer examples build it.

pub(crate) mod index;
pub(crate) mod runner;

/// The developer entry points: write the committed Suite Index and its
/// schema, and open a suite at an explicit Suite Index path.
#[cfg(feature = "conformance")]
pub mod support {
    pub use super::index::{artifacts, schema_digest, Member};
    pub use super::runner::{open, HarnessFailure, Opened};
}
