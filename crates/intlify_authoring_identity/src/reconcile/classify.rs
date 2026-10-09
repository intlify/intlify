// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Steps 3 to 7 of 016's procedure: what the inputs establish for every
//! current declaration and for every entry whose declaration is gone.
//!
//! The steps run in 016's order. Explicit decisions bound to this base and
//! inventory come first. An active entry that still holds its exact
//! declaration keeps it. An entry whose declaration is gone continues only
//! where exactly one account carries it onto exactly one current
//! declaration that nothing else claims, and is gone only where the evidence
//! shows that. A declaration left over is new only where the evidence shows
//! that. Everything else is reported with the reason it is not shown; the
//! planner never picks an identity to fill a gap.

use std::collections::BTreeSet;

use intlify_authoring::{
    AuthoringArtifactReference, AuthoringInventory, Completeness, MessageIntentId, Occurrence,
    Token,
};

use super::diagnostic::{conflict, update_required};
use super::explicit::ExplicitDecision;
use super::outcome::{Conflict, DeclarationClass, EntryClass};
use super::workspace::ReconcileWorkspace;
use crate::continuity::{fate, inserted, BasisGap, Claims, Evidence, RangeFate, ReplayGap};
use crate::registry::{
    check_base_state, AdmittedRegistry, EntryState, IdentityDecision, RegistryEntry, SourceEdit,
    TransitionFailure,
};

/// Find a declaration of the inventory, exactly.
pub(super) fn position(current: &AuthoringInventory, occurrence: &Occurrence) -> Option<usize> {
    let declarations = current.declarations();
    // The canonical order leaves out a snapshot's declared length, so the
    // neighbour a search finds has to be compared in full.
    declarations
        .binary_search_by(|facts| facts.occurrence().canonical_cmp(occurrence))
        .ok()
        .filter(|index| declarations[*index].occurrence() == occurrence)
}

/// One reconciliation's view of the base, the inventory and the evidence.
pub(super) struct Planner<'p> {
    pub(super) base: &'p AdmittedRegistry,
    pub(super) inventory: AuthoringArtifactReference,
    pub(super) current: &'p AuthoringInventory,
    pub(super) evidence: &'p Evidence<'p>,
    pub(super) claims: &'p Claims,
    pub(super) membership: &'p [Token],
    /// Whether newness and absence may be shown without a choice.
    pub(super) automatic: Result<(), BasisGap>,
}

impl<'p> Planner<'p> {
    /// Step 3: keep the explicit decisions that fit this base and inventory,
    /// and report the rest as conflicts.
    ///
    /// Returns the decisions that fit and the IDs whose entries they move.
    pub(super) fn explicit(
        &self,
        decisions: &'p [ExplicitDecision],
        workspace: &mut ReconcileWorkspace,
    ) -> (Vec<&'p IdentityDecision>, BTreeSet<&'p MessageIntentId>) {
        let mut alone: Vec<&'p IdentityDecision> = Vec::new();
        for explicit in decisions {
            let decision = explicit.decision();
            // Every explicit action gives a declaration an identity.
            let to = decision.to().expect("an explicit decision has a target");
            match self
                .fit(explicit)
                .and_then(|()| position(self.current, to).ok_or(Conflict::BaseMismatch))
            {
                Ok(_) => alone.push(decision),
                Err(kind) => self.refuse(decision, kind, workspace),
            }
        }

        // One decision per ID and one per declaration: two explicit
        // decisions that agree on either compete.
        let competing = |candidate: &IdentityDecision| {
            alone
                .iter()
                .filter(|other| {
                    other.intent_id() == candidate.intent_id() || other.to() == candidate.to()
                })
                .count()
                > 1
        };
        let (fitting, competing): (Vec<&IdentityDecision>, Vec<&IdentityDecision>) = alone
            .iter()
            .copied()
            .partition(|decision| !competing(decision));
        for decision in competing {
            self.refuse(decision, Conflict::CompetingClaim, workspace);
        }

        // An entry an explicit decision continues or restores leaves its
        // declaration. A declaration an entry keeps is not free for
        // another identity.
        let moved: BTreeSet<&MessageIntentId> = fitting
            .iter()
            .filter(|decision| !matches!(decision, IdentityDecision::Allocate(_)))
            .map(|decision| decision.intent_id())
            .collect();
        let mut accepted = Vec::new();
        for decision in fitting {
            let to = decision.to().expect("an explicit decision has a target");
            // An entry the same decision moves, or another one does, frees
            // the declaration it held.
            let taken = self
                .base
                .active_entry(to)
                .is_some_and(|holder| !moved.contains(holder.intent_id()));
            if taken {
                self.refuse(decision, Conflict::CompetingClaim, workspace);
                continue;
            }
            let index = position(self.current, to).expect("checked above");
            workspace.classes[index] =
                Some(DeclarationClass::Explicit(decision.intent_id().clone()));
            accepted.push(decision);
        }
        let moved = accepted
            .iter()
            .filter(|decision| !matches!(decision, IdentityDecision::Allocate(_)))
            .map(|decision| decision.intent_id())
            .collect();
        (accepted, moved)
    }

    /// Check one explicit decision against the inputs it names and the base
    /// entry it acts on.
    fn fit(&self, explicit: &ExplicitDecision) -> Result<(), Conflict> {
        let snapshot = self.base.snapshot();
        let decision = explicit.decision();
        if explicit.base() != &self.base.reference() || explicit.inventory() != &self.inventory {
            return Err(Conflict::BaseMismatch);
        }
        if decision.intent_id().owner() != snapshot.owner() {
            return Err(Conflict::ForeignOwner);
        }
        check_base_state(decision, snapshot, self.current).map_err(|failure| match failure {
            TransitionFailure::AlreadyAllocated => Conflict::Collision,
            TransitionFailure::NotActive
                if snapshot
                    .entry(decision.intent_id())
                    .is_some_and(|entry| entry.state() == EntryState::Retired) =>
            {
                Conflict::RetiredReuse
            }
            _ => Conflict::BaseMismatch,
        })
    }

