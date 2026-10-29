// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015 Phase 9's ordinary exact resolution of the selected declaration's
//! references against the Profile Resolution Artifact Set, and the Resolved
//! Profile Artifact Bindings it makes, under a fixture-owned confirmed policy.
//! The configuration is complete and selected, and the collection is admitted
//! into the Set before the interval, so the interval resolves each present
//! reference and binds the distinct ones. Neither the result nor its
//! observation is a Profile or a shared artifact.

use serde::{Deserialize, Serialize};

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::envelope::ExactTuple;
use crate::model::ProfileId;
use crate::resolver::artifact_resolution::{admit_collection, ArtifactSet};
use crate::resolver::evaluation::{
    Admitted, Causes, Check, Evaluation, EvaluationInvariant, IssueKey, Status, Subject, Unit,
};
use crate::resolver::exact_resolution::{resolve_references, ArtifactBindings, ResolvedReferences};
use crate::resolver::limits::ConfirmedPolicy;
use crate::resolver::resource_policy::{DeclarationReferences, ReferenceField};

use super::artifact_collection;
use super::observation::{Frame, Observation};
use super::operation::{Analysis, Config, OutputFailure};
use super::quantity::Quantity;

/// A case's complete configuration and selected declaration, the confirmed
/// policy Phase 9 runs under, and the collection past 015's pre-invocation
/// boundary.
pub(super) struct PreparedBindings {
    pub(super) analysis: Analysis,
    pub(super) config: Config,
    pub(super) selected: ProfileId,
    pub(super) policy: ConfirmedPolicy,
    pub(super) collection: FormalCollection,
}

/// What the interval borrows, built before it: the selected declaration's
/// references, and the Set the collection admission built or why there is
/// none. The admission's own record is no part of the interval's.
pub(super) struct BindingInputs<'a> {
    pub(super) references: DeclarationReferences<'a>,
    pub(super) set: Admitted<ArtifactSet<'a>>,
}

impl PreparedBindings {
    /// The inputs, when the configuration is complete and the collection
    /// admission runs without a defect.
    pub(super) fn inputs(&self) -> Option<BindingInputs<'_>> {
        if !self.analysis.is_complete() {
            return None;
        }
        let declaration = self.config.profiles().get(&self.selected)?;
        let set = admit_collection(
            &mut Evaluation::default(),
            Ok(&self.policy),
            &self.collection,
        )
        .ok()?;
        Some(BindingInputs {
            references: DeclarationReferences {
                policies: Ok(&declaration.policies),
                target_profiles: Ok(&declaration.target_profiles),
            },
            set,
        })
    }

    /// One untimed invocation, retained as a timed one is.
    pub(super) fn resolve(&self) -> Option<Resolved> {
        let inputs = self.inputs()?;
        Some(Resolved::retain(construct(&inputs)))
    }
}

/// What one invocation returns: the evaluation, and the resolved references
/// with the bindings they make, both borrowing the Set.
pub(super) struct Construction<'s> {
    evaluation: Evaluation,
    result: Result<Admitted<(ResolvedReferences<'s>, ArtifactBindings<'s>)>, EvaluationInvariant>,
}

/// Phase 9's exact resolution over a fresh evaluation, and then the bindings:
/// what every timed invocation runs.
pub(super) fn construct<'s>(inputs: &'s BindingInputs<'s>) -> Construction<'s> {
    let mut evaluation = Evaluation::default();
    let result = resolve_references(&mut evaluation, inputs.set.as_ref(), &inputs.references).map(
        |resolved| {
            resolved.map(|resolved| {
                let bindings = ArtifactBindings::bind(&resolved);
                (resolved, bindings)
            })
        },
    );
    Construction { evaluation, result }
}

/// The resolution as the output retains it: the tuple each resolved field
/// names, in the fields' order, and each binding's tuple with the canonical
/// body of its artifact's member, in the tuples' order. The admitted content
/// is a function of that body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Retained {
    pub(super) references: Vec<(ReferenceField, ExactTuple)>,
    pub(super) bindings: Vec<(ExactTuple, Vec<u8>)>,
}

