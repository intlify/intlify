// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Applying one update to its exact base: design 017's closed entry
//! transition.
//!
//! An update applies as a whole or not at all. Each decision has to find its
//! base entry in the state the transition table requires, every declaration
//! of the inventory has to end up with exactly one identity, and lineage
//! links have to name IDs that exist where they say. One failure leaves
//! nothing applied: there is no result with the offending entry dropped.
//!
//! What this checks is the transition, not the bases. A `verified-edit` is
//! not replayed here and a `confirmed-new` is not proven; the continuity
//! checks do that. An update whose bases do not hold can still apply, and its
//! result can still replay exactly, so a host never publishes on this check
//! alone.

use intlify_authoring::{
    AdmittedInventory, AuthoringInventory, Completeness, MessageIntentId, Occurrence, UnitOutcome,
};

use super::admit::{AdmittedRegistry, AdmittedUpdate};
use super::snapshot::{
    same_declaration_order, EntryState, IntentRegistrySnapshot, RegistryEntry, SnapshotFailure,
};
use super::update::IdentityDecision;

/// What applying an update to its base gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// The update has no decisions and no lineage links. 017 records no new
    /// state for it: the base is reused as it is.
    Unchanged,
    /// The state the update produces from its base. It is not sealed, and
    /// producing it publishes nothing.
    Applied(Box<IntentRegistrySnapshot>),
}

/// Why an update does not apply to a base.
///
/// Each variant names one rule, so a caller can tell a plan made against
/// another base from one whose decisions do not fit the base it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionFailure {
    /// The update was planned against another base.
    BaseMismatch,
    /// The update was planned from another inventory.
    InventoryMismatch,
    /// The update or the inventory belongs to an owner other than the base's.
    OwnerMismatch,
    /// The inventory declares another scope than the base's.
    ScopeMismatch,
    /// A unit of the inventory was not checked. A failed or blocked unit is
    /// not evidence of anything, so no update is planned from it.
    UncheckedUnit,
    /// A `continue` or `retire` names an ID with no active entry in the base.
    NotActive,
    /// An `allocate` names an ID the base already holds, active or retired.
    /// Retired IDs are never reused.
    AlreadyAllocated,
    /// A `restore` names an ID with no retired entry in the base.
    NotRetired,
    /// A decision's base declaration is not exactly the one its entry holds.
    FromMismatch,
    /// A `retire` comes with a partial inventory. Only a complete inventory
    /// can show that a declaration is gone.
    RetirementFromPartialInventory,
    /// A decision gives an identity to something that is not a declaration of
    /// the inventory.
    UnknownDeclaration,
    /// A decision gives an identity to a declaration an unchanged active
    /// entry already holds.
    DeclarationTaken,
    /// A declaration of the inventory has no identity after the update: no
    /// unchanged active entry holds it, and no decision gives it one.
    Uncovered,
    /// A lineage link names a predecessor the base does not hold, or a
    /// successor the result does not hold.
    UnresolvedLink,
    /// The result is not a well-formed snapshot.
    Result(SnapshotFailure),
}

