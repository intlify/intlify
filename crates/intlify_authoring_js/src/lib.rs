// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! JavaScript and TypeScript host Producer for design 016, Phase 2.
//!
//! A Producer reads host source and hands `intlify_authoring` what it found,
//! already decoded. What a message means, what its use site must supply, its
//! locale, class and revision are all decided there, so the same text never
//! means different things depending on which host found it. What this crate
//! owns is the part specific to JavaScript: which bytes a unit is, which
//! grammar reads them, and what a literal decodes to and from where.
//!
//! An invocation runs in two steps. [`admit_units`] checks everything the
//! caller supplied before anything is parsed: the context and profile pins,
//! each unit's owner and grammar, its bytes against its snapshot, and the
//! declared scope. [`analyze_unit`] then reads one admitted unit, parsing it
//! exactly once under the grammar its snapshot names.
//!
//! The grammar is always the caller's choice. There is no API that picks one
//! from a file name or suffix, and a unit that fails to parse is not retried
//! under another goal.
//!
//! No file, package, or network is retrieved, and no host code is evaluated.
//! As in `intlify_authoring`, the only context kind this phase admits is the
//! test context: one claiming a production kind is refused before anything is
//! read, because production admission needs checked 015 inputs that later
//! phases supply.

mod analysis;
// Decoding a message literal belongs to the recognizers that find one; until
// they exist, only the decoder's own tests reach it.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the explicit-form recognizers are its callers")
)]
mod cooked;
pub mod detail;
mod failure;
mod grammar;
mod limits;
mod parse;
mod profile;
mod unit;
mod workspace;

pub use analysis::{analyze_unit, UnitAnalysis, UnitWork};
pub use failure::ProducerFailure;
pub use grammar::{Grammar, GRAMMAR_REVISION};
pub use limits::{JsAuthoringLimits, JsLimitKind, JsLimitsError};
pub use profile::{JsAuthoringProfile, AUTHORING_PROFILE_IDENTITY, AUTHORING_PROFILE_REVISION};
pub use unit::{admit_units, AdmittedUnit, SourceUnit, UnitMember};
pub use workspace::JsAnalysisWorkspace;