/// Phase 9's record of every reference unit it ran, and the resolution and
/// bindings it made: the complete ordinary output, retained past the end read.
pub(super) struct Resolved {
    pub(super) evaluation: Evaluation,
    pub(super) result: Result<Admitted<Retained>, EvaluationInvariant>,
}

impl Resolved {
    /// The resolution and the bindings borrow the Set, and with it the
    /// prepared collection, so after the end read their tuples and bodies are
    /// copied out. The copy is observation, not work of the interval.
    pub(super) fn retain(construction: Construction<'_>) -> Self {
        let result = construction.result.map(|admitted| {
            admitted.map(|(resolved, bindings)| Retained {
                references: resolved
                    .iter()
                    .map(|(field, tuple, _)| (field.clone(), tuple.clone()))
                    .collect(),
                bindings: bindings
                    .iter()
                    .map(|(tuple, artifact)| {
                        (tuple.clone(), artifact.member.canonical_body().to_vec())
                    })
                    .collect(),
            })
        });
        Self {
            evaluation: construction.evaluation,
            result,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    selected_profile: String,
    policy: String,
    submitted_members: Quantity,
    prerequisites: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(super) fn observe(prepared: &PreparedBindings) -> Self {
        Self {
            scope: "every-present-reference-of-the-selected-declaration-against-the-set".into(),
            selected_profile: prepared.selected.as_str().into(),
            policy: "fixture-owned-confirmed-policy-not-an-admitted-resource-limit-policy".into(),
            submitted_members: Quantity::new(
                u64::try_from(prepared.collection.members().len()).unwrap_or(u64::MAX),
            ),
            prerequisites: "complete-configuration-selected-declaration-and-set-or-its-causes"
                .into(),
            input_storage: "borrowed-selected-references-and-set-over-resident-collection".into(),
            structural_preparation:
                "complete-model-selection-and-collection-admission-before-interval".into(),
            ordinary_output:
                "fresh-evaluation-with-references-and-bindings-copied-after-end-read-or-resolver-defect"
                    .into(),
        }
    }
}

/// A resolver defect is no blocked outcome: the case fails rather than
/// completing as an expected failure. Over a complete configuration, exact
/// resolution evaluates every reference when the Set exists and leaves every
/// one waiting on the Set's causes when it does not; it finds blocking results
/// without details, and no other result, so anything else is a defect too.
/// Logical work uses this screen.
pub(super) fn require_observable(
    resolved: &Resolved,
) -> Result<&Admitted<Retained>, OutputFailure> {
    let evaluation = &resolved.evaluation;
    let Ok(result) = &resolved.result else {
        return Err(OutputFailure::ResolvedBindingInvariant);
    };
    let evaluated = evaluation
        .statuses()
        .all(|(_, status)| *status == Status::Evaluated);
    // A unit with a blocking result was checked, so when every unit waits on
    // the Set's causes there is no blocking result either.
    let unset = match result {
        Ok(_) => false,
        Err(causes) => evaluation
            .statuses()
            .all(|(_, status)| *status == Status::NotEvaluated(causes.clone())),
    };
    let plain = evaluation
        .issues()
        .all(|key| evaluation.limit(key).is_none() && evaluation.shortfall(key).is_none());
    if (evaluated || unset) && plain && evaluation.notices().next().is_none() {
        Ok(result)
    } else {
        Err(OutputFailure::ResolvedBindingInvariant)
    }
}

fn count(value: usize) -> Result<u64, OutputFailure> {
    u64::try_from(value).map_err(|_| OutputFailure::ResolvedBindingInvariant)
}

fn field(frame: &mut Frame, field: &ReferenceField) {
    match field {
        ReferenceField::Policy(slot) => {
            frame.uint(0);
            frame.text(slot.name());
        }
        ReferenceField::TargetProfile(id) => {
            frame.uint(1);
            frame.text(id.as_str());
        }
    }
}

/// Only a reference's unit, under Phase 9's one check for it, runs in the
/// interval.
fn unit(frame: &mut Frame, unit: &Unit) -> Result<(), OutputFailure> {
    let Subject::ArtifactReference(reference) = &unit.subject else {
        return Err(OutputFailure::ResolvedBindingInvariant);
    };
    if unit.check != Check::ResolvedReference {
        return Err(OutputFailure::ResolvedBindingInvariant);
    }
    frame.text(unit.phase.id());
    field(frame, reference);
    Ok(())
}

/// A blocking result of a reference: its unit, code, and reason. Each reason
/// is a decision's, so one without a reason is a defect. Phase 11 frames the
/// causes an unresolved Target Profile reference leaves it with it.
pub(super) fn issue(frame: &mut Frame, key: &IssueKey) -> Result<(), OutputFailure> {
    unit(frame, key.unit())?;
    frame.text(key.code().as_str());
    let reason = key
        .reason()
        .ok_or(OutputFailure::ResolvedBindingInvariant)?;
    frame.text(reason.as_str());
    Ok(())
}

/// What a unit waits on: a reference's blocking result, or, when the
/// collection admission built no Set, one of the collection's.
fn causes(frame: &mut Frame, causes: &Causes) -> Result<(), OutputFailure> {
    let keys = causes.keys().collect::<Vec<_>>();
    frame.uint(count(keys.len())?);
    for key in keys {
        match key.unit().subject {
            Subject::ArtifactReference(_) => {
                frame.uint(0);
                issue(frame, key)?;
            }
            Subject::ArtifactCollection(_) => {
                frame.uint(1);
                artifact_collection::issue(frame, key)
                    .map_err(|_| OutputFailure::ResolvedBindingInvariant)?;
            }
            _ => return Err(OutputFailure::ResolvedBindingInvariant),
        }
    }
    Ok(())
}

fn status(frame: &mut Frame, status: &Status) -> Result<(), OutputFailure> {
    match status {
        Status::Evaluated => {
            frame.uint(0);
            Ok(())
        }
        Status::NotEvaluated(why) => {
            frame.uint(1);
            causes(frame, why)
        }
    }
}

/// The tuple each field names and the bindings, or why there are none.
fn result(frame: &mut Frame, result: &Admitted<Retained>) -> Result<(), OutputFailure> {
    match result {
        Ok(retained) => {
            frame.uint(0);
            frame.uint(count(retained.references.len())?);
            for (reference, named) in &retained.references {
                field(frame, reference);
                artifact_collection::tuple(frame, named);
            }
            frame.uint(count(retained.bindings.len())?);
            for (bound, body) in &retained.bindings {
                artifact_collection::tuple(frame, bound);
                frame.bytes(body);
            }
            Ok(())
        }
        Err(why) => {
            frame.uint(1);
            causes(frame, why)
        }
    }
}

/// The shared domain is what Phase 9 decided, none of it positional: every
/// reference unit and its status, each blocking result, and the resolution
/// with its bindings or why there are none. The order of the declaration's
/// members and of the collection is no part of the input, so reordering
/// either changes nothing here. A declaration's references are no entry's
/// own, so there is no entry domain.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let admitted = require_observable(resolved)?;
    let evaluation = &resolved.evaluation;
    let mut shared = Frame::new("resolved-binding-construction-result");
    shared.uint(count(evaluation.statuses().count())?);
    for (recorded, recorded_status) in evaluation.statuses() {
        unit(&mut shared, recorded)?;
        status(&mut shared, recorded_status)?;
    }
    shared.uint(count(evaluation.issues().count())?);
    for key in evaluation.issues() {
        issue(&mut shared, key)?;
    }
    result(&mut shared, admitted)?;
    Ok(Observation {
        shared: shared.finish(),
        entry: None,
    })
}

#[cfg(test)]
mod tests {
    use intlify_shared_json::token::Token;
    use serde_json::json;

