// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Draft 7 validator resolver construction validates schemas and bodies
//! with.
//!
//! Design 015 admits one JSON Schema dialect, Draft 7, under a validator that
//! counts its work in schema nodes, `$ref` occurrences, and
//! `(schema node, instance node)` pairs, so that the work never exceeds an
//! admitted bound. Draft 7 takes the regular expressions of `pattern` and
//! `patternProperties` from ECMA-262; [`pattern`] reads and matches them.

pub(crate) mod pattern;
