// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 016's reconciliation: one update planned from an inventory against
//! an exact registry base.
//!
//! The procedure follows 016's nine steps. The inputs are admitted first:
//! the base and the inventory have to pair, and the host's evidence and
//! candidates have to be well formed. The inventory's declarations were
//! classified by the Producer on their own, without reading history.
//! Explicit decisions bound to this exact base and inventory are applied
//! next. An entry that still holds its exact declaration keeps it; a
//! verified edit carries an entry onto exactly one current declaration;
//! what is left is new where the evidence shows it, and an entry is gone
//! where the evidence shows that. New declarations take the caller's
//! candidates in canonical order. Anything the evidence does not show is
//! reported, never guessed: no old ID is picked by wording or position, and
//! no fresh ID replaces one whose history was merely not found.
//!
//! The planner only proposes. Its plan is sealed, admitted and applied to
//! the base like any other update, and its bases are checked by
//! [`verify_bases`] against the same evidence, so planning and checking
//! cannot disagree about what a basis needs. Step 9, publishing the plan
//! under the exact-base rule, is the host's operation.

mod classify;
mod diagnostic;
mod explicit;
mod outcome;

use std::collections::BTreeSet;

use intlify_authoring::{
    AdmittedInventory, AuthoringArtifact, AuthoringInventory, Diagnostic, MessageIntentId,
    VersionedIdentity,
};

use self::classify::{Associations, Planner};
pub use self::diagnostic::detail;
pub use self::explicit::{ExplicitDecision, NotExplicit};
pub use self::outcome::{
    CandidateFailure, Classification, Conflict, DeclarationClass, Eligibility, EntryClass, Plan,
    ReconcileFailure, Reconciliation, Unresolved,
};
use crate::admission::IdentityAdmissionFailure;
use crate::continuity::{
    automatic, check_previous, check_supplied, verify_bases, BasisVerdict, ContinuityInputs,
    Evidence, EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION,
};
use crate::limits::{IdentityLimitKind, IdentityLimits};
use crate::registry::{
    admit_update_artifact, apply, check_pairing, AdmittedRegistry, AllocationBasis,
    ContinuationBasis, IdentityDecision, IntentRegistryUpdate, LineageLink, RegistryEntry,
    RegistryUpdateArtifact, SourceEdit, Transition,
};
use crate::workspace::IdentityWorkspace;

/// What a host supplies to reconcile an inventory against a base.
#[derive(Debug, Clone, Copy)]
pub struct ReconcileInputs<'a> {
    /// The continuity evidence: retained bytes, edits between snapshots,
    /// the owning scope's units, and the update that produced the base.
    pub evidence: ContinuityInputs<'a>,
    /// Explicit decisions, each bound to the base and inventory it was made
    /// for.
    pub explicit: &'a [ExplicitDecision],
    /// Lineage links the plan records as they are.
    pub links: &'a [LineageLink],
    /// Fresh Intent IDs for new declarations, in the order to use them. The
    /// host draws them; nothing here generates one.
    pub candidates: &'a [MessageIntentId],
}

/// Reconcile an inventory against an exact base.
pub fn reconcile(
    base: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    inputs: &ReconcileInputs<'_>,
    limits: &IdentityLimits,
    workspace: &mut IdentityWorkspace,
) -> Result<Reconciliation, ReconcileFailure> {
    reconcile_with_cancellation(base, inventory, inputs, limits, workspace, &|| false)
}

