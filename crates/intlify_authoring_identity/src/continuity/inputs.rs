// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The evidence a host supplies, checked before any rule reads it.
//!
//! Checking an update's bases and planning one both start from the same
//! host inputs: the update that produced the base, the edits between
//! snapshots, and the units of the owning scope. Each is held here to the
//! rules that make it evidence at all. A supplied edit is one account of how
//! a unit got from the base to the current inventory, so the edits together
//! have to read like 017's change list: each unit at most once on each side,
//! starting from a snapshot the base has and ending at one the inventory
//! holds. An edit that writes from nothing a unit the base already has, or
//! one that ends at a revision nobody holds any more, says nothing about the
//! base or about now.

use std::collections::BTreeSet;

use intlify_authoring::{
    AdmittedInventory, AuthoringInventory, Completeness, SourceSnapshot, Token, UnitResult,
};

use super::bases::{BasisGap, ContinuityFailure};
use super::sources::RetainedSources;
use crate::limits::{IdentityLimitKind, IdentityLimits};
use crate::registry::{validate_edit, AdmittedRegistry, AdmittedUpdate, EntryState, SourceEdit};

/// The update that produced a base, with the inventory it was planned from.
///
/// Newness and absence are only claimed automatically when the current
/// inventory was resolved against the same pins as this one: a change of
/// binding configuration can make declarations appear or vanish without any
/// edit to the source.
#[derive(Debug, Clone, Copy)]
pub struct PreviousUpdate<'a> {
    /// The update the base names as its own.
    pub update: &'a AdmittedUpdate,
    /// The inventory that update was planned from.
    pub inventory: &'a AdmittedInventory,
}

/// The evidence a host supplies when it plans an update.
#[derive(Debug, Clone, Copy)]
pub struct ContinuityInputs<'a> {
    /// The bytes of every snapshot an edit names.
    pub sources: &'a RetainedSources<'a>,
    /// Edits beyond the ones the decisions carry: how each changed unit got
    /// from its base snapshot to its current one.
    pub edits: &'a [SourceEdit],
    /// Every unit of the owning scope, as the host knows it.
    pub membership: &'a [Token],
    /// The update that produced the base, absent for a genesis base.
    pub previous: Option<PreviousUpdate<'a>>,
}

/// Why the supplied edits are not one account of how the base became the
/// current inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditSetFailure {
    /// An edit starts from a snapshot that is not one of the base's.
    BeforeOutsideBase,
    /// An edit writes from nothing a unit the base already has.
    UnitNotNew,
    /// An edit ends at a snapshot the inventory does not hold.
    AfterOutsideInventory,
    /// An edit removes a unit the inventory still holds.
    UnitNotRemoved,
    /// Two edits start from one unit, or two end at one unit.
    UnitRepeated,
}

/// Check that the previous update is the one the base names, and its
/// inventory the one it was planned from.
pub(crate) fn check_previous(
    base: &AdmittedRegistry,
    previous: Option<PreviousUpdate<'_>>,
) -> Result<(), ContinuityFailure> {
    // A genesis names no update, so no previous update can be its own.
    let snapshot = base.snapshot();
    match previous {
        None => Ok(()),
        Some(previous) => {
            if snapshot.update() != Some(&previous.update.reference())
                || previous.update.update().inventory() != &previous.inventory.reference()
            {
                return Err(ContinuityFailure::PreviousMismatch);
            }
            Ok(())
        }
    }
}

/// Hold the supplied edits to the caller's bounds, to the rules any edit
/// satisfies, and to the rules a set of them satisfies together.
pub(crate) fn check_supplied(
    edits: &[SourceEdit],
    base: &AdmittedRegistry,
    current: &AuthoringInventory,
    previous: Option<PreviousUpdate<'_>>,
    limits: &IdentityLimits,
) -> Result<(), ContinuityFailure> {
    if edits.len() as u64 > limits.source_edits {
        return Err(ContinuityFailure::Limit(IdentityLimitKind::SourceEdits));
    }
    let replacements: usize = edits.iter().map(|edit| edit.replacements().len()).sum();
    if replacements as u64 > limits.replacements {
        return Err(ContinuityFailure::Limit(IdentityLimitKind::Replacements));
    }
    let bytes: usize = edits
        .iter()
        .flat_map(SourceEdit::replacements)
        .map(|replacement| replacement.text().len())
        .sum();
    if bytes as u64 > limits.replacement_bytes {
        return Err(ContinuityFailure::Limit(
            IdentityLimitKind::ReplacementBytes,
        ));
    }
    let owner = base.snapshot().owner();
    for edit in edits {
        validate_edit(edit, owner).map_err(ContinuityFailure::InvalidEdit)?;
    }
    check_edit_set(edits, &base_snapshots(base, previous), current)
        .map_err(ContinuityFailure::EditSet)
}

