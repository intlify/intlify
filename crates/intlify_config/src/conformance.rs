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
//! index names by its file digest, and every manifest against the Case Schema
//! ([`case`], [`runner`]), with the fixtures the cases name and the bodies
//! they reference ([`fixture`]). Whatever stops a suite from opening is a
//! Conformance harness failure, never a construction failure, an
//! input-admission failure, a Finding, or a resolver outcome. A suite that
//! opened runs its cases: each construction case constructs from its
//! fixture's input and compares what construction does with what the case
//! expects ([`construction`]), and each input admission case of a boundary
//! kind evaluates its boundary on its fixture's input under the construction
//! the case names ([`admission`]).
//!
//! This crate checks in its suite below `conformance/`, with the index at
//! `suite-index-v0.json`, and writes its construction cases
//! ([`construction_cases`]) and input admission cases
//! ([`admission_cases`]). The runner reads files, so like the benchmark it is
//! no part of a resolution: only tests and the developer examples build it.

pub(crate) mod admission;
pub(crate) mod admission_cases;
pub(crate) mod case;
pub(crate) mod construction;
pub(crate) mod construction_cases;
pub(crate) mod fixture;
pub(crate) mod index;
pub(crate) mod resolution;
pub(crate) mod resolver_cases;
pub(crate) mod runner;

/// The developer entry points: write the committed suite, and open a suite at
/// an explicit Suite Index path and run its cases.
#[cfg(feature = "conformance")]
pub mod support {
    pub use super::case::schema_digest as case_schema_digest;
    pub use super::index::{artifacts, schema_digest, Member, SuiteFile};
    pub use super::runner::{open, HarnessFailure, Opened, Outcome};
}