/// Reconcile an inventory against an exact base under a caller-owned
/// cancellation probe.
///
/// The probe is consulted between steps. Cancelling yields no partial
/// result: a stopped reconciliation establishes nothing, so it can never be
/// read as an absence of declarations or of evidence.
pub fn reconcile_with_cancellation<C>(
    base: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    inputs: &ReconcileInputs<'_>,
    limits: &IdentityLimits,
    workspace: &mut IdentityWorkspace,
    cancelled: &C,
) -> Result<Reconciliation, ReconcileFailure>
where
    C: Fn() -> bool + ?Sized,
{
    workspace.clear();
    let current = inventory.inventory();
    admit(base, current, inputs, limits)?;
    let stop = || {
        if cancelled() {
            Err(ReconcileFailure::Cancelled)
        } else {
            Ok(())
        }
    };

    let evidence = Evidence::gather(&[], inputs.evidence.edits, inputs.evidence.sources, current);
    let claims = evidence.claims(base);
    let planner = Planner {
        associations: Associations {
            base,
            current,
            evidence: &evidence,
            claims: &claims,
        },
        inventory: inventory.reference(),
        membership: inputs.evidence.membership,
        automatic: automatic(base, current, &inputs.evidence),
    };
    workspace.classes.resize(current.declarations().len(), None);
    stop()?;
    let (explicit, moved) = planner.explicit(inputs.explicit, workspace);
    stop()?;
    let gone = planner.associations.retained(&moved, workspace);
    stop()?;
    let (continued, absent) = planner.carried(gone, workspace);
    stop()?;
    planner.remaining(workspace);
    for decision in &explicit {
        if !matches!(decision, IdentityDecision::Allocate(_)) {
            workspace
                .entries
                .push((decision.intent_id().clone(), EntryClass::Explicit));
        }
    }

    if workspace.diagnostics.len() as u64 > limits.diagnostics {
        return Err(ReconcileFailure::Limit(IdentityLimitKind::Diagnostics));
    }
    let classification = classify(current, workspace);
    if !workspace.diagnostics.is_empty() {
        return Ok(unresolved(&mut workspace.diagnostics, classification));
    }
    stop()?;

    let fresh = allocate(base, &classification, inputs.candidates, &explicit)?;
    let decisions = explicit
        .into_iter()
        .cloned()
        .chain(continued.iter().map(|(entry, edit, index)| {
            continuation(
                entry,
                edit,
                current.declarations()[*index].occurrence().clone(),
            )
        }))
        .chain(fresh)
        .chain(absent.iter().map(|entry| {
            IdentityDecision::retirement(entry.intent_id().clone(), entry.declaration().clone())
        }))
        .collect();
    plan(base, inventory, inputs, limits, decisions, classification)
}

/// Step 1: hold the inputs to the rules that make them inputs at all.
fn admit(
    base: &AdmittedRegistry,
    current: &AuthoringInventory,
    inputs: &ReconcileInputs<'_>,
    limits: &IdentityLimits,
) -> Result<(), ReconcileFailure> {
    check_pairing(base, current).map_err(ReconcileFailure::Pairing)?;
    check_previous(base, inputs.evidence.previous).map_err(ReconcileFailure::Evidence)?;
    check_supplied(
        inputs.evidence.edits,
        base,
        current,
        inputs.evidence.previous,
        limits,
    )
    .map_err(ReconcileFailure::Evidence)?;
    let candidates = inputs.candidates;
    if candidates.len() as u64 > limits.candidates {
        return Err(ReconcileFailure::Limit(IdentityLimitKind::Candidates));
    }
    let owner = base.snapshot().owner();
    if candidates
        .iter()
        .any(|candidate| candidate.owner() != owner)
    {
        return Err(ReconcileFailure::Candidates(CandidateFailure::ForeignOwner));
    }
    let distinct: BTreeSet<&MessageIntentId> = candidates.iter().collect();
    if distinct.len() != candidates.len() {
        return Err(ReconcileFailure::Candidates(CandidateFailure::Duplicate));
    }
    Ok(())
}

/// Take the classes out of the workspace, in their reporting order.
fn classify(current: &AuthoringInventory, workspace: &mut IdentityWorkspace) -> Classification {
    let declarations = current
        .declarations()
        .iter()
        .zip(workspace.classes.drain(..))
        .map(|(facts, class)| {
            (
                facts.occurrence().clone(),
                class.expect("every declaration is classified"),
            )
        })
        .collect();
    workspace
        .entries
        .sort_by(|left, right| left.0.cmp(&right.0));
    Classification {
        declarations,
        entries: workspace.entries.drain(..).collect(),
    }
}

