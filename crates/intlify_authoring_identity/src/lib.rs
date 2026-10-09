// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Persistent Intent identity for design 016, Phase 3.
//!
//! `intlify_authoring` decides what a message is and records what one
//! analysis found as an `authoring-inventory`, and it assigns no identity.
//! This crate adds persistent identity on top of that record: the registry an
//! owner's Intent IDs live in, the transitions that change it, the evidence
//! that lets an ID continue across an edit or a move, and the read-only
//! compilation that resolves an inventory against a registry.
//!
//! It never generates an identity's value, reads a file, or publishes a
//! registry. Design 017 requires a new value to come from operating-system
//! randomness on an authorized update host, and publication to happen through
//! that host's exact-base check, so both stay outside this crate.
//!
//! So far it holds the registry identity. A registry identity names a chain,
//! not a message lineage, so it is not an Intent ID even where the two spell
//! the same digits:
//!
//! ```compile_fail
//! use intlify_authoring::{MessageIntentId, OwnerIdentity, OwnerKind};
//! use intlify_authoring_identity::RegistryIdentity;
//!
//! let value = "0".repeat(32);
//! let owner = OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap();
//! let intent = MessageIntentId::retained(owner, &value).unwrap();
//! let registry = RegistryIdentity::retained(&value).unwrap();
//! assert!(registry != intent);
//! ```

mod admission;
mod continuity;
mod id;
mod intent;
mod limits;
mod reconcile;
mod registry;
pub mod schema;
mod workspace;

pub use admission::IdentityAdmissionFailure;
pub use continuity::{
    fate, inserted, is_edit_replay_profile, replay as replay_edit, verify_bases, BasisGap,
    BasisReport, BasisVerdict, ContinuityFailure, ContinuityInputs, EditSetFailure, PreviousUpdate,
    RangeFate, ReplayGap, RetainedSourceFailure, RetainedSources, EDIT_REPLAY_PROFILE,
    EDIT_REPLAY_REVISION,
};
pub use id::RegistryIdentity;
pub use intent::{
    admit_intent, admit_reference, AdmittedIntent, AdmittedReference, IntentContinuity,
    IntentFailure, MessageIntentArtifact, MessageIntentBody, MessageReferenceArtifact,
    MessageReferenceBody, ReferenceFailure, ReferenceTarget,
};
pub use limits::{IdentityLimitKind, IdentityLimits};
pub use reconcile::{
    detail, reconcile, reconcile_with_cancellation, CandidateFailure, Classification, Conflict,
    DeclarationClass, Eligibility, EntryClass, ExplicitDecision, NotExplicit, Plan,
    ReconcileFailure, ReconcileInputs, Reconciliation, Unresolved,
};
pub use registry::{
    admit_registry, admit_update, apply, replay, verify_history, AdmittedRegistry, AdmittedUpdate,
    Allocation, AllocationBasis, Anchor, CompleteAbsence, ConfirmedNew, Continuation,
    ContinuationBasis, EntryState, ExplicitBasis, HistoryFailure, HistoryOutcome, IdentityDecision,
    IntentRegistrySnapshot, IntentRegistryUpdate, LineageKind, LineageLink, RegistryArtifact,
    RegistryEntry, RegistryUpdateArtifact, Replacement, ReplayFailure, Restoration,
    RetainedHistory, Retirement, SnapshotFailure, SourceEdit, Transition, TransitionFailure,
    UnchangedSnapshot, UpdateFailure, VerifiedEdit,
};
pub use workspace::{IdentityCapacities, IdentityWorkspace};
