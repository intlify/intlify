// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Deterministic JavaScript and TypeScript message-reference extraction.
//!
//! The crate admits one exact source snapshot, parses it under an exact suffix
//! profile, recognizes only explicitly configured static callees, and returns
//! checked [`intlify_contract::MessageReference`] values. It is reentrant and
//! keeps scheduling, physical-source grouping, artifact identity, caching, and
//! project policy outside the frontend.

mod error;
mod frontend;
mod key;
mod profile;
mod recognizer;
mod static_eval;

pub use error::{
    JsProducerError, JsProducerFailure, JsProducerFailureReason, JsProducerFailureStage,
};
pub use frontend::{scan_source, scan_source_with_cancellation, JsSourceScan, SOURCE_BYTES_LIMIT};
pub use profile::{
    JsGrammarProfile, JsSelectedSourceGoal, JsSourceGoal, JsSourceProfile, JsSourceProfileError,
};
pub use recognizer::{
    JsKeySyntax, JsRecognizerBinding, JsRecognizerCallKind, JsRecognizerConfigurationError,
    JsRecognizerSet,
};