/// Report what no plan could be made from, in 016's reporting order.
fn unresolved(diagnostics: &mut Vec<Diagnostic>, classification: Classification) -> Reconciliation {
    diagnostics.sort_by(Diagnostic::reporting_cmp);
    Reconciliation::Unresolved(Box::new(Unresolved {
        diagnostics: diagnostics.drain(..).collect(),
        classification,
    }))
}

/// Step 6: give each new declaration the next candidate, in canonical
/// declaration order.
///
/// A candidate the base already holds, active or retired, or one an explicit
/// decision names, is refused rather than skipped: the host draws a new one.
fn allocate(
    base: &AdmittedRegistry,
    classification: &Classification,
    candidates: &[MessageIntentId],
    explicit: &[&IdentityDecision],
) -> Result<Vec<IdentityDecision>, ReconcileFailure> {
    let new: Vec<_> = classification
        .declarations()
        .iter()
        .filter(|(_, class)| *class == DeclarationClass::New)
        .map(|(declaration, _)| declaration)
        .collect();
    if new.len() > candidates.len() {
        return Err(ReconcileFailure::Candidates(CandidateFailure::Exhausted {
            needed: new.len(),
        }));
    }
    let snapshot = base.snapshot();
    let mut allocations = Vec::with_capacity(new.len());
    for (declaration, candidate) in new.into_iter().zip(candidates) {
        let named = explicit
            .iter()
            .any(|decision| decision.intent_id() == candidate);
        if snapshot.entry(candidate).is_some() || named {
            return Err(ReconcileFailure::Candidates(CandidateFailure::Collision(
                candidate.clone(),
            )));
        }
        allocations.push(IdentityDecision::allocation(
            candidate.clone(),
            declaration.clone(),
            AllocationBasis::confirmed_new(),
        ));
    }
    Ok(allocations)
}

/// A continuation the one edit that carries it establishes, as its whole
/// change list.
fn continuation(
    entry: &RegistryEntry,
    edit: &SourceEdit,
    to: intlify_authoring::Occurrence,
) -> IdentityDecision {
    IdentityDecision::continuation(
        entry.intent_id().clone(),
        entry.declaration().clone(),
        to,
        ContinuationBasis::verified_edit(
            VersionedIdentity::literal(EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION),
            vec![edit.clone()],
        ),
    )
}

