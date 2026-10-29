// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015 Phase 4 over the selected declaration of a complete configuration and
//! a formally admitted collection: the capability's pre-policy bounds, the
//! Bootstrap Resource Policy Candidate Admission, the comparison, and every
//! recheck under the admitted policy. The capability and 018's admission are
//! a fixture owner's; the admission admits every candidate. Neither the result
//! nor its observation is a Profile or a shared artifact.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::envelope::Envelope;
use crate::model::ProfileId;
use crate::references::PolicyReference;
use crate::resolver::bootstrap::ResourcePolicyVerification;
use crate::resolver::evaluation::{
    Actual, Admitted, Causes, Check, Evaluation, IssueKey, LimitExceeded, Shortfall, Status,
    Subject, Unit,
};
use crate::resolver::limits::{ImplementationCapability, ResolutionLimits};
use crate::resolver::resource_policy::{
    admit_resource_policy, ConfigurationMeasures, DeclarationReferences, PolicyInvariant,
    PolicyOutcome, PolicySubject, ReferenceField,
};
use crate::structural::selection::{SelectorInput, StringLengthFact};

use super::observation::{Frame, Observation};
use super::operation::{Analysis, Config, OutputFailure};
use super::quantity::Quantity;

/// 018's admission as the fixture owner states it: every candidate is
/// authentic, and every content's members are allowed.
pub(super) struct FixtureVerification;

impl ResourcePolicyVerification for FixtureVerification {
    fn authenticates(&self, _: &Envelope<'_>) -> bool {
        true
    }

    fn admits_common_body(&self, _: &Map<String, Value>) -> bool {
        true
    }
}

/// A case's complete configuration and selected declaration, the selector it
/// was selected with, the capability, and the collection past 015's
/// pre-invocation boundary.
pub(super) struct PreparedPolicy {
    pub(super) analysis: Analysis,
    pub(super) config: Config,
    pub(super) selected: ProfileId,
    pub(super) selector: SelectorInput,
    pub(super) capability: ImplementationCapability,
    pub(super) collection: FormalCollection,
    /// The bytes of every declared Profile ID, in their unsigned byte order.
    pub(super) profile_ids: Vec<u64>,
}

/// What Phase 4 borrows from the configuration, built before the interval.
pub(super) struct PolicyInputs<'a> {
    pub(super) resource_limits: &'a PolicyReference,
    pub(super) references: DeclarationReferences<'a>,
    pub(super) measures: ConfigurationMeasures<'a>,
}

impl PreparedPolicy {
    /// The selected declaration's references and what the earlier phases
    /// measured, when the configuration is complete.
    pub(super) fn inputs(&self) -> Option<PolicyInputs<'_>> {
        if !self.analysis.is_complete() {
            return None;
        }
        let declaration = self.config.profiles().get(&self.selected)?;
        let structural_units = self.analysis.benchmark_structural_units()?;
        Some(PolicyInputs {
            resource_limits: &declaration.policies.resource_limits,
            references: DeclarationReferences {
                policies: Ok(&declaration.policies),
                target_profiles: Ok(&declaration.target_profiles),
            },
            measures: ConfigurationMeasures {
                value: Ok(self.analysis.benchmark_input_counts().value),
                profile_ids: Ok(&self.profile_ids),
                selector: &self.selector,
                structural_units: Ok(structural_units),
            },
        })
    }
}

/// Phase 4's record of every unit it ran and what it settled: the complete
/// ordinary output.
pub(super) struct Resolved {
    pub(super) evaluation: Evaluation,
    pub(super) result: Result<PolicyOutcome, PolicyInvariant>,
}

