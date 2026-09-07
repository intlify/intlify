// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Workspace-internal, host-independent configuration foundations.
//!
//! This crate contains the CLI's JSON compatibility decoder, deterministic
//! schema helpers, and private structural authoring types. It does not expose a
//! project-profile resolver or claim Profile Specification revision "0" support.
//!
//! Discovery, file I/O, JSONC adaptation, and product diagnostics stay with their
//! callers. Decoded values own storage; no mutable global or shared scratch is
//! required. This crate is unpublished, and its workspace helpers are not a
//! reserved product API.
//!
//! Incomplete authoring types are not available to consumers:
//! ```compile_fail
//! use intlify_config::model::IntlifyConfig;
//! ```
//! Test reference encodings are absent from ordinary builds:
//! ```compile_fail
//! use intlify_config::fixtures::FixtureConfig;
//! ```

pub mod json;
pub mod location;
pub mod schema;

// The production entry will use these private types after structural admission.
// Reference encodings remain external type parameters, not placeholder artifacts.
#[allow(dead_code)]
mod model;

// Separate from the compatibility decoder: strict 015 entry is still internal.
#[allow(dead_code)]
mod input_limits;
#[allow(dead_code)]
mod materialize;
#[allow(dead_code)]
mod structural;

#[cfg(any(test, feature = "benchmark"))]
#[allow(dead_code)]
mod benchmark;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod materialize_tests;
#[cfg(test)]
mod model_tests;
