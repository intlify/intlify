// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The accounts and claims the continuity rules read.
//!
//! Checking an update's bases and planning one read the same facts: which
//! edits start from a base snapshot, whether the inventory still holds it,
//! and which current declaration each account carries an old one onto. They
//! are gathered here once, so the rules that check a plan and the rules that
//! make one cannot read the evidence differently.

use intlify_authoring::{AuthoringInventory, MessageIntentId, Occurrence, SourceSnapshot};

use super::edit::{fate, replay, RangeFate, ReplayGap};
use super::sources::RetainedSources;
use crate::registry::{
    declares, AdmittedRegistry, ContinuationBasis, EntryState, IdentityDecision, SourceEdit,
};

/// Every account of where a base snapshot's declarations went.
///
/// An edit from the snapshot is one account. The snapshot being still
/// current is another: its declarations stayed where they were. Two accounts
/// of one snapshot are a copy or a conflict, and neither is a continuation.
pub(crate) struct Evidence<'e> {
    edits: Vec<(&'e SourceEdit, Result<(), ReplayGap>)>,
    current: &'e AuthoringInventory,
}

impl<'e> Evidence<'e> {
    /// Collect every distinct edit the decisions carry and the host
    /// supplies, each replayed once.
    pub(crate) fn gather(
        decisions: &'e [IdentityDecision],
        supplied: &'e [SourceEdit],
        sources: &RetainedSources<'_>,
        current: &'e AuthoringInventory,
    ) -> Self {
        let carried = decisions.iter().flat_map(|decision| match decision {
            IdentityDecision::Continue(continuation) => match continuation.basis() {
                ContinuationBasis::VerifiedEdit(edit) => edit.changes(),
                _ => &[],
            },
            _ => &[],
        });
        let mut edits: Vec<(&'e SourceEdit, Result<(), ReplayGap>)> = Vec::new();
        for edit in carried.chain(supplied) {
            if edits.iter().all(|(known, _)| *known != edit) {
                edits.push((edit, replay(edit, sources)));
            }
        }
        Self { edits, current }
    }

    /// Borrow every distinct edit, with how replaying it went.
    pub(crate) fn edits(&self) -> &[(&'e SourceEdit, Result<(), ReplayGap>)] {
        &self.edits
    }

    /// Return whether the inventory holds a snapshot as one of its units.
    pub(crate) fn holds(&self, snapshot: &SourceSnapshot) -> bool {
        self.current
            .units()
            .iter()
            .any(|unit| unit.source() == snapshot)
    }

    /// Count the accounts of where a snapshot's declarations went.
    pub(crate) fn accounts(&self, snapshot: &SourceSnapshot) -> usize {
        self.from(snapshot).len() + usize::from(self.holds(snapshot))
    }

    /// The distinct edits that start from a snapshot.
    pub(crate) fn from(
        &self,
        snapshot: &SourceSnapshot,
    ) -> Vec<&(&'e SourceEdit, Result<(), ReplayGap>)> {
        self.edits
            .iter()
            .filter(|(edit, _)| edit.before() == Some(snapshot))
            .collect()
    }

    /// Which active base entries each account carries onto a current
    /// declaration: their own declaration when it is still there, and where
    /// each edit from their snapshot puts it.
    pub(crate) fn claims(&self, base: &AdmittedRegistry) -> Claims {
        let current = self.current;
        let mut claims = Vec::new();
        for entry in base.snapshot().entries() {
            if entry.state() != EntryState::Active {
                continue;
            }
            let declaration = entry.declaration();
            if declares(current, declaration) {
                claims.push((declaration.clone(), entry.intent_id().clone()));
            }
            for (edit, replayed) in self.from(declaration.source()) {
                let (Ok(()), Some(after), RangeFate::Moved(range)) =
                    (replayed, edit.after(), fate(edit, declaration.range()))
                else {
                    continue;
                };
                if let Ok(target) = Occurrence::new(after.clone(), range, declaration.role()) {
                    if declares(current, &target) {
                        claims.push((target, entry.intent_id().clone()));
                    }
                }
            }
        }
        Claims(claims)
    }
}

/// Current declarations, and the base entries an account carries onto each.
pub(crate) struct Claims(Vec<(Occurrence, MessageIntentId)>);

impl Claims {
    /// The entries carried onto one current declaration.
    pub(crate) fn on<'c>(
        &'c self,
        declaration: &'c Occurrence,
    ) -> impl Iterator<Item = &'c MessageIntentId> {
        self.0
            .iter()
            .filter(move |(target, _)| target == declaration)
            .map(|(_, id)| id)
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::SourceSnapshot;

    use super::*;
    use crate::registry::fixtures::{artifact, sources, Chain};

    fn retained(owned: &[(SourceSnapshot, String)]) -> RetainedSources<'_> {
        RetainedSources::new(
            owned
                .iter()
                .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
        )
        .unwrap()
    }

