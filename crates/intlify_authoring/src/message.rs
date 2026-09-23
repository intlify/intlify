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
//! A host decodes its own escapes before this crate sees the text, so a
//! complete map from emitted MF2 back to host source is the composition of the
//! host's map with this crate's. That composition lives here rather than in
//! each Producer, because every host needs the same rules about what a
//! correspondence is allowed to claim.
//!
//! The pipeline follows 012: parse first; only diagnostic-free parsing permits
//! semantic-model construction; only a constructed model permits parser-owned
//! semantic validation. Parser and semantic diagnostics keep their own codes,
//! and an invariant failure is an operational failure rather than an ordinary
//! malformed-message diagnostic.

mod analysis;
mod literal;
mod mapping;

pub use analysis::{analyze_message, MessageAnalysis, MessageFacts, MessageFailure, MessageInput};
pub use literal::ExtractionSegment;
pub use mapping::{compose_extraction_map, validate_input_map, InputSegment, MappingError};

// The harness measures the encoder at its own boundary, so it needs the entry
// point `analyze_message` calls rather than the combined operation.
#[cfg(feature = "benchmark")]
pub(crate) use literal::{encode, LiteralFailure};
