// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's `authoring-inventory` representation and its admission.
//!
//! An inventory is what a host Producer hands on: the declared scope of one
//! analysis, what happened to each unit in it, and the declarations,
//! references and exclusions it found. This module fixes that contract before
//! any Producer exists, so that a Producer's fixtures are checked against the
//! representation rather than the representation being whatever the Producer
//! happened to emit.
//!
//! Three questions are kept apart because they have different answers:
//! whether a record is well formed ([`AuthoringInventory::validate`]), whether
//! it is admitted under a context ([`admit_inventory`]), and whether it is
//! complete checked input a build may rely on
//! ([`AuthoringInventory::is_complete_checked`]).

mod admit;
mod artifact;
mod model;

#[cfg(feature = "test-context")]
pub(crate) use admit::{admit, Entry};
pub use admit::{admit_inventory, AdmissionFailure, AdmittedInventory, SourceBytes};
pub use artifact::{
    ArtifactKind, ArtifactRelation, AuthoringArtifactReference, InventoryArtifact,
    ARTIFACT_INTEGRITY_DOMAIN, ARTIFACT_SCHEMA_REVISION, AUTHORING_SPECIFICATION_IDENTITY,
    AUTHORING_SPECIFICATION_REVISION,
};
pub use model::{
    AuthoringInventory, Completeness, Exclusion, InventoryBuilder, InventoryFailure,
    ReferenceFacts, UnitOutcome, UnitResult,
};
