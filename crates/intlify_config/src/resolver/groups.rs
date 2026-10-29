// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 12 (`group-resolution`) over the selected declaration's
//! Deployment Compatibility Groups: the Group ID map under
//! `maxDeploymentGroups`, each Group ID under `maxGroupIdBytes`, each group's
//! member occurrences under `maxMembersPerGroup` and every group's under
//! `maxMembershipOccurrences`, each member naming a declared Target ID once in
//! its group, and every declared Target ID in exactly one group. The groups
//! form an exact partition or nothing: no group is cut short, no target is
//! dropped or placed by inference, and no group is chosen first.
//!
//! Not the hydration relations, which Phase 13 checks within each group, and
//! not the selection of one group, which 020 admits per compiler transaction.
//! A subject is a Group ID, a member's Target ID within its group, or a
//! declared Target ID; a group's members and the groups are sets, so their
//! authoring order is no part of any.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{DeploymentGroup, DeploymentGroupId, NonEmptyMap, TargetId};

use super::evaluation::{
    and, both, within, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant,
    Phase, Subject, Unit,
};
use super::limits::{BoundId, ConfirmedPolicy, Group};
use super::targets::ResolvedTargets;

const MAX_DEPLOYMENT_GROUPS: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxDeploymentGroups",
};

const MAX_GROUP_ID_BYTES: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxGroupIdBytes",
};

const MAX_MEMBERS_PER_GROUP: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxMembersPerGroup",
};

const MAX_MEMBERSHIP_OCCURRENCES: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxMembershipOccurrences",
};

/// What one Phase 12 unit checks. Kinds are declared so that each unit's
/// prerequisites order before it: the two bounds over every group, each
/// group's own bounds, its members, and then each Target ID's place.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GroupSubject {
    /// The submitted Group ID map, under `maxDeploymentGroups`.
    Groups,
    /// Every member occurrence of every group, under
    /// `maxMembershipOccurrences`.
    Memberships,
    /// One Group ID, under `maxGroupIdBytes`.
    Group(DeploymentGroupId),
    /// One group's member occurrences, under `maxMembersPerGroup`.
    Members(DeploymentGroupId),
    /// A member of one group, by the Target ID it spells.
    Member(DeploymentGroupId, TargetId),
    /// A declared Target ID's place among the groups.
    Assignment(TargetId),
}

/// The unit Phase 12 records for `check` over `subject`.
pub(crate) fn unit(subject: GroupSubject, check: Check) -> Unit {
    Unit {
        phase: Phase::GroupResolution,
        subject: Subject::Group(subject),
        check,
    }
}

/// What Phase 12 established for later phases: each group's members, once the
/// group's own checks held, and whether the groups partition the targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedGroups {
    groups: BTreeMap<DeploymentGroupId, Admitted<BTreeSet<TargetId>>>,
    partition: Admitted<()>,
}

impl ResolvedGroups {
    /// One group's members in Target ID order, or why they are unavailable.
    pub(crate) fn get(&self, id: &DeploymentGroupId) -> Option<&Admitted<BTreeSet<TargetId>>> {
        self.groups.get(id)
    }

    /// The groups in Group ID order.
    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = (&DeploymentGroupId, &Admitted<BTreeSet<TargetId>>)> {
        self.groups.iter()
    }

    /// Every group's members admitted and every Target ID in exactly one
    /// group, or the causes of each part that is not.
    pub(crate) const fn partition(&self) -> &Admitted<()> {
        &self.partition
    }
}

