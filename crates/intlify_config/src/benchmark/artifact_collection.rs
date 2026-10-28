// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015 Phase 9's complete collection admission over a formally admitted
//! collection, under a fixture-owned confirmed policy: each distinct
//! submitted body checked once, each tuple for a single occurrence, each
//! group of tuples for one body, and the duplicate-free Set built when every
//! unit holds. No declaration is an input, so no reference decides which
//! member is checked. Neither the result nor its observation is a Profile or
//! a shared artifact.

use serde::{Deserialize, Serialize};

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::envelope::ExactTuple;
use crate::resolver::artifact_resolution::{admit_collection, ArtifactSet, CollectionSubject};
use crate::resolver::evaluation::{
    Admitted, Causes, Check, Evaluation, EvaluationInvariant, IssueKey, Status, Subject, Unit,
};
use crate::resolver::limits::ConfirmedPolicy;

use super::observation::{Frame, Observation};
use super::operation::OutputFailure;
use super::quantity::Quantity;

/// A case's collection past 015's pre-invocation boundary and the confirmed
/// policy Phase 9 runs under.
pub(super) struct PreparedCollection {
    pub(super) policy: ConfirmedPolicy,
    pub(super) collection: FormalCollection,
}

impl PreparedCollection {
    /// One untimed invocation, retained as a timed one is.
    pub(super) fn resolve(&self) -> Resolved {
        Resolved::retain(admit(&self.policy, &self.collection))
    }
}

/// What one invocation returns: the evaluation, and the Set, which borrows
/// the collection's members.
pub(super) struct Admission<'a> {
    evaluation: Evaluation,
    result: Result<Admitted<ArtifactSet<'a>>, EvaluationInvariant>,
}

/// Phase 9's collection admission over a fresh evaluation: what every timed
/// invocation runs.
pub(super) fn admit<'a>(
    policy: &ConfirmedPolicy,
    collection: &'a FormalCollection,
) -> Admission<'a> {
    let mut evaluation = Evaluation::default();
    let result = admit_collection(&mut evaluation, Ok(policy), collection);
    Admission { evaluation, result }
}

/// One artifact of the Set as the output retains it: the tuple, and the
/// canonical body of the member that establishes it. The admitted content is
/// a function of that body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SetEntry {
    pub(super) tuple: ExactTuple,
    pub(super) body: Vec<u8>,
}

/// Phase 9's record of every unit it ran and the Set it built: the complete
/// ordinary output, retained past the end read.
pub(super) struct Resolved {
    pub(super) evaluation: Evaluation,
    pub(super) result: Result<Admitted<Vec<SetEntry>>, EvaluationInvariant>,
}

