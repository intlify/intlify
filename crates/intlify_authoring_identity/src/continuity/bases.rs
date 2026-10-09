// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checking the bases an update's decisions claim.
//!
//! Applying an update checks the transition. This checks why each decision
//! was made: that a `verified-edit` really carries the old declaration to the
//! new one and nothing else competes for it, that a `confirmed-new`
//! declaration is shown to be new, and that a `complete-absence` retirement is
//! shown to be gone. Each claim is either proven from the evidence, left to an
//! explicit choice, or not shown. Not shown is never turned into a guess:
//! wording, a path, a position, the order declarations were found in and
//! similarity are not evidence of anything here.
//!
//! Newness and absence are proven when an update is planned, from evidence
//! the host supplies: the source edits, the bytes they run over, the host's
//! membership of the owning scope, and the update that produced the base.
//! The update records none of that evidence for them, so a later replay does
//! not prove them again. A `verified-edit` carries its edits, so it can be
//! checked whenever its bytes are retained.

use intlify_authoring::{AdmittedInventory, MessageIntentId, Occurrence, Token};

use super::edit::{fate, inserted, is_edit_replay_profile, RangeFate, ReplayGap};
use super::evidence::{Claims, Evidence};
use super::inputs::{automatic, check_previous, check_supplied, ContinuityInputs, EditSetFailure};
use crate::limits::{IdentityLimitKind, IdentityLimits};
use crate::registry::{
    apply, AdmittedRegistry, AdmittedUpdate, AllocationBasis, ContinuationBasis, IdentityDecision,
    TransitionFailure, UpdateFailure,
};

/// Why a claimed basis is not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasisGap {
    /// A `verified-edit` names a verifier profile this crate does not
    /// implement.
    UnsupportedProfile,
    /// No edit starts from the snapshot the base declaration is in.
    EditMissing,
    /// The edit from the base declaration's snapshot does not end where it
    /// has to: at the current declaration's snapshot for a continuation, or
    /// at a snapshot the inventory holds for an absence.
    EditDisagrees,
    /// A `verified-edit` carries edits beyond the one between its own two
    /// snapshots, so its change list does not agree with its declarations.
    ExtraEdits,
    /// The bytes of a snapshot an edit names were not retained.
    SourceUnavailable,
    /// Replaying an edit gives other bytes than its after snapshot names.
    ReplayMismatch,
    /// A replacement touches or crosses an end of the base declaration.
    EditAtBoundary,
    /// A replacement replaces the whole base declaration.
    DeclarationReplaced,
    /// The base declaration is carried to another range or role.
    MappedElsewhere,
    /// Another base declaration is carried to the same current declaration,
    /// or two edits start from the same snapshot.
    Ambiguous,
    /// Neither an empty base nor inserted text shows the declaration is new.
    NewUnproven,
    /// Neither a removed unit nor a replacing edit shows the declaration is
    /// gone.
    AbsenceUnproven,
    /// The host's membership is not the inventory's units.
    MembershipMismatch,
    /// The inventory was resolved against other pins than the base's.
    BasisChanged,
    /// The update that produced the base was not supplied, so whether the
    /// pins changed is unknown.
    BasisUnknown,
    /// Another association in the update is not shown, so nothing is known
    /// to be absent.
    UnresolvedPlan,
}

/// What checking one decision's basis found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasisVerdict {
    /// The evidence shows the claim.
    Proven,
    /// The basis is an explicit choice. It needs the host's confirmation,
    /// bound to this exact base, inventory and action.
    Explicit,
    /// The evidence does not show the claim.
    Unproven(BasisGap),
}

/// What checking every decision of an update found, in decision order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasisReport {
    verdicts: Box<[(MessageIntentId, BasisVerdict)]>,
}

impl BasisReport {
    /// Borrow each decision's Intent ID and verdict, in decision order.
    #[must_use]
    pub fn verdicts(&self) -> &[(MessageIntentId, BasisVerdict)] {
        &self.verdicts
    }

    /// Return whether every claimed basis is proven from the evidence.
    #[must_use]
    pub fn is_proven(&self) -> bool {
        self.verdicts
            .iter()
            .all(|(_, verdict)| *verdict == BasisVerdict::Proven)
    }

    /// Find the verdict for one Intent ID.
    #[must_use]
    pub fn verdict(&self, id: &MessageIntentId) -> Option<BasisVerdict> {
        self.verdicts
            .binary_search_by(|(decided, _)| decided.cmp(id))
            .ok()
            .map(|index| self.verdicts[index].1)
    }
}

/// Why the bases could not be checked at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuityFailure {
    /// The update does not apply to its base, so its bases mean nothing yet.
    Transition(TransitionFailure),
    /// The previous update is not the one the base names, or is supplied for
    /// a genesis base.
    PreviousMismatch,
    /// A supplied edit is not well formed.
    InvalidEdit(UpdateFailure),
    /// The supplied edits are not one account of how the base became the
    /// current inventory.
    EditSet(EditSetFailure),
    /// A named bound was exhausted by the supplied edits.
    Limit(IdentityLimitKind),
}

/// Check the basis of every decision of an update against its base.
///
/// The update has to apply first; a basis of a decision that does not fit
/// its base is not worth checking. Every decision then gets a verdict, and
/// an unproven one does not stop the others from being checked, so a host
/// sees every gap at once.
pub fn verify_bases(
    base: &AdmittedRegistry,
    update: &AdmittedUpdate,
    inventory: &AdmittedInventory,
    inputs: &ContinuityInputs<'_>,
    limits: &IdentityLimits,
) -> Result<BasisReport, ContinuityFailure> {
    apply(base, update, inventory).map_err(ContinuityFailure::Transition)?;
    let plan = update.update();
    let current = inventory.inventory();
    check_previous(base, inputs.previous)?;
    check_supplied(inputs.edits, base, current, inputs.previous, limits)?;

    let evidence = Evidence::gather(plan.decisions(), inputs.edits, inputs.sources, current);
    let claims = evidence.claims(base);
    let automatic = automatic(base, current, inputs);

    let mut verdicts: Vec<(MessageIntentId, BasisVerdict)> = plan
        .decisions()
        .iter()
        .map(|decision| {
            let verdict = match decision {
                IdentityDecision::Continue(continuation) => match continuation.basis() {
                    ContinuationBasis::UnchangedSnapshot(_) => BasisVerdict::Proven,
                    ContinuationBasis::Explicit(_) => BasisVerdict::Explicit,
                    ContinuationBasis::VerifiedEdit(edit) => continued(
                        decision.intent_id(),
                        continuation.from(),
                        continuation.to(),
                        edit,
                        &evidence,
                        &claims,
                    ),
                },
                IdentityDecision::Allocate(allocation) => match allocation.basis() {
                    AllocationBasis::Explicit(_) => BasisVerdict::Explicit,
                    AllocationBasis::ConfirmedNew(_) => {
                        new(base, allocation.to(), automatic, &evidence, &claims)
                    }
                },
                IdentityDecision::Retire(retirement) => {
                    absent(retirement.from(), automatic, inputs.membership, &evidence)
                }
                IdentityDecision::Restore(_) => BasisVerdict::Explicit,
            };
            (decision.intent_id().clone(), verdict)
        })
        .collect();

    // Absence needs every other association resolved. A decision that is
    // not shown leaves the plan unresolved, and nothing is known to be gone
    // from it.
    let unresolved = plan
        .decisions()
        .iter()
        .zip(&verdicts)
        .any(|(decision, (_, verdict))| {
            !matches!(decision, IdentityDecision::Retire(_))
                && matches!(verdict, BasisVerdict::Unproven(_))
        });
    if unresolved {
        for (decision, (_, verdict)) in plan.decisions().iter().zip(verdicts.iter_mut()) {
            if matches!(decision, IdentityDecision::Retire(_)) && *verdict == BasisVerdict::Proven {
                *verdict = BasisVerdict::Unproven(BasisGap::UnresolvedPlan);
            }
        }
    }
    Ok(BasisReport {
        verdicts: verdicts.into_boxed_slice(),
    })
}

