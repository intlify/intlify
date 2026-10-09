// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's Intent and reference artifacts: what an inventory's
//! declarations and use sites resolve to under one exact registry.
//!
//! A `message-intent` gives one declaration its Intent ID and the revision
//! its current projection gives, and names the inventory and the registry
//! that establish both. A `message-reference` gives one use site the
//! identities of every declaration it may use, each through the
//! `message-intent` artifact that records it. Neither assigns an identity:
//! both are read off a registry by read-only compilation, and neither
//! changes it.
//!
//! These are the source-analysis and identity handoff only. They do not
//! claim that a message is reachable, approved or deliverable, and the
//! inventory they name keeps the source facts a consumer reads them with.

mod admit;
mod artifact;
mod model;

pub use admit::{admit_intent, admit_reference, AdmittedIntent, AdmittedReference};
pub use artifact::{MessageIntentArtifact, MessageReferenceArtifact};
pub use model::{
    IntentContinuity, IntentFailure, MessageIntentBody, MessageReferenceBody, ReferenceFailure,
    ReferenceTarget,
};
