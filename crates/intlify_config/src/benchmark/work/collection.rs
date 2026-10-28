// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;

use crate::artifacts::envelope::{ArtifactKind, Declaration, FormalMember};
use crate::benchmark::artifact_collection::{require_observable, PreparedCollection, Resolved};
use crate::resolver::artifact_resolution::CollectionSubject;
use crate::resolver::evaluation::Subject;

use super::{LogicalWork, UnavailableWork, WorkFailure, WorkKind, WorkStage, WorkValue};

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

impl LogicalWork {
    pub(super) fn collection(
        &mut self,
        prepared: &PreparedCollection,
        resolved: &Resolved,
    ) -> Result<(), WorkFailure> {
        let set = require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
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
            .collect::<BTreeSet<_>>();
        let tupled = members
            .iter()
            .filter(|member| member.envelope().is_some())
            .map(FormalMember::canonical_body)
            .collect::<BTreeSet<_>>();
        let artifact_bytes = sum(members.iter().map(FormalMember::canonical_bytes))?;
        // Each distinct body has a unit checking it, and each one that
        // establishes a tuple another checking its occurrences.
        let evaluation = &resolved.evaluation;
        let checked = evaluation
            .statuses()
            .filter(|(unit, _)| {
                matches!(
                    unit.subject,
                    Subject::ArtifactCollection(CollectionSubject::Member(_))
                )
            })
            .count();
        if Some(checked) != bodies.len().checked_add(tupled.len()) {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        for (kind, value) in [
            (
                WorkKind::ArtifactCollectionArtifactOccurrences,
                count(members.len())?,
            ),
            (
                WorkKind::ArtifactCollectionPolicyArtifacts,
                count(policies)?,
            ),
            (
                WorkKind::ArtifactCollectionTargetProfileArtifacts,
                count(targets)?,
            ),
            (
                WorkKind::ArtifactCollectionDistinctArtifactBodies,
                count(bodies.len())?,
            ),
            (
                WorkKind::ArtifactCollectionArtifactCanonicalBytes,
                artifact_bytes,
            ),
        ] {
            self.set(kind, WorkStage::PreparedInput, WorkValue::exact(value));
        }
        for (kind, value) in [
            (
                WorkKind::ArtifactCollectionEvaluationUnits,
                count(evaluation.statuses().count())?,
            ),
            (
                WorkKind::ArtifactCollectionBlockingIssues,
                count(evaluation.issues().count())?,
            ),
        ] {
            self.set(kind, WorkStage::OperationResult, WorkValue::exact(value));
        }
        // Only a complete collection admission builds the Set.
        let built = match set {
            Ok(entries) => WorkValue::exact(count(entries.len())?),
            Err(_) => WorkValue::Unavailable {
                reason: UnavailableWork::ArtifactCollectionNotAdmitted,
            },
        };
        self.set(
            WorkKind::ArtifactCollectionSetArtifacts,
            WorkStage::OperationResult,
            built,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::benchmark::cases::prepare::{prepare, Candidate};
    use crate::benchmark::cases::{declarations, ArtifactCollectionRecipe, Recipe};
    use crate::benchmark::operation::Prepared;

    use super::super::{LogicalWork, WorkFailure};

    fn candidate(recipe: ArtifactCollectionRecipe) -> Candidate {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::ArtifactCollection(recipe))
            .unwrap();
        prepare(&declaration).unwrap()
    }

    /// A collection the evaluation never checked is no workload of it.
    #[test]
    fn work_disagreeing_with_the_collection_is_rejected() {
        let mut lone = candidate(ArtifactCollectionRecipe::PolicyOnly);
        let mixed = candidate(ArtifactCollectionRecipe::Mixed);
        assert!(LogicalWork::observe(&lone.prepared, &lone.output).is_ok());
        let (Prepared::ArtifactCollection(prepared), Prepared::ArtifactCollection(other)) =
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
