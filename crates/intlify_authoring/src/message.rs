// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Literal encoding, MF2 parser handoff, and external parameter requirements.
//!
//! Ordinary UI text has literal semantics: it is encoded as an MF2 quoted
//! pattern using shared escaping rules and then analyzed by exactly the same
//! parser and semantic pipeline as explicitly authored MF2. Braces in displayed
//! text therefore never become interpolation merely because an application
//! enabled localization.
//!
//! The pipeline follows 012: parse first; only diagnostic-free parsing permits
//! semantic-model construction; only a constructed model permits parser-owned
//! semantic validation. Parser and semantic diagnostics keep their own codes,
//! and an invariant failure is an operational failure rather than an ordinary
//! malformed-message diagnostic.

mod analysis;
mod literal;

pub use analysis::{analyze_message, MessageAnalysis, MessageFacts, MessageFailure, MessageInput};
pub use literal::ExtractionSegment;

// The harness measures the encoder at its own boundary, so it needs the entry
// point `analyze_message` calls rather than the combined operation.
#[cfg(feature = "benchmark")]
pub(crate) use literal::{encode, LiteralFailure};
