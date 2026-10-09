// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's registry history: the snapshots an owner's Intent IDs live in
//! and the updates between them.
//!
//! A chain starts with a genesis, which has no history and no entries. Every
//! later snapshot names the snapshot it came from and the update that produced
//! it, and an update names its exact base and the inventory it was planned
//! from. Nothing names its own result, so the history has no cycle and each
//! artifact's digest can be computed before the next one exists.
//!
//! This module represents and admits both kinds, applies an update to its
//! base under 017's closed transition table, and replays a chain from an
//! anchor the host names. It does not decide whether a basis holds: a
//! `verified-edit` or `confirmed-new` that applies and replays is still only
//! a claim until the continuity checks test it.

mod admit;
mod apply;
mod artifact;
#[cfg(test)]
pub(crate) mod fixtures;
mod history;
mod snapshot;
mod update;

pub(crate) use admit::admit_update_artifact;
pub use admit::{admit_registry, admit_update, AdmittedRegistry, AdmittedUpdate};
pub use apply::{apply, Transition, TransitionFailure};
pub(crate) use apply::{check_base_state, check_pairing, declares};
pub use artifact::{RegistryArtifact, RegistryUpdateArtifact};
pub use history::{
    replay, verify_history, Anchor, HistoryFailure, HistoryOutcome, ReplayFailure, RetainedHistory,
};
pub use snapshot::{EntryState, IntentRegistrySnapshot, RegistryEntry, SnapshotFailure};
pub(crate) use update::validate_edit;
pub use update::{
    Allocation, AllocationBasis, CompleteAbsence, ConfirmedNew, Continuation, ContinuationBasis,
    ExplicitBasis, IdentityDecision, IntentRegistryUpdate, LineageKind, LineageLink, Replacement,
    Restoration, Retirement, SourceEdit, UnchangedSnapshot, UpdateFailure, VerifiedEdit,
};
