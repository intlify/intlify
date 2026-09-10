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
//! Formal references are internal structural types, not an artifact API:
//! ```compile_fail
//! use intlify_config::references::PolicyReference;
//! ```
//! The experimental locale provider is not a public configuration API:
//! ```compile_fail
//! use intlify_config::locale::Canonicalizer;
//! ```
//! The minimum locale result is not a consumer Profile or serializable artifact:
//! ```compile_fail
//! use intlify_config::locale::core::Core;
//! ```
//! The vertical-slice harness is test-only, not a production resolver entry:
//! ```compile_fail
//! use intlify_config::minimum_tests::harness::FixtureRunner;
//! ```
//! The optional observation facade cannot expose capture authority:
//! ```compile_fail
//! use intlify_config::measurement::RecordedRun;
//! ```
//! Completed observations cannot be constructed from arbitrary caller state:
//! ```compile_fail
//! let fabricated = intlify_config::measurement::CompletedObservation {};
//! ```

pub mod json;
pub mod location;
pub mod schema;

// An unstable, display-only bridge for the opt-in contributor example. No
// private authoring/core types or production resolver API become public.
#[cfg(feature = "dev-example")]
#[doc(hidden)]
pub use structural::example as example_support;

/// Non-default developer measurements only; no configuration resolver or Profile API.
#[cfg(feature = "benchmark")]
pub use benchmark::facade as measurement;

// The production entry will use these private types after structural admission.
// Formal 017 reference encodings remain distinct from test-only instantiations.
#[allow(dead_code)]
mod model;
#[allow(dead_code)]
mod references;

// Separate from the compatibility decoder: strict 015 entry is still internal.
#[allow(dead_code)]
mod input_limits;
#[allow(dead_code)]
mod materialize;
#[allow(dead_code)]
mod structural;

// Data-free internal provider boundary; no product locale API is reserved.
#[allow(dead_code)]
mod locale;

#[cfg(any(test, feature = "benchmark"))]
#[allow(dead_code)]
mod benchmark;

#[cfg(any(test, feature = "benchmark"))]
#[allow(dead_code)]
mod profile_fixtures;

#[cfg(test)]
#[allow(dead_code)]
mod fixtures;
#[cfg(test)]
mod materialize_tests;
#[cfg(test)]
mod minimum_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod reference_schema_tests;