/// Step 8: form the update, apply it to its base and check its bases.
fn plan(
    base: &AdmittedRegistry,
    inventory: &AdmittedInventory,
    inputs: &ReconcileInputs<'_>,
    limits: &IdentityLimits,
    decisions: Vec<IdentityDecision>,
    classification: Classification,
) -> Result<Reconciliation, ReconcileFailure> {
    let update = IntentRegistryUpdate::new(
        base.snapshot().owner().clone(),
        base.reference(),
        inventory.reference(),
        decisions,
        inputs.links.to_vec(),
    )
    .map_err(ReconcileFailure::Update)?;
    // An update with no decisions and no links applies as no change.
    let sealed = RegistryUpdateArtifact::seal(update).map_err(|_| ReconcileFailure::Unsealable)?;
    let update = admit_update_artifact(sealed, limits).map_err(|failure| match failure {
        IdentityAdmissionFailure::Limit(kind) => ReconcileFailure::Limit(kind),
        IdentityAdmissionFailure::Structure(failure) => ReconcileFailure::Update(failure),
        IdentityAdmissionFailure::Read(_) => ReconcileFailure::Unsealable,
    })?;
    let result = match apply(base, &update, inventory).map_err(ReconcileFailure::Transition)? {
        Transition::Applied(result) => *result,
        Transition::Unchanged => return Ok(Reconciliation::Unchanged),
    };
    let report = verify_bases(base, &update, inventory, &inputs.evidence, limits)
        .map_err(ReconcileFailure::Evidence)?;

    // The planner proposes only what the rules show, so every proposed basis
    // should be proven. Anything the check finds unproven is reported as if
    // the planner had found it, never planned around.
    let mut eligibility = Vec::with_capacity(report.verdicts().len());
    let mut gaps = Vec::new();
    for (decision, (id, verdict)) in update.update().decisions().iter().zip(report.verdicts()) {
        match verdict {
            BasisVerdict::Proven => eligibility.push((id.clone(), Eligibility::Automatic)),
            BasisVerdict::Explicit => {
                eligibility.push((id.clone(), Eligibility::RequiresConfirmation));
            }
            BasisVerdict::Unproven(gap) => {
                let location = decision
                    .to()
                    .or_else(|| decision.from())
                    .expect("a decision names a declaration")
                    .clone();
                gaps.push(diagnostic::update_required(location, *gap, vec![]));
            }
        }
    }
    if !gaps.is_empty() {
        return Ok(unresolved(&mut gaps, classification));
    }
    Ok(Reconciliation::Planned(Box::new(Plan {
        update,
        result,
        eligibility: eligibility.into_boxed_slice(),
        classification,
    })))
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{OwnerIdentity, OwnerKind, SourceSnapshot, Token};

    use super::*;
    use crate::continuity::{ContinuityFailure, EditSetFailure, PreviousUpdate, RetainedSources};
    use crate::registry::fixtures::{artifact, id, limits, sources, Chain};
    use crate::registry::{ExplicitBasis, LineageKind, TransitionFailure, UpdateFailure};

    /// The inputs one case varies.
    #[derive(Default)]
    struct Case<'c> {
        edits: &'c [SourceEdit],
        explicit: &'c [ExplicitDecision],
        links: &'c [LineageLink],
        candidates: &'c [MessageIntentId],
    }

    /// Reconcile inventory `current` against registry `base`, with the
    /// committed sources, both units as members, and the update that
    /// produced the base.
    fn run_with(
        chain: &Chain,
        (base, current): (usize, usize),
        case: &Case<'_>,
        limits: &IdentityLimits,
        workspace: &mut IdentityWorkspace,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Reconciliation, ReconcileFailure> {
        let owned: Vec<(SourceSnapshot, String)> = sources();
        let all = RetainedSources::new(
            owned
                .iter()
                .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
        )
        .unwrap();
        let membership = [Token::new("checkout").unwrap(), Token::new("nav").unwrap()];
        let inputs = ReconcileInputs {
            evidence: ContinuityInputs {
                sources: &all,
                edits: case.edits,
                membership: &membership,
                previous: (base > 0).then(|| PreviousUpdate {
                    update: chain.update(base),
                    inventory: chain.inventory(base),
                }),
            },
            explicit: case.explicit,
            links: case.links,
            candidates: case.candidates,
        };
        reconcile_with_cancellation(
            chain.registry(base),
            chain.inventory(current),
            &inputs,
            limits,
            workspace,
            cancelled,
        )
    }

    fn run(
        chain: &Chain,
        pair: (usize, usize),
        case: &Case<'_>,
    ) -> Result<Reconciliation, ReconcileFailure> {
        run_with(
            chain,
            pair,
            case,
            &limits(),
            &mut IdentityWorkspace::new(),
            &|| false,
        )
    }

    fn planned(result: Result<Reconciliation, ReconcileFailure>) -> Plan {
        match result {
            Ok(Reconciliation::Planned(plan)) => *plan,
            other => panic!("not planned: {other:?}"),
        }
    }

    fn unresolved(result: Result<Reconciliation, ReconcileFailure>) -> Unresolved {
        match result {
            Ok(Reconciliation::Unresolved(report)) => *report,
            other => panic!("not unresolved: {other:?}"),
        }
    }

    /// One decision of a committed update.
    fn committed(n: usize, index: usize) -> IdentityDecision {
        serde_json::from_value(artifact(&format!("update-{n}"))["body"]["decisions"][index].clone())
            .unwrap()
    }

    /// The edit update `n` carries.
    fn the_edit(n: usize) -> SourceEdit {
        serde_json::from_value(
            artifact(&format!("update-{n}"))["body"]["decisions"][0]["basis"]["changes"][0].clone(),
        )
        .unwrap()
    }

    fn id_of(n: usize, index: usize) -> MessageIntentId {
        committed(n, index).intent_id().clone()
    }

    #[test]
    fn the_committed_chain_is_planned_again_from_its_own_evidence() {
        let chain = Chain::load();
        // Update 1: the genesis has no history. Candidates go to the
        // declarations in canonical order: pay, cancel, then home.
        let first = planned(run(
            &chain,
            (0, 1),
            &Case {
                candidates: &[id_of(1, 0), id_of(1, 2), id_of(1, 1)],
                ..Case::default()
            },
        ));
        assert_eq!(first.update().reference(), chain.update(1).reference());
        assert_eq!(first.result(), chain.registry(1).snapshot());
        assert!(!first.requires_confirmation());

        // Update 2: pay continues across the edit, cancel's line is
        // replaced, and the copy is new; the copy link is the host's.
        let copy = id_of(2, 2);
        let link = LineageLink::new(LineageKind::Copy, vec![id_of(2, 0)], vec![copy.clone()]);
        let second = planned(run(
            &chain,
            (1, 2),
            &Case {
                edits: &[the_edit(2)],
                links: &[link],
                candidates: &[copy],
                ..Case::default()
            },
        ));
        assert_eq!(second.update().reference(), chain.update(2).reference());
        assert_eq!(second.result(), chain.registry(2).snapshot());
        assert_eq!(
            second.eligibility(),
            [
                (id_of(2, 0), Eligibility::Automatic),
                (id_of(2, 1), Eligibility::Automatic),
                (id_of(2, 2), Eligibility::Automatic)
            ]
        );

        // Update 3: pay and the copy continue; cancel's line is back, and
        // only an explicit restore gives it its old ID.
        let restore = ExplicitDecision::new(
            chain.registry(2).reference(),
            chain.inventory(3).reference(),
            committed(3, 1),
        )
        .unwrap();
        let third = planned(run(
            &chain,
            (2, 3),
            &Case {
                edits: &[the_edit(3)],
                explicit: &[restore],
                ..Case::default()
            },
        ));
        assert_eq!(third.update().reference(), chain.update(3).reference());
        assert_eq!(third.result(), chain.registry(3).snapshot());
        assert_eq!(
            third.eligibility(),
            [
                (id_of(3, 0), Eligibility::Automatic),
                (id_of(3, 1), Eligibility::RequiresConfirmation),
                (id_of(3, 2), Eligibility::Automatic)
            ]
        );
        assert!(third.requires_confirmation());
        assert_eq!(
            third.classification().declarations()[2].1,
            DeclarationClass::Explicit(id_of(3, 1))
        );
    }

    #[test]
    fn an_inventory_nothing_changed_in_changes_nothing() {
        let chain = Chain::load();
        assert_eq!(
            run(&chain, (1, 1), &Case::default()),
            Ok(Reconciliation::Unchanged)
        );
    }

    #[test]
    fn what_the_evidence_does_not_show_is_reported_and_nothing_is_planned() {
        let chain = Chain::load();
        let report = unresolved(run(
            &chain,
            (1, 2),
            &Case {
                candidates: &[id_of(2, 2)],
                ..Case::default()
            },
        ));
        let details: Vec<&str> = report
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.detail().unwrap().as_str())
            .collect();
        // Pay's and cancel's old places first, then the two new places.
        assert_eq!(
            details,
            [
                "identity-continuity-missing",
                "identity-continuity-missing",
                "identity-new-unproven",
                "identity-new-unproven"
            ]
        );
        assert_eq!(
            report.classification().entries(),
            [
                (
                    id_of(2, 0),
                    EntryClass::Unresolved(crate::continuity::BasisGap::AbsenceUnproven)
                ),
                (
                    id_of(2, 1),
                    EntryClass::Unresolved(crate::continuity::BasisGap::AbsenceUnproven)
                )
            ]
        );
        assert_eq!(
            report.classification().declarations()[2].1,
            DeclarationClass::Retained(
                chain.registry(1).snapshot().entries()[1]
                    .intent_id()
                    .clone()
            )
        );
    }

    #[test]
    fn candidates_are_used_in_order_and_never_made_up() {
        let chain = Chain::load();
        let edits = [the_edit(2)];
        let with = |candidates: &[MessageIntentId]| {
            run(
                &chain,
                (1, 2),
                &Case {
                    edits: &edits,
                    candidates,
                    ..Case::default()
                },
            )
        };
        let candidate = |failure| Err(ReconcileFailure::Candidates(failure));
        assert_eq!(
            with(&[]),
            candidate(CandidateFailure::Exhausted { needed: 1 })
        );
        // Pay's ID is held, cancel's is held: neither is a fresh one.
        assert_eq!(
            with(&[id_of(2, 0)]),
            candidate(CandidateFailure::Collision(id_of(2, 0)))
        );
        let fresh = id(&"f".repeat(32));
        assert_eq!(
            with(&[fresh.clone(), fresh.clone()]),
            candidate(CandidateFailure::Duplicate)
        );
        let foreign = MessageIntentId::retained(
            OwnerIdentity::new(OwnerKind::Application, "elsewhere").unwrap(),
            &"f".repeat(32),
        )
        .unwrap();
        assert_eq!(with(&[foreign]), candidate(CandidateFailure::ForeignOwner));
        // A spare candidate is left unused.
        let plan = planned(with(&[fresh.clone(), id(&"e".repeat(32))]));
        assert!(plan
            .update()
            .update()
            .decisions()
            .iter()
            .any(|decision| decision.intent_id() == &fresh));

        // A retired ID is never fresh: in registry 2 cancel is retired, and
        // its line is back in inventory 3 as new text.
        let retired = run(
            &chain,
            (2, 3),
            &Case {
                edits: &[the_edit(3)],
                candidates: &[id_of(3, 1)],
                ..Case::default()
            },
        );
        assert_eq!(retired, candidate(CandidateFailure::Collision(id_of(3, 1))));
        // Nor is an ID an explicit decision names: from the genesis, pay is
        // given one by choice and the next new declaration may not take it.
        let named = id(&"d".repeat(32));
        let pay = chain.inventory(1).inventory().declarations()[0]
            .occurrence()
            .clone();
        let chosen = ExplicitDecision::new(
            chain.registry(0).reference(),
            chain.inventory(1).reference(),
            IdentityDecision::allocation(
                named.clone(),
                pay,
                AllocationBasis::Explicit(ExplicitBasis::new("Chosen by hand.").unwrap()),
            ),
        )
        .unwrap();
        assert_eq!(
            run(
                &chain,
                (0, 1),
                &Case {
                    explicit: &[chosen],
                    candidates: &[named.clone(), fresh],
                    ..Case::default()
                },
            ),
            candidate(CandidateFailure::Collision(named))
        );
    }

    #[test]
    fn inputs_that_cannot_be_planned_from_stop_the_reconciliation() {
        let chain = Chain::load();
        // An inventory of another scope than the base's.
        let elsewhere = crate::registry::fixtures::genesis_of_scope("elsewhere-web");
        let owned = sources();
        let all = RetainedSources::new(
            owned
                .iter()
                .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
        )
        .unwrap();
        let membership = [Token::new("checkout").unwrap(), Token::new("nav").unwrap()];
        let inputs = |previous| ReconcileInputs {
            evidence: ContinuityInputs {
                sources: &all,
                edits: &[],
                membership: &membership,
                previous,
            },
            explicit: &[],
            links: &[],
            candidates: &[],
        };
        assert_eq!(
            reconcile(
                &elsewhere,
                chain.inventory(1),
                &inputs(None),
                &limits(),
                &mut IdentityWorkspace::new()
            ),
            Err(ReconcileFailure::Pairing(TransitionFailure::ScopeMismatch))
        );
        // An update that did not produce the base.
        assert_eq!(
            reconcile(
                chain.registry(1),
                chain.inventory(2),
                &inputs(Some(PreviousUpdate {
                    update: chain.update(2),
                    inventory: chain.inventory(2),
                })),
                &limits(),
                &mut IdentityWorkspace::new()
            ),
            Err(ReconcileFailure::Evidence(
                ContinuityFailure::PreviousMismatch
            ))
        );
        // An edit that ends at a revision the inventory does not hold.
        let stale = [the_edit(2)];
        assert_eq!(
            run(
                &chain,
                (1, 3),
                &Case {
                    edits: &stale,
                    ..Case::default()
                }
            ),
            Err(ReconcileFailure::Evidence(ContinuityFailure::EditSet(
                EditSetFailure::AfterOutsideInventory
            )))
        );
        // A copy link whose successor the plan does not allocate.
        let link = LineageLink::new(LineageKind::Copy, vec![id_of(2, 0)], vec![id_of(2, 1)]);
        let edits = [the_edit(2)];
        assert_eq!(
            run(
                &chain,
                (1, 2),
                &Case {
                    edits: &edits,
                    links: &[link],
                    candidates: &[id_of(2, 2)],
                    ..Case::default()
                }
            ),
            Err(ReconcileFailure::Update(UpdateFailure::CopyNotAllocated))
        );
        // A link to a predecessor the base does not hold.
        let unknown = LineageLink::new(
            LineageKind::Split,
            vec![id(&"9".repeat(32))],
            vec![id_of(2, 0), id_of(2, 2)],
        );
        assert_eq!(
            run(
                &chain,
                (1, 2),
                &Case {
                    edits: &edits,
                    links: &[unknown],
                    candidates: &[id_of(2, 2)],
                    ..Case::default()
                }
            ),
            Err(ReconcileFailure::Transition(
                TransitionFailure::UnresolvedLink
            ))
        );
    }

    #[test]
    fn bounds_and_cancellation_stop_before_anything_is_planned() {
        let chain = Chain::load();
        let edits = [the_edit(2)];
        let case = Case {
            edits: &edits,
            candidates: &[id_of(2, 2)],
            ..Case::default()
        };
        let mut workspace = IdentityWorkspace::new();
        let exact = IdentityLimits {
            candidates: 1,
            diagnostics: 4,
            ..limits()
        };
        assert!(matches!(
            run_with(&chain, (1, 2), &case, &exact, &mut workspace, &|| false),
            Ok(Reconciliation::Planned(_))
        ));
        assert!(matches!(
            run_with(
                &chain,
                (1, 2),
                &Case::default(),
                &exact,
                &mut workspace,
                &|| false
            ),
            Ok(Reconciliation::Unresolved(_))
        ));
        let tight = IdentityLimits {
            candidates: 0,
            ..limits()
        };
        assert_eq!(
            run_with(&chain, (1, 2), &case, &tight, &mut workspace, &|| false),
            Err(ReconcileFailure::Limit(IdentityLimitKind::Candidates))
        );
        let one_decision = IdentityLimits {
            decisions: 2,
            ..limits()
        };
        assert_eq!(
            run_with(
                &chain,
                (1, 2),
                &case,
                &one_decision,
                &mut workspace,
                &|| false
            ),
            Err(ReconcileFailure::Limit(IdentityLimitKind::Decisions))
        );
        // Four diagnostics against a bound of three.
        let quiet = IdentityLimits {
            diagnostics: 3,
            ..limits()
        };
        assert_eq!(
            run_with(
                &chain,
                (1, 2),
                &Case::default(),
                &quiet,
                &mut workspace,
                &|| false
            ),
            Err(ReconcileFailure::Limit(IdentityLimitKind::Diagnostics))
        );
        // The probe is asked between every two steps, and stopping at any
        // of them gives nothing back.
        let calls = std::cell::Cell::new(0);
        let counting = || {
            calls.set(calls.get() + 1);
            false
        };
        assert!(matches!(
            run_with(&chain, (1, 2), &case, &limits(), &mut workspace, &counting),
            Ok(Reconciliation::Planned(_))
        ));
        assert_eq!(calls.get(), 5);
        for stop_at in 0..5 {
            calls.set(0);
            let probe = || {
                calls.set(calls.get() + 1);
                calls.get() > stop_at
            };
            assert_eq!(
                run_with(&chain, (1, 2), &case, &limits(), &mut workspace, &probe),
                Err(ReconcileFailure::Cancelled),
                "stopped at check {stop_at}"
            );
        }
    }

    #[test]
    fn a_reused_workspace_gives_what_a_fresh_one_gives() {
        let chain = Chain::load();
        let edits = [the_edit(2)];
        let case = Case {
            edits: &edits,
            candidates: &[id_of(2, 2)],
            ..Case::default()
        };
        let mut workspace = IdentityWorkspace::new();
        let missing = run_with(
            &chain,
            (1, 2),
            &Case::default(),
            &limits(),
            &mut workspace,
            &|| false,
        );
        let reused = run_with(&chain, (1, 2), &case, &limits(), &mut workspace, &|| false);
        assert_eq!(reused, run(&chain, (1, 2), &case));
        // A run stopped halfway, with declarations and entries classified
        // and diagnostics found, leaves nothing behind for the next one.
        let calls = std::cell::Cell::new(0);
        let halfway = || {
            calls.set(calls.get() + 1);
            calls.get() > 3
        };
        assert_eq!(
            run_with(
                &chain,
                (1, 2),
                &Case::default(),
                &limits(),
                &mut workspace,
                &halfway
            ),
            Err(ReconcileFailure::Cancelled)
        );
        assert_eq!(
            run_with(&chain, (1, 2), &case, &limits(), &mut workspace, &|| false),
            run(&chain, (1, 2), &case)
        );
        assert!(matches!(missing, Ok(Reconciliation::Unresolved(_))));
        assert!(workspace.capacities().diagnostics >= 4);
    }

    #[test]
    fn the_order_explicit_decisions_come_in_does_not_matter() {
        let chain = Chain::load();
        let pay = &chain.registry(1).snapshot().entries()[0];
        let current = chain.inventory(2).inventory().declarations();
        let reason = || ExplicitBasis::new("Chosen by hand.").unwrap();
        let bound = |decision| {
            ExplicitDecision::new(
                chain.registry(1).reference(),
                chain.inventory(2).reference(),
                decision,
            )
            .unwrap()
        };
        let moved = bound(IdentityDecision::continuation(
            pay.intent_id().clone(),
            pay.declaration().clone(),
            current[0].occurrence().clone(),
            ContinuationBasis::Explicit(reason()),
        ));
        let copied = bound(IdentityDecision::allocation(
            id(&"f".repeat(32)),
            current[1].occurrence().clone(),
            AllocationBasis::Explicit(reason()),
        ));
        let edits = [the_edit(2)];
        let with = |explicit: &[ExplicitDecision]| {
            planned(run(
                &chain,
                (1, 2),
                &Case {
                    edits: &edits,
                    explicit,
                    ..Case::default()
                },
            ))
        };
        let forward = with(&[moved.clone(), copied.clone()]);
        let backward = with(&[copied, moved]);
        assert_eq!(forward, backward);
        // Pay's explicit move and cancel's absence, in Intent ID order.
        assert_eq!(
            forward.classification().entries(),
            [
                (pay.intent_id().clone(), EntryClass::Explicit),
                (id_of(2, 1), EntryClass::Absent)
            ]
        );
        assert_eq!(
            forward.eligibility().len(),
            3,
            "the two choices and cancel's retirement"
        );
    }
}
