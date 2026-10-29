// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;

use crate::benchmark::target_group::{require_observable, PreparedTargets, Resolved};
use crate::resolver::evaluation::Status;

use super::{LogicalWork, WorkFailure, WorkKind, WorkStage, WorkValue};

fn count(value: usize) -> Result<u64, WorkFailure> {
    u64::try_from(value).map_err(|_| WorkFailure::UnrepresentableCounter)
}

fn sum(mut values: impl Iterator<Item = usize>) -> Result<usize, WorkFailure> {
    values.try_fold(0_usize, |total, value| {
        total
            .checked_add(value)
            .ok_or(WorkFailure::UnrepresentableCounter)
    })
}

/// Phase 11's unit for the map and four for each target, and Phase 12's two
/// over every group, two for each group and for each of a group's distinct
/// members, and one for each target's place.
fn units(targets: usize, groups: usize, members: usize) -> Option<usize> {
    let targets = targets.checked_mul(5)?;
    let groups = groups.checked_mul(2)?;
    let members = members.checked_mul(2)?;
    3_usize
        .checked_add(targets)?
        .checked_add(groups)?
        .checked_add(members)
}

impl LogicalWork {
    /// The workload in the units of the six bounds, read from the selected
    /// declaration rather than from the resolver under test, and what the
    /// evaluation recorded and established.
    pub(super) fn targets(
        &mut self,
        prepared: &PreparedTargets,
        resolved: &Resolved,
    ) -> Result<(), WorkFailure> {
        let retained =
            require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        let declaration = prepared
            .declaration()
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        let targets = &declaration.target_profiles;
        let groups = &declaration.deployment_groups;
        let longest = |lengths: &mut dyn Iterator<Item = usize>| lengths.max().unwrap_or(0);
        let target_bytes = longest(&mut targets.iter().map(|(id, _)| id.as_str().len()));
        let group_bytes = longest(&mut groups.iter().map(|(id, _)| id.as_str().len()));
        let occurrences = || groups.values().map(|group| group.members.as_slice().len());
        let most_members = longest(&mut occurrences());
        let memberships = sum(occurrences())?;
        let distinct = sum(groups.values().map(|group| {
            group
                .members
                .as_slice()
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
        }))?;
        // Each target, group, and distinct member has its units.
        let evaluation = &resolved.evaluation;
        let expected = units(targets.len(), groups.len(), distinct)
            .ok_or(WorkFailure::UnrepresentableCounter)?;
        if evaluation.statuses().count() != expected {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        for (kind, value) in [
            (WorkKind::TargetAndGroupTargets, targets.len()),
            (WorkKind::TargetAndGroupMaximumTargetIdBytes, target_bytes),
            (WorkKind::TargetAndGroupGroups, groups.len()),
            (WorkKind::TargetAndGroupMaximumGroupIdBytes, group_bytes),
            (WorkKind::TargetAndGroupMaximumMembersPerGroup, most_members),
            (WorkKind::TargetAndGroupMembershipOccurrences, memberships),
        ] {
            self.set(
                kind,
                WorkStage::PreparedInput,
                WorkValue::exact(count(value)?),
            );
        }
        let not_evaluated = evaluation
            .statuses()
            .filter(|(_, status)| matches!(status, Status::NotEvaluated(_)))
            .count();
        let resolved_targets = retained
            .targets
            .iter()
            .filter(|target| target.resolved.is_ok())
            .count();
        let resolved_groups = retained
            .groups
            .iter()
            .filter(|group| group.members.is_ok())
            .count();
        for (kind, value) in [
            (
                WorkKind::TargetAndGroupEvaluationUnits,
                evaluation.statuses().count(),
            ),
            (WorkKind::TargetAndGroupNotEvaluatedUnits, not_evaluated),
            (
                WorkKind::TargetAndGroupBlockingIssues,
                evaluation.issues().count(),
            ),
            (WorkKind::TargetAndGroupResolvedTargets, resolved_targets),
            (WorkKind::TargetAndGroupResolvedGroups, resolved_groups),
        ] {
            self.set(
                kind,
                WorkStage::OperationResult,
                WorkValue::exact(count(value)?),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::benchmark::cases::prepare::{prepare, Candidate};
    use crate::benchmark::cases::{declarations, Recipe, TargetAndGroupRecipe};
    use crate::benchmark::operation::Prepared;

    use super::super::{LogicalWork, WorkFailure};
    use super::units;

    fn candidate(recipe: TargetAndGroupRecipe) -> Candidate {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::TargetAndGroup(recipe) && case.limit.is_none())
            .unwrap();
        prepare(&declaration).unwrap()
    }

    #[test]
    fn the_units_follow_the_targets_groups_and_distinct_members() {
        // The storefront: two targets in one group of two.
        assert_eq!(units(2, 1, 2), Some(19));
        assert_eq!(units(1, 1, 1), Some(12));
        assert_eq!(units(usize::MAX, 1, 1), None);
        assert_eq!(units(1, usize::MAX, 1), None);
        assert_eq!(units(1, 1, usize::MAX), None);
        // Each product fits, but not their sum.
        let most = (usize::MAX - 3) / 5;
        assert_eq!(units(most, 1, 0), Some(usize::MAX));
        assert_eq!(units(most, 1, 1), None);
        assert_eq!(units(usize::MAX / 5, 0, 0), None);
    }

    /// A declaration the evaluation never resolved is no workload of it.
    #[test]
    fn work_disagreeing_with_the_declaration_is_rejected() {
        let swapped = |recipe, other| {
            let mut case = candidate(recipe);
            let donor = candidate(other);
            assert!(LogicalWork::observe(&case.prepared, &case.output).is_ok());
            let (Prepared::TargetAndGroup(prepared), Prepared::TargetAndGroup(donor)) =
                (&mut case.prepared, donor.prepared)
            else {
                unreachable!()
            };
            prepared.analysis = donor.analysis;
            prepared.config = donor.config;
            LogicalWork::observe(&case.prepared, &case.output)
        };
        // One target fewer, and one group of one.
        assert_eq!(
            swapped(
                TargetAndGroupRecipe::Storefront,
                TargetAndGroupRecipe::Minimal
            ),
            Err(WorkFailure::InvalidOrdinaryResult)
        );
        // As many targets, but one distinct member more.
        assert_eq!(
            swapped(
                TargetAndGroupRecipe::Storefront,
                TargetAndGroupRecipe::MemberUnknown
            ),
            Err(WorkFailure::InvalidOrdinaryResult)
        );
        // As many targets and members, but a group more.
        assert_eq!(
            swapped(
                TargetAndGroupRecipe::ManyTargets,
                TargetAndGroupRecipe::ManyGroups
            ),
            Err(WorkFailure::InvalidOrdinaryResult)
        );
    }
}
