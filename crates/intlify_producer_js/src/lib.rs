// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Deterministic JavaScript and TypeScript message-reference extraction.
//!
//! The crate accepts host-grouped physical source snapshots and scans each
//! physical group once under an exact suffix profile. Fixed-goal sources use
//! one parse attempt; bounded-unambiguous sources may retry the same bytes once
//! as script after a module grammar rejection. It recognizes only explicitly
//! configured static callees and emits checked
//! [`intlify_contract::MessageReferenceArtifact`] values. It is reentrant and
//! safe for caller-owned parallel scheduling. Filesystem discovery, physical
//! identity, snapshots, completeness mapping, and project linker policy remain
//! outside this crate.

mod cache;
mod error;
mod frontend;
mod group;
mod key;
mod producer;
mod profile;
mod provenance;
mod recognizer;
mod static_eval;

pub use cache::{JsProducerCache, JsProducerCacheIoError, JsProducerCacheKey};
pub use error::{
    JsProducerError, JsProducerFailure, JsProducerFailureReason, JsProducerFailureStage,
};
pub use frontend::{scan_source, scan_source_with_cancellation, JsSourceScan, SOURCE_BYTES_LIMIT};
pub use group::{JsPhysicalSourceGroup, JsPhysicalSourceGroupError};
pub use producer::{
    produce_reference_artifacts, produce_reference_artifacts_with_cache,
    produce_reference_artifacts_with_cache_and_cancellation,
    produce_reference_artifacts_with_cancellation, JsProducerOutcome, SOURCE_BYTES_TOTAL_LIMIT,
    SOURCE_GROUPS_LIMIT,
};
pub use profile::{
    JsGrammarProfile, JsSelectedSourceGoal, JsSourceGoal, JsSourceProfile, JsSourceProfileError,
};
pub use provenance::{js_producer_identity, JS_PRODUCER_ID, JS_PRODUCER_REVISION};
pub use recognizer::{
    JsKeySyntax, JsRecognizerBinding, JsRecognizerCallKind, JsRecognizerConfigurationError,
    JsRecognizerSet,
};