impl Resolved {
    /// The Set borrows the prepared collection, so after the end read its
    /// tuples and bodies are copied out. The copy is observation, not work
    /// of the interval.
    pub(super) fn retain(admission: Admission<'_>) -> Self {
        let result = admission.result.map(|set| {
            set.map(|set| {
                set.iter()
                    .map(|(tuple, artifact)| SetEntry {
                        tuple: tuple.clone(),
                        body: artifact.member.canonical_body().to_vec(),
                    })
                    .collect()
            })
        });
        Self {
            evaluation: admission.evaluation,
            result,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    policy: String,
    submitted_members: Quantity,
    artifact_limit: Quantity,
    prerequisites: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(super) fn observe(prepared: &PreparedCollection) -> Self {
        Self {
            scope: "complete-submitted-collection-whether-or-not-referenced".into(),
            policy: "fixture-owned-confirmed-policy-not-an-admitted-resource-limit-policy".into(),
            submitted_members: Quantity::new(
                u64::try_from(prepared.collection.members().len()).unwrap_or(u64::MAX),
            ),
            // Phase 4 rechecked the collection under it; Phase 9 counts no
            // occurrence again.
            artifact_limit: Quantity::new(
                prepared
                    .policy
                    .limits()
                    .artifact_admission
                    .max_artifacts
                    .get(),
            ),
            prerequisites: "confirmed-policy-and-formal-collection".into(),
            input_storage: "resident-formal-collection".into(),
            structural_preparation: "formal-collection-and-confirmed-policy-before-interval".into(),
            ordinary_output: "fresh-evaluation-with-set-copied-after-end-read-or-resolver-defect"
                .into(),
        }
    }
}

/// A resolver defect is no blocked outcome: the case fails rather than
/// completing as an expected failure. Under a confirmed policy, Phase 9's
/// collection admission evaluates every unit, and finds blocking results
/// without details and no other result, so anything else is a defect too.
/// Logical work uses this screen.
pub(super) fn require_observable(
    resolved: &Resolved,
) -> Result<&Admitted<Vec<SetEntry>>, OutputFailure> {
    let evaluation = &resolved.evaluation;
    let evaluated = evaluation
        .statuses()
        .all(|(_, status)| *status == Status::Evaluated);
    let plain = evaluation
        .issues()
        .all(|key| evaluation.limit(key).is_none() && evaluation.shortfall(key).is_none());
    match &resolved.result {
        Ok(set) if evaluated && plain && evaluation.notices().next().is_none() => Ok(set),
        _ => Err(OutputFailure::ArtifactCollectionInvariant),
    }
}

fn count(value: usize) -> Result<u64, OutputFailure> {
    u64::try_from(value).map_err(|_| OutputFailure::ArtifactCollectionInvariant)
}

/// Only Phase 9's collection units can appear: no other check runs in the
/// interval.
fn unit(frame: &mut Frame, unit: &Unit) -> Result<(), OutputFailure> {
    let Subject::ArtifactCollection(subject) = &unit.subject else {
        return Err(OutputFailure::ArtifactCollectionInvariant);
    };
    frame.text(unit.phase.id());
    let (kind, rank) = match subject {
        CollectionSubject::Member(rank) => (0, rank),
        CollectionSubject::Group(rank) => (1, rank),
    };
    frame.uint(kind);
    frame.uint(count(*rank)?);
    frame.uint(match unit.check {
        Check::AdmissibleMember => 0,
        Check::SingleOccurrence => 1,
        Check::ConsistentContent => 2,
        Check::SupportedAssertion
        | Check::MatchingData
        | Check::IntactArtifact
        | Check::AssertedArtifact
        | Check::OpaqueIdentity
        | Check::WithinCapability
        | Check::WithinBound
        | Check::CanonicalLocale
        | Check::UniqueLocale
        | Check::RequestedMember
        | Check::DistinctFromSource
        | Check::UniqueCandidate
        | Check::EffectiveMode
        | Check::VocabularyMember
        | Check::UniqueSurfaceClass
        | Check::UniqueDomain
        | Check::BootstrapCandidate
        | Check::CompatiblePolicy
        | Check::ResolvedReference => return Err(OutputFailure::ArtifactCollectionInvariant),
    });
    Ok(())
}

/// A blocking result of the collection: its unit, code, and reason. Each of
/// the four reasons is a collection failure's, so one without a reason is a
/// defect.
fn issue(frame: &mut Frame, key: &IssueKey) -> Result<(), OutputFailure> {
    unit(frame, key.unit())?;
    frame.text(key.code().as_str());
    let reason = key
        .reason()
        .ok_or(OutputFailure::ArtifactCollectionInvariant)?;
    frame.text(reason.as_str());
    Ok(())
}

fn causes(frame: &mut Frame, causes: &Causes) -> Result<(), OutputFailure> {
    let keys = causes.keys().collect::<Vec<_>>();
    frame.uint(count(keys.len())?);
    for key in keys {
        issue(frame, key)?;
    }
    Ok(())
}

fn tuple(frame: &mut Frame, tuple: &ExactTuple) {
    frame.text(tuple.kind.spelling());
    frame.text(tuple.identity.as_str());
    frame.text(tuple.revision.as_str());
    frame.text(tuple.specification_revision.as_str());
    frame.text(tuple.semantic_digest.as_str());
}

/// The Set's tuples and bodies in the tuples' order, or why there is none.
fn set(frame: &mut Frame, set: &Admitted<Vec<SetEntry>>) -> Result<(), OutputFailure> {
    match set {
        Ok(entries) => {
            frame.uint(0);
            frame.uint(count(entries.len())?);
            for entry in entries {
                tuple(frame, &entry.tuple);
                frame.bytes(&entry.body);
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
/// unit, each blocking result, and the Set or why there is none. Submission
/// order is no part of the input, so reordering the collection changes
/// nothing here. A collection is no entry's own, so there is no entry domain.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let admitted = require_observable(resolved)?;
    let evaluation = &resolved.evaluation;
    let mut shared = Frame::new("artifact-collection-result");
    shared.uint(count(evaluation.statuses().count())?);
    for (recorded, _) in evaluation.statuses() {
        unit(&mut shared, recorded)?;
    }
    shared.uint(count(evaluation.issues().count())?);
    for key in evaluation.issues() {
        issue(&mut shared, key)?;
    }
    set(&mut shared, admitted)?;
    Ok(Observation {
        shared: shared.finish(),
        entry: None,
    })
}

#[cfg(test)]
mod tests {
    use intlify_shared_json::token::Token;
    use serde_json::Value;

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, FormalMember};
    use crate::input_limits::Bound;
    use crate::resolver::artifact_resolution::unit as collection_unit;
    use crate::resolver::evaluation::{
        Actual, Blocking, Code, Details, LimitExceeded, Phase, Reason, Shortfall,
    };
    use crate::resolver::limits::{fixture_limits, BoundId, Group};
    use crate::resolver::locales::fixtures::stand_in;
    use crate::resolver::specification::Slot;

    const ROOM: CollectionLimits = CollectionLimits {
        members: u64::MAX,
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

    fn member(kind: ArtifactKind, identity: &str) -> Value {
        envelope_of(kind, identity, "1", sample(kind))
    }

    fn prepared(values: &[Value]) -> PreparedCollection {
        let texts: Vec<String> = values.iter().map(Value::to_string).collect();
        let bytes: Vec<&[u8]> = texts.iter().map(String::as_bytes).collect();
        PreparedCollection {
            policy: ConfirmedPolicy::fixture_owned(fixture_limits(1)),
            collection: read_collection(Some(&bytes), &ROOM).unwrap(),
        }
    }

    fn digest(frame_with: impl FnOnce(&mut Frame)) -> super::super::observation::Digest {
        let mut frame = Frame::new("artifact-collection-test");
        frame_with(&mut frame);
        frame.finish()
    }

    fn apart<T: PartialEq>(values: &[T]) -> bool {
        values
            .iter()
            .enumerate()
            .all(|(index, left)| values[index + 1..].iter().all(|right| right != left))
    }

    #[test]
    fn the_retained_set_is_the_set_the_admission_built() {
        let values = [
            member(ArtifactKind::TargetProfile, "storefront-web"),
            member(ArtifactKind::ApprovalPolicy, "storefront-approval"),
        ];
        let prepared = prepared(&values);
        let admission = admit(&prepared.policy, &prepared.collection);
        let built = admission
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .iter()
            .map(|(tuple, artifact)| (tuple.clone(), artifact.member.canonical_body().to_vec()))
            .collect::<Vec<_>>();
        let statuses = admission.evaluation.statuses().count();
        let retained = Resolved::retain(admission);
        assert_eq!(retained.evaluation.statuses().count(), statuses);
        let entries = retained.result.unwrap().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries
                .into_iter()
                .map(|entry| (entry.tuple, entry.body))
                .collect::<Vec<_>>(),
            built
        );
        // The bodies are the members' own, in the tuples' order: the approval
        // policy's kind sorts first.
        assert_eq!(
            built[0].1,
            FormalMember::new(values[1].clone())
                .unwrap()
                .canonical_body()
        );
    }

    #[test]
    fn a_resolver_defect_fails_the_case() {
        let resolved = Resolved {
            evaluation: Evaluation::default(),
            result: Err(EvaluationInvariant::UnitRecordedTwice(collection_unit(
                CollectionSubject::Member(0),
                Check::AdmissibleMember,
            ))),
        };
        assert_eq!(
            require_observable(&resolved).err(),
            Some(OutputFailure::ArtifactCollectionInvariant)
        );
        assert_eq!(
            observe(&resolved).err(),
            Some(OutputFailure::ArtifactCollectionInvariant)
        );
    }

    /// What Phase 9 cannot record under a confirmed policy fails the case: a
    /// unit left unevaluated, a blocking result with details or without a
    /// reason, and a non-blocking result.
    #[test]
    fn only_what_a_confirmed_collection_admission_records_is_observed() {
        let values = [member(ArtifactKind::TargetProfile, "storefront-web")];
        let first = collection_unit(CollectionSubject::Member(0), Check::AdmissibleMember);
        let blocked = |blocking: Blocking| {
            let mut evaluation = Evaluation::default();
            let _ = evaluation
                .check(first.clone(), Ok(()), |()| Err::<(), _>(blocking))
                .unwrap();
            evaluation
        };
        let resolved = |evaluation| Resolved {
            evaluation,
            result: Ok(Ok(Vec::new())),
        };
        // The control: an ordinary admission is observed.
        assert!(observe(&prepared(&values).resolve()).is_ok());
        let not_evaluated = {
            let prepared = prepared(&values);
            let mut evaluation = Evaluation::default();
            let _ = admit_collection(
                &mut evaluation,
                Err(&stand_in(Slot::ProjectProfile)),
                &prepared.collection,
            )
            .unwrap();
            evaluation
        };
        // Each detail on a result that has its reason, so only the detail is
        // what Phase 9 cannot record.
        let detailed = |details| {
            blocked(Blocking {
                code: Code::ArtifactCollectionNotAdmitted,
                reason: Some(Reason::EnvelopeInvalid),
                details: Some(details),
            })
        };
        let bound = || BoundId {
            group: Group::ArtifactAdmission,
            name: "maxArtifacts",
        };
        let limited = detailed(Details::Limit(LimitExceeded {
            bound: bound(),
            limit: Bound::new(1).unwrap(),
            actual: Actual::Exact(2),
        }));
        let short = detailed(Details::Shortfall(Shortfall {
            bound: bound(),
            required: Bound::new(2).unwrap(),
            available: Bound::new(1).unwrap(),
        }));
        let unexplained = blocked(Blocking {
            code: Code::ArtifactCollectionNotAdmitted,
            reason: None,
            details: None,
        });
        let noticed = {
            let mut evaluation = prepared(&values).resolve().evaluation;
            evaluation
                .notice(first.clone(), Code::ArtifactCollectionNotAdmitted)
                .unwrap();
            evaluation
        };
        for (name, evaluation) in [
            ("not evaluated", not_evaluated),
            ("limit", limited),
            ("shortfall", short),
            ("no reason", unexplained),
            ("notice", noticed),
        ] {
            assert_eq!(
                observe(&resolved(evaluation)).err(),
                Some(OutputFailure::ArtifactCollectionInvariant),
                "{name}"
            );
        }
        // A reason, and no other part of the result, makes it observable.
        let explained = blocked(Blocking {
            code: Code::ArtifactCollectionNotAdmitted,
            reason: Some(Reason::EnvelopeInvalid),
            details: None,
        });
        assert!(observe(&resolved(explained)).is_ok());
    }

    #[test]
    fn only_phase_9_collection_units_frame_and_each_check_frames_apart() {
        let mut frame = Frame::new("artifact-collection-test");
        let foreign = Unit {
            phase: Phase::ArtifactReferenceResolution,
            subject: Subject::Vocabulary,
            check: Check::AdmissibleMember,
        };
        assert_eq!(
            unit(&mut frame, &foreign),
            Err(OutputFailure::ArtifactCollectionInvariant)
        );
        for check in [
            Check::SupportedAssertion,
            Check::MatchingData,
            Check::IntactArtifact,
            Check::AssertedArtifact,
            Check::OpaqueIdentity,
            Check::WithinCapability,
            Check::WithinBound,
            Check::CanonicalLocale,
            Check::UniqueLocale,
            Check::RequestedMember,
            Check::DistinctFromSource,
            Check::UniqueCandidate,
            Check::EffectiveMode,
            Check::VocabularyMember,
            Check::UniqueSurfaceClass,
            Check::UniqueDomain,
            Check::BootstrapCandidate,
            Check::CompatiblePolicy,
            Check::ResolvedReference,
        ] {
            assert_eq!(
                unit(
                    &mut frame,
                    &collection_unit(CollectionSubject::Member(0), check)
                ),
                Err(OutputFailure::ArtifactCollectionInvariant),
                "{check:?}"
            );
        }
        let framed = [
            (CollectionSubject::Member(0), Check::AdmissibleMember),
            (CollectionSubject::Member(0), Check::SingleOccurrence),
            (CollectionSubject::Member(0), Check::ConsistentContent),
            (CollectionSubject::Member(1), Check::AdmissibleMember),
            (CollectionSubject::Group(0), Check::AdmissibleMember),
            (CollectionSubject::Group(1), Check::AdmissibleMember),
        ]
        .map(|(subject, check)| {
            digest(|frame| unit(frame, &collection_unit(subject, check)).unwrap())
        });
        assert!(apart(&framed));
        // The phase is part of the unit.
        let other_phase = Unit {
            phase: Phase::ResourcePolicyAdmission,
            ..collection_unit(CollectionSubject::Member(0), Check::AdmissibleMember)
        };
        assert_ne!(
            digest(|frame| unit(frame, &other_phase).unwrap()),
            framed[0]
        );
    }

    #[test]
    fn every_part_of_a_set_frames_apart() {
        fn token(text: &str) -> Token {
            Token::new(text).unwrap()
        }

        let values = [member(ArtifactKind::TargetProfile, "storefront-web")];
        let entries = prepared(&values).resolve().result.unwrap().unwrap();
        let entry = entries[0].clone();
        let with = |change: fn(&mut SetEntry)| {
            let mut changed = entry.clone();
            change(&mut changed);
            Ok(vec![changed])
        };
        let blocked = {
            let mut evaluation = Evaluation::default();
            evaluation
                .check(
                    collection_unit(CollectionSubject::Member(0), Check::AdmissibleMember),
                    Ok(()),
                    |()| {
                        Err::<Vec<SetEntry>, _>(Blocking {
                            code: Code::ArtifactCollectionNotAdmitted,
                            reason: Some(Reason::KindUnsupported),
                            details: None,
                        })
                    },
                )
                .unwrap()
        };
        let framed = [
            Ok(vec![entry.clone()]),
            Ok(Vec::new()),
            Ok(vec![entry.clone(), entry.clone()]),
            with(|entry| entry.tuple.kind = ArtifactKind::TrustPolicy),
            with(|entry| entry.tuple.identity = token("storefront-server")),
            with(|entry| entry.tuple.revision = token("2")),
            with(|entry| entry.tuple.specification_revision = token("1")),
            with(|entry| {
                entry.tuple.semantic_digest = crate::artifacts::SemanticDigest::from_hash([1; 32]);
            }),
            with(|entry| entry.body.push(0)),
            blocked,
        ]
        .map(|admitted| digest(|frame| set(frame, &admitted).unwrap()));
        assert!(apart(&framed));
    }
}