/// Check a `verified-edit` continuation from one declaration to another.
fn continued(
    id: &MessageIntentId,
    from: &Occurrence,
    to: &Occurrence,
    basis: &crate::registry::VerifiedEdit,
    evidence: &Evidence<'_>,
    claims: &Claims,
) -> BasisVerdict {
    let gap = |gap| BasisVerdict::Unproven(gap);
    if !is_edit_replay_profile(basis.profile()) {
        return gap(BasisGap::UnsupportedProfile);
    }
    let Some(edit) = basis
        .changes()
        .iter()
        .find(|edit| edit.before() == Some(from.source()))
    else {
        return gap(BasisGap::EditMissing);
    };
    if edit.after() != Some(to.source()) {
        return gap(BasisGap::EditDisagrees);
    }
    // The change list has to agree with this decision's pair of
    // declarations: one edit, from its snapshot to its snapshot.
    if basis.changes().len() != 1 {
        return gap(BasisGap::ExtraEdits);
    }
    // A second account of the base snapshot, another edit from it or the
    // snapshot still being current, makes this a copy or a conflict.
    if evidence.accounts(from.source()) > 1 {
        return gap(BasisGap::Ambiguous);
    }
    if let Some((_, Err(replayed))) = evidence.from(from.source()).first() {
        return gap(match replayed {
            ReplayGap::SourceUnavailable => BasisGap::SourceUnavailable,
            ReplayGap::Mismatch => BasisGap::ReplayMismatch,
        });
    }
    match fate(edit, from.range()) {
        RangeFate::Touched => gap(BasisGap::EditAtBoundary),
        RangeFate::Replaced => gap(BasisGap::DeclarationReplaced),
        RangeFate::Moved(range) if range != to.range() || from.role() != to.role() => {
            gap(BasisGap::MappedElsewhere)
        }
        RangeFate::Moved(_) if claims.on(to).any(|claimant| claimant != id) => {
            gap(BasisGap::Ambiguous)
        }
        RangeFate::Moved(_) => BasisVerdict::Proven,
    }
}

/// Check a `confirmed-new` allocation.
fn new(
    base: &AdmittedRegistry,
    to: &Occurrence,
    automatic: Result<(), BasisGap>,
    evidence: &Evidence<'_>,
    claims: &Claims,
) -> BasisVerdict {
    if let Err(gap) = automatic {
        return BasisVerdict::Unproven(gap);
    }
    // A base with no history has nothing the declaration could continue.
    if base.snapshot().entries().is_empty() {
        return BasisVerdict::Proven;
    }
    // An old declaration still here, or carried here, makes this a
    // continuation, not a new message, however the update labels it.
    if claims.on(to).next().is_some() {
        return BasisVerdict::Unproven(BasisGap::NewUnproven);
    }
    let shown = evidence.edits().iter().any(|(edit, replayed)| {
        replayed.is_ok() && edit.after() == Some(to.source()) && inserted(edit, to.range())
    });
    if shown {
        BasisVerdict::Proven
    } else {
        BasisVerdict::Unproven(BasisGap::NewUnproven)
    }
}