    use super::*;
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, Recipe, ResolvedBindingRecipe};
    use crate::benchmark::operation::Prepared;
    use crate::input_limits::Bound;
    use crate::resolver::artifact_resolution::{unit as collection_unit, CollectionSubject};
    use crate::resolver::evaluation::{
        Actual, Blocking, Code, Details, LimitExceeded, Phase, Reason, Shortfall,
    };
    use crate::resolver::exact_resolution::unit as reference_unit;
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::resource_policy::PolicySlot;

    fn prepared(recipe: ResolvedBindingRecipe) -> PreparedBindings {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::ResolvedBinding(recipe))
            .unwrap();
        let Prepared::ResolvedBinding(prepared) = prepare(&declaration).unwrap().prepared else {
            unreachable!()
        };
        *prepared
    }

    fn resolved(recipe: ResolvedBindingRecipe) -> Resolved {
        prepared(recipe).resolve().unwrap()
    }

    fn digest(frame_with: impl FnOnce(&mut Frame)) -> super::super::observation::Digest {
        let mut frame = Frame::new("resolved-binding-test");
        frame_with(&mut frame);
        frame.finish()
    }

    fn apart<T: PartialEq>(values: &[T]) -> bool {
        values
            .iter()
            .enumerate()
            .all(|(index, left)| values[index + 1..].iter().all(|right| right != left))
    }

    const fn trust() -> ReferenceField {
        ReferenceField::Policy(PolicySlot::Trust)
    }

    fn target(id: &str) -> ReferenceField {
        ReferenceField::TargetProfile(serde_json::from_value(json!(id)).unwrap())
    }

    const fn mismatch(reason: Option<Reason>, details: Option<Details>) -> Blocking {
        Blocking {
            code: Code::ArtifactReferenceNotAdmitted,
            reason,
            details,
        }
    }

    /// An evaluation whose one unit, `recorded`, `blocking` blocks, and the
    /// causes that leaves.
    fn blocked_unit(recorded: Unit, blocking: Blocking) -> (Evaluation, Causes) {
        let mut evaluation = Evaluation::default();
        let Err(causes) = evaluation
            .check(recorded, Ok(()), |()| Err::<(), _>(blocking))
            .unwrap()
        else {
            unreachable!()
        };
        (evaluation, causes)
    }

    fn blocked(field: ReferenceField, blocking: Blocking) -> (Evaluation, Causes) {
        blocked_unit(reference_unit(field), blocking)
    }

    /// The causes a collection that built no Set leaves.
    fn set_causes() -> Causes {
        let Ok(Err(causes)) = resolved(ResolvedBindingRecipe::CollectionNotAdmitted).result else {
            unreachable!()
        };
        causes
    }

    #[test]
    fn the_retained_resolution_is_the_one_the_interval_made() {
        let prepared = prepared(ResolvedBindingRecipe::EveryReference);
        let inputs = prepared.inputs().unwrap();
        let construction = construct(&inputs);
        let (references, bindings) = {
            let (resolution, bindings) = construction.result.as_ref().unwrap().as_ref().unwrap();
            (
                resolution
                    .iter()
                    .map(|(field, tuple, _)| (field.clone(), tuple.clone()))
                    .collect::<Vec<_>>(),
                bindings
                    .iter()
                    .map(|(tuple, artifact)| {
                        (tuple.clone(), artifact.member.canonical_body().to_vec())
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let statuses = construction.evaluation.statuses().count();
        let retained = Resolved::retain(construction);
        assert_eq!(retained.evaluation.statuses().count(), statuses);
        let retained = retained.result.unwrap().unwrap();
        // Seven slots and two targets, each naming its own artifact.
        assert_eq!((retained.references.len(), retained.bindings.len()), (9, 9));
        assert_eq!(retained.references, references);
        assert_eq!(retained.bindings, bindings);
        // Each binding's body is the one the Set holds under its tuple.
        let Ok(set) = &inputs.set else { unreachable!() };
        for (tuple, body) in &retained.bindings {
            let (_, artifact) = set.get(tuple).unwrap();
            assert_eq!(artifact.member.canonical_body(), body.as_slice());
        }
        assert_eq!(prepared.resolve().unwrap().result, Ok(Ok(retained)));
    }

    #[test]
    fn a_resolver_defect_fails_the_case() {
        let resolved = Resolved {
            evaluation: Evaluation::default(),
            result: Err(EvaluationInvariant::UnitRecordedTwice(reference_unit(
                trust(),
            ))),
        };
        assert_eq!(
            require_observable(&resolved).err(),
            Some(OutputFailure::ResolvedBindingInvariant)
        );
        assert_eq!(
            observe(&resolved).err(),
            Some(OutputFailure::ResolvedBindingInvariant)
        );
    }

    /// What exact resolution cannot record over a complete configuration
    /// fails the case: a unit waiting on anything but the missing Set, or
    /// beside one that was checked, a resolution while units wait, a blocking
    /// result with details or without a reason, and a non-blocking result.
    #[test]
    fn only_what_exact_resolution_records_is_observed() {
        let observed = |evaluation, result| {
            observe(&Resolved {
                evaluation,
                result: Ok(result),
            })
        };
        // The controls: a resolution, a missing Set, and a failed reference
        // with its reason are observed.
        assert!(observe(&resolved(ResolvedBindingRecipe::RequiredOnly)).is_ok());
        assert!(observe(&resolved(ResolvedBindingRecipe::CollectionNotAdmitted)).is_ok());
        let (evaluation, causes) = blocked(trust(), mismatch(Some(Reason::NotFound), None));
        assert!(observed(evaluation, Err(causes)).is_ok());
        let waiting = |prerequisite: Causes| {
            let mut evaluation = Evaluation::default();
            let _ = evaluation
                .check(reference_unit(trust()), Err::<(), _>(prerequisite), |()| {
                    Ok(())
                })
                .unwrap();
            evaluation
        };
        let elsewhere = waiting(blocked(target("web"), mismatch(Some(Reason::NotFound), None)).1);
        let beside = {
            let mut evaluation = waiting(set_causes());
            let _ = evaluation
                .check(
                    reference_unit(ReferenceField::Policy(PolicySlot::Approval)),
                    Ok(()),
                    |()| Ok(()),
                )
                .unwrap();
            evaluation
        };
        let id = || BoundId {
            group: Group::ArtifactAdmission,
            name: "maxArtifactReferences",
        };
        let detailed = |details| blocked(trust(), mismatch(Some(Reason::NotFound), Some(details)));
        let (limited, limited_causes) = detailed(Details::Limit(LimitExceeded {
            bound: id(),
            limit: Bound::new(1).unwrap(),
            actual: Actual::Exact(2),
        }));
        let (short, short_causes) = detailed(Details::Shortfall(Shortfall {
            bound: id(),
            required: Bound::new(2).unwrap(),
            available: Bound::new(1).unwrap(),
        }));
        let (unexplained, unexplained_causes) = blocked(trust(), mismatch(None, None));
        let noticed = {
            let mut evaluation = resolved(ResolvedBindingRecipe::RequiredOnly).evaluation;
            evaluation
                .notice(reference_unit(trust()), Code::ArtifactReferenceNotAdmitted)
                .unwrap();
            evaluation
        };
        let empty = || {
            Ok(Retained {
                references: Vec::new(),
                bindings: Vec::new(),
            })
        };
        for (name, evaluation, result) in [
            ("waiting on another cause", elsewhere, Err(set_causes())),
            ("waiting beside a checked unit", beside, Err(set_causes())),
            ("resolved while waiting", waiting(set_causes()), empty()),
            ("limit", limited, Err(limited_causes)),
            ("shortfall", short, Err(short_causes)),
            ("no reason", unexplained, Err(unexplained_causes)),
            ("notice", noticed, empty()),
        ] {
            assert_eq!(
                observed(evaluation, result).err(),
                Some(OutputFailure::ResolvedBindingInvariant),
                "{name}"
            );
        }
    }

    #[test]
    fn only_reference_units_frame_and_collection_units_only_as_causes() {
        let mut frame = Frame::new("resolved-binding-test");
        let foreign = [
            collection_unit(CollectionSubject::Member(0), Check::AdmissibleMember),
            Unit {
                check: Check::ConsistentContent,
                ..reference_unit(trust())
            },
            Unit {
                phase: Phase::ArtifactReferenceResolution,
                subject: Subject::Vocabulary,
                check: Check::ResolvedReference,
            },
        ];
        for recorded in &foreign {
            assert_eq!(
                unit(&mut frame, recorded),
                Err(OutputFailure::ResolvedBindingInvariant),
                "{recorded:?}"
            );
        }
        // Each field frames apart, a slot apart from a target of its name,
        // and the phase is part of the unit.
        let framed = [
            trust(),
            ReferenceField::Policy(PolicySlot::Approval),
            target("trust"),
            target("web"),
        ]
        .map(|field| digest(|frame| unit(frame, &reference_unit(field)).unwrap()));
        assert!(apart(&framed));
        let other_phase = Unit {
            phase: Phase::DeliveryResolution,
            ..reference_unit(trust())
        };
        assert_ne!(
            digest(|frame| unit(frame, &other_phase).unwrap()),
            framed[0]
        );
        // A collection's blocking result frames only as a cause, apart from a
        // reference's; a cause naming any other unit is a defect.
        let reference_causes = blocked(trust(), mismatch(Some(Reason::NotFound), None)).1;
        let collection_causes = set_causes();
        assert!(collection_causes
            .keys()
            .all(|key| matches!(key.unit().subject, Subject::ArtifactCollection(_))));
        let first = collection_causes.keys().next().unwrap().clone();
        assert_eq!(
            issue(&mut frame, &first),
            Err(OutputFailure::ResolvedBindingInvariant)
        );
        let framed = [&reference_causes, &collection_causes]
            .map(|why| digest(|frame| causes(frame, why).unwrap()));
        assert!(apart(&framed));
        let (_, other_causes) = blocked_unit(
            Unit {
                phase: Phase::ArtifactReferenceResolution,
                subject: Subject::Vocabulary,
                check: Check::VocabularyMember,
            },
            mismatch(Some(Reason::NotFound), None),
        );
        assert_eq!(
            causes(&mut frame, &other_causes),
            Err(OutputFailure::ResolvedBindingInvariant)
        );
    }

    #[test]
    fn every_part_of_a_result_frames_apart() {
        fn token(text: &str) -> Token {
            Token::new(text).unwrap()
        }

        let Ok(Ok(retained)) = resolved(ResolvedBindingRecipe::RequiredOnly).result else {
            unreachable!()
        };
        let with = |change: fn(&mut Retained)| {
            let mut changed = retained.clone();
            change(&mut changed);
            Ok(changed)
        };
        let failed = Err(blocked(trust(), mismatch(Some(Reason::NotFound), None)).1);
        let framed = [
            Ok(retained.clone()),
            with(|retained| {
                retained.references.pop();
            }),
            with(|retained| {
                retained.bindings.pop();
            }),
            with(|retained| retained.references[0].0 = target("approval")),
            with(|retained| retained.references[0].1.identity = token("storefront-other")),
            with(|retained| retained.bindings[0].0.revision = token("2")),
            with(|retained| retained.bindings[0].1.push(0)),
            failed,
        ]
        .map(|admitted| digest(|frame| result(frame, &admitted).unwrap()));
        assert!(apart(&framed));
        // A status frames whether the unit was checked and what it waits on.
        let statuses = [
            Status::Evaluated,
            Status::NotEvaluated(set_causes()),
            Status::NotEvaluated(blocked(trust(), mismatch(Some(Reason::NotFound), None)).1),
        ]
        .map(|recorded| digest(|frame| status(frame, &recorded).unwrap()));
        assert!(apart(&statuses));
    }
}