/// Phase 4 over a fresh evaluation: what preparation and every timed
/// invocation run.
pub(super) fn resolve(
    capability: &ImplementationCapability,
    collection: &FormalCollection,
    inputs: &PolicyInputs<'_>,
) -> Resolved {
    let mut evaluation = Evaluation::default();
    let result = admit_resource_policy(
        &mut evaluation,
        capability,
        collection,
        Ok(inputs.resource_limits),
        &inputs.references,
        &inputs.measures,
        &FixtureVerification,
    );
    Resolved { evaluation, result }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    selected_profile: String,
    capability: String,
    verification: String,
    submitted_members: Quantity,
    artifact_limit: Quantity,
    profile_id_byte_limit: Quantity,
    prerequisites: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(super) fn observe(prepared: &PreparedPolicy) -> Self {
        let limits = prepared.capability.policy_comparable();
        Self {
            scope: "selected-declaration-references-and-complete-submitted-collection".into(),
            selected_profile: prepared.selected.as_str().into(),
            capability: "fixture-owned-policy-comparable-bounds-not-an-admitted-capability".into(),
            verification: "fixture-owned-admission-admitting-every-candidate".into(),
            submitted_members: Quantity::new(
                u64::try_from(prepared.collection.members().len()).unwrap_or(u64::MAX),
            ),
            artifact_limit: Quantity::new(limits.artifact_admission.max_artifacts.get()),
            profile_id_byte_limit: Quantity::new(
                limits
                    .configuration_input
                    .structural
                    .max_profile_id_bytes
                    .get(),
            ),
            prerequisites: "complete-configuration-provisional-selection-and-formal-collection"
                .into(),
            input_storage: "borrowed-selected-references-and-resident-formal-collection".into(),
            structural_preparation:
                "complete-model-selection-measures-and-collection-before-interval".into(),
            ordinary_output: "fresh-evaluation-with-policy-outcome-or-resolver-defect".into(),
        }
    }
}

/// A resolver defect is no blocked outcome: the case fails rather than
/// completing as an expected failure. Logical work uses this screen too.
pub(super) fn require_observable(resolved: &Resolved) -> Result<&PolicyOutcome, OutputFailure> {
    resolved
        .result
        .as_ref()
        .map_err(|_| OutputFailure::ResourcePolicyInvariant)
}