/// Phase 12 under the confirmed policy, over the declared groups and the
/// targets Phase 11 resolved. The two bounds over every group are counted
/// first; each group's checks wait on the map's bound, its members on the
/// group's own bounds and the total, and a member's Target ID on the set of
/// declared Target IDs Phase 11 admitted. A Target ID's place waits on its own
/// admission and on every group's bounds, never on another group's members.
pub(crate) fn resolve_groups(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    declared: Admitted<&NonEmptyMap<DeploymentGroupId, DeploymentGroup>>,
    targets: Result<&ResolvedTargets<'_>, &Causes>,
) -> Result<Admitted<ResolvedGroups>, EvaluationInvariant> {
    let policy = policy.map_err(Causes::clone);
    let bounded = evaluation.check(
        unit(GroupSubject::Groups, Check::WithinBound),
        both(policy.clone(), declared.clone()),
        |(policy, declared)| {
            within(
                MAX_DEPLOYMENT_GROUPS,
                policy.limits().target_grouping.max_deployment_groups,
                declared.len(),
            )
        },
    )?;
    let memberships = evaluation.check(
        unit(GroupSubject::Memberships, Check::WithinBound),
        both(policy.clone(), declared.clone()),
        |(policy, declared)| {
            within(
                MAX_MEMBERSHIP_OCCURRENCES,
                policy.limits().target_grouping.max_membership_occurrences,
                declared
                    .values()
                    .map(|group| group.members.as_slice().len())
                    .sum(),
            )
        },
    )?;
    let declared = match declared {
        Ok(declared) => declared,
        Err(causes) => return Ok(Err(causes)),
    };
    let targets = targets.map_err(Causes::clone);
    let known = targets
        .clone()
        .and_then(|targets| targets.bounded().clone().map(|()| targets));
    let mut groups = BTreeMap::new();
    let mut every_group = memberships.clone();
    for (id, group) in declared.iter() {
        let own = own_bounds(evaluation, &policy, &bounded, id, group)?;
        every_group = and(every_group, own.clone());
        let gate = and(own, memberships.clone());
        groups.insert(id.clone(), members(evaluation, &gate, &known, id, group)?);
    }
    let assignments = match targets {
        Ok(targets) => assignments(evaluation, &every_group, declared, targets)?,
        Err(causes) => Err(causes),
    };
    let mut partition = assignments;
    for members in groups.values() {
        partition = and(
            partition,
            members.as_ref().map(|_| ()).map_err(Causes::clone),
        );
    }
    Ok(Ok(ResolvedGroups { groups, partition }))
}

/// One group's two bounds, after the map's: its ID's bytes and its member
/// occurrences.
fn own_bounds(
    evaluation: &mut Evaluation,
    policy: &Admitted<&ConfirmedPolicy>,
    bounded: &Admitted<()>,
    id: &DeploymentGroupId,
    group: &DeploymentGroup,
) -> Result<Admitted<()>, EvaluationInvariant> {
    let prerequisite = both(policy.clone(), bounded.clone());
    let bytes = evaluation.check(
        unit(GroupSubject::Group(id.clone()), Check::WithinBound),
        prerequisite.clone(),
        |(policy, ())| {
            within(
                MAX_GROUP_ID_BYTES,
                policy.limits().target_grouping.max_group_id_bytes,
                id.as_str().len(),
            )
        },
    )?;
    let occurrences = evaluation.check(
        unit(GroupSubject::Members(id.clone()), Check::WithinBound),
        prerequisite,
        |(policy, ())| {
            within(
                MAX_MEMBERS_PER_GROUP,
                policy.limits().target_grouping.max_members_per_group,
                group.members.as_slice().len(),
            )
        },
    )?;
    Ok(and(bytes, occurrences))
}

/// The group's members by the Target ID each spells. Each must name a
/// declared Target ID and occur once; repeated occurrences are one subject.
/// The set exists only when every member holds.
fn members(
    evaluation: &mut Evaluation,
    gate: &Admitted<()>,
    known: &Admitted<&ResolvedTargets<'_>>,
    id: &DeploymentGroupId,
    group: &DeploymentGroup,
) -> Result<Admitted<BTreeSet<TargetId>>, EvaluationInvariant> {
    let mut occurrences: BTreeMap<&TargetId, usize> = BTreeMap::new();
    for member in group.members.as_slice() {
        *occurrences.entry(member).or_default() += 1;
    }
    let mut holds = Ok(());
    for (&member, &count) in &occurrences {
        let subject = || GroupSubject::Member(id.clone(), member.clone());
        let declared = evaluation.check(
            unit(subject(), Check::KnownMember),
            both(gate.clone(), known.clone()),
            |((), targets)| {
                if targets.get(member).is_some() {
                    Ok(())
                } else {
                    Err(issue(Code::GroupMemberUnknown))
                }
            },
        )?;
        let once = evaluation.check(unit(subject(), Check::UniqueMember), gate.clone(), |()| {
            if count == 1 {
                Ok(())
            } else {
                Err(issue(Code::GroupMemberDuplicate))
            }
        })?;
        holds = and(holds, and(declared, once));
    }
    Ok(holds.map(|()| occurrences.into_keys().cloned().collect()))
}

