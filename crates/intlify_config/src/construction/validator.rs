// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Draft 7 validator resolver construction validates schemas and bodies
//! with.
//!
//! Design 015 admits one JSON Schema dialect, Draft 7, under a validator that
//! counts its work in schema nodes, `$ref` occurrences, and
//! `(schema node, instance node)` pairs, so that the work never exceeds an
//! admitted bound. Its keywords read values exactly: [`number`] reads JSON
//! numbers as decimals for the numeric keywords, and [`equal`] keys values for
//! `const`, `enum`, and `uniqueItems`. Draft 7 takes the regular expressions
//! of `pattern` and `patternProperties` from ECMA-262, which [`pattern`] reads
//! and matches. Schemas name one another by URI references, which [`uri`]
//! resolves and compares, and a reference's fragment may be a JSON Pointer,
//! which [`pointer`] reads and follows. [`schema`] reads a schema document
//! with these parts: it counts its nodes and references on any JSON, names
//! its resources and anchors, resolves every `$ref` to a node of the document,
//! refuses cycles of in-place applicators, and reads every keyword.
//! [`validation`] validates an instance against a read schema, visiting each
//! instance node once, and counts the pairs it decides.

pub(crate) mod equal;
pub(crate) mod number;
pub(crate) mod pattern;
pub(crate) mod pointer;
pub(crate) mod schema;
pub(crate) mod uri;
pub(crate) mod validation;

#[cfg(test)]
mod differential_tests;
#[cfg(test)]
mod suite_tests;