fn count(value: usize) -> Result<u64, OutputFailure> {
    u64::try_from(value).map_err(|_| OutputFailure::ResourcePolicyInvariant)
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

fn subject(frame: &mut Frame, subject: &PolicySubject) -> Result<(), OutputFailure> {
    match subject {
        PolicySubject::Nodes => frame.uint(0),
        PolicySubject::Depth => frame.uint(1),
        PolicySubject::CollectionEntries => frame.uint(2),
        PolicySubject::StringBytes => frame.uint(3),
        PolicySubject::LongestString => frame.uint(4),
        PolicySubject::Profiles => frame.uint(5),
        PolicySubject::ProfileId(rank) => {
            frame.uint(6);
            frame.uint(count(*rank)?);
        }
        PolicySubject::Selector => frame.uint(7),
        PolicySubject::StructuralAnalysis => frame.uint(8),
        PolicySubject::Artifacts => frame.uint(9),
        PolicySubject::PolicyArtifacts => frame.uint(10),
        PolicySubject::TargetProfileArtifacts => frame.uint(11),
        PolicySubject::References => frame.uint(12),
        PolicySubject::Artifact(rank) => {
            frame.uint(13);
            frame.uint(count(*rank)?);
        }
        PolicySubject::ArtifactBytes => frame.uint(14),
        PolicySubject::Reference(own) => {
            frame.uint(15);
            field(frame, own);
        }
        PolicySubject::ReferenceBytes => frame.uint(16),
        PolicySubject::ResourceLimits => frame.uint(17),
    }
    Ok(())
}

/// Only Phase 4's own units can appear: no other phase runs in the interval.
fn unit(frame: &mut Frame, unit: &Unit) -> Result<(), OutputFailure> {
    let Subject::ResourcePolicy(policy) = &unit.subject else {
        return Err(OutputFailure::ResourcePolicyInvariant);
    };
    frame.text(unit.phase.id());
    subject(frame, policy)?;
    frame.uint(match unit.check {
        Check::WithinCapability => 0,
        Check::WithinBound => 1,
        Check::BootstrapCandidate => 2,
        Check::CompatiblePolicy => 3,
        Check::SupportedAssertion
        | Check::MatchingData
        | Check::IntactArtifact
        | Check::AssertedArtifact
        | Check::OpaqueIdentity
        | Check::CanonicalLocale
        | Check::UniqueLocale
        | Check::RequestedMember
        | Check::DistinctFromSource
        | Check::UniqueCandidate
        | Check::EffectiveMode
        | Check::VocabularyMember
        | Check::UniqueSurfaceClass
        | Check::UniqueDomain
        | Check::AdmissibleMember
        | Check::SingleOccurrence
        | Check::ConsistentContent
        | Check::ResolvedReference
        | Check::ProjectSubset
        | Check::SupportedDefault
        | Check::KnownMember
        | Check::UniqueMember
        | Check::SingleGroup
        | Check::KnownEndpoints
        | Check::DistinctEndpoints
        | Check::SameGroupEndpoints
        | Check::UniqueRelation
        | Check::ServerRole
        | Check::ClientRole
        | Check::SameRequestedLocales
        | Check::SameEffectiveDefault
        | Check::StaticCompatibility
        | Check::SingleRole => return Err(OutputFailure::ResourcePolicyInvariant),
    });
    Ok(())
}

fn issue(frame: &mut Frame, key: &IssueKey) -> Result<(), OutputFailure> {
    unit(frame, key.unit())?;
    frame.text(key.code().as_str());
    frame.flag(key.reason().is_some());
    if let Some(reason) = key.reason() {
        frame.text(reason.as_str());
    }
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

/// An issue's details: none, the bound it crossed, or the bound the policy
/// needs more of than the capability makes available.
fn details(frame: &mut Frame, limit: Option<LimitExceeded>, shortfall: Option<Shortfall>) {
    match (limit, shortfall) {
        (Some(exceeded), _) => {
            frame.uint(1);
            frame.text(exceeded.bound.group.path());
            frame.text(exceeded.bound.name);
            frame.uint(exceeded.limit.get());
            match exceeded.actual {
                Actual::Exact(value) => {
                    frame.uint(0);
                    frame.uint(value);
                }
                Actual::AtLeast(value) => {
                    frame.uint(1);
                    frame.uint(value);
                }
            }
        }
        (None, Some(shortfall)) => {
            frame.uint(2);
            frame.text(shortfall.bound.group.path());
            frame.text(shortfall.bound.name);
            frame.uint(shortfall.required.get());
            frame.uint(shortfall.available.get());
        }
        (None, None) => frame.uint(0),
    }
}

/// Every bound of a confirmed policy, in 015's canonical order.
fn limits(frame: &mut Frame, limits: &ResolutionLimits) {
    for (id, bound) in limits.bounds() {
        frame.text(id.group.path());
        frame.text(id.name);
        frame.uint(bound.get());
    }
}

fn confirmed<T>(
    frame: &mut Frame,
    value: &Admitted<T>,
    present: impl FnOnce(&mut Frame, &T),
) -> Result<(), OutputFailure> {
    match value {
        Ok(value) => {
            frame.uint(0);
            present(frame, value);
            Ok(())
        }
        Err(why) => {
            frame.uint(1);
            causes(frame, why)
        }
    }
}

/// The selector's String Length Fact: none for an absent or non-string
/// selector, its exact bytes, or only that it crossed the bound.
fn length(frame: &mut Frame, fact: Option<StringLengthFact>) {
    match fact {
        None => frame.uint(0),
        Some(StringLengthFact::Exact(bytes)) => {
            frame.uint(1);
            frame.uint(bytes);
        }
        Some(StringLengthFact::OverLimit) => frame.uint(2),
    }
}

/// The shared domain is what Phase 4 decided, none of it positional: every
/// unit's status, the blocking results with their details, the notices, and
/// the confirmed policy's bounds or why there is none. Submission order is no
/// part of the input, so reordering the collection changes nothing here. The
/// entry domain is the selector's String Length Fact under the final
/// projection authority, the first part of the selector's projection.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let outcome = require_observable(resolved)?;
    let evaluation = &resolved.evaluation;
    let mut shared = Frame::new("resource-policy-result");
    shared.uint(count(evaluation.statuses().count())?);
    for (recorded, status) in evaluation.statuses() {
        unit(&mut shared, recorded)?;
        match status {
            Status::Evaluated => shared.uint(0),
            Status::NotEvaluated(why) => {
                shared.uint(1);
                causes(&mut shared, why)?;
            }
        }
    }
    shared.uint(count(evaluation.issues().count())?);
    for key in evaluation.issues() {
        issue(&mut shared, key)?;
        details(
            &mut shared,
            evaluation.limit(key),
            evaluation.shortfall(key),
        );
    }
    shared.uint(count(evaluation.notices().count())?);
    for key in evaluation.notices() {
        issue(&mut shared, key)?;
    }
    confirmed(&mut shared, &outcome.confirmed, |frame, policy| {
        limits(frame, policy.limits());
    })?;
    let mut entry = Frame::new("resource-policy-entry");
    length(&mut entry, outcome.selector_length);
    Ok(Observation {
        shared: shared.finish(),
        entry: Some(entry.finish()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::evaluation::{EvaluationInvariant, Phase};
    use crate::resolver::resource_policy::unit as policy_unit;
    use crate::structural::selection::SelectionInvariant;

    fn digest(frame_with: impl FnOnce(&mut Frame)) -> super::super::observation::Digest {
        let mut frame = Frame::new("resource-policy-test");
        frame_with(&mut frame);
        frame.finish()
    }

    #[test]
    fn a_resolver_defect_fails_the_case() {
        for invariant in [
            PolicyInvariant::Evaluation(EvaluationInvariant::UnitRecordedTwice(policy_unit(
                PolicySubject::Artifacts,
                Check::WithinCapability,
            ))),
            PolicyInvariant::Selection(SelectionInvariant::BootstrapBoundMismatch),
        ] {
            let resolved = Resolved {
                evaluation: Evaluation::default(),
                result: Err(invariant),
            };
            assert_eq!(
                require_observable(&resolved).err(),
                Some(OutputFailure::ResourcePolicyInvariant)
            );
            assert_eq!(
                observe(&resolved).err(),
                Some(OutputFailure::ResourcePolicyInvariant)
            );
        }
    }

    #[test]
    fn only_phase_4_units_frame_and_each_check_frames_apart() {
        let mut frame = Frame::new("resource-policy-test");
        let foreign = Unit {
            phase: Phase::LocaleResolution,
            subject: Subject::Vocabulary,
            check: Check::WithinBound,
        };
        assert_eq!(
            unit(&mut frame, &foreign),
            Err(OutputFailure::ResourcePolicyInvariant)
        );
        for check in [
            Check::SupportedAssertion,
            Check::MatchingData,
            Check::IntactArtifact,
            Check::AssertedArtifact,
            Check::OpaqueIdentity,
            Check::CanonicalLocale,
            Check::UniqueLocale,
            Check::RequestedMember,
            Check::DistinctFromSource,
            Check::UniqueCandidate,
            Check::EffectiveMode,
            Check::VocabularyMember,
            Check::UniqueSurfaceClass,
            Check::UniqueDomain,
            Check::AdmissibleMember,
            Check::SingleOccurrence,
            Check::ConsistentContent,
            Check::ResolvedReference,
        ] {
            assert_eq!(
                unit(&mut frame, &policy_unit(PolicySubject::Artifacts, check)),
                Err(OutputFailure::ResourcePolicyInvariant)
            );
        }
        let checks = [
            Check::WithinCapability,
            Check::WithinBound,
            Check::BootstrapCandidate,
            Check::CompatiblePolicy,
        ]
        .map(|check| {
            digest(|frame| {
                unit(frame, &policy_unit(PolicySubject::Artifacts, check)).unwrap();
            })
        });
        for (index, left) in checks.iter().enumerate() {
            assert!(checks[index + 1..].iter().all(|right| right != left));
        }
    }

    #[test]
    fn every_subject_frames_apart() {
        use crate::model::TargetId;
        use crate::resolver::resource_policy::PolicySlot;

        let target = |id: &str| -> TargetId { serde_json::from_value(Value::from(id)).unwrap() };
        let subjects = [
            PolicySubject::Nodes,
            PolicySubject::Depth,
            PolicySubject::CollectionEntries,
            PolicySubject::StringBytes,
            PolicySubject::LongestString,
            PolicySubject::Profiles,
            PolicySubject::ProfileId(0),
            PolicySubject::ProfileId(1),
            PolicySubject::Selector,
            PolicySubject::StructuralAnalysis,
            PolicySubject::Artifacts,
            PolicySubject::PolicyArtifacts,
            PolicySubject::TargetProfileArtifacts,
            PolicySubject::References,
            PolicySubject::Artifact(0),
            PolicySubject::Artifact(1),
            PolicySubject::ArtifactBytes,
            PolicySubject::Reference(ReferenceField::Policy(PolicySlot::Approval)),
            PolicySubject::Reference(ReferenceField::Policy(PolicySlot::Trust)),
            PolicySubject::Reference(ReferenceField::TargetProfile(target("browser"))),
            PolicySubject::Reference(ReferenceField::TargetProfile(target("server"))),
            PolicySubject::ReferenceBytes,
            PolicySubject::ResourceLimits,
        ]
        .map(|own| digest(|frame| subject(frame, &own).unwrap()));
        for (index, left) in subjects.iter().enumerate() {
            assert!(subjects[index + 1..].iter().all(|right| right != left));
        }
    }

    #[test]
    fn length_facts_frame_apart() {
        let framed = [
            None,
            Some(StringLengthFact::Exact(0)),
            Some(StringLengthFact::Exact(3)),
            Some(StringLengthFact::OverLimit),
        ]
        .map(|fact| digest(|frame| length(frame, fact)));
        for (index, left) in framed.iter().enumerate() {
            assert!(framed[index + 1..].iter().all(|right| right != left));
        }
    }

    #[test]
    fn details_frame_apart_by_kind_and_value() {
        use crate::input_limits::Bound;
        use crate::resolver::limits::{BoundId, Group};

        let bound = |value| Bound::new(value).unwrap();
        let id = |name| BoundId {
            group: Group::ArtifactAdmission,
            name,
        };
        let limit = |actual| {
            Some(LimitExceeded {
                bound: id("maxArtifacts"),
                limit: bound(3),
                actual,
            })
        };
        let shortfall = |name, required| {
            Some(Shortfall {
                bound: id(name),
                required: bound(required),
                available: bound(3),
            })
        };
        let framed = [
            (None, None),
            (limit(Actual::Exact(4)), None),
            (limit(Actual::AtLeast(4)), None),
            (limit(Actual::Exact(5)), None),
            (None, shortfall("maxArtifacts", 4)),
            (None, shortfall("maxArtifacts", 5)),
            (None, shortfall("maxPolicyArtifacts", 4)),
        ]
        .map(|(limit, shortfall)| digest(|frame| details(frame, limit, shortfall)));
        for (index, left) in framed.iter().enumerate() {
            assert!(framed[index + 1..].iter().all(|right| right != left));
        }
    }
}
