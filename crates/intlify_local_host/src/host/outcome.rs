// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What a host operation gives back.
//!
//! Design 029 reports each operation's own result and implies no next step:
//! a read publishes nothing, a preparation is not a permit, a confirmation is
//! not a publication. A failure keeps its owner's cause: the binding's state,
//! an authority that could not be established, a denied action, a conflict
//! with the current state, or the identity crate's own failure.

use std::sync::Arc;

use intlify_authoring::{AdmittedInventory, AuthoringArtifactReference, MessageIntentId};
use intlify_authoring_identity::{
    AdmittedRegistry, CompileFailure, IdentityAdmissionFailure, Plan, ReconcileFailure,
    SnapshotFailure, Unresolved,
};

use super::random::RandomnessFailure;
use crate::authority::{AuthorizationFailure, EstablishmentFailure, Principal, UpdateMode};

/// Why the owner binding cannot serve an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// No owner binding is enrolled in this host.
    NotEnrolled,
    /// The owner binding is already enrolled. Enrolling it again would bind
    /// the owner twice.
    AlreadyEnrolled,
    /// The binding has no registry chain yet: initialize it first.
    Uninitialized,
    /// The binding's control state cannot be validated. It is never read as
    /// a new owner or an uninitialized binding.
    Unavailable,
    /// The reference names no snapshot of this chain that the session holds.
    UnknownSnapshot,
}

/// Why a write found the current state other than it was prepared against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    /// The registry the update was prepared against is no longer current.
    StaleBase,
    /// The authority state changed after the session opened: grants or the
    /// development session. The write has to be authorized again.
    AuthorityChanged,
    /// Another initialization made the binding active first.
    AlreadyInitialized,
}

/// Why a host operation gave no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostFailure {
    /// The owner binding cannot serve the operation.
    Blocked(Blocked),
    /// No authority could be established from the host state and the
    /// session's acquisition.
    Establishment(EstablishmentFailure),
    /// The caller is not permitted the operation on these inputs.
    Denied(AuthorizationFailure),
    /// The current state is not the one the write was prepared against.
    /// Nothing changed.
    Conflict(Conflict),
    /// Reconciliation could not plan from the inputs.
    Reconcile(ReconcileFailure),
    /// Compilation could not run on the inputs.
    Compile(CompileFailure),
    /// Fresh bytes could not be drawn for an allocation. No value stands in.
    Randomness(RandomnessFailure),
    /// The result could not be sealed. Nothing became current.
    Unsealable,
    /// The sealed result is not a registry the host's own limits admit.
    /// Nothing became current.
    Unadmissible(IdentityAdmissionFailure<SnapshotFailure>),
    /// The host retains as many generations as its bound allows. It refuses
    /// the write rather than drop history.
    HistoryFull,
}

/// Whether a read gave the current registry or a pinned earlier one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    /// The registry that was current when the session opened. Whether it
    /// still is, only a write finds out.
    Current,
    /// An earlier snapshot of the chain, pinned for reading. It is never
    /// current, and reading it grants nothing about the chain.
    Historical,
}

/// One registry read through a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRead {
    pub(super) registry: Arc<AdmittedRegistry>,
    pub(super) currency: Currency,
}

impl RegistryRead {
    /// Borrow the registry.
    #[must_use]
    pub fn registry(&self) -> &AdmittedRegistry {
        &self.registry
    }

    /// Return whether it was current or pinned.
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
}

/// One update reconciliation planned, held for confirmation and publication.
///
/// A prepared update is not a permit: publishing it authorizes it again,
/// under the authority in force, against the state current at that moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedUpdate {
    pub(super) base: Arc<AdmittedRegistry>,
    pub(super) inventory: AdmittedInventory,
    pub(super) plan: Plan,
}

impl PreparedUpdate {
    /// Borrow the registry the plan was made against.
    #[must_use]
    pub fn base(&self) -> &AdmittedRegistry {
        &self.base
    }

    /// Borrow the inventory the plan was made from.
    #[must_use]
    pub const fn inventory(&self) -> &AdmittedInventory {
        &self.inventory
    }

    /// Borrow the plan: the update, its result and each decision's
    /// eligibility.
    #[must_use]
    pub const fn plan(&self) -> &Plan {
        &self.plan
    }
}

/// What preparing an update gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preparation {
    /// One complete plan, ready to confirm and publish.
    Prepared(Box<PreparedUpdate>),
    /// Nothing to decide: the base stays current, and nothing is published.
    Unchanged,
    /// Some identity is not established. Nothing can be published until
    /// every one is resolved.
    Unresolved(Box<Unresolved>),
}

/// Which write made a generation current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// An empty genesis for an enrolled binding.
    Initialize,
    /// One update, requested in this mode.
    Update(UpdateMode),
}

/// Where a publication lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// In this process's memory only. A publication here is not evidence
    /// that anything was written to disk or would survive a restart.
    InMemory,
}

/// The provenance a host keeps for one publication: who made which state
/// current, from what, under which authority state.
///
/// It records past facts. Nothing in it is an authority, and nothing can be
/// rebuilt from it into one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationRecord {
    pub(super) generation: u64,
    pub(super) authority_revision: u64,
    pub(super) publisher: Principal,
    pub(super) operation: Operation,
    pub(super) base: Option<AuthoringArtifactReference>,
    pub(super) inventory: Option<AuthoringArtifactReference>,
    pub(super) update: Option<AuthoringArtifactReference>,
    pub(super) result: AuthoringArtifactReference,
    pub(super) confirmations: Box<[(Principal, MessageIntentId)]>,
}

impl PublicationRecord {
    /// Return the host generation this publication made current.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Return the revision of the authority state it was authorized under.
    #[must_use]
    pub const fn authority_revision(&self) -> u64 {
        self.authority_revision
    }

    /// Borrow the principal who published.
    #[must_use]
    pub const fn publisher(&self) -> &Principal {
        &self.publisher
    }

    /// Return the operation and, for an update, its mode.
    #[must_use]
    pub const fn operation(&self) -> Operation {
        self.operation
    }

    /// Borrow the base, absent for an initialization.
    #[must_use]
    pub const fn base(&self) -> Option<&AuthoringArtifactReference> {
        self.base.as_ref()
    }

    /// Borrow the inventory the update was planned from, absent for an
    /// initialization.
    #[must_use]
    pub const fn inventory(&self) -> Option<&AuthoringArtifactReference> {
        self.inventory.as_ref()
    }

    /// Borrow the update, absent for an initialization.
    #[must_use]
    pub const fn update(&self) -> Option<&AuthoringArtifactReference> {
        self.update.as_ref()
    }

    /// Borrow the registry made current.
    #[must_use]
    pub const fn result(&self) -> &AuthoringArtifactReference {
        &self.result
    }

    /// Borrow who confirmed each explicit decision, by Intent ID.
    #[must_use]
    pub fn confirmations(&self) -> &[(Principal, MessageIntentId)] {
        &self.confirmations
    }
}

/// One publication: the registry it made current, and where that lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    pub(super) record: PublicationRecord,
    pub(super) durability: Durability,
}

impl Publication {
    /// Borrow its provenance record.
    #[must_use]
    pub const fn record(&self) -> &PublicationRecord {
        &self.record
    }

    /// Return where the publication lives: in memory only, here.
    #[must_use]
    pub const fn durability(&self) -> Durability {
        self.durability
    }
}