/// Apply one update to its exact base, under the inventory it was planned
/// from.
///
/// The base, the update and the inventory are admitted artifacts. The update
/// has to name exactly this base and this inventory, and all three have to
/// agree on the owner, and the base and the inventory on the scope.
pub fn apply(
    base: &AdmittedRegistry,
    update: &AdmittedUpdate,
    inventory: &AdmittedInventory,
) -> Result<Transition, TransitionFailure> {
    let snapshot = base.snapshot();
    let plan = update.update();
    let current = inventory.inventory();
    if plan.base() != &base.reference() {
        return Err(TransitionFailure::BaseMismatch);
    }
    if plan.inventory() != &inventory.reference() {
        return Err(TransitionFailure::InventoryMismatch);
    }
    if plan.owner() != snapshot.owner() || current.owner() != snapshot.owner() {
        return Err(TransitionFailure::OwnerMismatch);
    }
    if current.scope() != snapshot.scope() {
        return Err(TransitionFailure::ScopeMismatch);
    }
    if current
        .units()
        .iter()
        .any(|unit| unit.outcome() != UnitOutcome::Checked)
    {
        return Err(TransitionFailure::UncheckedUnit);
    }

    // Decisions are in Intent ID order with one per ID, so whether an ID is
    // decided is one search.
    let decided = |id: &MessageIntentId| {
        plan.decisions()
            .binary_search_by(|decision| decision.intent_id().cmp(id))
            .is_ok()
    };
    for decision in plan.decisions() {
        check_base_state(decision, snapshot, current)?;
        if let Some(to) = decision.to() {
            if !declares(current, to) {
                return Err(TransitionFailure::UnknownDeclaration);
            }
            // A declaration held by an entry this update also decides is
            // being freed or kept by that decision; one held by an entry the
            // update leaves alone is not free.
            if let Some(holder) = base.active_entry(to) {
                if holder.intent_id() != decision.intent_id() && !decided(holder.intent_id()) {
                    return Err(TransitionFailure::DeclarationTaken);
                }
            }
        }
    }

    let mut targets: Vec<&Occurrence> = plan
        .decisions()
        .iter()
        .filter_map(IdentityDecision::to)
        .collect();
    targets.sort_unstable_by(|left, right| same_declaration_order(left, right));
    for facts in current.declarations() {
        let declaration = facts.occurrence();
        let given = targets
            .binary_search_by(|target| same_declaration_order(target, declaration))
            .is_ok();
        let kept = base
            .active_entry(declaration)
            .is_some_and(|entry| !decided(entry.intent_id()));
        if !given && !kept {
            return Err(TransitionFailure::Uncovered);
        }
    }

    // The result holds every base entry and every allocated ID, so a
    // successor resolves there exactly when the base holds it or a decision
    // names it.
    for link in plan.lineage_links() {
        if link
            .predecessors()
            .iter()
            .any(|id| snapshot.entry(id).is_none())
            || link
                .successors()
                .iter()
                .any(|id| snapshot.entry(id).is_none() && !decided(id))
        {
            return Err(TransitionFailure::UnresolvedLink);
        }
    }

    if plan.is_empty() {
        return Ok(Transition::Unchanged);
    }
    let mut entries: Vec<RegistryEntry> = snapshot
        .entries()
        .iter()
        .filter(|entry| !decided(entry.intent_id()))
        .cloned()
        .collect();
    entries.extend(plan.decisions().iter().map(next_entry));
    IntentRegistrySnapshot::successor(base.artifact(), update.artifact(), entries)
        .map(|result| Transition::Applied(Box::new(result)))
        .map_err(TransitionFailure::Result)
}

/// Check one decision against the base entry it names.
fn check_base_state(
    decision: &IdentityDecision,
    snapshot: &IntentRegistrySnapshot,
    current: &AuthoringInventory,
) -> Result<(), TransitionFailure> {
    let entry = snapshot.entry(decision.intent_id());
    let holding = |state: EntryState, refused: TransitionFailure| {
        let entry = entry
            .filter(|entry| entry.state() == state)
            .ok_or(refused)?;
        if Some(entry.declaration()) != decision.from() {
            return Err(TransitionFailure::FromMismatch);
        }
        Ok(())
    };
    match decision {
        IdentityDecision::Allocate(_) if entry.is_some() => {
            Err(TransitionFailure::AlreadyAllocated)
        }
        IdentityDecision::Allocate(_) => Ok(()),
        IdentityDecision::Retire(_) if current.completeness() != Completeness::Complete => {
            Err(TransitionFailure::RetirementFromPartialInventory)
        }
        IdentityDecision::Continue(_) | IdentityDecision::Retire(_) => {
            holding(EntryState::Active, TransitionFailure::NotActive)
        }
        IdentityDecision::Restore(_) => holding(EntryState::Retired, TransitionFailure::NotRetired),
    }
}

/// The entry a decision leaves for its ID.
fn next_entry(decision: &IdentityDecision) -> RegistryEntry {
    let id = decision.intent_id().clone();
    match decision {
        IdentityDecision::Continue(decision) => {
            RegistryEntry::new(id, EntryState::Active, decision.to().clone())
        }
        IdentityDecision::Allocate(decision) => {
            RegistryEntry::new(id, EntryState::Active, decision.to().clone())
        }
        // A retired ID keeps its last declaration, which the base state check
        // found to be exactly this one.
        IdentityDecision::Retire(decision) => {
            RegistryEntry::new(id, EntryState::Retired, decision.from().clone())
        }
        IdentityDecision::Restore(decision) => {
            RegistryEntry::new(id, EntryState::Active, decision.to().clone())
        }
    }
}

/// Return whether an occurrence is exactly one of the inventory's
/// declarations.
fn declares(inventory: &AuthoringInventory, occurrence: &Occurrence) -> bool {
    let declarations = inventory.declarations();
    // The canonical order leaves out a snapshot's declared length, so the
    // neighbour a search finds has to be compared in full.
    declarations
        .binary_search_by(|facts| facts.occurrence().canonical_cmp(occurrence))
        .is_ok_and(|index| declarations[index].occurrence() == occurrence)
}