/// Every snapshot the base has: each active entry's, and each unit of the
/// inventory that produced the base.
///
/// A retired entry keeps its last declaration from whichever inventory it
/// was retired against, which says nothing about the base.
fn base_snapshots<'a>(
    base: &'a AdmittedRegistry,
    previous: Option<PreviousUpdate<'a>>,
) -> Vec<&'a SourceSnapshot> {
    let mut snapshots: Vec<&SourceSnapshot> = base
        .snapshot()
        .entries()
        .iter()
        .filter(|entry| entry.state() == EntryState::Active)
        .map(|entry| entry.declaration().source())
        .collect();
    if let Some(previous) = previous {
        snapshots.extend(
            previous
                .inventory
                .inventory()
                .units()
                .iter()
                .map(UnitResult::source),
        );
    }
    snapshots
}

/// Check that the edits read like one change list from the base to the
/// current inventory.
fn check_edit_set(
    edits: &[SourceEdit],
    base: &[&SourceSnapshot],
    current: &AuthoringInventory,
) -> Result<(), EditSetFailure> {
    let base_units: BTreeSet<&Token> = base.iter().map(|snapshot| snapshot.unit()).collect();
    let current_units: BTreeSet<&Token> = current
        .units()
        .iter()
        .map(|unit| unit.source().unit())
        .collect();
    let mut starts: BTreeSet<&Token> = BTreeSet::new();
    let mut ends: BTreeSet<&Token> = BTreeSet::new();
    for edit in edits {
        match (edit.before(), edit.after()) {
            (Some(before), _) if !base.contains(&before) => {
                return Err(EditSetFailure::BeforeOutsideBase)
            }
            (None, Some(after)) if base_units.contains(after.unit()) => {
                return Err(EditSetFailure::UnitNotNew)
            }
            (_, Some(after)) if !current.units().iter().any(|unit| unit.source() == after) => {
                return Err(EditSetFailure::AfterOutsideInventory)
            }
            (Some(before), None) if current_units.contains(before.unit()) => {
                return Err(EditSetFailure::UnitNotRemoved)
            }
            _ => {}
        }
        let repeated = edit
            .before()
            .is_some_and(|before| !starts.insert(before.unit()))
            || edit.after().is_some_and(|after| !ends.insert(after.unit()));
        if repeated {
            return Err(EditSetFailure::UnitRepeated);
        }
    }
    Ok(())
}

