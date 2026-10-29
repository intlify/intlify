// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;

use crate::benchmark::hydration_relation::{require_observable, PreparedHydration, Resolved};
use crate::model::TargetId;
use crate::resolver::evaluation::Status;

use super::{LogicalWork, WorkFailure, WorkKind, WorkStage, WorkValue};

fn count(value: usize) -> Result<u64, WorkFailure> {
    u64::try_from(value).map_err(|_| WorkFailure::UnrepresentableCounter)
}

/// The relations' unit, ten for each distinct relation, and one for each
/// Target ID that takes a role between distinct endpoints in its group.
fn units(relations: usize, roles: usize) -> Option<usize> {
    relations
        .checked_mul(10)?
        .checked_add(roles)?
        .checked_add(1)
}

/// What one group declares: its relation occurrences, its distinct
/// relations, its longest endpoint, and the Target IDs that take a role.
#[derive(Debug, Default, PartialEq, Eq)]
struct Declared {
    occurrences: usize,
    distinct: usize,
    endpoint_bytes: usize,
    roles: usize,
}

fn declared<'a>(relations: impl Iterator<Item = (&'a TargetId, &'a TargetId)>) -> Declared {
    let mut occurrences = 0;
    let mut distinct = BTreeSet::new();
    for pair in relations {
        occurrences += 1;
        distinct.insert(pair);
    }
    let endpoint_bytes = distinct
        .iter()
        .flat_map(|(server, client)| [server.as_str().len(), client.as_str().len()])
        .max()
        .unwrap_or(0);
    let roles = distinct
        .iter()
        .filter(|(server, client)| server != client)
        .flat_map(|(server, client)| [*server, *client])
        .collect::<BTreeSet<_>>()
        .len();
    Declared {
        occurrences,
        distinct: distinct.len(),
        endpoint_bytes,
        roles,
    }
}

impl LogicalWork {
    /// The workload in the units of the relations' bound and the endpoint's,
    /// read from the selected declaration rather than from the resolver under
    /// test, and what the evaluation recorded and established.
    pub(super) fn hydration(
        &mut self,
        prepared: &PreparedHydration,
        resolved: &Resolved,
    ) -> Result<(), WorkFailure> {
        let retained =
            require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        let declaration = prepared
            .declaration()
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        let mut total = Declared::default();
        for group in declaration.deployment_groups.values() {
            let relations = group
                .hydration_relations
                .as_option()
                .map_or(&[][..], Vec::as_slice);
            let own = declared(
                relations
                    .iter()
                    .map(|relation| (&relation.server, &relation.client)),
            );
            total = Declared {
                occurrences: total.occurrences + own.occurrences,
                distinct: total.distinct + own.distinct,
                endpoint_bytes: total.endpoint_bytes.max(own.endpoint_bytes),
                roles: total.roles + own.roles,
            };
        }
        // Each distinct relation and each role has its units.
        let evaluation = &resolved.evaluation;
        let expected =
            units(total.distinct, total.roles).ok_or(WorkFailure::UnrepresentableCounter)?;
        if evaluation.statuses().count() != expected {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        for (kind, value) in [
            (WorkKind::HydrationRelationOccurrences, total.occurrences),
            (WorkKind::HydrationDistinctRelations, total.distinct),
            (
                WorkKind::HydrationMaximumEndpointIdBytes,
                total.endpoint_bytes,
            ),
            (WorkKind::HydrationRoleTargets, total.roles),
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
        let checked = retained
            .groups
            .iter()
            .filter_map(|group| group.relations.as_ref().ok())
            .map(Vec::len)
            .sum();
        let checked_groups = retained
            .groups
            .iter()
            .filter(|group| group.relations.is_ok())
            .count();
        for (kind, value) in [
            (
                WorkKind::HydrationEvaluationUnits,
                evaluation.statuses().count(),
            ),
            (WorkKind::HydrationNotEvaluatedUnits, not_evaluated),
            (
                WorkKind::HydrationBlockingIssues,
                evaluation.issues().count(),
            ),
            (WorkKind::HydrationCheckedRelations, checked),
            (WorkKind::HydrationCheckedGroups, checked_groups),
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
    use serde_json::json;

    use crate::benchmark::cases::prepare::{prepare, Candidate};
    use crate::benchmark::cases::{declarations, HydrationRecipe, Recipe};
    use crate::benchmark::operation::Prepared;
    use crate::model::TargetId;

    use super::super::{LogicalWork, WorkFailure};
    use super::{declared, units, Declared};

    fn candidate(recipe: HydrationRecipe) -> Candidate {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Hydration(recipe) && case.limit.is_none())
            .unwrap();
        prepare(&declaration).unwrap()
    }

    fn target(id: &str) -> TargetId {
        serde_json::from_value(json!(id)).unwrap()
    }

    #[test]
    fn the_units_follow_the_relations_and_the_roles() {
        // 015's example: one relation, and two Target IDs with a role.
        assert_eq!(units(1, 2), Some(13));
        assert_eq!(units(0, 0), Some(1));
        assert_eq!(units(usize::MAX, 0), None);
        assert_eq!(units(usize::MAX / 10, usize::MAX), None);
        assert_eq!(units(0, usize::MAX), None);
    }

    #[test]
    fn a_group_counts_occurrences_distinct_pairs_and_roles_between_distinct_endpoints() {
        let (ssr, browser, app) = (target("ssr"), target("browser"), target("app"));
        // The repeated pair is one relation, and the equal-endpoint relation
        // takes no role.
        let pairs = [(&ssr, &browser), (&ssr, &browser), (&app, &app)];
        assert_eq!(
            declared(pairs.into_iter()),
            Declared {
                occurrences: 3,
                distinct: 2,
                endpoint_bytes: 7,
                roles: 2,
            }
        );
        assert_eq!(declared(std::iter::empty()), Declared::default());
    }

    /// A declaration the evaluation never validated is no workload of it.
    #[test]
    fn work_disagreeing_with_the_declaration_is_rejected() {
        let swapped = |recipe, other| {
            let mut case = candidate(recipe);
            let donor = candidate(other);
            assert!(LogicalWork::observe(&case.prepared, &case.output).is_ok());
            let (Prepared::Hydration(prepared), Prepared::Hydration(donor)) =
                (&mut case.prepared, donor.prepared)
            else {
                unreachable!()
            };
            prepared.analysis = donor.analysis;
            prepared.config = donor.config;
            LogicalWork::observe(&case.prepared, &case.output)
        };
        // One relation fewer.
        assert_eq!(
            swapped(HydrationRecipe::Storefront, HydrationRecipe::NoRelations),
            Err(WorkFailure::InvalidOrdinaryResult)
        );
        // As many occurrences, but one distinct relation between two Target
        // IDs.
        assert_eq!(
            swapped(
                HydrationRecipe::DualRole,
                HydrationRecipe::DuplicateRelation
            ),
            Err(WorkFailure::InvalidOrdinaryResult)
        );
    }
}