    /// Report one explicit decision that cannot be applied.
    fn refuse(
        &self,
        decision: &IdentityDecision,
        kind: Conflict,
        workspace: &mut ReconcileWorkspace,
    ) {
        let to = decision.to().expect("an explicit decision has a target");
        if let Some(index) = position(self.current, to) {
            workspace.classes[index] = Some(DeclarationClass::Conflict(kind));
        }
        workspace.diagnostics.push(conflict(
            to.clone(),
            kind,
            decision.from().cloned().into_iter().collect(),
        ));
    }

    /// Step 4, unchanged associations: an active entry that still holds its
    /// exact declaration keeps it.
    ///
    /// Returns the active entries whose declarations are gone.
    pub(super) fn retained(
        &self,
        moved: &BTreeSet<&MessageIntentId>,
        workspace: &mut ReconcileWorkspace,
    ) -> Vec<&'p RegistryEntry> {
        let mut gone = Vec::new();
        for entry in self.base.snapshot().entries() {
            if entry.state() != EntryState::Active || moved.contains(entry.intent_id()) {
                continue;
            }
            match position(self.current, entry.declaration()) {
                Some(index) => {
                    // An explicit decision competing for it was refused above.
                    if workspace.classes[index].is_none() {
                        workspace.classes[index] =
                            Some(DeclarationClass::Retained(entry.intent_id().clone()));
                    }
                }
                None => gone.push(entry),
            }
        }
        gone
    }

    /// Steps 4 and 7 for the entries whose declarations are gone: carried
    /// onto exactly one current declaration, shown to be gone, kept unseen
    /// by a partial inventory, or not shown.
    pub(super) fn carried(
        &self,
        gone: Vec<&'p RegistryEntry>,
        workspace: &mut ReconcileWorkspace,
    ) -> (
        Vec<(&'p RegistryEntry, &'p SourceEdit, usize)>,
        Vec<&'p RegistryEntry>,
    ) {
        let covered: BTreeSet<&Token> = self
            .current
            .units()
            .iter()
            .map(|unit| unit.source().unit())
            .collect();
        let partial = self.current.completeness() == Completeness::Partial;
        let mut proposed = Vec::new();
        let mut absent = Vec::new();
        for entry in gone {
            let source = entry.declaration().source();
            let class = if partial && !covered.contains(source.unit()) {
                EntryClass::Kept
            } else {
                match self.account(entry) {
                    Account::Carried(edit, index) => {
                        proposed.push((entry, edit, index));
                        continue;
                    }
                    Account::Gone(edit) => self.absence(entry, edit),
                    Account::Unshown(gap) => EntryClass::Unresolved(gap),
                }
            };
            settle(entry, class, &mut absent, workspace);
        }

        // An entry continues only onto a declaration nothing else claims:
        // no other entry stays there or is carried there, and no explicit
        // decision gave it an identity.
        let mut continued = Vec::new();
        for (entry, edit, index) in proposed {
            let target = self.current.declarations()[index].occurrence();
            let rivals: Vec<&MessageIntentId> = self
                .claims
                .on(target)
                .filter(|id| *id != entry.intent_id())
                .collect();
            if rivals.is_empty() && workspace.classes[index].is_none() {
                workspace.classes[index] =
                    Some(DeclarationClass::Continued(entry.intent_id().clone()));
                workspace.entries.push((
                    entry.intent_id().clone(),
                    EntryClass::Continued(target.clone()),
                ));
                continued.push((entry, edit, index));
                continue;
            }
            workspace.entries.push((
                entry.intent_id().clone(),
                EntryClass::Conflict(Conflict::CompetingClaim),
            ));
            workspace.classes[index] = Some(DeclarationClass::Conflict(Conflict::CompetingClaim));
            let related = self
                .claims
                .on(target)
                .filter_map(|id| self.base.snapshot().entry(id))
                .map(|claimant| claimant.declaration().clone())
                .collect();
            workspace
                .diagnostics
                .push(conflict(target.clone(), Conflict::CompetingClaim, related));
        }
        (continued, absent)
    }

    /// Read the one account of where an entry's declaration went.
    fn account(&self, entry: &'p RegistryEntry) -> Account<'p> {
        let declaration = entry.declaration();
        let source = declaration.source();
        let edits = self.evidence.from(source);
        let held = self.evidence.holds(source);
        match (edits.as_slice(), held) {
            // No edit and a vanished unit: the unit's membership decides.
            ([], false) => Account::Gone(None),
            // The unit did not change, yet the declaration is not declared.
            ([], true) => Account::Unshown(BasisGap::AbsenceUnproven),
            ([(_, Err(gap))], false) => Account::Unshown(match gap {
                ReplayGap::SourceUnavailable => BasisGap::SourceUnavailable,
                ReplayGap::Mismatch => BasisGap::ReplayMismatch,
            }),
            ([(edit, Ok(()))], false) => match (edit.after(), fate(edit, declaration.range())) {
                (None, _) | (Some(_), RangeFate::Replaced) => Account::Gone(Some(*edit)),
                (Some(_), RangeFate::Touched) => Account::Unshown(BasisGap::EditAtBoundary),
                (Some(after), RangeFate::Moved(range)) => {
                    Occurrence::new(after.clone(), range, declaration.role())
                        .ok()
                        .and_then(|target| position(self.current, &target))
                        .map_or(Account::Unshown(BasisGap::MappedElsewhere), |index| {
                            Account::Carried(edit, index)
                        })
                }
            },
            // Two accounts of one snapshot: a copy or a conflict.
            _ => Account::Unshown(BasisGap::Ambiguous),
        }
    }

    /// Step 7: whether an entry the account says is gone is shown to be.
    fn absence(&self, entry: &RegistryEntry, edit: Option<&SourceEdit>) -> EntryClass {
        if self.current.completeness() == Completeness::Partial {
            return EntryClass::Kept;
        }
        if let Err(gap) = self.automatic {
            return EntryClass::Unresolved(gap);
        }
        let member = self
            .membership
            .contains(entry.declaration().source().unit());
        // Without an edit only the unit's departure shows it is gone, and a
        // removal has to agree with the membership.
        let removal = edit.is_none_or(|edit| edit.after().is_none());
        if removal && member {
            return EntryClass::Unresolved(BasisGap::AbsenceUnproven);
        }
        EntryClass::Absent
    }

    /// Step 5: whether each declaration nothing else gave an identity is
    /// shown to be new.
    pub(super) fn remaining(&self, workspace: &mut ReconcileWorkspace) {
        let snapshot = self.base.snapshot();
        for (index, facts) in self.current.declarations().iter().enumerate() {
            if workspace.classes[index].is_some() {
                continue;
            }
            let declaration = facts.occurrence();
            let claimants: Vec<&RegistryEntry> = self
                .claims
                .on(declaration)
                .filter_map(|id| snapshot.entry(id))
                .collect();
            let class = if let Err(gap) = self.automatic {
                DeclarationClass::Unresolved(gap)
            } else if snapshot.entries().is_empty() {
                // A base with no history has nothing to continue.
                DeclarationClass::New
            } else if claimants.is_empty() && self.inserted(declaration) {
                DeclarationClass::New
            } else {
                DeclarationClass::Unresolved(BasisGap::NewUnproven)
            };
            let related: Vec<Occurrence> = claimants
                .iter()
                .map(|claimant| claimant.declaration().clone())
                .collect();
            if let DeclarationClass::Unresolved(gap) = class {
                workspace
                    .diagnostics
                    .push(update_required(declaration.clone(), gap, related));
            }
            workspace.classes[index] = Some(class);
        }
    }

    /// Return whether an edit that ends at the declaration's own snapshot
    /// inserted all of its text.
    fn inserted(&self, declaration: &Occurrence) -> bool {
        self.evidence.edits().iter().any(|(edit, replayed)| {
            replayed.is_ok()
                && edit.after() == Some(declaration.source())
                && inserted(edit, declaration.range())
        })
    }
}

