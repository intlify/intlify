// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;

use crate::benchmark::resolved_binding::{require_observable, PreparedBindings, Resolved};
use crate::resolver::evaluation::Status;

use super::policy::references;
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
    pub(super) fn bindings(
        &mut self,
        prepared: &PreparedBindings,
        resolved: &Resolved,
    ) -> Result<(), WorkFailure> {
        let result =
            require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        let inputs = prepared
            .inputs()
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        let declaration = prepared
            .config
            .profiles()
            .get(&prepared.selected)
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        // Every present reference, read from the declaration rather than from
        // the resolver under test, and the distinct tuples they name.
        let references = references(declaration);
        let distinct = references
            .iter()
            .map(|(tuple, _)| tuple)
            .collect::<BTreeSet<_>>()
            .len();
        let bytes = sum(references.iter().map(|(_, bytes)| *bytes))?;
        // Each present reference has one unit.
        let evaluation = &resolved.evaluation;
        if evaluation.statuses().count() != references.len() {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        // The Set the collection admission built before the interval.
        let set = match &inputs.set {
            Ok(set) => WorkValue::exact(count(set.len())?),
            Err(_) => WorkValue::Unavailable {
                reason: UnavailableWork::ArtifactCollectionNotAdmitted,
            },
        };
        for (kind, value) in [
            (
                WorkKind::ResolvedBindingReferences,
                WorkValue::exact(count(references.len())?),
            ),
            (
                WorkKind::ResolvedBindingDistinctReferences,
                WorkValue::exact(count(distinct)?),
            ),
            (
                WorkKind::ResolvedBindingReferenceCanonicalBytes,
                WorkValue::exact(bytes),
            ),
            (WorkKind::ResolvedBindingSetArtifacts, set),
        ] {
            self.set(kind, WorkStage::PreparedInput, value);
        }
        // Only a complete resolution binds, once for each distinct reference.
        let bound = match result {
            Ok(retained) if retained.bindings.len() == distinct => {
                WorkValue::exact(count(retained.bindings.len())?)
            }
            Ok(_) => return Err(WorkFailure::InvalidOrdinaryResult),
            Err(_) => WorkValue::Unavailable {
                reason: UnavailableWork::ArtifactReferencesNotResolved,
            },
        };
        let not_evaluated = evaluation
            .statuses()
            .filter(|(_, status)| matches!(status, Status::NotEvaluated(_)))
            .count();
        for (kind, value) in [
            (
                WorkKind::ResolvedBindingEvaluationUnits,
                WorkValue::exact(count(evaluation.statuses().count())?),
            ),
            (
                WorkKind::ResolvedBindingNotEvaluatedUnits,
                WorkValue::exact(count(not_evaluated)?),
            ),
            (
                WorkKind::ResolvedBindingBlockingIssues,
                WorkValue::exact(count(evaluation.issues().count())?),
            ),
            (WorkKind::ResolvedBindingBindings, bound),
        ] {
            self.set(kind, WorkStage::OperationResult, value);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::benchmark::cases::prepare::{prepare, Candidate};
    use crate::benchmark::cases::{declarations, Recipe, ResolvedBindingRecipe};
    use crate::benchmark::operation::Prepared;

    use super::super::{LogicalWork, WorkFailure};

    fn candidate(recipe: ResolvedBindingRecipe) -> Candidate {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::ResolvedBinding(recipe))
            .unwrap();
        prepare(&declaration).unwrap()
    }

    /// A declaration the evaluation never resolved is no workload of it.
    #[test]
    fn work_disagreeing_with_the_declaration_is_rejected() {
        let swapped = |recipe, other| {
            let mut case = candidate(recipe);
            let donor = candidate(other);
            assert!(LogicalWork::observe(&case.prepared, &case.output).is_ok());
            let (Prepared::ResolvedBinding(prepared), Prepared::ResolvedBinding(donor)) =
                (&mut case.prepared, donor.prepared)
            else {
                unreachable!()
            };
            prepared.analysis = donor.analysis;
            prepared.config = donor.config;
            LogicalWork::observe(&case.prepared, &case.output)
        };
        // Nine references, but the evaluation resolved six, and bound none.
        assert_eq!(
            swapped(
                ResolvedBindingRecipe::NotFound,
                ResolvedBindingRecipe::EveryReference
            ),
            Err(WorkFailure::InvalidOrdinaryResult)
        );
        // As many references and units, but 68 distinct references and six
        // bindings.
        assert_eq!(
            swapped(
                ResolvedBindingRecipe::SharedTargetProfile,
                ResolvedBindingRecipe::ManyTargets
            ),
            Err(WorkFailure::InvalidOrdinaryResult)
        );
    }
}
