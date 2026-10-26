// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;

use crate::artifacts::envelope::{ArtifactKind, Declaration, FormalMember};
use crate::benchmark::resource_policy::{require_observable, PreparedPolicy, Resolved};
use crate::model::{ProfileDeclaration, RequiredNullable};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::resolver::evaluation::{Status, Subject};
use crate::resolver::resource_policy::PolicySubject;

use super::{LogicalWork, WorkFailure, WorkKind, WorkStage, WorkValue};

fn count(value: usize) -> Result<u64, WorkFailure> {
    u64::try_from(value).map_err(|_| WorkFailure::UnrepresentableCounter)
}

fn sum(mut values: impl Iterator<Item = u64>) -> Result<u64, WorkFailure> {
    values.try_fold(0_u64, |total, value| {
        total
            .checked_add(value)
            .ok_or(WorkFailure::UnrepresentableCounter)
    })
}

const fn nullable(slot: &RequiredNullable<PolicyReference>) -> Option<&PolicyReference> {
    match slot {
        RequiredNullable::Reference(reference) => Some(reference),
        RequiredNullable::Null => None,
    }
}

/// The canonical bytes of every present `policies` reference and each
/// target's `profile`; an explicit `null` is no reference.
fn references(
    declaration: &ProfileDeclaration<PolicyReference, TargetProfileReference>,
) -> Vec<u64> {
    let slots = &declaration.policies;
    [
        Some(&slots.resource_limits),
        Some(&slots.trust),
        Some(&slots.source_admission),
        Some(&slots.approval),
        Some(&slots.selection),
        nullable(&slots.provider_routing),
        nullable(&slots.glossary_set),
    ]
    .into_iter()
    .flatten()
    .map(PolicyReference::canonical_bytes)
    .chain(
        declaration
            .target_profiles
            .values()
            .map(|target| target.profile.canonical_bytes()),
    )
    .collect()
}

impl LogicalWork {
    pub(super) fn policy(
        &mut self,
        prepared: &PreparedPolicy,
        resolved: &Resolved,
    ) -> Result<(), WorkFailure> {
        require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        if prepared.inputs().is_none() {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        let declaration = prepared
            .config
            .profiles()
            .get(&prepared.selected)
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        // Every occurrence 015's `artifactAdmission` bounds count, read from
        // the collection itself rather than from the resolver under test. A
        // kind counts only when its member's 017 declaration admits it.
        let members = prepared.collection.members();
        let declared = |wanted: fn(ArtifactKind) -> bool| {
            members
                .iter()
                .filter(|member| {
                    matches!(member.declaration(), Declaration::Admitted(kind) if wanted(kind))
                })
                .count()
        };
        let policies = declared(ArtifactKind::is_policy);
        let targets = declared(|kind| kind == ArtifactKind::TargetProfile);
        let bodies = members
            .iter()
            .map(FormalMember::canonical_body)
            .collect::<BTreeSet<_>>()
            .len();
        let artifact_bytes = sum(members.iter().map(FormalMember::canonical_bytes))?;
        let references = references(declaration);
        // Each distinct body has one unit under the capability and one under
        // the policy, both recorded whatever the selection.
        let evaluation = &resolved.evaluation;
        let checked = evaluation
            .statuses()
            .filter(|(unit, _)| {
                matches!(
                    unit.subject,
                    Subject::ResourcePolicy(PolicySubject::Artifact(_))
                )
            })
            .count();
        if Some(checked) != bodies.checked_mul(2) {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        for (kind, value) in [
            (
                WorkKind::ResourcePolicyArtifactOccurrences,
                count(members.len())?,
            ),
            (WorkKind::ResourcePolicyPolicyArtifacts, count(policies)?),
            (
                WorkKind::ResourcePolicyTargetProfileArtifacts,
                count(targets)?,
            ),
            (
                WorkKind::ResourcePolicyDistinctArtifactBodies,
                count(bodies)?,
            ),
            (
                WorkKind::ResourcePolicyArtifactCanonicalBytes,
                artifact_bytes,
            ),
            (WorkKind::ResourcePolicyReferences, count(references.len())?),
            (
                WorkKind::ResourcePolicyReferenceCanonicalBytes,
                sum(references.into_iter())?,
            ),
        ] {
            self.set(kind, WorkStage::PreparedInput, WorkValue::exact(value));
        }
        let not_evaluated = evaluation
            .statuses()
            .filter(|(_, status)| matches!(status, Status::NotEvaluated(_)))
            .count();
        for (kind, value) in [
            (
                WorkKind::ResourcePolicyEvaluationUnits,
                count(evaluation.statuses().count())?,
            ),
            (
                WorkKind::ResourcePolicyNotEvaluatedUnits,
                count(not_evaluated)?,
            ),
            (
                WorkKind::ResourcePolicyBlockingIssues,
                count(evaluation.issues().count())?,
            ),
        ] {
            self.set(kind, WorkStage::OperationResult, WorkValue::exact(value));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, Recipe, ResourcePolicyRecipe};
    use crate::benchmark::operation::Prepared;

    use super::super::{LogicalWork, WorkFailure};

    fn candidate(recipe: ResourcePolicyRecipe) -> crate::benchmark::cases::prepare::Candidate {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::ResourcePolicy(recipe) && case.limit.is_none())
            .unwrap();
        prepare(&declaration).unwrap()
    }

    /// Present nullable policies and every target count; a `null` does not.
    #[test]
    fn every_present_reference_counts_once() {
        use std::sync::Arc;

        use crate::benchmark::operation::Schema;
        use crate::materialize::materialize_file;
        use crate::profile_fixtures::{complete_config, minimal_config};
        use crate::structural::StructuralLimits;

        let declared = |value: &serde_json::Value| {
            let doc = materialize_file(
                Arc::from(serde_json::to_vec(value).unwrap()),
                crate::materialize_tests::limits(),
            )
            .unwrap();
            let bound = |value| crate::input_limits::Bound::new(value).unwrap();
            let limits = StructuralLimits {
                max_profiles: bound(64),
                max_profile_id_bytes: bound(256),
                max_structural_analysis_units: bound(1_000_000),
            };
            let config = Schema::for_model()
                .unwrap()
                .analyze(Arc::new(doc), limits)
                .unwrap()
                .construct()
                .unwrap()
                .unwrap();
            let app = serde_json::from_value(serde_json::json!("app")).unwrap();
            super::references(config.profiles().get(&app).unwrap())
        };
        // Five required `policies` and the browser target.
        assert_eq!(declared(&minimal_config()).len(), 6);
        // Both nullable `policies` present, and the server target too.
        let complete = declared(&complete_config());
        assert_eq!(complete.len(), 9);
        assert!(complete.iter().all(|&bytes| bytes > 0));
    }

    /// A collection the evaluation never checked is no workload of it.
    #[test]
    fn work_disagreeing_with_the_collection_is_rejected() {
        let mut lone = candidate(ResourcePolicyRecipe::PolicyOnly);
        let mixed = candidate(ResourcePolicyRecipe::Mixed);
        let (Prepared::ResourcePolicy(prepared), Prepared::ResourcePolicy(other)) =
            (&mut lone.prepared, &mixed.prepared)
        else {
            unreachable!()
        };
        // Three distinct bodies, but the evaluation checked one.
        prepared.collection = other.collection.clone();
        assert_eq!(
            LogicalWork::observe(&lone.prepared, &lone.output),
            Err(WorkFailure::InvalidOrdinaryResult)
        );
    }
}