    fn snapshot(n: usize, unit: usize) -> SourceSnapshot {
        serde_json::from_value(
            artifact(&format!("inventory-{n}"))["body"]["units"][unit]["source"].clone(),
        )
        .unwrap()
    }

    /// The edit update-2 carries, from checkout revision 1 to revision 2.
    fn checkout_edit() -> SourceEdit {
        serde_json::from_value(
            artifact("update-2")["body"]["decisions"][0]["basis"]["changes"][0].clone(),
        )
        .unwrap()
    }

    #[test]
    fn each_distinct_edit_is_kept_once_and_replayed() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        // Update-2's continuation carries the edit, and the host supplies it
        // again: one account, not two.
        let supplied = [checkout_edit()];
        let evidence = Evidence::gather(
            chain.update(2).update().decisions(),
            &supplied,
            &all,
            chain.inventory(2).inventory(),
        );
        assert_eq!(evidence.edits().len(), 1);
        assert_eq!(evidence.edits()[0].1, Ok(()));
        let checkout_1 = snapshot(1, 0);
        assert_eq!(evidence.from(&checkout_1).len(), 1);
        assert_eq!(evidence.accounts(&checkout_1), 1);

        // Without the bytes of revision 2 the edit is still an account, one
        // that did not replay.
        let only_1: Vec<(SourceSnapshot, String)> = owned
            .iter()
            .filter(|(snapshot, _)| snapshot.revision().as_str() == "1")
            .cloned()
            .collect();
        let partial = retained(&only_1);
        let evidence = Evidence::gather(&[], &supplied, &partial, chain.inventory(2).inventory());
        assert_eq!(evidence.edits()[0].1, Err(ReplayGap::SourceUnavailable));
        assert_eq!(evidence.accounts(&checkout_1), 1);
    }

    #[test]
    fn a_snapshot_the_inventory_still_holds_is_an_account_of_its_own() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let evidence = Evidence::gather(&[], &[], &all, chain.inventory(2).inventory());
        // Navigation did not change between inventory 1 and 2; checkout did.
        let (checkout_1, nav_1, checkout_2) = (snapshot(1, 0), snapshot(1, 1), snapshot(2, 0));
        assert!(evidence.holds(&nav_1));
        assert!(evidence.holds(&checkout_2));
        assert!(!evidence.holds(&checkout_1));
        assert_eq!(evidence.accounts(&nav_1), 1);
        assert_eq!(evidence.accounts(&checkout_1), 0);
    }

    #[test]
    fn active_entries_claim_where_they_stayed_or_were_carried() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let supplied = [checkout_edit()];
        let evidence = Evidence::gather(&[], &supplied, &all, chain.inventory(2).inventory());
        let claims = evidence.claims(chain.registry(1));
        let entries = chain.registry(1).snapshot().entries();
        let (pay, home, cancel) = (&entries[0], &entries[1], &entries[2]);
        let current = chain.inventory(2).inventory().declarations();
        let (pay_2, copy_2) = (current[0].occurrence(), current[1].occurrence());
        // The edit carries pay to its new place, home stayed where it was,
        // and the copy's text is inserted, so nothing claims it.
        assert_eq!(claims.on(pay_2).collect::<Vec<_>>(), [pay.intent_id()]);
        assert_eq!(
            claims.on(home.declaration()).collect::<Vec<_>>(),
            [home.intent_id()]
        );
        assert_eq!(claims.on(copy_2).count(), 0);
        // Cancel's line was replaced, so it is carried nowhere.
        assert!(current.iter().all(|facts| claims
            .on(facts.occurrence())
            .all(|id| id != cancel.intent_id())));
    }

    #[test]
    fn retired_entries_claim_nothing() {
        // In registry 2 cancel is retired; its last declaration is in
        // checkout revision 1, which inventory 1 holds again.
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let evidence = Evidence::gather(&[], &[], &all, chain.inventory(1).inventory());
        let claims = evidence.claims(chain.registry(2));
        let cancel = &chain.registry(2).snapshot().entries()[2];
        assert_eq!(cancel.state(), EntryState::Retired);
        assert_eq!(claims.on(cancel.declaration()).count(), 0);
    }
}
