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
    AdmittedInventory, AuthoringArtifactReference, AuthoringInventory, Completeness,
    MessageIntentId, Occurrence, UnitOutcome,
};

use super::admit::{AdmittedRegistry, AdmittedUpdate};
use super::snapshot::{
    same_declaration_order, EntryState, IntentRegistrySnapshot, RegistryEntry, SnapshotFailure,
};
use super::update::{IdentityDecision, IntentRegistryUpdate};

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
/// agree on the owner, and the base and the inventory on the scope. The
/// checks run in a fixed order, so one input always reports the same
/// failure: the names, then each decision, then coverage, then the links.
pub fn apply(
    base: &AdmittedRegistry,
    update: &AdmittedUpdate,
    inventory: &AdmittedInventory,
) -> Result<Transition, TransitionFailure> {
    let plan = update.update();
    let current = inventory.inventory();
    check_names(base, plan, &inventory.reference(), current)?;
    for decision in plan.decisions() {
        check_base_state(decision, base.snapshot(), current)?;
        check_target(decision, base, plan, current)?;
    }
    check_coverage(base, plan, current)?;
    check_links(base.snapshot(), plan)?;
    if plan.is_empty() {
        return Ok(Transition::Unchanged);
    }
    IntentRegistrySnapshot::successor(
        base.artifact(),
        update.artifact(),
        next_entries(base.snapshot(), plan),
    )
    .map(|result| Transition::Applied(Box::new(result)))
    .map_err(TransitionFailure::Result)
}

/// Check that an update names this base and this inventory, and that the
/// three agree on whose identities and which scope they are about.
fn check_names(
    base: &AdmittedRegistry,
    plan: &IntentRegistryUpdate,
    inventory: &AuthoringArtifactReference,
    current: &AuthoringInventory,
) -> Result<(), TransitionFailure> {
    if plan.base() != &base.reference() {
        return Err(TransitionFailure::BaseMismatch);
    }
    if plan.inventory() != inventory {
        return Err(TransitionFailure::InventoryMismatch);
    }
    if plan.owner() != base.snapshot().owner() {
        return Err(TransitionFailure::OwnerMismatch);
    }
    check_pairing(base, current)
}

/// Check that an inventory can be planned from against a base: the same
/// owner and scope, and every unit checked.
pub(crate) fn check_pairing(
    base: &AdmittedRegistry,
    current: &AuthoringInventory,
) -> Result<(), TransitionFailure> {
    let snapshot = base.snapshot();
    if current.owner() != snapshot.owner() {
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
    Ok(())
}

/// Return whether an update decides anything about an ID.
///
/// Decisions are in Intent ID order with one per ID, so this is one search.
fn decides(plan: &IntentRegistryUpdate, id: &MessageIntentId) -> bool {
    plan.decisions()
        .binary_search_by(|decision| decision.intent_id().cmp(id))
        .is_ok()
}

/// Check that the declaration a decision gives an identity to is one of the
/// inventory's, and free.
///
/// A declaration held by an entry the same update also decides is being
/// freed, or kept, by that decision. One held by an entry the update leaves
/// alone is not free.
fn check_target(
    decision: &IdentityDecision,
    base: &AdmittedRegistry,
    plan: &IntentRegistryUpdate,
    current: &AuthoringInventory,
) -> Result<(), TransitionFailure> {
    let Some(to) = decision.to() else {
        return Ok(());
    };
    if !declares(current, to) {
        return Err(TransitionFailure::UnknownDeclaration);
    }
    if let Some(holder) = base.active_entry(to) {
        if holder.intent_id() != decision.intent_id() && !decides(plan, holder.intent_id()) {
            return Err(TransitionFailure::DeclarationTaken);
        }
    }
    Ok(())
}

/// Check that every declaration of the inventory ends up with an identity:
/// an active entry the update leaves alone keeps it, or a decision gives it
/// one.
fn check_coverage(
    base: &AdmittedRegistry,
    plan: &IntentRegistryUpdate,
    current: &AuthoringInventory,
) -> Result<(), TransitionFailure> {
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
            .is_some_and(|entry| !decides(plan, entry.intent_id()));
        if !given && !kept {
            return Err(TransitionFailure::Uncovered);
        }
    }
    Ok(())
}

