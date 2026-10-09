// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What reconciliation gives back.
//!
//! A reconciliation either changes nothing, plans one complete update, or
//! reports why it cannot. A plan is the update and the state it produces,
//! applied and checked but neither published nor turned into a registry
//! artifact. A report carries the diagnostics and the classification that
//! led to them; it never carries a partial plan.

use intlify_authoring::{Diagnostic, MessageIntentId, Occurrence};

use crate::continuity::{BasisGap, ContinuityFailure};
use crate::limits::IdentityLimitKind;
use crate::registry::{AdmittedUpdate, IntentRegistrySnapshot, TransitionFailure, UpdateFailure};

/// Why two inputs cannot both hold.
///
/// A conflict is about the inputs themselves, not about missing evidence:
/// supplying more evidence does not resolve it, a different decision does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Conflict {
    /// More than one identity claims one current declaration, or one ID is
    /// decided twice.
    CompetingClaim,
    /// An explicit allocation names an ID the base already holds.
    Collision,
    /// An explicit continuation names a retired ID, which only an explicit
    /// restore brings back.
    RetiredReuse,
    /// An explicit decision names another owner's ID.
    ForeignOwner,
    /// An explicit decision was made for another base or inventory, or does
    /// not fit this one.
    BaseMismatch,
}

/// What reconciliation found for one current declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationClass {
    /// An active entry holds exactly this declaration and keeps it.
    Retained(MessageIntentId),
    /// A verified edit carries an active entry onto this declaration.
    Continued(MessageIntentId),
    /// An explicit decision gives this declaration an identity, pending
    /// confirmation.
    Explicit(MessageIntentId),
    /// The evidence shows this declaration is new.
    New,
    /// The evidence does not show where this declaration's identity comes
    /// from.
    Unresolved(BasisGap),
    /// Inputs conflict about this declaration.
    Conflict(Conflict),
}

/// What reconciliation found for one active entry whose declaration the
/// inventory no longer declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryClass {
    /// A verified edit carries the entry onto this current declaration.
    Continued(Occurrence),
    /// An explicit decision moves the entry.
    Explicit,
    /// The evidence shows the declaration is gone.
    Absent,
    /// A partial inventory does not show what became of it, so it is kept
    /// as it is.
    Kept,
    /// The evidence does not show where the declaration went.
    Unresolved(BasisGap),
    /// Inputs conflict about where the declaration went.
    Conflict(Conflict),
}

/// How reconciliation classified the inventory against the base, for
/// inspection.
///
/// The classification is established independently of any decision the
/// host might prefer: it is what the evidence shows, and the reason
/// wherever it shows nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub(super) declarations: Box<[(Occurrence, DeclarationClass)]>,
    pub(super) entries: Box<[(MessageIntentId, EntryClass)]>,
}

impl Classification {
    /// Borrow each current declaration's class, in canonical order.
    #[must_use]
    pub fn declarations(&self) -> &[(Occurrence, DeclarationClass)] {
        &self.declarations
    }

    /// Borrow the class of each active entry whose declaration the inventory
    /// no longer declares, in Intent ID order.
    #[must_use]
    pub fn entries(&self) -> &[(MessageIntentId, EntryClass)] {
        &self.entries
    }
}

/// Whether one planned decision may be accepted without asking anyone.
///
/// Design 016 lets an enabled development auto-update accept a verified
/// continuation, a confirmed new allocation and a complete-absence
/// retirement. An explicit choice and a restore always need the host's
/// confirmation, mode or no mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eligibility {
    /// The evidence proves the decision.
    Automatic,
    /// The decision is an explicit choice that has to be confirmed.
    RequiresConfirmation,
}

/// One planned update, applied to its base and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub(super) update: AdmittedUpdate,
    pub(super) result: IntentRegistrySnapshot,
    pub(super) eligibility: Box<[(MessageIntentId, Eligibility)]>,
    pub(super) classification: Classification,
}

impl Plan {
    /// Borrow the update, sealed and admitted.
    ///
    /// Sealing names the update by its digest, which the result has to
    /// carry. It publishes nothing.
    #[must_use]
    pub const fn update(&self) -> &AdmittedUpdate {
        &self.update
    }

    /// Borrow the registry state the update produces. It is not sealed.
    #[must_use]
    pub const fn result(&self) -> &IntentRegistrySnapshot {
        &self.result
    }

    /// Borrow each decision's eligibility, in decision order.
    #[must_use]
    pub fn eligibility(&self) -> &[(MessageIntentId, Eligibility)] {
        &self.eligibility
    }

    /// Return whether any decision needs the host's confirmation.
    #[must_use]
    pub fn requires_confirmation(&self) -> bool {
        self.eligibility
            .iter()
            .any(|(_, eligibility)| *eligibility == Eligibility::RequiresConfirmation)
    }

    /// Borrow the classification the plan was made from.
    #[must_use]
    pub const fn classification(&self) -> &Classification {
        &self.classification
    }
}

/// Why no plan was made: the diagnostics, and the classification behind
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub(super) diagnostics: Box<[Diagnostic]>,
    pub(super) classification: Classification,
}

impl Unresolved {
    /// Borrow the diagnostics, in 016's reporting order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Borrow the classification.
    #[must_use]
    pub const fn classification(&self) -> &Classification {
        &self.classification
    }
}

/// What reconciling an inventory against a base gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciliation {
    /// Every declaration keeps its entry and nothing else is decided. 017
    /// records no new state: the base stays current.
    Unchanged,
    /// One complete update, ready for the host to confirm and publish.
    Planned(Box<Plan>),
    /// Some identity is not established, or inputs conflict. No plan is made
    /// until every one is resolved.
    Unresolved(Box<Unresolved>),
}

/// Why the allocation candidates cannot serve the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateFailure {
    /// A candidate is another owner's ID.
    ForeignOwner,
    /// One candidate is offered twice.
    Duplicate,
    /// A candidate the plan would use is an ID the base already holds,
    /// active or retired, or one an explicit decision names. A new candidate
    /// is needed; none is made up here.
    Collision(MessageIntentId),
    /// Fewer candidates than new declarations.
    Exhausted {
        /// The candidates the plan needs.
        needed: usize,
    },
}

/// Why reconciliation stopped without a result.
///
/// These are operational: the inputs cannot be planned from at all, or the
/// caller stopped the work. None of them says anything about identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileFailure {
    /// The inventory cannot be planned from against this base: another
    /// owner or scope, or a unit that was not checked.
    Pairing(TransitionFailure),
    /// The host's continuity evidence is not admissible.
    Evidence(ContinuityFailure),
    /// The allocation candidates cannot serve the plan.
    Candidates(CandidateFailure),
    /// The planned update is not well formed, such as through a lineage
    /// link supplied in the inputs.
    Update(UpdateFailure),
    /// The planned update does not apply to its base, such as through a
    /// lineage link that does not resolve.
    Transition(TransitionFailure),
    /// The planned update could not be sealed, such as when its encoding
    /// exceeds the encoder's capacity.
    Unsealable,
    /// A named bound was exceeded.
    Limit(IdentityLimitKind),
    /// The caller's probe asked this reconciliation to stop. Nothing is
    /// established about the declarations it did reach.
    Cancelled,
}