/// Return whether newness and absence may be claimed without an explicit
/// decision: the host's membership is the inventory's, and the pins are the
/// base's.
pub(crate) fn automatic(
    base: &AdmittedRegistry,
    current: &AuthoringInventory,
    inputs: &ContinuityInputs<'_>,
) -> Result<(), BasisGap> {
    let units: BTreeSet<&Token> = current
        .units()
        .iter()
        .map(|unit| unit.source().unit())
        .collect();
    let members: BTreeSet<&Token> = inputs.membership.iter().collect();
    let fits = match current.completeness() {
        Completeness::Complete => units == members,
        Completeness::Partial => units.is_subset(&members),
    };
    if !fits {
        return Err(BasisGap::MembershipMismatch);
    }
    if base.snapshot().is_genesis() {
        return Ok(());
    }
    match inputs.previous {
        None => Err(BasisGap::BasisUnknown),
        Some(previous) if previous.inventory.inventory().basis() != current.basis() => {
            Err(BasisGap::BasisChanged)
        }
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{ByteRange, Completeness};
    use serde_json::json;

    use super::*;
    use crate::registry::fixtures::{
        artifact, genesis, inventory_of, limits, sources, Base, Chain, Unit,
    };
    use crate::registry::Replacement;

    fn snapshot_of(
        owned: &[(SourceSnapshot, String)],
        unit: &str,
        revision: &str,
    ) -> SourceSnapshot {
        owned
            .iter()
            .find(|(snapshot, _)| {
                snapshot.unit().as_str() == unit && snapshot.revision().as_str() == revision
            })
            .map(|(snapshot, _)| snapshot.clone())
            .expect("a committed snapshot")
    }

    /// An edit between two sides, written whole.
    fn edit(before: Option<&SourceSnapshot>, after: Option<&SourceSnapshot>) -> SourceEdit {
        let length = before.map_or(0, SourceSnapshot::byte_length);
        SourceEdit::new(
            before.cloned(),
            after.cloned(),
            vec![Replacement::new(ByteRange::new(0, length).unwrap(), "")],
        )
    }

    fn produced(chain: &Chain, n: usize) -> PreviousUpdate<'_> {
        PreviousUpdate {
            update: chain.update(n),
            inventory: chain.inventory(n),
        }
    }

    /// Check edits against registry 1, produced from inventory 1, for
    /// inventory 2.
    fn for_second(edits: &[SourceEdit]) -> Result<(), ContinuityFailure> {
        let chain = Chain::load();
        check_supplied(
            edits,
            chain.registry(1),
            chain.inventory(2).inventory(),
            Some(produced(&chain, 1)),
            &limits(),
        )
    }

    fn set(failure: EditSetFailure) -> Result<(), ContinuityFailure> {
        Err(ContinuityFailure::EditSet(failure))
    }

    #[test]
    fn edits_from_the_base_to_the_inventory_are_one_account() {
        let owned = sources();
        let (checkout_1, checkout_2, nav_1) = (
            snapshot_of(&owned, "checkout", "1"),
            snapshot_of(&owned, "checkout", "2"),
            snapshot_of(&owned, "nav", "1"),
        );
        assert_eq!(
            for_second(&[
                edit(Some(&checkout_1), Some(&checkout_2)),
                edit(Some(&nav_1), Some(&nav_1)),
            ]),
            Ok(())
        );
        assert_eq!(for_second(&[]), Ok(()));
    }

    #[test]
    fn an_edit_starts_at_a_snapshot_the_base_has() {
        let owned = sources();
        let (checkout_1, checkout_2, checkout_3) = (
            snapshot_of(&owned, "checkout", "1"),
            snapshot_of(&owned, "checkout", "2"),
            snapshot_of(&owned, "checkout", "3"),
        );
        assert_eq!(
            for_second(&[edit(Some(&checkout_3), Some(&checkout_2))]),
            set(EditSetFailure::BeforeOutsideBase)
        );
        // In registry 2 cancel is retired, its last declaration still in
        // checkout revision 1. That says nothing about the base, which
        // inventory 2 produced.
        let chain = Chain::load();
        assert_eq!(
            check_supplied(
                &[edit(Some(&checkout_1), Some(&checkout_3))],
                chain.registry(2),
                chain.inventory(3).inventory(),
                Some(produced(&chain, 2)),
                &limits(),
            ),
            set(EditSetFailure::BeforeOutsideBase)
        );
        // A genesis has nothing to start from.
        assert_eq!(
            check_supplied(
                &[edit(Some(&checkout_1), Some(&checkout_2))],
                &genesis("0123456789abcdef0123456789abcdef"),
                chain.inventory(2).inventory(),
                None,
                &limits(),
            ),
            set(EditSetFailure::BeforeOutsideBase)
        );
    }

    #[test]
    fn only_a_unit_new_to_the_base_is_written_from_nothing() {
        let owned = sources();
        let checkout_2 = snapshot_of(&owned, "checkout", "2");
        assert_eq!(
            for_second(&[edit(None, Some(&checkout_2))]),
            set(EditSetFailure::UnitNotNew)
        );
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app], &[&"a".repeat(32)]);
        let left = Unit::new("left.js", "1", "intent('Next')\n");
        let current = inventory_of(&[&app, &left], Completeness::Complete);
        assert_eq!(
            check_supplied(
                &[edit(None, Some(&left.snapshot()))],
                &base.registry,
                current.inventory(),
                Some(base.previous()),
                &limits(),
            ),
            Ok(())
        );
    }

    #[test]
    fn an_edit_ends_at_a_snapshot_the_inventory_holds() {
        let owned = sources();
        let (checkout_1, checkout_3) = (
            snapshot_of(&owned, "checkout", "1"),
            snapshot_of(&owned, "checkout", "3"),
        );
        assert_eq!(
            for_second(&[edit(Some(&checkout_1), Some(&checkout_3))]),
            set(EditSetFailure::AfterOutsideInventory)
        );
    }

    #[test]
    fn only_a_unit_gone_from_the_inventory_is_removed() {
        let owned = sources();
        let checkout_1 = snapshot_of(&owned, "checkout", "1");
        assert_eq!(
            for_second(&[edit(Some(&checkout_1), None)]),
            set(EditSetFailure::UnitNotRemoved)
        );
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let nav = Unit::new("nav.js", "1", "intent('Home')\n");
        let base = Base::of(&[&app, &nav], &[&"a".repeat(32), &"b".repeat(32)]);
        let current = inventory_of(&[&app], Completeness::Complete);
        assert_eq!(
            check_supplied(
                &[edit(Some(&nav.snapshot()), None)],
                &base.registry,
                current.inventory(),
                Some(base.previous()),
                &limits(),
            ),
            Ok(())
        );
    }

    #[test]
    fn each_unit_is_on_each_side_at_most_once() {
        let owned = sources();
        let (checkout_1, checkout_2, nav_1) = (
            snapshot_of(&owned, "checkout", "1"),
            snapshot_of(&owned, "checkout", "2"),
            snapshot_of(&owned, "nav", "1"),
        );
        assert_eq!(
            for_second(&[
                edit(Some(&checkout_1), Some(&checkout_2)),
                edit(Some(&checkout_1), Some(&nav_1)),
            ]),
            set(EditSetFailure::UnitRepeated),
            "two accounts of one unit"
        );
        assert_eq!(
            for_second(&[
                edit(Some(&checkout_1), Some(&checkout_2)),
                edit(Some(&nav_1), Some(&checkout_2)),
            ]),
            set(EditSetFailure::UnitRepeated),
            "two units becoming one"
        );
    }

    #[test]
    fn a_unit_without_declarations_is_still_one_of_the_base_s() {
        // The base has no entry in the module unit, which declares nothing,
        // but the inventory that produced the base has it.
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let module = Unit::new("module.js", "1", "export {}\n");
        let base = Base::of(&[&app, &module], &[&"a".repeat(32)]);
        let changed = Unit::new("module.js", "2", "export const x = 1\n");
        let current = inventory_of(&[&app, &changed], Completeness::Complete);
        let check = |edits: &[SourceEdit]| {
            check_supplied(
                edits,
                &base.registry,
                current.inventory(),
                Some(base.previous()),
                &limits(),
            )
        };
        assert_eq!(
            check(&[edit(Some(&module.snapshot()), Some(&changed.snapshot()))]),
            Ok(())
        );
        assert_eq!(
            check(&[edit(None, Some(&changed.snapshot()))]),
            set(EditSetFailure::UnitNotNew)
        );
    }

    #[test]
    fn a_partial_inventory_needs_only_its_own_units_in_the_membership() {
        // A partial inventory sees part of the scope: the host's membership
        // may hold more units than it does, but not fewer.
        let chain = Chain::load();
        let owned = sources();
        let all = RetainedSources::new(
            owned
                .iter()
                .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
        )
        .unwrap();
        let mut partial = artifact("inventory-2")["body"].clone();
        partial["completeness"] = json!("partial");
        let partial: AuthoringInventory = serde_json::from_value(partial).unwrap();
        let with = |names: &[&str]| {
            let membership: Vec<Token> =
                names.iter().map(|name| Token::new(name).unwrap()).collect();
            automatic(
                chain.registry(1),
                &partial,
                &ContinuityInputs {
                    sources: &all,
                    edits: &[],
                    membership: &membership,
                    previous: Some(produced(&chain, 1)),
                },
            )
        };
        assert_eq!(with(&["checkout", "nav", "settings"]), Ok(()));
        assert_eq!(with(&["checkout", "nav"]), Ok(()));
        assert_eq!(with(&["checkout"]), Err(BasisGap::MembershipMismatch));
    }
}
