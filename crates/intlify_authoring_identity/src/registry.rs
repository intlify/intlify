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
//! This module represents and admits both kinds. It does not apply an update,
//! replay a chain, or decide whether a basis holds.

mod admit;
mod artifact;
mod snapshot;
mod update;

pub use admit::{
    admit_registry, admit_update, AdmittedRegistry, AdmittedUpdate, RegistryAdmissionFailure,
};
pub use artifact::{RegistryArtifact, RegistryUpdateArtifact};
pub use snapshot::{EntryState, IntentRegistrySnapshot, RegistryEntry, SnapshotFailure};
pub use update::{
    Allocation, AllocationBasis, CompleteAbsence, ConfirmedNew, Continuation, ContinuationBasis,
    ExplicitBasis, IdentityDecision, IntentRegistryUpdate, LineageKind, LineageLink, Replacement,
    Restoration, Retirement, SourceEdit, UnchangedSnapshot, UpdateFailure, VerifiedEdit,
};