/// Check that each lineage link names predecessors the base holds and
/// successors the result holds.
///
/// The result holds every base entry and every allocated ID, so a successor
/// resolves there exactly when the base holds it or a decision names it.
fn check_links(
    snapshot: &IntentRegistrySnapshot,
    plan: &IntentRegistryUpdate,
) -> Result<(), TransitionFailure> {
    for link in plan.lineage_links() {
        if link
            .predecessors()
            .iter()
            .any(|id| snapshot.entry(id).is_none())
            || link
                .successors()
                .iter()
                .any(|id| snapshot.entry(id).is_none() && !decides(plan, id))
        {
            return Err(TransitionFailure::UnresolvedLink);
        }
    }
    Ok(())
}

/// The entries an update leaves: every base entry it does not decide about,
/// as it was, and the entry each decision leaves.
fn next_entries(
    snapshot: &IntentRegistrySnapshot,
    plan: &IntentRegistryUpdate,
) -> Vec<RegistryEntry> {
    snapshot
        .entries()
        .iter()
        .filter(|entry| !decides(plan, entry.intent_id()))
        .cloned()
        .chain(plan.decisions().iter().map(next_entry))
        .collect()
}

/// Check one decision against the base entry it names.
pub(crate) fn check_base_state(
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
pub(crate) fn declares(inventory: &AuthoringInventory, occurrence: &Occurrence) -> bool {
    let declarations = inventory.declarations();
    // The canonical order leaves out a snapshot's declared length, so the
    // neighbour a search finds has to be compared in full.
    declarations
        .binary_search_by(|facts| facts.occurrence().canonical_cmp(occurrence))
        .is_ok_and(|index| declarations[index].occurrence() == occurrence)
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use intlify_authoring::{AuthoringArtifact, AuthoringInventory, MessageIntentId, Occurrence};
    use serde_json::{json, Value};

    use super::*;
    use crate::registry::admit::admit_update;
    use crate::registry::artifact::RegistryUpdateArtifact;
    use crate::registry::fixtures::{artifact, limits, owner, Chain};
    use crate::registry::update::{AllocationBasis, ContinuationBasis, ExplicitBasis, LineageLink};

    type Outcome = Result<(), TransitionFailure>;

    /// A labelled change to inventory-2's body, and the rule it breaks.
    type InventoryCase = (&'static str, fn(&mut Value), TransitionFailure);

    /// A well-formed update of the committed owner against one base and
    /// inventory.
    fn plan(
        base: &AdmittedRegistry,
        inventory: &AdmittedInventory,
        decisions: Vec<IdentityDecision>,
        links: Vec<LineageLink>,
    ) -> IntentRegistryUpdate {
        IntentRegistryUpdate::new(
            owner(),
            base.reference(),
            inventory.reference(),
            decisions,
            links,
        )
        .unwrap()
    }

    /// An ID the committed chain never allocated.
    fn stranger() -> MessageIntentId {
        MessageIntentId::retained(owner(), &"d".repeat(32)).unwrap()
    }

    /// A body read from edited JSON, without any of the checks admission
    /// runs, so one rule at a time can be broken.
    fn read<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn the_transition_table_admits_exactly_its_rows() {
        // registry-2 holds pay active, cancel retired, and home active. Each
        // decision kind meets an ID that is absent, active and retired, with
        // a `from` that is the entry's declaration or another one.
        let chain = Chain::load();
        let snapshot = chain.registry(2).snapshot();
        let complete = chain.inventory(2).inventory();
        let mut partial = artifact("inventory-2")["body"].clone();
        partial["completeness"] = json!("partial");
        let partial: AuthoringInventory = serde_json::from_value(partial).unwrap();

        let active = &snapshot.entries()[0];
        let retired = &snapshot.entries()[2];
        assert_eq!(active.state(), EntryState::Active);
        assert_eq!(retired.state(), EntryState::Retired);
        let absent = MessageIntentId::retained(owner(), &"0".repeat(32)).unwrap();
        let elsewhere = snapshot.entries()[1].declaration();
        let to = elsewhere.clone();

        let continuation = |id: &MessageIntentId, from: &Occurrence| {
            IdentityDecision::continuation(
                id.clone(),
                from.clone(),
                to.clone(),
                ContinuationBasis::unchanged_snapshot(),
            )
        };
        let allocation = |id: &MessageIntentId| {
            IdentityDecision::allocation(id.clone(), to.clone(), AllocationBasis::confirmed_new())
        };
        let retirement = |id: &MessageIntentId, from: &Occurrence| {
            IdentityDecision::retirement(id.clone(), from.clone())
        };
        let restoration = |id: &MessageIntentId, from: &Occurrence| {
            IdentityDecision::restoration(
                id.clone(),
                from.clone(),
                to.clone(),
                ExplicitBasis::new("Back by choice.").unwrap(),
            )
        };

        let (a, r) = (active.intent_id(), retired.intent_id());
        let (af, rf) = (active.declaration(), retired.declaration());
        let rows: [(&str, IdentityDecision, &AuthoringInventory, Outcome); 19] = [
            (
                "continue an absent ID",
                continuation(&absent, af),
                complete,
                Err(TransitionFailure::NotActive),
            ),
            (
                "continue an active ID",
                continuation(a, af),
                complete,
                Ok(()),
            ),
            (
                "continue from elsewhere",
                continuation(a, elsewhere),
                complete,
                Err(TransitionFailure::FromMismatch),
            ),
            (
                "continue a retired ID",
                continuation(r, rf),
                complete,
                Err(TransitionFailure::NotActive),
            ),
            (
                "allocate an absent ID",
                allocation(&absent),
                complete,
                Ok(()),
            ),
            (
                "allocate an active ID",
                allocation(a),
                complete,
                Err(TransitionFailure::AlreadyAllocated),
            ),
            (
                "allocate a retired ID",
                allocation(r),
                complete,
                Err(TransitionFailure::AlreadyAllocated),
            ),
            (
                "retire an absent ID",
                retirement(&absent, af),
                complete,
                Err(TransitionFailure::NotActive),
            ),
            ("retire an active ID", retirement(a, af), complete, Ok(())),
            (
                "retire from elsewhere",
                retirement(a, elsewhere),
                complete,
                Err(TransitionFailure::FromMismatch),
            ),
            (
                "retire a retired ID",
                retirement(r, rf),
                complete,
                Err(TransitionFailure::NotActive),
            ),
            (
                "retire with a partial view",
                retirement(a, af),
                &partial,
                Err(TransitionFailure::RetirementFromPartialInventory),
            ),
            (
                "retire a retired ID with a partial view",
                retirement(r, rf),
                &partial,
                Err(TransitionFailure::RetirementFromPartialInventory),
            ),
            (
                "restore an absent ID",
                restoration(&absent, rf),
                complete,
                Err(TransitionFailure::NotRetired),
            ),
            (
                "restore an active ID",
                restoration(a, af),
                complete,
                Err(TransitionFailure::NotRetired),
            ),
            ("restore a retired ID", restoration(r, rf), complete, Ok(())),
            (
                "restore from elsewhere",
                restoration(r, elsewhere),
                complete,
                Err(TransitionFailure::FromMismatch),
            ),
            (
                "continue with a partial view",
                continuation(a, af),
                &partial,
                Ok(()),
            ),
            (
                "restore with a partial view",
                restoration(r, rf),
                &partial,
                Ok(()),
            ),
        ];
        for (label, decision, inventory, expected) in rows {
            assert_eq!(
                check_base_state(&decision, snapshot, inventory),
                expected,
                "{label}"
            );
        }
    }

    #[test]
    fn each_decision_leaves_the_entry_the_table_describes() {
        let chain = Chain::load();
        let entries = chain.registry(1).snapshot().entries();
        let id = entries[0].intent_id().clone();
        let (from, to) = (entries[0].declaration(), entries[1].declaration());
        let at = |state, declaration: &Occurrence| {
            RegistryEntry::new(id.clone(), state, declaration.clone())
        };
        let cases = [
            (
                IdentityDecision::continuation(
                    id.clone(),
                    from.clone(),
                    to.clone(),
                    ContinuationBasis::unchanged_snapshot(),
                ),
                at(EntryState::Active, to),
            ),
            (
                IdentityDecision::allocation(
                    id.clone(),
                    to.clone(),
                    AllocationBasis::confirmed_new(),
                ),
                at(EntryState::Active, to),
            ),
            // A retired ID keeps the declaration it last held.
            (
                IdentityDecision::retirement(id.clone(), from.clone()),
                at(EntryState::Retired, from),
            ),
            (
                IdentityDecision::restoration(
                    id.clone(),
                    from.clone(),
                    to.clone(),
                    ExplicitBasis::new("Back.").unwrap(),
                ),
                at(EntryState::Active, to),
            ),
        ];
        for (decision, expected) in cases {
            assert_eq!(next_entry(&decision), expected);
        }
    }

    #[test]
    fn a_declaration_is_found_only_when_it_is_exactly_one_of_the_inventory() {
        let chain = Chain::load();
        let inventory = chain.inventory(2).inventory();
        let declared = inventory.declarations()[0].occurrence();
        assert!(declares(inventory, declared));

        // The same position in a snapshot that claims another length sits
        // where the declaration does in canonical order, and is not it.
        let mut neighbour = serde_json::to_value(declared).unwrap();
        let length = declared.source().byte_length();
        neighbour["source"]["byteLength"] = json!((length + 1).to_string());
        let neighbour: Occurrence = serde_json::from_value(neighbour).unwrap();
        assert_eq!(neighbour.canonical_cmp(declared), Ordering::Equal);
        assert!(!declares(inventory, &neighbour));

        // A use site of that declaration is not a declaration.
        let call = inventory.references()[0].occurrence();
        assert!(!declares(inventory, call));
    }

    #[test]
    fn what_the_update_names_is_checked_before_what_it_decides() {
        // update-2 names registry-1. Against registry-2 its decisions do not
        // fit either: pay is no longer at its `from`, and cancel is retired.
        // The base it names is what gets reported.
        let chain = Chain::load();
        assert_eq!(
            apply(chain.registry(2), chain.update(2), chain.inventory(2)),
            Err(TransitionFailure::BaseMismatch)
        );
        // With the right base but another inventory, the inventory is.
        assert_eq!(
            apply(chain.registry(1), chain.update(2), chain.inventory(1)),
            Err(TransitionFailure::InventoryMismatch)
        );
    }

    #[test]
    fn names_are_checked_against_the_base_the_inventory_and_each_other() {
        // update-2 names registry-1 and inventory-2.
        let chain = Chain::load();
        let base = chain.registry(1);
        let named = chain.inventory(2).reference();
        let update_2 = chain.update(2).update();
        let inventory_2 = chain.inventory(2).inventory();
        assert_eq!(check_names(base, update_2, &named, inventory_2), Ok(()));
        assert_eq!(
            check_names(chain.registry(2), update_2, &named, inventory_2),
            Err(TransitionFailure::BaseMismatch)
        );
        assert_eq!(
            check_names(base, update_2, &chain.inventory(3).reference(), inventory_2),
            Err(TransitionFailure::InventoryMismatch)
        );

        let mut library = artifact("update-2")["body"].clone();
        library["owner"]["kind"] = json!("library");
        let library: IntentRegistryUpdate = read(library);
        assert_eq!(
            check_names(base, &library, &named, inventory_2),
            Err(TransitionFailure::OwnerMismatch)
        );

        let inventory = |change: fn(&mut Value)| -> AuthoringInventory {
            let mut body = artifact("inventory-2")["body"].clone();
            change(&mut body);
            read(body)
        };
        let cases: [InventoryCase; 4] = [
            (
                "an inventory of another owner",
                |body| body["owner"]["kind"] = json!("library"),
                TransitionFailure::OwnerMismatch,
            ),
            (
                "an inventory of another scope",
                |body| body["scope"] = json!("elsewhere-web"),
                TransitionFailure::ScopeMismatch,
            ),
            (
                "a failed unit",
                |body| body["units"][1]["outcome"] = json!("failed"),
                TransitionFailure::UncheckedUnit,
            ),
            (
                "a blocked unit",
                |body| body["units"][0]["outcome"] = json!("blocked"),
                TransitionFailure::UncheckedUnit,
            ),
        ];
        for (label, change, expected) in cases {
            assert_eq!(
                check_names(base, update_2, &named, &inventory(change)),
                Err(expected),
                "{label}"
            );
        }
    }

    #[test]
    fn a_target_has_to_be_a_free_declaration_of_the_inventory() {
        // registry-1 holds pay, home and cancel, all active. Inventory-2 adds
        // the copy of pay, which nothing holds yet.
        let chain = Chain::load();
        let base = chain.registry(1);
        let entries = base.snapshot().entries();
        let (pay, cancel) = (&entries[0], &entries[2]);
        let copy = chain.inventory(2).inventory().declarations()[1].occurrence();
        let allocate = |to: &Occurrence| {
            IdentityDecision::allocation(stranger(), to.clone(), AllocationBasis::confirmed_new())
        };
        let solo = |decision: &IdentityDecision, inventory| {
            plan(base, inventory, vec![decision.clone()], vec![])
        };

        let free = allocate(copy);
        assert_eq!(
            check_target(
                &free,
                base,
                &solo(&free, chain.inventory(2)),
                chain.inventory(2).inventory()
            ),
            Ok(())
        );

        let mut beside = serde_json::to_value(copy).unwrap();
        beside["range"]["end"] = json!((copy.range().end() - 1).to_string());
        let beside = allocate(&read(beside));
        assert_eq!(
            check_target(
                &beside,
                base,
                &solo(&beside, chain.inventory(2)),
                chain.inventory(2).inventory()
            ),
            Err(TransitionFailure::UnknownDeclaration)
        );

        // Cancel holds its declaration while nothing decides about cancel.
        let current = chain.inventory(1).inventory();
        let taken = allocate(cancel.declaration());
        assert_eq!(
            check_target(&taken, base, &solo(&taken, chain.inventory(1)), current),
            Err(TransitionFailure::DeclarationTaken)
        );
        // Retiring cancel in the same update frees it.
        let retire =
            IdentityDecision::retirement(cancel.intent_id().clone(), cancel.declaration().clone());
        let freed = plan(
            base,
            chain.inventory(1),
            vec![retire.clone(), taken.clone()],
            vec![],
        );
        assert_eq!(check_target(&taken, base, &freed, current), Ok(()));
        // A retirement gives nothing, and an ID may keep its own declaration.
        assert_eq!(check_target(&retire, base, &freed, current), Ok(()));
        let keep = IdentityDecision::continuation(
            pay.intent_id().clone(),
            pay.declaration().clone(),
            pay.declaration().clone(),
            ContinuationBasis::unchanged_snapshot(),
        );
        assert_eq!(
            check_target(&keep, base, &solo(&keep, chain.inventory(1)), current),
            Ok(())
        );
    }

    #[test]
    fn every_declaration_ends_up_with_exactly_one_identity() {
        let chain = Chain::load();
        let base = chain.registry(1);
        let cancel = &base.snapshot().entries()[2];
        let retire =
            IdentityDecision::retirement(cancel.intent_id().clone(), cancel.declaration().clone());
        let replace = IdentityDecision::allocation(
            stranger(),
            cancel.declaration().clone(),
            AllocationBasis::confirmed_new(),
        );
        let first = chain.inventory(1).inventory();
        let second = chain.inventory(2).inventory();
        let cases: [(&str, IntentRegistryUpdate, &AuthoringInventory, Outcome); 5] = [
            (
                "nothing changed and nothing decided",
                plan(base, chain.inventory(1), vec![], vec![]),
                first,
                Ok(()),
            ),
            (
                "a current declaration whose ID is retired",
                plan(base, chain.inventory(1), vec![retire.clone()], vec![]),
                first,
                Err(TransitionFailure::Uncovered),
            ),
            (
                "the same declaration given another ID",
                plan(base, chain.inventory(1), vec![retire, replace], vec![]),
                first,
                Ok(()),
            ),
            (
                "a changed unit with nothing decided",
                plan(base, chain.inventory(2), vec![], vec![]),
                second,
                Err(TransitionFailure::Uncovered),
            ),
            (
                "a changed unit with every declaration decided",
                chain.update(2).update().clone(),
                second,
                Ok(()),
            ),
        ];
        for (label, update, inventory, expected) in cases {
            assert_eq!(
                check_coverage(base, &update, inventory),
                expected,
                "{label}"
            );
        }
    }

    #[test]
    fn links_name_ids_the_base_and_the_result_hold() {
        // update-2 copies pay into a newly allocated ID.
        let chain = Chain::load();
        let snapshot = chain.registry(1).snapshot();
        let update_2 = chain.update(2).update();
        assert_eq!(check_links(snapshot, update_2), Ok(()));

        let link = |predecessors: Value, successors: Value| -> IntentRegistryUpdate {
            let mut body = artifact("update-2")["body"].clone();
            body["lineageLinks"] =
                json!([{"kind": "split", "predecessors": predecessors, "successors": successors}]);
            read(body)
        };
        let id = |value: &MessageIntentId| serde_json::to_value(value).unwrap();
        let pay = id(snapshot.entries()[0].intent_id());
        let copy = id(update_2.decisions()[2].intent_id());
        let unknown = id(&stranger());
        assert_eq!(
            check_links(snapshot, &link(json!([unknown]), json!([pay, copy]))),
            Err(TransitionFailure::UnresolvedLink),
            "a predecessor the base does not hold"
        );
        assert_eq!(
            check_links(snapshot, &link(json!([pay]), json!([pay, unknown]))),
            Err(TransitionFailure::UnresolvedLink),
            "a successor the result does not hold"
        );
        assert_eq!(
            check_links(snapshot, &link(json!([pay]), json!([pay, copy]))),
            Ok(()),
            "a successor this update allocates"
        );

        // A merge resolves the same way: every predecessor in the base and
        // every successor in the result.
        let merge = |predecessors: Value, successors: Value| -> IntentRegistryUpdate {
            let mut body = artifact("update-2")["body"].clone();
            body["lineageLinks"] =
                json!([{"kind": "merge", "predecessors": predecessors, "successors": successors}]);
            read(body)
        };
        let other = id(snapshot.entries()[2].intent_id());
        assert_eq!(
            check_links(snapshot, &merge(json!([pay, other]), json!([copy]))),
            Ok(()),
            "two entries of the base merged into one this update allocates"
        );
        assert_eq!(
            check_links(snapshot, &merge(json!([pay, unknown]), json!([copy]))),
            Err(TransitionFailure::UnresolvedLink),
            "a merged entry the base does not hold"
        );
    }

    #[test]
    fn the_result_keeps_every_undecided_entry_and_adds_what_each_decision_leaves() {
        // Applied to registry-1, update-2 leaves exactly the entries written
        // by hand in registry-2.
        let chain = Chain::load();
        let mut entries = next_entries(chain.registry(1).snapshot(), chain.update(2).update());
        entries.sort_by(|left, right| left.intent_id().cmp(right.intent_id()));
        assert_eq!(entries, chain.registry(2).snapshot().entries());
    }

    #[test]
    fn an_update_without_decisions_is_unchanged_only_after_every_check() {
        let chain = Chain::load();
        let admitted = |update: IntentRegistryUpdate| {
            let sealed = RegistryUpdateArtifact::seal(update).unwrap();
            admit_update(&serde_json::to_vec(&sealed).unwrap(), &limits()).unwrap()
        };
        let nothing = admitted(plan(chain.registry(3), chain.inventory(3), vec![], vec![]));
        assert_eq!(
            apply(chain.registry(3), &nothing, chain.inventory(3)),
            Ok(Transition::Unchanged)
        );
        // An empty update still has to cover the inventory it names.
        let stale = admitted(plan(chain.registry(2), chain.inventory(3), vec![], vec![]));
        assert_eq!(
            apply(chain.registry(2), &stale, chain.inventory(3)),
            Err(TransitionFailure::Uncovered)
        );
    }
}