/// Each declared Target ID's place: in exactly one group, counting a group
/// once however often it names the target.
fn assignments(
    evaluation: &mut Evaluation,
    every_group: &Admitted<()>,
    declared: &NonEmptyMap<DeploymentGroupId, DeploymentGroup>,
    targets: &ResolvedTargets<'_>,
) -> Result<Admitted<()>, EvaluationInvariant> {
    let mut containing: BTreeMap<&TargetId, usize> = BTreeMap::new();
    for group in declared.values() {
        let distinct: BTreeSet<&TargetId> = group.members.as_slice().iter().collect();
        for member in distinct {
            *containing.entry(member).or_default() += 1;
        }
    }
    let mut holds = Ok(());
    for (target, facts) in targets.iter() {
        let placed = evaluation.check(
            unit(GroupSubject::Assignment(target.clone()), Check::SingleGroup),
            both(facts.admitted.clone(), every_group.clone()),
            |((), ())| match containing.get(target).copied().unwrap_or(0) {
                1 => Ok(()),
                0 => Err(issue(Code::TargetUnassigned)),
                _ => Err(issue(Code::TargetAssignedToMultipleGroups)),
            },
        )?;
        holds = and(holds, placed);
    }
    Ok(holds)
}

const fn issue(code: Code) -> Blocking {
    Blocking {
        code,
        reason: None,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map, Value};

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple, FormalMember};
    use crate::input_limits::Bound;
    use crate::model::ProfileDeclaration;
    use crate::profile_fixtures::minimal_config;
    use crate::references::{PolicyReference, TargetProfileReference};
    use crate::resolver::artifact_resolution::admit_collection;
    use crate::resolver::evaluation::{Actual, LimitExceeded, Status};
    use crate::resolver::exact_resolution::resolve_fields;
    use crate::resolver::limits::{fixture_limits, ResolutionLimits};
    use crate::resolver::locales::fixtures::{canonicalizer, stand_in, target, waits_on};
    use crate::resolver::locales::{resolve_locales, LocaleFields};
    use crate::resolver::resource_policy::DeclarationReferences;
    use crate::resolver::specification::Slot;
    use crate::resolver::targets::{self, resolve_targets, TargetSubject};

    type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

    /// Collection limits no test reaches.
    const ROOM: CollectionLimits = CollectionLimits {
        members: u64::MAX,
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

    fn profile_of(id: &str) -> Value {
        envelope_of(
            ArtifactKind::TargetProfile,
            &format!("storefront-{id}"),
            "1",
            sample(ArtifactKind::TargetProfile),
        )
    }

    fn tuple_of(member: &Value) -> ExactTuple {
        FormalMember::new(member.clone())
            .unwrap()
            .envelope()
            .unwrap()
            .tuple()
            .clone()
    }

    /// The reference a declaration writes for `tuple`.
    fn reference_value(tuple: &ExactTuple) -> Value {
        json!({
            "kind": tuple.kind.spelling(),
            "identity": tuple.identity.as_str(),
            "revision": tuple.revision.as_str(),
            "specificationRevision": tuple.specification_revision.as_str(),
            "semanticDigest": tuple.semantic_digest.as_str(),
        })
    }

    fn group(id: &str) -> DeploymentGroupId {
        serde_json::from_value(json!(id)).unwrap()
    }

    /// What Phase 12 reads, which a test edits: the declaration, the policy's
    /// bounds, and whether a policy was confirmed and structural admission
    /// admitted the Group ID map.
    struct Inputs {
        app: Value,
        limits: ResolutionLimits,
        policy: Admitted<()>,
        declared: Admitted<()>,
    }

    impl Inputs {
        /// Targets `browser`, `server`, and `worker`, each requesting `en`
        /// and naming its own Target Profile; group `web` with `browser` and
        /// `server`, and group `jobs` with `worker`.
        fn new() -> Self {
            let mut app = minimal_config()["profiles"]["app"].clone();
            let mut targets = Map::new();
            for id in ["browser", "server", "worker"] {
                targets.insert(
                    id.into(),
                    json!({
                        "profile": reference_value(&tuple_of(&profile_of(id))),
                        "requestedLocales": ["en"],
                    }),
                );
            }
            app["targetProfiles"] = Value::Object(targets);
            app["deploymentGroups"] = json!({
                "web": {"members": ["browser", "server"]},
                "jobs": {"members": ["worker"]},
            });
            Self {
                app,
                limits: fixture_limits(100),
                policy: Ok(()),
                declared: Ok(()),
            }
        }

        fn members(&mut self, id: &str) -> &mut Value {
            &mut self.app["deploymentGroups"][id]["members"]
        }

        /// Phases 7, 9, 11, and 12 into one evaluation; `inspect` reads it and
        /// what Phase 12 returned.
        fn resolve<R>(&self, inspect: impl FnOnce(Evaluation, Admitted<ResolvedGroups>) -> R) -> R {
            let declaration: Declaration = serde_json::from_value(self.app.clone()).unwrap();
            let confirmed = ConfirmedPolicy::fixture_owned(self.limits.clone());
            let mut evaluation = Evaluation::default();
            let locales = resolve_locales(
                &mut evaluation,
                Ok(&confirmed),
                Ok(&canonicalizer(64)),
                &LocaleFields::declared(&declaration),
            )
            .unwrap();
            let members: Vec<String> = declaration
                .target_profiles
                .iter()
                .map(|(id, _)| profile_of(id.as_str()).to_string())
                .collect();
            let bytes: Vec<&[u8]> = members.iter().map(String::as_bytes).collect();
            let collection = read_collection(Some(&bytes), &ROOM).unwrap();
            let set = admit_collection(&mut evaluation, Ok(&confirmed), &collection).unwrap();
            let references = resolve_fields(
                &mut evaluation,
                set.as_ref(),
                &DeclarationReferences {
                    policies: Err(stand_in(Slot::ProjectProfile)),
                    target_profiles: Ok(&declaration.target_profiles),
                },
            )
            .unwrap();
            let unconfirmed = self.policy.clone().err();
            let policy = unconfirmed.as_ref().map_or(Ok(&confirmed), Err);
            let targets = resolve_targets(
                &mut evaluation,
                policy,
                Ok(&declaration.target_profiles),
                &locales,
                &references,
            )
            .unwrap();
            let declared = self
                .declared
                .clone()
                .map(|()| &declaration.deployment_groups);
            let groups =
                resolve_groups(&mut evaluation, policy, declared, targets.as_ref()).unwrap();
            inspect(evaluation, groups)
        }
    }

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    fn group_unit(subject: GroupSubject, check: Check) -> Unit {
        unit(subject, check)
    }

    fn member(group_id: &str, member: &str, check: Check) -> Unit {
        unit(GroupSubject::Member(group(group_id), target(member)), check)
    }

    fn assignment(id: &str) -> Unit {
        unit(GroupSubject::Assignment(target(id)), Check::SingleGroup)
    }

    /// Phase 12's units and their statuses, in 015's order.
    fn statuses(evaluation: &Evaluation) -> Vec<(Unit, Status)> {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.phase == Phase::GroupResolution)
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    /// Every blocking result, as its unit and code.
    fn issues(evaluation: &Evaluation) -> Vec<(Unit, Code)> {
        evaluation
            .issues()
            .map(|key| (key.unit().clone(), key.code()))
            .collect()
    }

    /// Every unit Phase 12 records for the default inputs, in 015's order.
    fn every_unit() -> Vec<Unit> {
        let mut units = vec![
            group_unit(GroupSubject::Groups, Check::WithinBound),
            group_unit(GroupSubject::Memberships, Check::WithinBound),
        ];
        for id in ["jobs", "web"] {
            units.push(group_unit(
                GroupSubject::Group(group(id)),
                Check::WithinBound,
            ));
        }
        for id in ["jobs", "web"] {
            units.push(group_unit(
                GroupSubject::Members(group(id)),
                Check::WithinBound,
            ));
        }
        for (group_id, id) in [("jobs", "worker"), ("web", "browser"), ("web", "server")] {
            units.push(member(group_id, id, Check::KnownMember));
            units.push(member(group_id, id, Check::UniqueMember));
        }
        for id in ["browser", "server", "worker"] {
            units.push(assignment(id));
        }
        units
    }

    fn names(members: &BTreeSet<TargetId>) -> Vec<&str> {
        members.iter().map(TargetId::as_str).collect()
    }

    #[test]
    fn subjects_order_the_two_bounds_then_groups_members_and_assignments() {
        let ordered = every_unit();
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        // Phase 12 commits after every Phase 11 unit.
        let last_of_phase_11 =
            targets::unit(TargetSubject::Target(target("zz")), Check::SupportedDefault);
        assert!(last_of_phase_11 < ordered[0]);
    }

    #[test]
    fn targets_partitioned_exactly_once_form_their_groups() {
        Inputs::new().resolve(|evaluation, groups| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(
                statuses(&evaluation),
                every_unit()
                    .into_iter()
                    .map(|unit| (unit, Status::Evaluated))
                    .collect::<Vec<_>>()
            );
            let groups = groups.unwrap();
            assert_eq!(groups.partition(), &Ok(()));
            assert_eq!(
                groups
                    .iter()
                    .map(|(id, members)| (id.as_str(), names(members.as_ref().unwrap())))
                    .collect::<Vec<_>>(),
                [("jobs", vec!["worker"]), ("web", vec!["browser", "server"])]
            );
        });
    }

    #[test]
    fn one_target_in_one_group_is_a_partition() {
        let mut inputs = Inputs::new();
        let app = &mut inputs.app;
        app["targetProfiles"]
            .as_object_mut()
            .unwrap()
            .retain(|id, _| id == "browser");
        app["deploymentGroups"] = json!({"web": {"members": ["browser"]}});
        inputs.resolve(|evaluation, groups| {
            assert_eq!(issues(&evaluation), []);
            let groups = groups.unwrap();
            assert_eq!(groups.partition(), &Ok(()));
            assert_eq!(
                names(groups.get(&group("web")).unwrap().as_ref().unwrap()),
                ["browser"]
            );
        });
    }

    #[test]
    fn a_member_naming_no_target_is_unknown_and_leaves_no_partition() {
        let mut inputs = Inputs::new();
        *inputs.members("web") = json!(["browser", "server", "phone"]);
        inputs.resolve(|evaluation, groups| {
            let unknown = member("web", "phone", Check::KnownMember);
            assert_eq!(
                issues(&evaluation),
                [(unknown.clone(), Code::GroupMemberUnknown)]
            );
            let groups = groups.unwrap();
            // Only `web` lacks its members; `jobs` and every target's place
            // were checked.
            assert!(groups.get(&group("web")).unwrap().is_err());
            assert!(groups.get(&group("jobs")).unwrap().is_ok());
            for id in ["browser", "server", "worker"] {
                assert_eq!(waits_on(&evaluation, &assignment(id)), None, "{id}");
            }
            let causes = groups.partition().clone().unwrap_err();
            assert_eq!(
                causes
                    .keys()
                    .map(|key| key.unit().clone())
                    .collect::<Vec<_>>(),
                [unknown]
            );
        });
    }

    #[test]
    fn a_member_named_twice_in_its_group_is_one_duplicate() {
        let mut inputs = Inputs::new();
        *inputs.members("web") = json!(["browser", "server", "browser"]);
        inputs.resolve(|evaluation, groups| {
            assert_eq!(
                issues(&evaluation),
                [(
                    member("web", "browser", Check::UniqueMember),
                    Code::GroupMemberDuplicate
                )]
            );
            // The repeat is not a second group: `browser` is still placed once.
            assert_eq!(waits_on(&evaluation, &assignment("browser")), None);
            let groups = groups.unwrap();
            assert!(groups.get(&group("web")).unwrap().is_err());
            assert!(groups.partition().is_err());
        });
    }

    #[test]
    fn a_target_in_no_group_or_in_two_is_no_partition() {
        let mut inputs = Inputs::new();
        *inputs.members("jobs") = json!(["server"]);
        inputs.resolve(|evaluation, groups| {
            assert_eq!(
                issues(&evaluation),
                [
                    (assignment("server"), Code::TargetAssignedToMultipleGroups),
                    (assignment("worker"), Code::TargetUnassigned),
                ]
            );
            let groups = groups.unwrap();
            // Each group's own members hold; only the partition does not.
            for id in ["jobs", "web"] {
                assert!(groups.get(&group(id)).unwrap().is_ok(), "{id}");
            }
            assert!(groups.partition().is_err());
        });
    }

    #[test]
    fn the_group_map_holds_at_its_bound_and_one_group_more_leaves_each_group_unchecked() {
        for (limit, over) in [(2, false), (1, true)] {
            let mut inputs = Inputs::new();
            inputs.limits.target_grouping.max_deployment_groups = bound(limit);
            inputs.resolve(|evaluation, groups| {
                if !over {
                    assert_eq!(issues(&evaluation), [], "{limit}");
                    return;
                }
                let map = group_unit(GroupSubject::Groups, Check::WithinBound);
                assert_eq!(
                    issues(&evaluation),
                    [(map.clone(), Code::ResourceLimitExceeded)]
                );
                let key = evaluation.issues().next().unwrap().clone();
                assert_eq!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        bound: MAX_DEPLOYMENT_GROUPS,
                        limit: bound(1),
                        actual: Actual::Exact(2),
                    })
                );
                // The total is counted all the same; everything else waits.
                for unit in &every_unit()[2..] {
                    assert_eq!(
                        waits_on(&evaluation, unit),
                        Some(vec![map.clone()]),
                        "{unit:?}"
                    );
                }
                assert!(groups.unwrap().partition().is_err());
            });
        }
    }

    #[test]
    fn every_member_occurrence_counts_toward_the_total_and_one_more_leaves_members_unchecked() {
        // Four occurrences, the repeated `browser` among them.
        for (limit, over) in [(4, false), (3, true)] {
            let mut inputs = Inputs::new();
            *inputs.members("web") = json!(["browser", "server", "browser"]);
            inputs.limits.target_grouping.max_membership_occurrences = bound(limit);
            inputs.resolve(|evaluation, _| {
                let total = group_unit(GroupSubject::Memberships, Check::WithinBound);
                let found: Vec<Unit> = issues(&evaluation)
                    .into_iter()
                    .map(|(unit, _)| unit)
                    .collect();
                if !over {
                    assert_eq!(found, [member("web", "browser", Check::UniqueMember)]);
                    return;
                }
                assert_eq!(found, std::slice::from_ref(&total));
                let key = evaluation.issues().next().unwrap().clone();
                assert_eq!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        bound: MAX_MEMBERSHIP_OCCURRENCES,
                        limit: bound(3),
                        actual: Actual::Exact(4),
                    })
                );
                // Each group's own bounds are still checked.
                assert_eq!(
                    waits_on(
                        &evaluation,
                        &group_unit(GroupSubject::Members(group("web")), Check::WithinBound)
                    ),
                    None
                );
                for unit in [
                    member("web", "browser", Check::KnownMember),
                    member("jobs", "worker", Check::UniqueMember),
                    assignment("browser"),
                ] {
                    assert_eq!(
                        waits_on(&evaluation, &unit),
                        Some(vec![total.clone()]),
                        "{unit:?}"
                    );
                }
            });
        }
    }

    #[test]
    fn each_group_id_and_member_count_holds_at_its_bound_and_one_more_blocks_that_group() {
        // `jobs` is four bytes; `web` has two members.
        let cases = [
            ("bytes", 4, None),
            ("bytes", 3, Some(GroupSubject::Group(group("jobs")))),
            ("members", 2, None),
            ("members", 1, Some(GroupSubject::Members(group("web")))),
        ];
        for (bound_kind, limit, over) in cases {
            let mut inputs = Inputs::new();
            let grouping = &mut inputs.limits.target_grouping;
            if bound_kind == "bytes" {
                grouping.max_group_id_bytes = bound(limit);
            } else {
                grouping.max_members_per_group = bound(limit);
            }
            inputs.resolve(|evaluation, groups| {
                let Some(subject) = over else {
                    assert_eq!(issues(&evaluation), [], "{bound_kind} {limit}");
                    return;
                };
                let own = group_unit(subject.clone(), Check::WithinBound);
                assert_eq!(
                    issues(&evaluation),
                    [(own.clone(), Code::ResourceLimitExceeded)]
                );
                let (blocked, other) = match subject {
                    GroupSubject::Group(_) => ("jobs", "web"),
                    _ => ("web", "jobs"),
                };
                let groups = groups.unwrap();
                assert!(groups.get(&group(blocked)).unwrap().is_err());
                assert!(groups.get(&group(other)).unwrap().is_ok());
                // Every place waits on every group's bounds.
                for id in ["browser", "server", "worker"] {
                    assert_eq!(
                        waits_on(&evaluation, &assignment(id)),
                        Some(vec![own.clone()])
                    );
                }
            });
        }
    }

    #[test]
    fn a_member_waits_on_the_target_map_phase_11_bounds() {
        let mut inputs = Inputs::new();
        inputs.limits.target_grouping.max_target_profiles = bound(2);
        inputs.resolve(|evaluation, _| {
            let map = targets::unit(TargetSubject::Targets, Check::WithinBound);
            for (group_id, id) in [("jobs", "worker"), ("web", "browser"), ("web", "server")] {
                assert_eq!(
                    waits_on(&evaluation, &member(group_id, id, Check::KnownMember)),
                    Some(vec![map.clone()])
                );
                // A repeat needs no Target ID.
                assert_eq!(
                    waits_on(&evaluation, &member(group_id, id, Check::UniqueMember)),
                    None
                );
            }
            for id in ["browser", "server", "worker"] {
                assert_eq!(
                    waits_on(&evaluation, &assignment(id)),
                    Some(vec![map.clone()])
                );
            }
        });
    }

    #[test]
    fn a_place_waits_on_its_own_target_id_alone() {
        let mut inputs = Inputs::new();
        // `browser` is seven bytes, the others six.
        inputs.limits.target_grouping.max_target_id_bytes = bound(6);
        inputs.resolve(|evaluation, groups| {
            let id = targets::unit(TargetSubject::Target(target("browser")), Check::WithinBound);
            assert_eq!(
                waits_on(&evaluation, &assignment("browser")),
                Some(vec![id.clone()])
            );
            for other in ["server", "worker"] {
                assert_eq!(waits_on(&evaluation, &assignment(other)), None);
            }
            // The member still names a declared Target ID.
            assert_eq!(
                waits_on(&evaluation, &member("web", "browser", Check::KnownMember)),
                None
            );
            let groups = groups.unwrap();
            assert!(groups.get(&group("web")).unwrap().is_ok());
            assert_eq!(
                groups
                    .partition()
                    .clone()
                    .unwrap_err()
                    .keys()
                    .map(|key| key.unit().clone())
                    .collect::<Vec<_>>(),
                [id]
            );
        });
    }

    #[test]
    fn without_a_confirmed_policy_no_group_is_checked() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::Coverage);
        inputs.policy = Err(causes.clone());
        inputs.resolve(|evaluation, groups| {
            assert!(statuses(&evaluation)
                .iter()
                .all(|(_, status)| *status == Status::NotEvaluated(causes.clone())));
            assert_eq!(statuses(&evaluation).len(), every_unit().len());
            assert_eq!(groups.unwrap().partition(), &Err(causes));
        });
    }

    #[test]
    fn a_group_map_structural_admission_did_not_admit_records_only_its_two_bounds() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::DeliveryGraph);
        inputs.declared = Err(causes.clone());
        inputs.resolve(|evaluation, groups| {
            assert_eq!(
                statuses(&evaluation),
                [
                    (
                        group_unit(GroupSubject::Groups, Check::WithinBound),
                        Status::NotEvaluated(causes.clone())
                    ),
                    (
                        group_unit(GroupSubject::Memberships, Check::WithinBound),
                        Status::NotEvaluated(causes.clone())
                    ),
                ]
            );
            assert_eq!(groups, Err(causes));
        });
    }

    #[test]
    fn reordering_groups_and_members_changes_nothing_phase_12_records() {
        let complete = Inputs::new();
        let mut blocked = Inputs::new();
        *blocked.members("web") = json!(["browser", "phone", "browser"]);
        *blocked.members("jobs") = json!(["worker", "browser"]);
        for authored in [complete, blocked] {
            let mut reordered = Inputs::new();
            reordered.app = authored.app.clone();
            let groups = reordered.app["deploymentGroups"].as_object_mut().unwrap();
            for declared in groups.values_mut() {
                declared["members"].as_array_mut().unwrap().reverse();
            }
            authored.resolve(|first_evaluation, first| {
                reordered.resolve(|second_evaluation, second| {
                    assert_eq!(statuses(&first_evaluation), statuses(&second_evaluation));
                    assert_eq!(issues(&first_evaluation), issues(&second_evaluation));
                    assert_eq!(first, second);
                });
            });
        }
    }
}