/// Check a `complete-absence` retirement.
fn absent(
    from: &Occurrence,
    automatic: Result<(), BasisGap>,
    membership: &[Token],
    evidence: &Evidence<'_>,
) -> BasisVerdict {
    if let Err(gap) = automatic {
        return BasisVerdict::Unproven(gap);
    }
    // A snapshot the inventory still holds still holds the declaration.
    if evidence.holds(from.source()) {
        return BasisVerdict::Unproven(BasisGap::AbsenceUnproven);
    }
    let unit = from.source().unit();
    let member = membership.contains(unit);
    let accounts = evidence.from(from.source());
    match accounts.as_slice() {
        // No account of where it went: gone only if its whole unit is.
        [] if member => BasisVerdict::Unproven(BasisGap::AbsenceUnproven),
        [] => BasisVerdict::Proven,
        [(_, Err(replayed))] => BasisVerdict::Unproven(match replayed {
            ReplayGap::SourceUnavailable => BasisGap::SourceUnavailable,
            ReplayGap::Mismatch => BasisGap::ReplayMismatch,
        }),
        // A removal says the unit is gone, which the host's membership has
        // to agree with.
        [(edit, Ok(()))] if edit.after().is_none() && member => {
            BasisVerdict::Unproven(BasisGap::AbsenceUnproven)
        }
        // An edit that ends at a snapshot the inventory does not hold says
        // nothing about where the declaration is now.
        [(edit, Ok(()))] if edit.after().is_some_and(|after| !evidence.holds(after)) => {
            BasisVerdict::Unproven(BasisGap::EditDisagrees)
        }
        [(edit, Ok(()))] => match fate(edit, from.range()) {
            RangeFate::Replaced => BasisVerdict::Proven,
            RangeFate::Moved(_) | RangeFate::Touched => {
                BasisVerdict::Unproven(BasisGap::AbsenceUnproven)
            }
        },
        _ => BasisVerdict::Unproven(BasisGap::Ambiguous),
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{
        AuthoringArtifact, AuthoringInventory, Completeness, IntegrityDigest, SourceSnapshot, Token,
    };
    use intlify_shared_json::encoding::{hash, Domain};
    use serde_json::{json, Value};

    use super::super::inputs::PreviousUpdate;
    use super::super::sources::RetainedSources;
    use super::*;
    use crate::registry::fixtures::{
        admitted_inventory, artifact, declaration, id, inventory_of, limits, owner, sealed,
        sources, Base, Chain, Unit,
    };
    use crate::registry::{
        admit_update, ExplicitBasis, IntentRegistryUpdate, RegistryUpdateArtifact, Replacement,
        SourceEdit,
    };

    fn retained(owned: &[(SourceSnapshot, String)]) -> RetainedSources<'_> {
        RetainedSources::new(
            owned
                .iter()
                .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
        )
        .unwrap()
    }

    fn units(names: &[&str]) -> Vec<Token> {
        names.iter().map(|name| Token::new(name).unwrap()).collect()
    }

    /// Seal an edited update body and admit it.
    fn update(body: Value) -> AdmittedUpdate {
        let body: IntentRegistryUpdate = serde_json::from_value(body).unwrap();
        let sealed = RegistryUpdateArtifact::seal(body).unwrap();
        admit_update(&serde_json::to_vec(&sealed).unwrap(), &limits()).unwrap()
    }

    /// Seal an edited inventory artifact and admit it.
    fn inventory(mut value: Value) -> AdmittedInventory {
        value.as_object_mut().unwrap().remove("integrityDigest");
        let digest = hash(Domain::literal("authoring-artifact-integrity"), &value).unwrap();
        value["integrityDigest"] = json!(IntegrityDigest::from_hash(digest).as_str());
        admitted_inventory(&value)
    }

    /// The edit update-2 carries, from checkout revision 1 to revision 2.
    fn checkout_edit() -> SourceEdit {
        serde_json::from_value(
            artifact("update-2")["body"]["decisions"][0]["basis"]["changes"][0].clone(),
        )
        .unwrap()
    }

    /// The verdicts of update `n` against registry `n - 1`.
    fn verdicts(
        chain: &Chain,
        n: usize,
        update: &AdmittedUpdate,
        inputs: &ContinuityInputs<'_>,
    ) -> Vec<BasisVerdict> {
        verify_bases(
            chain.registry(n - 1),
            update,
            chain.inventory(n),
            inputs,
            &limits(),
        )
        .unwrap()
        .verdicts()
        .iter()
        .map(|(_, verdict)| *verdict)
        .collect()
    }

    /// The update `n` and its inventory, as the update that produced
    /// registry `n`.
    fn produced(chain: &Chain, n: usize) -> PreviousUpdate<'_> {
        PreviousUpdate {
            update: chain.update(n),
            inventory: chain.inventory(n),
        }
    }

    /// A labelled change to pay's basis in update-2, and the gap it opens.
    type BasisCase = (&'static str, fn(&mut Value), BasisGap);

    const PROVEN: BasisVerdict = BasisVerdict::Proven;

    fn gap(gap: BasisGap) -> BasisVerdict {
        BasisVerdict::Unproven(gap)
    }

    #[test]
    fn the_committed_chain_proves_every_basis_it_claims() {
        let chain = Chain::load();
        let owned = sources();
        let sources = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let inputs = |previous| ContinuityInputs {
            sources: &sources,
            edits: &[],
            membership: &membership,
            previous,
        };
        // Update 1: three allocations against an empty genesis.
        assert_eq!(
            verdicts(&chain, 1, chain.update(1), &inputs(None)),
            [PROVEN, PROVEN, PROVEN]
        );
        // Update 2: pay continues across the edit, cancel's declaration is
        // replaced by it, and the copy sits in the inserted text.
        assert_eq!(
            verdicts(
                &chain,
                2,
                chain.update(2),
                &inputs(Some(produced(&chain, 1)))
            ),
            [PROVEN, PROVEN, PROVEN]
        );
        // Update 3: pay and the copy continue; cancel comes back by choice.
        assert_eq!(
            verdicts(
                &chain,
                3,
                chain.update(3),
                &inputs(Some(produced(&chain, 2)))
            ),
            [PROVEN, BasisVerdict::Explicit, PROVEN]
        );
    }

    #[test]
    fn a_continuation_is_unproven_whenever_its_edit_is_not_evidence() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let inputs = |sources, edits| ContinuityInputs {
            sources,
            edits,
            membership: &membership,
            previous: Some(produced(&chain, 1)),
        };
        let pay = |change: fn(&mut Value)| {
            let mut body = artifact("update-2")["body"].clone();
            change(&mut body["decisions"][0]["basis"]);
            update(body)
        };
        let cases: [BasisCase; 4] = [
            (
                "another verifier profile",
                |basis| basis["profile"]["identity"] = json!("some-other-diff"),
                BasisGap::UnsupportedProfile,
            ),
            (
                "no edit at all",
                |basis| basis["changes"] = json!([]),
                BasisGap::EditMissing,
            ),
            (
                "an edit that ends in another unit",
                |basis| {
                    let nav = artifact("inventory-1")["body"]["units"][1]["source"].clone();
                    basis["changes"][0]["after"] = nav;
                },
                BasisGap::EditDisagrees,
            ),
            (
                "a change list with an edit of another unit as well",
                |basis| {
                    let nav = artifact("inventory-1")["body"]["units"][1]["source"].clone();
                    basis["changes"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"before": nav, "after": nav, "replacements": []}));
                },
                BasisGap::ExtraEdits,
            ),
        ];
        for (label, change, expected) in cases {
            let verdict = verdicts(&chain, 2, &pay(change), &inputs(&all, &[]))[0];
            assert_eq!(verdict, gap(expected), "{label}");
        }

        let mut off = artifact("update-2")["body"].clone();
        off["decisions"][0]["basis"]["changes"][0]["replacements"][0]["text"] =
            json!("// checkout actions \n");
        assert_eq!(
            verdicts(&chain, 2, &update(off), &inputs(&all, &[]))[0],
            gap(BasisGap::ReplayMismatch),
            "an edit one byte off"
        );

        let without_revision_2: Vec<(SourceSnapshot, String)> = owned
            .iter()
            .filter(|(snapshot, _)| snapshot.revision().as_str() != "2")
            .cloned()
            .collect();
        let partial = retained(&without_revision_2);
        assert_eq!(
            verdicts(&chain, 2, chain.update(2), &inputs(&partial, &[]))[0],
            gap(BasisGap::SourceUnavailable),
            "bytes that were not retained"
        );

        // A second account of checkout revision 1, which replays too, leaves
        // no single account of where pay went: here, the host says checkout
        // became the navigation unit.
        let into_nav = became(
            &owned,
            &snapshot_of(&owned, "checkout", "1"),
            &snapshot_of(&owned, "nav", "1"),
        );
        assert_eq!(
            verdicts(&chain, 2, chain.update(2), &inputs(&all, &[into_nav]))[0],
            gap(BasisGap::Ambiguous),
            "two accounts of one snapshot"
        );
    }

    #[test]
    fn a_new_declaration_needs_an_empty_base_or_inserted_text() {
        // With pay's continuation chosen explicitly, the update carries no
        // edit, and nothing shows the copy is new or cancel is gone.
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let mut body = artifact("update-2")["body"].clone();
        body["decisions"][0]["basis"] = json!({"kind": "explicit", "reason": "Moved by hand."});
        let explicit = update(body);
        let inputs = |edits| ContinuityInputs {
            sources: &all,
            edits,
            membership: &membership,
            previous: Some(produced(&chain, 1)),
        };
        assert_eq!(
            verdicts(&chain, 2, &explicit, &inputs(&[])),
            [
                BasisVerdict::Explicit,
                gap(BasisGap::AbsenceUnproven),
                gap(BasisGap::NewUnproven)
            ]
        );
        // The host's own account of the edit is evidence for both.
        assert_eq!(
            verdicts(&chain, 2, &explicit, &inputs(&[checkout_edit()])),
            [BasisVerdict::Explicit, PROVEN, PROVEN]
        );
    }

    #[test]
    fn absence_needs_a_resolved_plan_and_a_replacing_edit_or_a_removed_unit() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);

        // Pay's continuation names a verifier this crate does not have: the
        // plan is unresolved, so cancel is not known to be gone, though the
        // edit does replace it.
        let mut body = artifact("update-2")["body"].clone();
        body["decisions"][0]["basis"]["profile"]["identity"] = json!("some-other-diff");
        let membership = units(&["checkout", "nav"]);
        let inputs = ContinuityInputs {
            sources: &all,
            edits: &[],
            membership: &membership,
            previous: Some(produced(&chain, 1)),
        };
        assert_eq!(
            verdicts(&chain, 2, &update(body), &inputs),
            [
                gap(BasisGap::UnsupportedProfile),
                gap(BasisGap::UnresolvedPlan),
                PROVEN
            ]
        );

        // The navigation unit is removed: home is gone with it.
        let mut without_nav = artifact("inventory-2");
        let in_checkout = |value: &Value| value["unit"] == json!("checkout");
        let body_of = &mut without_nav["body"];
        body_of["units"]
            .as_array_mut()
            .unwrap()
            .retain(|unit| in_checkout(&unit["source"]));
        body_of["declarations"]
            .as_array_mut()
            .unwrap()
            .retain(|facts| in_checkout(&facts["occurrence"]["source"]));
        body_of["references"]
            .as_array_mut()
            .unwrap()
            .retain(|reference| in_checkout(&reference["occurrence"]["source"]));
        let without_nav = inventory(without_nav);
        let home = artifact("registry-1")["body"]["entries"][1].clone();
        let mut body = artifact("update-2")["body"].clone();
        body["inventory"] = serde_json::to_value(without_nav.reference()).unwrap();
        body["decisions"].as_array_mut().unwrap().insert(
            1,
            json!({"kind": "retire", "intentId": home["intentId"], "from": home["declaration"], "basis": {"kind": "complete-absence"}}),
        );
        let removal = update(body);
        let verdicts_with = |membership: &[Token]| -> Vec<BasisVerdict> {
            let inputs = ContinuityInputs {
                sources: &all,
                edits: &[],
                membership,
                previous: Some(produced(&chain, 1)),
            };
            verify_bases(
                chain.registry(1),
                &removal,
                &without_nav,
                &inputs,
                &limits(),
            )
            .unwrap()
            .verdicts()
            .iter()
            .map(|(_, verdict)| *verdict)
            .collect()
        };
        assert_eq!(verdicts_with(&units(&["checkout"])), [PROVEN; 4]);
        // The host says the unit still exists: nothing is automatic.
        assert_eq!(
            verdicts_with(&units(&["checkout", "nav"])),
            [
                PROVEN,
                gap(BasisGap::MembershipMismatch),
                gap(BasisGap::MembershipMismatch),
                gap(BasisGap::MembershipMismatch)
            ]
        );
    }

    #[test]
    fn an_absence_is_unproven_whenever_its_edit_is_not_evidence() {
        // With pay's continuation chosen explicitly, the update carries no
        // edit, and the host's edits are the only account of where cancel
        // went.
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let mut body = artifact("update-2")["body"].clone();
        body["decisions"][0]["basis"] = json!({"kind": "explicit", "reason": "Moved by hand."});
        let explicit = update(body);
        let cancel = |sources: &RetainedSources<'_>, edits: &[SourceEdit]| {
            let inputs = ContinuityInputs {
                sources,
                edits,
                membership: &membership,
                previous: Some(produced(&chain, 1)),
            };
            verdicts(&chain, 2, &explicit, &inputs)[1]
        };
        assert_eq!(cancel(&all, &[checkout_edit()]), PROVEN);

        let mut off = serde_json::to_value(checkout_edit()).unwrap();
        off["replacements"][0]["text"] = json!("// checkout actions \n");
        let off: SourceEdit = serde_json::from_value(off).unwrap();
        assert_eq!(
            cancel(&all, &[off]),
            gap(BasisGap::ReplayMismatch),
            "an edit one byte off"
        );

        let without_revision_2: Vec<(SourceSnapshot, String)> = owned
            .iter()
            .filter(|(snapshot, _)| snapshot.revision().as_str() != "2")
            .cloned()
            .collect();
        assert_eq!(
            cancel(&retained(&without_revision_2), &[checkout_edit()]),
            gap(BasisGap::SourceUnavailable),
            "bytes that were not retained"
        );

        // Two accounts of checkout revision 1: update-2's own edit, and the
        // host's that checkout became the navigation unit.
        let into_nav = [became(
            &owned,
            &snapshot_of(&owned, "checkout", "1"),
            &snapshot_of(&owned, "nav", "1"),
        )];
        let both = ContinuityInputs {
            sources: &all,
            edits: &into_nav,
            membership: &membership,
            previous: Some(produced(&chain, 1)),
        };
        assert_eq!(
            verdicts(&chain, 2, chain.update(2), &both)[1],
            gap(BasisGap::Ambiguous),
            "two accounts of one snapshot"
        );

        // The same bytes, rewritten across cancel's end: the edit touches
        // cancel and does not say whether it went.
        let edit = checkout_edit();
        let across = SourceEdit::new(
            edit.before().cloned(),
            edit.after().cloned(),
            vec![
                Replacement::new(at(0, 0), HEADER_LINE),
                Replacement::new(at(30, 56), &PAY_LATER_LINE[..26]),
                Replacement::new(at(56, 62), &PAY_LATER_LINE[26..]),
            ],
        );
        assert_eq!(
            cancel(&all, &[across]),
            gap(BasisGap::AbsenceUnproven),
            "an edit across its end"
        );
    }

    #[test]
    fn newness_and_absence_wait_for_the_pins_of_the_base() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let inputs = |previous| ContinuityInputs {
            sources: &all,
            edits: &[],
            membership: &membership,
            previous,
        };
        // Without the update that produced the base, a change of pins could
        // go unseen.
        assert_eq!(
            verdicts(&chain, 2, chain.update(2), &inputs(None)),
            [
                PROVEN,
                gap(BasisGap::BasisUnknown),
                gap(BasisGap::BasisUnknown)
            ]
        );
        // The same source resolved against another surface vocabulary, as a
        // change of binding configuration would leave it.
        let mut repinned = artifact("inventory-2")["body"].clone();
        repinned["basis"]["surfaceVocabulary"]["revision"] = json!("1");
        let repinned: AuthoringInventory = serde_json::from_value(repinned).unwrap();
        assert_eq!(
            automatic(
                chain.registry(1),
                &repinned,
                &inputs(Some(produced(&chain, 1)))
            ),
            Err(BasisGap::BasisChanged)
        );
        assert_eq!(
            automatic(
                chain.registry(1),
                chain.inventory(2).inventory(),
                &inputs(Some(produced(&chain, 1)))
            ),
            Ok(())
        );
    }

    #[test]
    fn the_inputs_themselves_are_checked_before_any_basis() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let run = |base: usize, previous, edits: &[SourceEdit], limits: &IdentityLimits| {
            verify_bases(
                chain.registry(base),
                chain.update(2),
                chain.inventory(2),
                &ContinuityInputs {
                    sources: &all,
                    edits,
                    membership: &membership,
                    previous,
                },
                limits,
            )
            .err()
        };
        assert_eq!(
            run(2, None, &[], &limits()),
            Some(ContinuityFailure::Transition(
                TransitionFailure::BaseMismatch
            )),
            "an update that does not apply"
        );
        assert_eq!(
            run(1, Some(produced(&chain, 2)), &[], &limits()),
            Some(ContinuityFailure::PreviousMismatch),
            "an update that did not produce the base"
        );
        assert_eq!(
            run(
                1,
                Some(PreviousUpdate {
                    update: chain.update(1),
                    inventory: chain.inventory(2),
                }),
                &[],
                &limits()
            ),
            Some(ContinuityFailure::PreviousMismatch),
            "an inventory the update was not planned from"
        );

        let mut overlapping = serde_json::to_value(checkout_edit()).unwrap();
        overlapping["replacements"][0]["range"] = json!({"start": "0", "end": "40"});
        let overlapping: SourceEdit = serde_json::from_value(overlapping).unwrap();
        assert_eq!(
            run(1, Some(produced(&chain, 1)), &[overlapping], &limits()),
            Some(ContinuityFailure::InvalidEdit(
                UpdateFailure::ReplacementsUnordered
            ))
        );
        let tight = IdentityLimits {
            source_edits: 0,
            ..limits()
        };
        assert_eq!(
            run(1, Some(produced(&chain, 1)), &[checkout_edit()], &tight),
            Some(ContinuityFailure::Limit(IdentityLimitKind::SourceEdits))
        );

        // Exactly at every bound is within it, and one less is not.
        let supplied = [checkout_edit()];
        let count = supplied[0].replacements().len() as u64;
        let bytes: u64 = supplied[0]
            .replacements()
            .iter()
            .map(|replacement| replacement.text().len() as u64)
            .sum();
        let bounded = |replacements, replacement_bytes| IdentityLimits {
            source_edits: 1,
            replacements,
            replacement_bytes,
            ..limits()
        };
        assert_eq!(
            run(
                1,
                Some(produced(&chain, 1)),
                &supplied,
                &bounded(count, bytes)
            ),
            None
        );
        assert_eq!(
            run(
                1,
                Some(produced(&chain, 1)),
                &supplied,
                &bounded(count - 1, bytes)
            ),
            Some(ContinuityFailure::Limit(IdentityLimitKind::Replacements))
        );
        assert_eq!(
            run(
                1,
                Some(produced(&chain, 1)),
                &supplied,
                &bounded(count, bytes - 1)
            ),
            Some(ContinuityFailure::Limit(
                IdentityLimitKind::ReplacementBytes
            ))
        );
    }

    #[test]
    fn a_report_is_proven_only_when_every_basis_is() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let report = |n: usize| {
            let inputs = ContinuityInputs {
                sources: &all,
                edits: &[],
                membership: &membership,
                previous: Some(produced(&chain, n - 1)),
            };
            verify_bases(
                chain.registry(n - 1),
                chain.update(n),
                chain.inventory(n),
                &inputs,
                &limits(),
            )
            .unwrap()
        };
        assert!(report(2).is_proven());
        // Cancel's restore in update-3 is a choice, not a proof.
        let third = report(3);
        assert!(!third.is_proven());
        for (id, verdict) in third.verdicts() {
            assert_eq!(third.verdict(id), Some(*verdict));
        }
        assert_eq!(third.verdict(&stranger()), None);
    }

    /// Seal and admit an update of the committed owner against registry-1,
    /// planned from inventory-1, so checkout and nav are both unchanged.
    fn against_unchanged(chain: &Chain, decisions: Vec<IdentityDecision>) -> AdmittedUpdate {
        let plan = IntentRegistryUpdate::new(
            owner(),
            chain.registry(1).reference(),
            chain.inventory(1).reference(),
            decisions,
            vec![],
        )
        .unwrap();
        let sealed = RegistryUpdateArtifact::seal(plan).unwrap();
        admit_update(&serde_json::to_vec(&sealed).unwrap(), &limits()).unwrap()
    }

    fn unchanged_verdicts(chain: &Chain, update: &AdmittedUpdate) -> Vec<BasisVerdict> {
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let inputs = ContinuityInputs {
            sources: &all,
            edits: &[],
            membership: &membership,
            previous: Some(produced(chain, 1)),
        };
        verify_bases(
            chain.registry(1),
            update,
            chain.inventory(1),
            &inputs,
            &limits(),
        )
        .unwrap()
        .verdicts()
        .iter()
        .map(|(_, verdict)| *verdict)
        .collect()
    }

    #[test]
    fn a_declaration_still_in_place_keeps_its_id_against_any_label() {
        // Nothing changed, yet the update retires cancel and gives its
        // declaration a new ID. The transition allows that; the evidence
        // shows cancel's declaration is still exactly where it was, so it is
        // neither gone nor new.
        let chain = Chain::load();
        let cancel = &chain.registry(1).snapshot().entries()[2];
        let newcomer =
            intlify_authoring::MessageIntentId::retained(owner(), &"d".repeat(32)).unwrap();
        let replaced = against_unchanged(
            &chain,
            vec![
                IdentityDecision::retirement(
                    cancel.intent_id().clone(),
                    cancel.declaration().clone(),
                ),
                IdentityDecision::allocation(
                    newcomer,
                    cancel.declaration().clone(),
                    AllocationBasis::confirmed_new(),
                ),
            ],
        );
        assert_eq!(
            unchanged_verdicts(&chain, &replaced),
            [gap(BasisGap::AbsenceUnproven), gap(BasisGap::NewUnproven)]
        );
    }

    #[test]
    fn swapping_two_declarations_that_did_not_move_is_ambiguous() {
        // An edit from checkout revision 1 to itself replays exactly, and
        // would carry each declaration onto the other's place. The snapshot
        // is still current, which is a second account of where they went.
        let chain = Chain::load();
        let entries = chain.registry(1).snapshot().entries();
        let (pay, cancel) = (&entries[0], &entries[2]);
        let first = pay.declaration().source().clone();
        let edit = || {
            edit_replay(vec![SourceEdit::new(
                Some(first.clone()),
                Some(first.clone()),
                vec![],
            )])
        };
        let swapped = against_unchanged(
            &chain,
            vec![
                IdentityDecision::continuation(
                    pay.intent_id().clone(),
                    pay.declaration().clone(),
                    cancel.declaration().clone(),
                    edit(),
                ),
                IdentityDecision::continuation(
                    cancel.intent_id().clone(),
                    cancel.declaration().clone(),
                    pay.declaration().clone(),
                    edit(),
                ),
            ],
        );
        assert_eq!(
            unchanged_verdicts(&chain, &swapped),
            [gap(BasisGap::Ambiguous), gap(BasisGap::Ambiguous)]
        );
        // The same exchange chosen explicitly is a choice, not a proof.
        let reason =
            || ContinuationBasis::Explicit(ExplicitBasis::new("Swapped on purpose.").unwrap());
        let chosen = against_unchanged(
            &chain,
            vec![
                IdentityDecision::continuation(
                    pay.intent_id().clone(),
                    pay.declaration().clone(),
                    cancel.declaration().clone(),
                    reason(),
                ),
                IdentityDecision::continuation(
                    cancel.intent_id().clone(),
                    cancel.declaration().clone(),
                    pay.declaration().clone(),
                    reason(),
                ),
            ],
        );
        assert_eq!(
            unchanged_verdicts(&chain, &chosen),
            [BasisVerdict::Explicit, BasisVerdict::Explicit]
        );
    }

    /// A `verified-edit` basis under the one profile this crate implements.
    fn edit_replay(changes: Vec<SourceEdit>) -> ContinuationBasis {
        ContinuationBasis::verified_edit(
            intlify_authoring::VersionedIdentity::literal(
                super::super::edit::EDIT_REPLAY_PROFILE,
                super::super::edit::EDIT_REPLAY_REVISION,
            ),
            changes,
        )
    }

    /// One of the committed chain's snapshots, by unit and revision.
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

    fn at(start: u64, end: u64) -> intlify_authoring::ByteRange {
        intlify_authoring::ByteRange::new(start, end).unwrap()
    }

    const HEADER_LINE: &str = "// checkout actions\n";
    const PAY_LATER_LINE: &str = "const payLater = intent('Pay now')\n";
    const CANCEL_LINE: &str = "const cancel = intent('Cancel')\n";

    /// Seal and admit an update of the committed owner.
    fn planned(
        base: &AdmittedRegistry,
        inventory: &AdmittedInventory,
        decisions: Vec<IdentityDecision>,
    ) -> AdmittedUpdate {
        let plan = IntentRegistryUpdate::new(
            owner(),
            base.reference(),
            inventory.reference(),
            decisions,
            vec![],
        )
        .unwrap();
        let sealed = RegistryUpdateArtifact::seal(plan).unwrap();
        admit_update(&serde_json::to_vec(&sealed).unwrap(), &limits()).unwrap()
    }

    /// Check an update against registry `base` with the committed sources,
    /// both units as members, and the update that produced the base.
    fn outcome(
        chain: &Chain,
        base: usize,
        plan: &AdmittedUpdate,
        inventory: &AdmittedInventory,
        edits: &[SourceEdit],
    ) -> Result<Vec<BasisVerdict>, ContinuityFailure> {
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let inputs = ContinuityInputs {
            sources: &all,
            edits,
            membership: &membership,
            previous: (base > 0).then(|| produced(chain, base)),
        };
        verify_bases(chain.registry(base), plan, inventory, &inputs, &limits()).map(|report| {
            report
                .verdicts()
                .iter()
                .map(|(_, verdict)| *verdict)
                .collect()
        })
    }

    /// Check an update whose inputs are all admissible.
    fn checked(
        chain: &Chain,
        base: usize,
        plan: &AdmittedUpdate,
        inventory: &AdmittedInventory,
        edits: &[SourceEdit],
    ) -> Vec<BasisVerdict> {
        outcome(chain, base, plan, inventory, edits).unwrap()
    }

    fn stranger() -> intlify_authoring::MessageIntentId {
        intlify_authoring::MessageIntentId::retained(owner(), &"f".repeat(32)).unwrap()
    }

    #[test]
    fn a_continuation_onto_another_declaration_than_its_edit_carries_is_not_shown() {
        // The update's pay and copy targets are exchanged: the edit carries
        // pay onto its own new place, not onto the copy.
        let chain = Chain::load();
        let mut body = artifact("update-2")["body"].clone();
        let pay_target = body["decisions"][0]["to"].clone();
        body["decisions"][0]["to"] = body["decisions"][2]["to"].clone();
        body["decisions"][2]["to"] = pay_target;
        assert_eq!(
            checked(&chain, 1, &update(body), chain.inventory(2), &[]),
            [
                gap(BasisGap::MappedElsewhere),
                gap(BasisGap::UnresolvedPlan),
                gap(BasisGap::NewUnproven)
            ]
        );
    }

    #[test]
    fn a_declaration_where_it_was_needs_no_edit_and_a_choice_is_left_to_the_host() {
        let chain = Chain::load();
        // Pay is exactly where it was: there is nothing to show.
        let pay = &chain.registry(1).snapshot().entries()[0];
        let stays = against_unchanged(
            &chain,
            vec![IdentityDecision::continuation(
                pay.intent_id().clone(),
                pay.declaration().clone(),
                pay.declaration().clone(),
                ContinuationBasis::unchanged_snapshot(),
            )],
        );
        assert_eq!(unchanged_verdicts(&chain, &stays), [PROVEN]);
        // The copy given its ID by choice is left to the host to confirm,
        // though the edit would show it new.
        let mut body = artifact("update-2")["body"].clone();
        body["decisions"][2]["basis"] = json!({"kind": "explicit", "reason": "Copied by hand."});
        assert_eq!(
            checked(&chain, 1, &update(body), chain.inventory(2), &[]),
            [PROVEN, PROVEN, BasisVerdict::Explicit]
        );
    }

    #[test]
    fn a_declaration_its_edit_replaces_does_not_continue() {
        // Update-2's edit replaces cancel's whole line with the copy's. Read
        // as cancel continuing onto the copy, it carries nothing: cancel was
        // replaced, not moved.
        let chain = Chain::load();
        let base = chain.registry(1);
        let entries = base.snapshot().entries();
        let (pay, cancel) = (&entries[0], &entries[2]);
        let current = chain.inventory(2).inventory().declarations();
        let plan = planned(
            base,
            chain.inventory(2),
            vec![
                IdentityDecision::continuation(
                    pay.intent_id().clone(),
                    pay.declaration().clone(),
                    current[0].occurrence().clone(),
                    edit_replay(vec![checkout_edit()]),
                ),
                IdentityDecision::continuation(
                    cancel.intent_id().clone(),
                    cancel.declaration().clone(),
                    current[1].occurrence().clone(),
                    edit_replay(vec![checkout_edit()]),
                ),
            ],
        );
        assert_eq!(
            checked(&chain, 1, &plan, chain.inventory(2), &[]),
            [PROVEN, gap(BasisGap::DeclarationReplaced)]
        );
    }

    #[test]
    fn a_declaration_an_edit_carries_elsewhere_is_neither_gone_nor_new_where_it_lands() {
        // Revision 1 to revision 3 in one edit: a header goes in, and the
        // copy's line goes in before cancel. Cancel moved; it did not go.
        let chain = Chain::load();
        let owned = sources();
        let first = snapshot_of(&owned, "checkout", "1");
        let third = snapshot_of(&owned, "checkout", "3");
        let pay_line = "const pay = intent('Pay now')\n".len() as u64;
        let edit = SourceEdit::new(
            Some(first),
            Some(third),
            vec![
                Replacement::new(at(0, 0), HEADER_LINE),
                Replacement::new(at(pay_line, pay_line), PAY_LATER_LINE),
            ],
        );
        let base = chain.registry(1);
        let entries = base.snapshot().entries();
        let (pay, cancel) = (&entries[0], &entries[2]);
        let current = chain.inventory(3).inventory().declarations();
        let (pay_3, copy_3, cancel_3) = (
            current[0].occurrence().clone(),
            current[1].occurrence().clone(),
            current[2].occurrence().clone(),
        );
        let copy = artifact("update-2")["body"]["decisions"][2]["intentId"].clone();
        let copy: intlify_authoring::MessageIntentId = serde_json::from_value(copy).unwrap();
        let plan = planned(
            base,
            chain.inventory(3),
            vec![
                IdentityDecision::continuation(
                    pay.intent_id().clone(),
                    pay.declaration().clone(),
                    pay_3,
                    edit_replay(vec![edit]),
                ),
                IdentityDecision::retirement(
                    cancel.intent_id().clone(),
                    cancel.declaration().clone(),
                ),
                IdentityDecision::allocation(copy, copy_3, AllocationBasis::confirmed_new()),
                IdentityDecision::allocation(
                    stranger(),
                    cancel_3,
                    AllocationBasis::confirmed_new(),
                ),
            ],
        );
        assert_eq!(
            checked(&chain, 1, &plan, chain.inventory(3), &[]),
            [
                PROVEN,
                gap(BasisGap::AbsenceUnproven),
                PROVEN,
                gap(BasisGap::NewUnproven)
            ]
        );
    }

    #[test]
    fn an_edit_to_a_snapshot_the_inventory_does_not_hold_shows_no_absence() {
        // Checkout went through revision 2, which replaced cancel's line, to
        // revision 3, where it is back. An edit that ends at revision 2 says
        // nothing about where cancel is now, and the plan gives cancel's line
        // a new ID by choice.
        let chain = Chain::load();
        let base = chain.registry(1);
        let entries = base.snapshot().entries();
        let (pay, cancel) = (&entries[0], &entries[2]);
        let current = chain.inventory(3).inventory().declarations();
        let chosen = || AllocationBasis::Explicit(ExplicitBasis::new("Chosen by hand.").unwrap());
        let copy = artifact("update-2")["body"]["decisions"][2]["intentId"].clone();
        let copy: intlify_authoring::MessageIntentId = serde_json::from_value(copy).unwrap();
        let plan = |pay_basis| {
            planned(
                base,
                chain.inventory(3),
                vec![
                    IdentityDecision::continuation(
                        pay.intent_id().clone(),
                        pay.declaration().clone(),
                        current[0].occurrence().clone(),
                        pay_basis,
                    ),
                    IdentityDecision::retirement(
                        cancel.intent_id().clone(),
                        cancel.declaration().clone(),
                    ),
                    IdentityDecision::allocation(
                        copy.clone(),
                        current[1].occurrence().clone(),
                        chosen(),
                    ),
                    IdentityDecision::allocation(
                        stranger(),
                        current[2].occurrence().clone(),
                        chosen(),
                    ),
                ],
            )
        };
        // From the host it is no account of the change at all.
        let moved = ContinuationBasis::Explicit(ExplicitBasis::new("Moved by hand.").unwrap());
        assert_eq!(
            outcome(
                &chain,
                1,
                &plan(moved),
                chain.inventory(3),
                &[checkout_edit()]
            ),
            Err(ContinuityFailure::EditSet(
                EditSetFailure::AfterOutsideInventory
            ))
        );
        // Carried by pay's continuation, it shows neither pay's move nor
        // cancel's absence.
        assert_eq!(
            checked(
                &chain,
                1,
                &plan(edit_replay(vec![checkout_edit()])),
                chain.inventory(3),
                &[]
            ),
            [
                gap(BasisGap::EditDisagrees),
                gap(BasisGap::EditDisagrees),
                BasisVerdict::Explicit,
                BasisVerdict::Explicit
            ]
        );
    }

    #[test]
    fn a_declaration_both_carried_and_inserted_is_not_new() {
        // Cancel's continuation carries update-2's edit, which carries pay to
        // its new place. The host's edit says the navigation unit became
        // checkout revision 2, written whole: pay's new place is inserted
        // text under it, and still pay's place under the first.
        let chain = Chain::load();
        let owned = sources();
        let into_checkout = became(
            &owned,
            &snapshot_of(&owned, "nav", "1"),
            &snapshot_of(&owned, "checkout", "2"),
        );
        let base = chain.registry(1);
        let entries = base.snapshot().entries();
        let (pay, cancel) = (&entries[0], &entries[2]);
        let current = chain.inventory(2).inventory().declarations();
        let plan = planned(
            base,
            chain.inventory(2),
            vec![
                IdentityDecision::retirement(pay.intent_id().clone(), pay.declaration().clone()),
                IdentityDecision::continuation(
                    cancel.intent_id().clone(),
                    cancel.declaration().clone(),
                    current[1].occurrence().clone(),
                    edit_replay(vec![checkout_edit()]),
                ),
                IdentityDecision::allocation(
                    stranger(),
                    current[0].occurrence().clone(),
                    AllocationBasis::confirmed_new(),
                ),
            ],
        );
        assert_eq!(
            checked(&chain, 1, &plan, chain.inventory(2), &[into_checkout]),
            [
                gap(BasisGap::AbsenceUnproven),
                gap(BasisGap::DeclarationReplaced),
                gap(BasisGap::NewUnproven)
            ]
        );
    }

    #[test]
    fn a_retired_id_does_not_claim_the_place_it_last_held() {
        // Checkout goes back to revision 1, and cancel's old declaration is
        // back exactly where it was. Cancel is retired: only an explicit
        // restore brings it back, so a new ID there is new.
        let chain = Chain::load();
        let owned = sources();
        let back = SourceEdit::new(
            Some(snapshot_of(&owned, "checkout", "2")),
            Some(snapshot_of(&owned, "checkout", "1")),
            vec![
                Replacement::new(at(0, HEADER_LINE.len() as u64), ""),
                Replacement::new(at(50, 85), CANCEL_LINE),
            ],
        );
        let base = chain.registry(2);
        let entries = base.snapshot().entries();
        let (pay, copy) = (&entries[0], &entries[3]);
        let first = chain.inventory(1).inventory().declarations();
        let plan = planned(
            base,
            chain.inventory(1),
            vec![
                IdentityDecision::continuation(
                    pay.intent_id().clone(),
                    pay.declaration().clone(),
                    first[0].occurrence().clone(),
                    edit_replay(vec![back]),
                ),
                IdentityDecision::retirement(copy.intent_id().clone(), copy.declaration().clone()),
                IdentityDecision::allocation(
                    stranger(),
                    first[1].occurrence().clone(),
                    AllocationBasis::confirmed_new(),
                ),
            ],
        );
        assert_eq!(
            checked(&chain, 2, &plan, chain.inventory(1), &[]),
            [PROVEN, PROVEN, PROVEN]
        );
    }

    #[test]
    fn a_genesis_was_produced_by_no_update() {
        let chain = Chain::load();
        let owned = sources();
        let all = retained(&owned);
        let membership = units(&["checkout", "nav"]);
        let inputs = ContinuityInputs {
            sources: &all,
            edits: &[],
            membership: &membership,
            previous: Some(produced(&chain, 1)),
        };
        assert_eq!(
            verify_bases(
                chain.registry(0),
                chain.update(1),
                chain.inventory(1),
                &inputs,
                &limits()
            )
            .err(),
            Some(ContinuityFailure::PreviousMismatch)
        );
    }

    /// The text of one committed snapshot.
    fn text_of(owned: &[(SourceSnapshot, String)], snapshot: &SourceSnapshot) -> String {
        owned
            .iter()
            .find(|(known, _)| known == snapshot)
            .map(|(_, text)| text.clone())
            .expect("a committed text")
    }

    /// An edit saying one committed snapshot became another, written whole.
    fn became(
        owned: &[(SourceSnapshot, String)],
        before: &SourceSnapshot,
        after: &SourceSnapshot,
    ) -> SourceEdit {
        SourceEdit::new(
            Some(before.clone()),
            Some(after.clone()),
            vec![Replacement::new(
                at(0, before.byte_length()),
                &text_of(owned, after),
            )],
        )
    }

    /// An edit that writes a snapshot from nothing.
    fn written(owned: &[(SourceSnapshot, String)], snapshot: &SourceSnapshot) -> SourceEdit {
        SourceEdit::new(
            None,
            Some(snapshot.clone()),
            vec![Replacement::new(at(0, 0), &text_of(owned, snapshot))],
        )
    }

    /// update-2 with pay's continuation chosen explicitly, so it carries no
    /// edit and only what the host supplies is evidence.
    fn update_2_explicit() -> AdmittedUpdate {
        let mut body = artifact("update-2")["body"].clone();
        body["decisions"][0]["basis"] = json!({"kind": "explicit", "reason": "Moved by hand."});
        update(body)
    }

    #[test]
    fn landing_on_a_declaration_of_another_role_is_landing_elsewhere() {
        // inventory-2 again, but with pay's new declaration read as a UI
        // literal. The edit carries pay onto its range, not onto it.
        let chain = Chain::load();
        let mut changed = artifact("inventory-2");
        changed["body"]["declarations"][0]["occurrence"]["role"] = json!("ui-literal");
        let calls = changed["body"]["references"].as_array_mut().unwrap();
        for reference in calls {
            for target in reference["declarations"].as_array_mut().unwrap() {
                if target["range"] == json!({"start": "39", "end": "48"}) {
                    target["role"] = json!("ui-literal");
                }
            }
        }
        let changed = inventory(changed);
        let mut body = artifact("update-2")["body"].clone();
        body["inventory"] = serde_json::to_value(changed.reference()).unwrap();
        body["decisions"][0]["to"]["role"] = json!("ui-literal");
        assert_eq!(
            checked(&chain, 1, &update(body), &changed, &[])[0],
            gap(BasisGap::MappedElsewhere)
        );
    }

    #[test]
    fn another_old_declaration_carried_onto_the_same_place_makes_a_continuation_ambiguous() {
        // An edit from the navigation unit to checkout revision 2 that
        // replays exactly: the text before home's literal becomes the header
        // and pay's opening, home's content becomes 'Pay now' inside the
        // literal, and the copy's line follows. It carries home onto pay's
        // new place, where pay's own edit also lands.
        let chain = Chain::load();
        let owned = sources();
        let nav = snapshot_of(&owned, "nav", "1");
        let second = snapshot_of(&owned, "checkout", "2");
        let onto_pay = SourceEdit::new(
            Some(nav),
            Some(second),
            vec![
                Replacement::new(at(0, 19), "// checkout actions\nconst pay = intent"),
                Replacement::new(at(21, 25), "Pay now"),
                Replacement::new(at(27, 28), &format!("\n{PAY_LATER_LINE}")),
            ],
        );
        assert_eq!(
            checked(&chain, 1, chain.update(2), chain.inventory(2), &[onto_pay])[0],
            gap(BasisGap::Ambiguous)
        );
    }

    #[test]
    fn text_inserted_into_another_snapshot_shows_nothing_new_here() {
        // Two new files with the same text: an account of one is no account
        // of the other, though their declarations sit at the same positions.
        let app = Unit::new("app.js", "1", "intent('Save')\n");
        let base = Base::of(&[&app], &[&"a".repeat(32)]);
        let left = Unit::new("left.js", "1", "intent('Next')\n");
        let right = Unit::new("right.js", "1", "intent('Next')\n");
        let current = inventory_of(&[&app, &left, &right], Completeness::Complete);
        let plan = sealed(
            IntentRegistryUpdate::new(
                owner(),
                base.registry.reference(),
                current.reference(),
                vec![
                    IdentityDecision::allocation(
                        id(&"b".repeat(32)),
                        declaration(&current, &left, 0),
                        AllocationBasis::confirmed_new(),
                    ),
                    IdentityDecision::allocation(
                        id(&"c".repeat(32)),
                        declaration(&current, &right, 0),
                        AllocationBasis::confirmed_new(),
                    ),
                ],
                vec![],
            )
            .unwrap(),
        );
        let created = [SourceEdit::new(
            None,
            Some(left.snapshot()),
            vec![Replacement::new(at(0, 0), &left.text)],
        )];
        let sources = RetainedSources::new(
            [&app, &left, &right]
                .iter()
                .map(|unit| (unit.snapshot(), unit.text.as_bytes())),
        )
        .unwrap();
        let membership = units(&["app.js", "left.js", "right.js"]);
        let inputs = ContinuityInputs {
            sources: &sources,
            edits: &created,
            membership: &membership,
            previous: Some(base.previous()),
        };
        let report = verify_bases(&base.registry, &plan, &current, &inputs, &limits()).unwrap();
        assert_eq!(
            report
                .verdicts()
                .iter()
                .map(|(_, verdict)| *verdict)
                .collect::<Vec<_>>(),
            [PROVEN, gap(BasisGap::NewUnproven)]
        );
    }

    #[test]
    fn a_removal_of_a_unit_still_in_the_scope_is_not_an_absence() {
        // An edit that deletes checkout revision 1 outright, while checkout
        // is still a unit of the inventory.
        let chain = Chain::load();
        let owned = sources();
        let first = snapshot_of(&owned, "checkout", "1");
        let length = first.byte_length();
        let removal = || {
            SourceEdit::new(
                Some(first.clone()),
                None,
                vec![Replacement::new(at(0, length), "")],
            )
        };
        // From the host it is no account of the change at all.
        assert_eq!(
            outcome(
                &chain,
                1,
                &update_2_explicit(),
                chain.inventory(2),
                &[removal()]
            ),
            Err(ContinuityFailure::EditSet(EditSetFailure::UnitNotRemoved))
        );
        // Carried by pay's continuation, it is no account of pay or of
        // cancel either.
        let mut body = artifact("update-2")["body"].clone();
        body["decisions"][0]["basis"]["changes"] =
            json!([serde_json::to_value(removal()).unwrap()]);
        assert_eq!(
            checked(&chain, 1, &update(body), chain.inventory(2), &[]),
            [
                gap(BasisGap::EditDisagrees),
                gap(BasisGap::AbsenceUnproven),
                gap(BasisGap::NewUnproven)
            ]
        );
    }

    #[test]
    fn home_s_unchanged_declaration_is_neither_gone_nor_free_for_a_new_id() {
        // update-2 also retires home and gives its declaration a new ID,
        // though the navigation unit did not change.
        let chain = Chain::load();
        let owned = sources();
        let nav = snapshot_of(&owned, "nav", "1");
        let second = snapshot_of(&owned, "checkout", "2");
        let home = artifact("registry-1")["body"]["entries"][1].clone();
        let mut body = artifact("update-2")["body"].clone();
        let decisions = body["decisions"].as_array_mut().unwrap();
        decisions.insert(
            1,
            json!({"kind": "retire", "intentId": home["intentId"], "from": home["declaration"], "basis": {"kind": "complete-absence"}}),
        );
        let mut newcomer = home["intentId"].clone();
        newcomer["value"] = json!("f".repeat(32));
        decisions.push(json!({"kind": "allocate", "intentId": newcomer, "to": home["declaration"], "basis": {"kind": "confirmed-new"}}));
        let plan = update(body);

        // An edit that replaces the whole navigation text: home's range is
        // replaced under it, but the unit is still there unchanged.
        let replaced = SourceEdit::new(
            Some(nav.clone()),
            Some(second.clone()),
            vec![Replacement::new(
                at(0, nav.byte_length()),
                &text_of(&owned, &second),
            )],
        );
        assert_eq!(
            checked(&chain, 1, &plan, chain.inventory(2), &[replaced]),
            [
                PROVEN,
                gap(BasisGap::AbsenceUnproven),
                gap(BasisGap::UnresolvedPlan),
                PROVEN,
                gap(BasisGap::NewUnproven)
            ]
        );
        // An account that writes the navigation unit from nothing, though
        // the base has it, is no account of the change at all.
        assert_eq!(
            outcome(
                &chain,
                1,
                &plan,
                chain.inventory(2),
                &[written(&owned, &nav)]
            ),
            Err(ContinuityFailure::EditSet(EditSetFailure::UnitNotNew))
        );
    }
}
