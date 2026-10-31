// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's resolver construction, starting from its built-in root.
//!
//! The Resolver Construction Root Package is the finite root every
//! construction is validated under. Its five members are written from the
//! values in these modules and checked in below `conformance/construction/`;
//! the implementation compiles them in and checks, on first use, that they are
//! the package whose two digests it pins.

pub(crate) mod admission;
pub(crate) mod envelope;
pub(crate) mod failure;
pub(crate) mod meta_schema;
pub(crate) mod root;
pub(crate) mod rules;
pub(crate) mod validator;

#[cfg(any(test, feature = "vectors"))]
pub mod generate;