/// Record one gone entry's class, and report it when it is not shown.
fn settle<'p>(
    entry: &'p RegistryEntry,
    class: EntryClass,
    absent: &mut Vec<&'p RegistryEntry>,
    workspace: &mut ReconcileWorkspace,
) {
    match &class {
        EntryClass::Absent => absent.push(entry),
        EntryClass::Unresolved(gap) => {
            workspace
                .diagnostics
                .push(update_required(entry.declaration().clone(), *gap, vec![]));
        }
        _ => {}
    }
    workspace.entries.push((entry.intent_id().clone(), class));
}

/// What the evidence says became of one entry's declaration.
enum Account<'p> {
    /// One edit carries it onto the declaration at this position.
    Carried(&'p SourceEdit, usize),
    /// It may be gone: its unit left with no edit, or one edit removed or
    /// replaced it.
    Gone(Option<&'p SourceEdit>),
    /// Nothing shows where it went.
    Unshown(BasisGap),
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{
        AdmittedInventory, ByteRange, Diagnostic, OccurrenceRole, OwnerIdentity, OwnerKind,
    };

    use super::*;
    use crate::continuity::{automatic, ContinuityInputs, PreviousUpdate, RetainedSources};
    use crate::registry::fixtures::{
        declaration, genesis, id, inventory_of, inventory_with_role, sources, Base, Chain, Unit,
    };
    use crate::registry::{AllocationBasis, ContinuationBasis, ExplicitBasis, Replacement};

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccc";

    /// What the planner found: each declaration's class, each gone entry's
    /// class, and each diagnostic's detail and related count, in reporting
    /// order.
    #[derive(Debug)]
    struct Found {
        classes: Vec<DeclarationClass>,
        entries: Vec<(MessageIntentId, EntryClass)>,
        details: Vec<(&'static str, usize)>,
    }

    struct Scene<'s> {
        base: &'s AdmittedRegistry,
        previous: Option<PreviousUpdate<'s>>,
        current: &'s AdmittedInventory,
        sources: &'s RetainedSources<'s>,
        edits: &'s [SourceEdit],
        membership: &'s [&'s str],
        explicit: &'s [ExplicitDecision],
    }

    /// Run steps 3 to 5 the way reconciliation runs them.
    fn found(scene: &Scene<'_>) -> Found {
        let membership: Vec<Token> = scene
            .membership
            .iter()
            .map(|unit| Token::new(unit).unwrap())
            .collect();
        let current = scene.current.inventory();
        let inputs = ContinuityInputs {
            sources: scene.sources,
            edits: scene.edits,
            membership: &membership,
            previous: scene.previous,
        };
        let evidence = Evidence::gather(&[], scene.edits, scene.sources, current);
        let claims = evidence.claims(scene.base);
        let planner = Planner {
            base: scene.base,
            inventory: scene.current.reference(),
            current,
            evidence: &evidence,
            claims: &claims,
            membership: &membership,
            automatic: automatic(scene.base, current, &inputs),
        };
        let mut workspace = ReconcileWorkspace::new();
        workspace.classes.resize(current.declarations().len(), None);
        let (_, moved) = planner.explicit(scene.explicit, &mut workspace);
        let gone = planner.retained(&moved, &mut workspace);
        planner.carried(gone, &mut workspace);
        planner.remaining(&mut workspace);
        workspace.diagnostics.sort_by(Diagnostic::reporting_cmp);
        workspace
            .entries
            .sort_by(|left, right| left.0.cmp(&right.0));
        Found {
            classes: workspace.classes.into_iter().map(Option::unwrap).collect(),
            entries: workspace.entries,
            details: workspace
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    (
                        diagnostic.detail().unwrap().as_str(),
                        diagnostic.related().len(),
                    )
                })
                .collect(),
        }
    }

    fn retained<'u>(units: &[&'u Unit]) -> RetainedSources<'u> {
        RetainedSources::new(
            units
                .iter()
                .map(|unit| (unit.snapshot(), unit.text.as_bytes())),
        )
        .unwrap()
    }

    /// An edit that leaves the bytes as they were, from one unit to another.
    fn renamed(before: &Unit, after: &Unit) -> SourceEdit {
        SourceEdit::new(Some(before.snapshot()), Some(after.snapshot()), vec![])
    }

    fn at(start: u64, end: u64) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    fn unshown(gap: BasisGap) -> EntryClass {
        EntryClass::Unresolved(gap)
    }

    fn reason() -> ExplicitBasis {
        ExplicitBasis::new("Chosen by hand.").unwrap()
    }

    #[test]
    fn an_entry_whose_declaration_is_still_there_keeps_it() {
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app], &[A]);
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &base.inventory,
            sources: &retained(&[&app]),
            edits: &[],
            membership: &["app.js"],
            explicit: &[],
        });
        assert_eq!(found.classes, [DeclarationClass::Retained(id(A))]);
        assert!(found.entries.is_empty());
        assert!(found.details.is_empty());
    }

    #[test]
    fn one_edit_carries_an_entry_onto_one_declaration() {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&first], &[A]);
        let second = Unit::new("app.js", "2", "// actions\nintent('Save')\n");
        let current = inventory_of(&[&second], Completeness::Complete);
        let edit = SourceEdit::new(
            Some(first.snapshot()),
            Some(second.snapshot()),
            vec![Replacement::new(at(0, 0), "// actions\n")],
        );
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&first, &second]),
            edits: &[edit],
            membership: &["app.js"],
            explicit: &[],
        });
        assert_eq!(found.classes, [DeclarationClass::Continued(id(A))]);
        assert_eq!(
            found.entries,
            [(
                id(A),
                EntryClass::Continued(declaration(&current, &second, 0))
            )]
        );
        assert!(found.details.is_empty());
    }

    #[test]
    fn what_no_edit_accounts_for_is_neither_continued_nor_new() {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&first], &[A]);
        let second = Unit::new("app.js", "2", "intent('Store')\n");
        let current = inventory_of(&[&second], Completeness::Complete);
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&first, &second]),
            edits: &[],
            membership: &["app.js"],
            explicit: &[],
        });
        assert_eq!(
            found.classes,
            [DeclarationClass::Unresolved(BasisGap::NewUnproven)]
        );
        assert_eq!(found.entries, [(id(A), unshown(BasisGap::AbsenceUnproven))]);
        assert_eq!(
            found.details,
            [
                ("identity-continuity-missing", 0),
                ("identity-new-unproven", 0)
            ]
        );
    }

    #[test]
    fn an_edit_that_is_not_evidence_shows_nothing() {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&first], &[A]);
        let check = |second: &Unit, edit: SourceEdit, sources: &RetainedSources<'_>| {
            let current = inventory_of(&[second], Completeness::Complete);
            found(&Scene {
                base: &base.registry,
                previous: Some(base.previous()),
                current: &current,
                sources,
                edits: &[edit],
                membership: &["app.js"],
                explicit: &[],
            })
            .entries
        };
        // A line break right before the opening quote touches the literal.
        let broken = Unit::new("app.js", "2", "intent(\n'Save')\n");
        let touching = SourceEdit::new(
            Some(first.snapshot()),
            Some(broken.snapshot()),
            vec![Replacement::new(at(7, 7), "\n")],
        );
        assert_eq!(
            check(&broken, touching.clone(), &retained(&[&first, &broken])),
            [(id(A), unshown(BasisGap::EditAtBoundary))]
        );
        // Without the after bytes the edit cannot be replayed.
        assert_eq!(
            check(&broken, touching, &retained(&[&first])),
            [(id(A), unshown(BasisGap::SourceUnavailable))]
        );
        // An edit whose replacement does not give the after bytes.
        let worded = Unit::new("app.js", "2", "intent('Saved')\n");
        let wrong = SourceEdit::new(
            Some(first.snapshot()),
            Some(worded.snapshot()),
            vec![Replacement::new(at(12, 12), "x")],
        );
        assert_eq!(
            check(&worded, wrong, &retained(&[&first, &worded])),
            [(id(A), unshown(BasisGap::ReplayMismatch))]
        );
    }

    #[test]
    fn a_range_carried_onto_no_declaration_is_carried_elsewhere() {
        // The bytes did not change, but the declaration is now read as a UI
        // literal: the carried range lands on no intent literal.
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&first], &[A]);
        let second = Unit::new("app.js", "2", "intent('Save')\n");
        let current = inventory_with_role(
            &[&second],
            Completeness::Complete,
            OccurrenceRole::UiLiteral,
        );
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&first, &second]),
            edits: &[renamed(&first, &second)],
            membership: &["app.js"],
            explicit: &[],
        });
        assert_eq!(found.entries, [(id(A), unshown(BasisGap::MappedElsewhere))]);
    }

    #[test]
    fn a_held_snapshot_whose_declaration_is_gone_is_unshown_or_ambiguous() {
        // The same bytes, read with another role: the base snapshot is still
        // the inventory's, yet the base declaration is not declared.
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app], &[A]);
        let alone = inventory_with_role(&[&app], Completeness::Complete, OccurrenceRole::UiLiteral);
        let held = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &alone,
            sources: &retained(&[&app]),
            edits: &[],
            membership: &["app.js"],
            explicit: &[],
        });
        assert_eq!(held.entries, [(id(A), unshown(BasisGap::AbsenceUnproven))]);
        // A partial view that shows the unit shows the same, rather than
        // keeping the entry as unseen.
        let partial =
            inventory_with_role(&[&app], Completeness::Partial, OccurrenceRole::UiLiteral);
        let shown = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &partial,
            sources: &retained(&[&app]),
            edits: &[],
            membership: &["app.js"],
            explicit: &[],
        });
        assert_eq!(shown.entries, [(id(A), unshown(BasisGap::AbsenceUnproven))]);
        // An edit from the held snapshot as well is a second account.
        let copy = Unit::new("copy.js", "1", "intent('Save')\n");
        let both = inventory_with_role(
            &[&app, &copy],
            Completeness::Complete,
            OccurrenceRole::UiLiteral,
        );
        let ambiguous = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &both,
            sources: &retained(&[&app, &copy]),
            edits: &[renamed(&app, &copy)],
            membership: &["app.js", "copy.js"],
            explicit: &[],
        });
        assert_eq!(ambiguous.entries, [(id(A), unshown(BasisGap::Ambiguous))]);
    }

    #[test]
    fn an_entry_carried_onto_a_place_another_entry_keeps_competes() {
        // app.js was folded into common.js, which held the same text under
        // its own ID and did not change.
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let common = Unit::new("common.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app, &common], &[A, B]);
        let current = inventory_of(&[&common], Completeness::Complete);
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&app, &common]),
            edits: &[renamed(&app, &common)],
            membership: &["common.js"],
            explicit: &[],
        });
        assert_eq!(
            found.classes,
            [DeclarationClass::Conflict(Conflict::CompetingClaim)]
        );
        assert_eq!(
            found.entries,
            [(id(A), EntryClass::Conflict(Conflict::CompetingClaim))]
        );
        assert_eq!(found.details, [("identity-competing-claim", 2)]);
    }

    #[test]
    fn an_entry_is_gone_only_where_the_evidence_shows_it() {
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let nav = Unit::new("nav.js", "1", "intent('Home')\n");
        let base = Base::of(&[&app, &nav], &[A, B]);
        let current = inventory_of(&[&app], Completeness::Complete);
        let sources = retained(&[&app, &nav]);
        let removal = [SourceEdit::new(
            Some(nav.snapshot()),
            None,
            vec![Replacement::new(at(0, nav.text.len() as u64), "")],
        )];
        let entries = |edits: &[SourceEdit], membership: &[&str]| {
            found(&Scene {
                base: &base.registry,
                previous: Some(base.previous()),
                current: &current,
                sources: &sources,
                edits,
                membership,
                explicit: &[],
            })
            .entries
        };
        // The unit left the scope, with or without an edit saying so.
        assert_eq!(entries(&[], &["app.js"]), [(id(B), EntryClass::Absent)]);
        assert_eq!(
            entries(&removal, &["app.js"]),
            [(id(B), EntryClass::Absent)]
        );
        // The host still counts the unit a member: nothing is automatic.
        assert_eq!(
            entries(&[], &["app.js", "nav.js"]),
            [(id(B), unshown(BasisGap::MembershipMismatch))]
        );
    }

    #[test]
    fn a_replacing_edit_shows_a_declaration_gone_and_its_replacement_new() {
        let first = Unit::new("app.js", "1", "intent('Home')\n");
        let base = Base::of(&[&first], &[A]);
        let second = Unit::new("app.js", "2", "intent('Start')\n");
        let current = inventory_of(&[&second], Completeness::Complete);
        let replaced = SourceEdit::new(
            Some(first.snapshot()),
            Some(second.snapshot()),
            vec![Replacement::new(at(7, 13), "'Start'")],
        );
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&first, &second]),
            edits: &[replaced],
            membership: &["app.js"],
            explicit: &[],
        });
        assert_eq!(found.classes, [DeclarationClass::New]);
        assert_eq!(found.entries, [(id(A), EntryClass::Absent)]);
    }

    #[test]
    fn a_partial_inventory_keeps_what_it_does_not_show() {
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let nav = Unit::new("nav.js", "1", "intent('Home')\n");
        let base = Base::of(&[&app, &nav], &[A, B]);
        let changed = Unit::new("app.js", "2", "intent('Store')\n");
        let partial = inventory_of(&[&changed], Completeness::Partial);
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &partial,
            sources: &retained(&[&app, &nav, &changed]),
            edits: &[],
            membership: &["app.js", "nav.js"],
            explicit: &[],
        });
        // nav.js is not in the view at all, and app.js's old declaration has
        // no account; neither is retired.
        assert_eq!(
            found.entries,
            [(id(A), EntryClass::Kept), (id(B), EntryClass::Kept)]
        );
    }

    #[test]
    fn a_declaration_is_new_only_where_the_evidence_shows_it() {
        // A genesis has no history: everything is new.
        let app = Unit::new("app.js", "1", "intent('Save')\nintent('Home')\n");
        let root = genesis("0123456789abcdef0123456789abcdef");
        let current = inventory_of(&[&app], Completeness::Complete);
        let fresh = found(&Scene {
            base: &root,
            previous: None,
            current: &current,
            sources: &retained(&[&app]),
            edits: &[],
            membership: &["app.js"],
            explicit: &[],
        });
        assert_eq!(
            fresh.classes,
            [DeclarationClass::New, DeclarationClass::New]
        );

        // With history, a copy carried from declarations still in place is
        // not new; text written from nothing in a new unit is.
        let base = Base::of(&[&app], &[A, B]);
        let copy = Unit::new("copy.js", "1", "intent('Save')\nintent('Home')\n");
        let other = Unit::new("other.js", "1", "intent('Next')\n");
        let current = inventory_of(&[&app, &copy, &other], Completeness::Complete);
        let created = SourceEdit::new(
            None,
            Some(other.snapshot()),
            vec![Replacement::new(at(0, 0), &other.text)],
        );
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&app, &copy, &other]),
            edits: &[renamed(&app, &copy), created],
            membership: &["app.js", "copy.js", "other.js"],
            explicit: &[],
        });
        assert_eq!(
            found.classes,
            [
                DeclarationClass::Retained(id(A)),
                DeclarationClass::Retained(id(B)),
                DeclarationClass::Unresolved(BasisGap::NewUnproven),
                DeclarationClass::Unresolved(BasisGap::NewUnproven),
                DeclarationClass::New,
            ]
        );
        assert_eq!(
            found.details,
            [("identity-new-unproven", 1), ("identity-new-unproven", 1)]
        );
        // Without the membership's agreement, nothing is new by itself.
        let unsure = Scene {
            base: &base.registry,
            previous: None,
            current: &current,
            sources: &retained(&[&app, &copy, &other]),
            edits: &[],
            membership: &["app.js", "copy.js", "other.js"],
            explicit: &[],
        };
        assert_eq!(
            found_classes(&unsure)[4],
            DeclarationClass::Unresolved(BasisGap::BasisUnknown)
        );
    }

    fn found_classes(scene: &Scene<'_>) -> Vec<DeclarationClass> {
        found(scene).classes
    }

    #[test]
    fn an_explicit_decision_is_taken_as_bound_and_fitting_or_refused() {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&first], &[A]);
        let second = Unit::new("app.js", "2", "intent('Store')\n");
        let current = inventory_of(&[&second], Completeness::Complete);
        let to = declaration(&current, &second, 0);
        let from = declaration(&base.inventory, &first, 0);
        let moved = IdentityDecision::continuation(
            id(A),
            from,
            to.clone(),
            ContinuationBasis::Explicit(reason()),
        );
        let sources = retained(&[&first, &second]);
        let scene = |explicit: &[ExplicitDecision]| {
            found(&Scene {
                base: &base.registry,
                previous: Some(base.previous()),
                current: &current,
                sources: &sources,
                edits: &[],
                membership: &["app.js"],
                explicit,
            })
        };
        let bound = |decision| {
            ExplicitDecision::new(base.registry.reference(), current.reference(), decision).unwrap()
        };

        let accepted = scene(&[bound(moved.clone())]);
        assert_eq!(accepted.classes, [DeclarationClass::Explicit(id(A))]);
        assert!(accepted.details.is_empty());

        let refused = |explicit: ExplicitDecision| scene(&[explicit]).classes[0].clone();
        let conflict = DeclarationClass::Conflict;
        // Made for the inventory the base was produced from, not this one.
        let stale = ExplicitDecision::new(
            base.registry.reference(),
            base.inventory.reference(),
            moved.clone(),
        )
        .unwrap();
        assert_eq!(refused(stale), conflict(Conflict::BaseMismatch));
        let foreign = MessageIntentId::retained(
            OwnerIdentity::new(OwnerKind::Application, "elsewhere").unwrap(),
            B,
        )
        .unwrap();
        assert_eq!(
            refused(bound(IdentityDecision::allocation(
                foreign,
                to.clone(),
                AllocationBasis::Explicit(reason()),
            ))),
            conflict(Conflict::ForeignOwner)
        );
        assert_eq!(
            refused(bound(IdentityDecision::allocation(
                id(A),
                to.clone(),
                AllocationBasis::Explicit(reason()),
            ))),
            conflict(Conflict::Collision)
        );
        // A continuation from a declaration the entry does not hold.
        assert_eq!(
            refused(bound(IdentityDecision::continuation(
                id(A),
                to.clone(),
                to.clone(),
                ContinuationBasis::Explicit(reason()),
            ))),
            conflict(Conflict::BaseMismatch)
        );

        // Two choices for one declaration compete, whichever comes first,
        // and neither applies: A's old declaration is left unaccounted for.
        let other =
            IdentityDecision::allocation(id(B), to.clone(), AllocationBasis::Explicit(reason()));
        let twice = scene(&[bound(other.clone()), bound(moved.clone())]);
        assert_eq!(
            twice.details,
            [
                ("identity-continuity-missing", 0),
                ("identity-competing-claim", 0),
                ("identity-competing-claim", 1)
            ]
        );
        // An allocation is free to take a declaration the explicit move of
        // its entry left.
        let next = Unit::new("next.js", "1", "intent('Store')\n");
        let alongside = inventory_of(&[&first, &next], Completeness::Complete);
        let kept = declaration(&alongside, &first, 0);
        let elsewhere = declaration(&alongside, &next, 0);
        let bound_alongside = |decision| {
            ExplicitDecision::new(base.registry.reference(), alongside.reference(), decision)
                .unwrap()
        };
        let both_sources = retained(&[&first, &next]);
        let run = |explicit: &[ExplicitDecision]| {
            found(&Scene {
                base: &base.registry,
                previous: Some(base.previous()),
                current: &alongside,
                sources: &both_sources,
                edits: &[],
                membership: &["app.js", "next.js"],
                explicit,
            })
            .classes
        };
        let taken =
            IdentityDecision::allocation(id(C), kept.clone(), AllocationBasis::Explicit(reason()));
        assert_eq!(
            run(&[bound_alongside(taken.clone())])[0],
            conflict(Conflict::CompetingClaim),
            "a declaration its unchanged entry keeps"
        );
        let away = IdentityDecision::continuation(
            id(A),
            kept,
            elsewhere,
            ContinuationBasis::Explicit(reason()),
        );
        assert_eq!(
            run(&[bound_alongside(taken), bound_alongside(away)]),
            [
                DeclarationClass::Explicit(id(C)),
                DeclarationClass::Explicit(id(A))
            ],
            "once the entry is moved"
        );
    }

    #[test]
    fn a_retired_id_comes_back_only_by_restore() {
        // Registry 2 of the committed chain retired cancel; its line is back
        // in checkout revision 3.
        let chain = Chain::load();
        let base = chain.registry(2);
        let cancel = &base.snapshot().entries()[2];
        assert_eq!(cancel.state(), EntryState::Retired);
        let current = chain.inventory(3);
        let back = current.inventory().declarations()[2].occurrence().clone();
        let owned = sources();
        let sources = RetainedSources::new(
            owned
                .iter()
                .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
        )
        .unwrap();
        let class = |decision| {
            let explicit =
                [ExplicitDecision::new(base.reference(), current.reference(), decision).unwrap()];
            found(&Scene {
                base,
                previous: Some(PreviousUpdate {
                    update: chain.update(2),
                    inventory: chain.inventory(2),
                }),
                current,
                sources: &sources,
                edits: &[],
                membership: &["checkout", "nav"],
                explicit: &explicit,
            })
            .classes[2]
                .clone()
        };
        assert_eq!(
            class(IdentityDecision::continuation(
                cancel.intent_id().clone(),
                cancel.declaration().clone(),
                back.clone(),
                ContinuationBasis::Explicit(reason()),
            )),
            DeclarationClass::Conflict(Conflict::RetiredReuse)
        );
        assert_eq!(
            class(IdentityDecision::restoration(
                cancel.intent_id().clone(),
                cancel.declaration().clone(),
                back,
                reason(),
            )),
            DeclarationClass::Explicit(cancel.intent_id().clone())
        );
    }

    #[test]
    fn an_explicit_decision_that_names_no_declaration_or_twice_one_id_is_refused() {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&first], &[A]);
        let second = Unit::new("app.js", "2", "intent('Store')\n");
        let next = Unit::new("next.js", "1", "intent('Next')\n");
        let current = inventory_of(&[&second, &next], Completeness::Complete);
        let sources = retained(&[&first, &second, &next]);
        let bound = |decision| {
            ExplicitDecision::new(base.registry.reference(), current.reference(), decision).unwrap()
        };
        let run = |explicit: &[ExplicitDecision]| {
            found(&Scene {
                base: &base.registry,
                previous: Some(base.previous()),
                current: &current,
                sources: &sources,
                edits: &[],
                membership: &["app.js", "next.js"],
                explicit,
            })
        };
        // A target that is not one of the inventory's declarations: the
        // decision was made for other source.
        let old_place = declaration(&base.inventory, &first, 0);
        let nowhere = run(&[bound(IdentityDecision::allocation(
            id(B),
            old_place,
            AllocationBasis::Explicit(reason()),
        ))]);
        assert!(nowhere.details.contains(&("identity-base-mismatch", 0)));
        assert!(!nowhere
            .classes
            .contains(&DeclarationClass::Conflict(Conflict::BaseMismatch)));
        // One ID given two declarations.
        let twice = run(&[
            bound(IdentityDecision::allocation(
                id(B),
                declaration(&current, &second, 0),
                AllocationBasis::Explicit(reason()),
            )),
            bound(IdentityDecision::allocation(
                id(B),
                declaration(&current, &next, 0),
                AllocationBasis::Explicit(reason()),
            )),
        ]);
        assert_eq!(
            twice.classes,
            [
                DeclarationClass::Conflict(Conflict::CompetingClaim),
                DeclarationClass::Conflict(Conflict::CompetingClaim)
            ]
        );
    }

    #[test]
    fn an_explicit_decision_competes_with_an_entry_carried_onto_its_declaration() {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&first], &[A]);
        let second = Unit::new("app.js", "2", "// actions\nintent('Save')\n");
        let current = inventory_of(&[&second], Completeness::Complete);
        let edit = SourceEdit::new(
            Some(first.snapshot()),
            Some(second.snapshot()),
            vec![Replacement::new(at(0, 0), "// actions\n")],
        );
        let explicit = [ExplicitDecision::new(
            base.registry.reference(),
            current.reference(),
            IdentityDecision::allocation(
                id(B),
                declaration(&current, &second, 0),
                AllocationBasis::Explicit(reason()),
            ),
        )
        .unwrap()];
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&first, &second]),
            edits: &[edit],
            membership: &["app.js"],
            explicit: &explicit,
        });
        assert_eq!(
            found.classes,
            [DeclarationClass::Conflict(Conflict::CompetingClaim)]
        );
        assert_eq!(
            found.entries,
            [(id(A), EntryClass::Conflict(Conflict::CompetingClaim))]
        );
    }

    #[test]
    fn only_a_replaying_edit_into_the_declaration_s_own_unit_shows_it_new() {
        // Two new files with the same text, from a base with history.
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app], &[A]);
        let left = Unit::new("left.js", "1", "intent('Next')\n");
        let right = Unit::new("right.js", "1", "intent('Next')\n");
        let current = inventory_of(&[&app, &left, &right], Completeness::Complete);
        let created = |unit: &Unit, text: &str| {
            SourceEdit::new(
                None,
                Some(unit.snapshot()),
                vec![Replacement::new(at(0, 0), text)],
            )
        };
        let sources = retained(&[&app, &left, &right]);
        let run = |edits: &[SourceEdit]| {
            found(&Scene {
                base: &base.registry,
                previous: Some(base.previous()),
                current: &current,
                sources: &sources,
                edits,
                membership: &["app.js", "left.js", "right.js"],
                explicit: &[],
            })
            .classes
        };
        let unproven = DeclarationClass::Unresolved(BasisGap::NewUnproven);
        // An account of left.js is no account of right.js.
        assert_eq!(
            run(&[created(&left, &left.text)]),
            [
                DeclarationClass::Retained(id(A)),
                DeclarationClass::New,
                unproven.clone()
            ]
        );
        // An account that does not replay shows nothing.
        assert_eq!(run(&[created(&left, "intent('Back')\n")])[1], unproven);
    }

    #[test]
    fn an_explicit_decision_may_keep_an_entry_where_it_is() {
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app], &[A]);
        let here = declaration(&base.inventory, &app, 0);
        let explicit = [ExplicitDecision::new(
            base.registry.reference(),
            base.inventory.reference(),
            IdentityDecision::continuation(
                id(A),
                here.clone(),
                here,
                ContinuationBasis::Explicit(reason()),
            ),
        )
        .unwrap()];
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &base.inventory,
            sources: &retained(&[&app]),
            edits: &[],
            membership: &["app.js"],
            explicit: &explicit,
        });
        assert_eq!(found.classes, [DeclarationClass::Explicit(id(A))]);
    }

    #[test]
    fn an_entry_a_partial_view_does_not_cover_stays_even_if_an_edit_brings_it_in() {
        // nav.js is not in the view. An edit says it became app.js revision
        // 2, but whether nav.js is still there is not seen, so its entry is
        // kept and the declaration it would land on is not new either.
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let nav = Unit::new("nav.js", "1", "intent('Home')\n");
        let base = Base::of(&[&app, &nav], &[A, B]);
        let changed = Unit::new("app.js", "2", "intent('Home')\n");
        let partial = inventory_of(&[&changed], Completeness::Partial);
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &partial,
            sources: &retained(&[&app, &nav, &changed]),
            edits: &[renamed(&nav, &changed)],
            membership: &["app.js", "nav.js"],
            explicit: &[],
        });
        assert_eq!(
            found.entries,
            [(id(A), EntryClass::Kept), (id(B), EntryClass::Kept)]
        );
        assert_eq!(
            found.classes,
            [DeclarationClass::Unresolved(BasisGap::NewUnproven)]
        );
    }

    #[test]
    fn a_declaration_both_claimed_and_inserted_is_not_new() {
        // app.js did not change, and its entry is moved away by choice. The
        // host's edit says nav.js became app.js, written whole: the old
        // declaration's place is inserted text under it, and still the old
        // declaration's place.
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let nav = Unit::new("nav.js", "1", "intent('Home')\n");
        let base = Base::of(&[&app, &nav], &[A, B]);
        let next = Unit::new("next.js", "1", "intent('Next')\n");
        let current = inventory_of(&[&app, &next], Completeness::Complete);
        let written = SourceEdit::new(
            Some(nav.snapshot()),
            Some(app.snapshot()),
            vec![Replacement::new(at(0, nav.text.len() as u64), &app.text)],
        );
        let explicit = [ExplicitDecision::new(
            base.registry.reference(),
            current.reference(),
            IdentityDecision::continuation(
                id(A),
                declaration(&base.inventory, &app, 0),
                declaration(&current, &next, 0),
                ContinuationBasis::Explicit(reason()),
            ),
        )
        .unwrap()];
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&app, &nav, &next]),
            edits: &[written],
            membership: &["app.js", "next.js"],
            explicit: &explicit,
        });
        assert_eq!(
            found.classes[0],
            DeclarationClass::Unresolved(BasisGap::NewUnproven)
        );
    }

    #[test]
    fn an_entry_carried_onto_a_place_its_holder_was_moved_from_still_competes() {
        // home.js did not change; its entry is moved to next.js by choice.
        // An edit says old.js became home.js: old.js's entry would land on
        // home's place, which still holds home's text.
        let home = Unit::new("home.js", "1", "intent('Save')\n");
        let old = Unit::new("old.js", "1", "intent('Save')\n");
        let base = Base::of(&[&home, &old], &[A, B]);
        let next = Unit::new("next.js", "1", "intent('Next')\n");
        let current = inventory_of(&[&home, &next], Completeness::Complete);
        let explicit = [ExplicitDecision::new(
            base.registry.reference(),
            current.reference(),
            IdentityDecision::continuation(
                id(A),
                declaration(&base.inventory, &home, 0),
                declaration(&current, &next, 0),
                ContinuationBasis::Explicit(reason()),
            ),
        )
        .unwrap()];
        let found = found(&Scene {
            base: &base.registry,
            previous: Some(base.previous()),
            current: &current,
            sources: &retained(&[&home, &old, &next]),
            edits: &[renamed(&old, &home)],
            membership: &["home.js", "next.js"],
            explicit: &explicit,
        });
        // The planner leaves explicit moves for reconciliation to record.
        assert_eq!(
            found.entries,
            [(id(B), EntryClass::Conflict(Conflict::CompetingClaim))]
        );
        assert_eq!(
            found.classes[0],
            DeclarationClass::Conflict(Conflict::CompetingClaim)
        );
    }
}
