// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 12 (`group-resolution`) over the selected declaration's
//! Deployment Compatibility Groups: the Group ID map under
//! `maxDeploymentGroups`, each Group ID under `maxGroupIdBytes`, each group's
//! member occurrences under `maxMembersPerGroup` and every group's under
//! `maxMembershipOccurrences`, each member under `maxTargetIdBytes` and naming
//! a declared Target ID once in its group, and every declared Target ID in
//! exactly one group. The groups form an exact partition or nothing: no group
//! is cut short, no target is dropped or placed by inference, and no group is
//! chosen first.
//!
//! Not the hydration relations, which Phase 13 checks within each group, and
//! not the selection of one group, which 020 admits per compiler transaction.
//! A subject is a Group ID, a member's Target ID within its group, or a
//! declared Target ID; a group's members and the groups are sets, so their
//! authoring order is no part of any.
//!
//! A count past its bound is observed where the count is, beside the
//! confirmed policy, which supplies the bound (D15:2250): the Group ID map
//! for the groups and the memberships, a Group ID's key for its bytes, a
//! group's `members` for their count, and each occurrence of a member for its
//! bytes (D15:2388).
//!
//! A member that names no declared Target ID is observed at each of its
//! occurrences beside the declared Target IDs, the set it must belong to, and
//! a repeated member at each occurrence beside the others (D15:2250). A
//! declared Target ID that no group names is observed at its key beside the
//! groups, the set it must belong to; one that several groups name, at each
//! of its occurrences beside those in the other groups, the conflicting
//! group/member evidence (D15:2439).
//!
//! Each group and each member occurrence is read on its own (D15:2150, 2368,
//! 3445). The counts take what was submitted, an entry whose key failed and a
//! member that failed included. A value that failed has no subject here, and
//! holds back only what needs it: a group's members and the partition wait on
//! every occurrence, while a target's place is found from the groups that can
//! be read and waits on the others only when none of those names it
//! (D15:2185, 2366, 3479).

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{DeploymentGroup, DeploymentGroupId, NonEmptyMap, TargetId};

use super::declaration_observer::{beside, each_beside, DeclarationObserver, ObserverInvariant};
use super::evaluation::{
    and, both, within, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant,
    Found, Observation, Phase, Subject, Unit,
};
use super::evidence::EvidenceProjection;
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

const MAX_TARGET_ID_BYTES: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxTargetIdBytes",
};

/// A resolver bug Phase 12 refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupInvariant {
    Evaluation(EvaluationInvariant),
    /// What a check looked at in the declaration has no Evidence.
    Observer(ObserverInvariant),
}

impl From<EvaluationInvariant> for GroupInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

impl From<ObserverInvariant> for GroupInvariant {
    fn from(invariant: ObserverInvariant) -> Self {
        Self::Observer(invariant)
    }
}

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
    /// A member of one group, by the Target ID it spells: under
    /// `maxTargetIdBytes`, then known and unique.
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

/// One entry of the Group ID map with an admitted key: its Group ID and its
/// members, element by element, or why it has none to read.
#[derive(Debug, Clone)]
pub(crate) struct GroupField<'a> {
    pub(crate) id: &'a DeploymentGroupId,
    pub(crate) members: Admitted<Vec<Admitted<&'a TargetId>>>,
}

/// The Group ID map as Phase 12 reads it: how many entries it holds and how
/// many member occurrences their arrays hold, whatever their keys and values
/// (D15:1933, 1936), whether every key was admitted, and each entry with an
/// admitted key, in Group ID order. An entry whose key failed has no subject
/// here (D15:2185).
#[derive(Debug, Clone)]
pub(crate) struct GroupFields<'a> {
    pub(crate) submitted: usize,
    pub(crate) memberships: Admitted<usize>,
    pub(crate) keys: Admitted<()>,
    pub(crate) entries: Vec<GroupField<'a>>,
}

impl<'a> GroupFields<'a> {
    /// The fields of a map structural admission admitted whole.
    pub(crate) fn declared(groups: &'a NonEmptyMap<DeploymentGroupId, DeploymentGroup>) -> Self {
        Self {
            submitted: groups.len(),
            memberships: Ok(groups
                .values()
                .map(|group| group.members.as_slice().len())
                .sum()),
            keys: Ok(()),
            entries: groups
                .iter()
                .map(|(id, group)| GroupField {
                    id,
                    members: Ok(group.members.as_slice().iter().map(Ok).collect()),
                })
                .collect(),
        }
    }
}

/// Each member a group's admitted elements spell, by its positions among the
/// group's elements.
type Occurrences<'a> = BTreeMap<&'a TargetId, Vec<usize>>;

/// The Target IDs a group's admitted members name, and whether every element
/// was read: what a later membership check reads, whatever each member's own
/// checks found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamedMembers {
    known: BTreeSet<TargetId>,
    lacking: Admitted<()>,
}

impl NamedMembers {
    /// Whether the group names `id`: an admitted member proves it, and naming
    /// none is proved only once every element was read (D15:2366, 2368).
    pub(crate) fn holds(&self, id: &TargetId) -> Admitted<bool> {
        if self.known.contains(id) {
            Ok(true)
        } else {
            self.lacking.clone().map(|()| false)
        }
    }
}

/// What Phase 12 established for later phases: each group's own bounds, the
/// Target IDs its members name, its members once the group's own checks held,
/// and whether the groups partition the targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedGroups {
    groups: BTreeMap<DeploymentGroupId, Admitted<BTreeSet<TargetId>>>,
    bounds: BTreeMap<DeploymentGroupId, Admitted<()>>,
    named: BTreeMap<DeploymentGroupId, Admitted<NamedMembers>>,
    partition: Admitted<()>,
}

impl ResolvedGroups {
    /// What every later check of one group waits on: the Group ID map's
    /// bound and the group's own ID bytes, or why either does not hold.
    pub(crate) fn bounds(&self, id: &DeploymentGroupId) -> Option<&Admitted<()>> {
        self.bounds.get(id)
    }

    /// The Target IDs one group's admitted members name, once its member
    /// count holds.
    pub(crate) fn named(&self, id: &DeploymentGroupId) -> Option<&Admitted<NamedMembers>> {
        self.named.get(id)
    }

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
/// first; each group's checks wait on the map's bound, its member count on its
/// ID's bytes, and its members on the group's own bounds and the total. A
/// member's other checks wait on its bytes, and whether it names a Target ID
/// also on the Target IDs Phase 11 admitted. A Target ID's place waits on its
/// own admission and on the total, and is found from the groups whose own
/// bounds hold; it waits on what could not be read only when none of them
/// names it. The checks observe through `observer`, the selected declaration's,
/// when there is one.
///
/// # Errors
///
/// Returns the resolver bug of a unit recorded against its phase's rules, or
/// of a declared value without Evidence.
pub(crate) fn resolve_groups(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    declared: &Admitted<GroupFields<'_>>,
    (targets, observer): (
        Result<&ResolvedTargets<'_>, &Causes>,
        Option<&DeclarationObserver<'_>>,
    ),
) -> Result<Admitted<ResolvedGroups>, GroupInvariant> {
    // The map is there to observe only once it can be read.
    let at_map = match declared {
        Ok(_) => beside(policy, observer, |observer| {
            observer.value(&["deploymentGroups"])
        })?,
        Err(_) => None,
    };
    let policy = policy.map_err(Causes::clone);
    let fields = declared.as_ref().map_err(Causes::clone);
    let bounded = evaluation.check_found(
        unit(GroupSubject::Groups, Check::WithinBound),
        both(
            policy.clone(),
            fields.clone().map(|fields| fields.submitted),
        ),
        |(policy, submitted)| {
            within(
                MAX_DEPLOYMENT_GROUPS,
                policy.limits().target_grouping.max_deployment_groups,
                submitted,
            )
            .map_err(|blocking| Found::through(blocking, at_map.clone()))
        },
    )?;
    let memberships = evaluation.check_found(
        unit(GroupSubject::Memberships, Check::WithinBound),
        both(
            policy.clone(),
            fields.clone().and_then(|fields| fields.memberships.clone()),
        ),
        |(policy, total)| {
            within(
                MAX_MEMBERSHIP_OCCURRENCES,
                policy.limits().target_grouping.max_membership_occurrences,
                total,
            )
            .map_err(|blocking| Found::through(blocking, at_map.clone()))
        },
    )?;
    let targets = targets.map_err(Causes::clone);
    // The declared Target IDs are there to observe once Phase 11 read them.
    let at_targets = match (observer, &targets) {
        (Some(observer), Ok(_)) => Some(observer.value(&["targetProfiles"])?),
        _ => None,
    };
    let mut groups = BTreeMap::new();
    let mut bounds = BTreeMap::new();
    let mut named = BTreeMap::new();
    let mut read = Vec::new();
    // Why a target that no group is seen to name may still be in one.
    let mut unseen = fields.clone().and_then(|fields| fields.keys.clone());
    let entries = declared
        .as_ref()
        .map_or(&[][..], |fields| fields.entries.as_slice());
    for field in entries {
        let id = field.id;
        let (identified, own) = own_bounds(evaluation, (&policy, observer), &bounded, field)?;
        bounds.insert(id.clone(), identified);
        let (occurrences, lacking) = occurrences_of(field);
        named.insert(
            id.clone(),
            own.clone().map(|()| NamedMembers {
                known: occurrences.keys().map(|&member| member.clone()).collect(),
                lacking: lacking.clone(),
            }),
        );
        unseen = and(unseen, and(own.clone(), lacking.clone()));
        let gate =
            both(policy.clone(), and(own.clone(), memberships.clone())).map(|(policy, ())| policy);
        groups.insert(
            id.clone(),
            members(
                evaluation,
                (&gate, observer),
                (&targets, at_targets.as_ref()),
                (id, &occurrences, lacking),
            )?,
        );
        if own.is_ok() {
            read.push((id, occurrences));
        }
    }
    let assignments = match &targets {
        Ok(targets) => and(
            assignments(
                evaluation,
                (&memberships, &unseen, observer),
                &read,
                targets,
            )?,
            targets.keys().clone(),
        ),
        Err(causes) => Err(causes.clone()),
    };
    let mut partition = and(assignments, unseen);
    for members in groups.values() {
        partition = and(
            partition,
            members.as_ref().map(|_| ()).map_err(Causes::clone),
        );
    }
    Ok(fields.map(|_| ResolvedGroups {
        groups,
        bounds,
        named,
        partition,
    }))
}

/// One group's two bounds, after the map's: its ID's bytes, which every later
/// check of the group waits on, and then its member occurrences, which wait
/// on them too and on an array to count.
fn own_bounds(
    evaluation: &mut Evaluation,
    (policy, observer): (
        &Admitted<&ConfirmedPolicy>,
        Option<&DeclarationObserver<'_>>,
    ),
    bounded: &Admitted<()>,
    field: &GroupField<'_>,
) -> Result<(Admitted<()>, Admitted<()>), GroupInvariant> {
    let id = field.id;
    let confirmed = policy.as_ref().map(|policy| *policy);
    let at_key = beside(confirmed, observer, |observer| {
        observer.member_key(&["deploymentGroups"], id.as_str())
    })?;
    // The array is there to observe only once it can be read.
    let at_members = match &field.members {
        Ok(_) => beside(confirmed, observer, |observer| {
            observer.value(&["deploymentGroups", id.as_str(), "members"])
        })?,
        Err(_) => None,
    };
    let prerequisite = both(policy.clone(), bounded.clone());
    let bytes = evaluation.check_found(
        unit(GroupSubject::Group(id.clone()), Check::WithinBound),
        prerequisite.clone(),
        |(policy, ())| {
            within(
                MAX_GROUP_ID_BYTES,
                policy.limits().target_grouping.max_group_id_bytes,
                id.as_str().len(),
            )
            .map_err(|blocking| Found::through(blocking, at_key))
        },
    )?;
    let count = field.members.as_ref().map(Vec::len).map_err(Causes::clone);
    let occurrences = evaluation.check_found(
        unit(GroupSubject::Members(id.clone()), Check::WithinBound),
        both(both(prerequisite, bytes.clone()), count),
        |(((policy, ()), ()), count)| {
            within(
                MAX_MEMBERS_PER_GROUP,
                policy.limits().target_grouping.max_members_per_group,
                count,
            )
            .map_err(|blocking| Found::through(blocking, at_members))
        },
    )?;
    Ok((bytes, occurrences))
}

/// Each member the group's admitted elements spell, by its positions among
/// them, and why the group lacks the others: an element that failed keeps its
/// position and has no subject (D15:1932, 2185).
fn occurrences_of<'a>(field: &GroupField<'a>) -> (Occurrences<'a>, Admitted<()>) {
    let mut occurrences: Occurrences<'a> = BTreeMap::new();
    let mut lacking = field.members.as_ref().map(|_| ()).map_err(Causes::clone);
    for (index, item) in field.members.iter().flatten().enumerate() {
        match item {
            Ok(member) => occurrences.entry(*member).or_default().push(index),
            Err(causes) => lacking = and(lacking, Err(causes.clone())),
        }
    }
    (occurrences, lacking)
}

/// The group's members by the Target ID each spells. Each must be within
/// `maxTargetIdBytes`, name a declared Target ID, and occur once; repeated
/// occurrences are one subject, all spelling the same bytes. The set exists
/// only when every member holds and none is `lacking`. A check that fails
/// observes each occurrence of the member (D15:2388): past the bound beside
/// the policy, naming no Target ID beside the declared Target IDs at
/// `at_targets`, and repeated beside the other occurrences.
fn members(
    evaluation: &mut Evaluation,
    (gate, observer): (
        &Admitted<&ConfirmedPolicy>,
        Option<&DeclarationObserver<'_>>,
    ),
    (known, at_targets): (&Admitted<&ResolvedTargets<'_>>, Option<&EvidenceProjection>),
    (id, occurrences, lacking): (&DeploymentGroupId, &Occurrences<'_>, Admitted<()>),
) -> Result<Admitted<BTreeSet<TargetId>>, GroupInvariant> {
    let mut holds = lacking;
    for (&member, positions) in occurrences {
        let subject = || GroupSubject::Member(id.clone(), member.clone());
        // Each occurrence at its token, when there is a declaration to
        // observe.
        let at = positions
            .iter()
            .map(|&index| {
                observer
                    .map(|observer| {
                        observer.element(&["deploymentGroups", id.as_str(), "members"], index)
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let bytes = evaluation.check_found(
            unit(subject(), Check::WithinBound),
            gate.clone(),
            |policy| {
                within(
                    MAX_TARGET_ID_BYTES,
                    policy.limits().target_grouping.max_target_id_bytes,
                    member.as_str().len(),
                )
                .map_err(|blocking| {
                    Found::each(blocking, each_beside(&at, Some(policy.evidence())))
                })
            },
        )?;
        let declared = evaluation.check_found(
            unit(subject(), Check::KnownMember),
            both(
                bytes.clone(),
                known.clone().and_then(|targets| targets.declares(member)),
            ),
            |((), declared)| {
                if declared {
                    Ok(())
                } else {
                    let unknown = issue(Code::GroupMemberUnknown);
                    Err(Found::each(unknown, each_beside(&at, at_targets)))
                }
            },
        )?;
        let once = evaluation.check_found(unit(subject(), Check::UniqueMember), bytes, |()| {
            if at.len() == 1 {
                Ok(())
            } else {
                Err(Found::repeated(issue(Code::GroupMemberDuplicate), &at))
            }
        })?;
        holds = and(holds, and(declared, once));
    }
    Ok(holds.map(|()| occurrences.keys().map(|&member| member.clone()).collect()))
}

/// Each declared Target ID's place: in exactly one group, counting a group
/// once however often it names the target, among the groups in `read`. Two of
/// them naming it prove the overlap, and one proves it placed, whatever else
/// failed; none proves it unassigned only when nothing is `unseen`, and it
/// waits on that otherwise (D15:2185, 2366). A target out of place is observed
/// through `observer`, when there is one.
fn assignments(
    evaluation: &mut Evaluation,
    (memberships, unseen, observer): (
        &Admitted<()>,
        &Admitted<()>,
        Option<&DeclarationObserver<'_>>,
    ),
    read: &[(&DeploymentGroupId, Occurrences<'_>)],
    targets: &ResolvedTargets<'_>,
) -> Result<Admitted<()>, GroupInvariant> {
    // Every occurrence of each Target ID among the groups, by its group and
    // position. A group's occurrences are adjacent, so each run is a group.
    let mut occurrences: BTreeMap<&TargetId, Vec<(&DeploymentGroupId, usize)>> = BTreeMap::new();
    for (id, members) in read {
        for (&member, positions) in members {
            let found = occurrences.entry(member).or_default();
            found.extend(positions.iter().map(|&index| (*id, index)));
        }
    }
    let mut holds = Ok(());
    for (target, facts) in targets.iter() {
        let found = occurrences.get(target).map_or(&[][..], Vec::as_slice);
        let groups = found.chunk_by(|(one, _), (other, _)| one == other).count();
        // Where a target out of place is observed: the declaration has its
        // key and every occurrence, whatever the check waits on. Relating each
        // occurrence to the other groups' grows with their square, so it
        // waits until the check runs.
        let (key, at) = match groups {
            0 if unseen.is_ok() => (unassigned(observer, target)?, Vec::new()),
            0 | 1 => (None, Vec::new()),
            _ => (None, located(observer, found)?),
        };
        let lacking = if groups == 0 { unseen.clone() } else { Ok(()) };
        let placed = evaluation.check_found(
            unit(GroupSubject::Assignment(target.clone()), Check::SingleGroup),
            both(both(facts.admitted.clone(), memberships.clone()), lacking),
            |_| match groups {
                1 => Ok(()),
                0 => Err(Found::each(issue(Code::TargetUnassigned), vec![key])),
                _ => Err(Found::each(
                    issue(Code::TargetAssignedToMultipleGroups),
                    beside_other_groups(&at),
                )),
            },
        )?;
        holds = and(holds, placed);
    }
    Ok(holds)
}

/// A declared Target ID that no group names, at its key beside the groups,
/// the set it must belong to (D15:2250).
fn unassigned(
    observer: Option<&DeclarationObserver<'_>>,
    target: &TargetId,
) -> Result<Option<Observation>, ObserverInvariant> {
    observer
        .map(|observer| {
            Ok(Observation {
                primary: observer.member_key(&["targetProfiles"], target.as_str())?,
                related: vec![observer.value(&["deploymentGroups"])?],
            })
        })
        .transpose()
}

/// Where each occurrence `found` of a Target ID is, by its group, when it
/// is observed.
fn located<'g>(
    observer: Option<&DeclarationObserver<'_>>,
    found: &[(&'g DeploymentGroupId, usize)],
) -> Result<Vec<(&'g DeploymentGroupId, Option<EvidenceProjection>)>, ObserverInvariant> {
    found
        .iter()
        .map(|&(id, index)| {
            observer
                .map(|observer| {
                    observer.element(&["deploymentGroups", id.as_str(), "members"], index)
                })
                .transpose()
                .map(|at| (id, at))
        })
        .collect()
}

/// Each occurrence at `at` of a Target ID that several groups name, beside
/// its occurrences in the other groups (D15:2388, 2439). An occurrence in the
/// same group is no other group the check counts.
fn beside_other_groups(
    at: &[(&DeploymentGroupId, Option<EvidenceProjection>)],
) -> Vec<Option<Observation>> {
    at.iter()
        .map(|(id, primary)| {
            Some(Observation {
                primary: primary.clone()?,
                related: at
                    .iter()
                    .filter(|(other, _)| other != id)
                    .filter_map(|(_, related)| related.clone())
                    .collect(),
            })
        })
        .collect()
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
    use crate::artifacts::collection::{read_collection, CollectionLimits, CollectionReport};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple, FormalMember};
    use crate::input_limits::Bound;
    use crate::model::ProfileDeclaration;
    use crate::profile_fixtures::minimal_config;
    use crate::references::{PolicyReference, TargetProfileReference};
    use crate::resolver::artifact_resolution::admit_collection;
    use crate::resolver::declaration_observer::fixtures::{view, Observed};
    use crate::resolver::evaluation::{Actual, LimitExceeded, Status};
    use crate::resolver::evidence::EvidenceProjection;
    use crate::resolver::exact_resolution::resolve_fields;
    use crate::resolver::limits::{fixture_limits, ConfirmedPolicy, ResolutionLimits};
    use crate::resolver::locales::fixtures::{canonicalizer, stand_in, target, waits_on};
    use crate::resolver::locales::{self, resolve_locales, LocaleFields, LocaleSubject};
    use crate::resolver::resource_policy::{
        DeclarationReferences, PolicyReferences, TargetReferences,
    };
    use crate::resolver::specification::Slot;
    use crate::resolver::targets::{self, resolve_targets, TargetSubject};

    type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

    /// Collection limits no test reaches.
    const ROOM: CollectionLimits = CollectionLimits {
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
            let unconfirmed = self.policy.clone().err();
            let policy = unconfirmed.as_ref().map_or(Ok(&confirmed), Err);
            let mut evaluation = Evaluation::default();
            let observed = Observed::declaration(&self.app);
            let locales = observed
                .app(|observer| {
                    resolve_locales(
                        &mut evaluation,
                        policy,
                        Ok(&canonicalizer(64)),
                        &LocaleFields::declared(&declaration),
                        Some(observer),
                    )
                })
                .unwrap();
            let members: Vec<String> = declaration
                .target_profiles
                .iter()
                .map(|(id, _)| profile_of(id.as_str()).to_string())
                .collect();
            let bytes: Vec<&[u8]> = members.iter().map(String::as_bytes).collect();
            let collection =
                read_collection(Some(CollectionReport::Formal(&bytes)), &ROOM).unwrap();
            let set = admit_collection(&mut evaluation, Ok(&confirmed), &collection).unwrap();
            let references = observed
                .app(|observer| {
                    resolve_fields(
                        &mut evaluation,
                        set.as_ref(),
                        &DeclarationReferences {
                            policies: PolicyReferences {
                                slots: Vec::new(),
                                object: Ok(()),
                            },
                            target_profiles: Ok(TargetReferences::declared(
                                &declaration.target_profiles,
                            )),
                        },
                        (locales.targets.as_deref(), Some(observer)),
                    )
                })
                .unwrap();
            let targets = observed
                .app(|observer| {
                    resolve_targets(&mut evaluation, &locales, (&references, Some(observer)))
                })
                .unwrap();
            let declared = self
                .declared
                .clone()
                .map(|()| GroupFields::declared(&declaration.deployment_groups));
            let groups = observed
                .app(|observer| {
                    resolve_groups(
                        &mut evaluation,
                        policy,
                        &declared,
                        (targets.as_ref(), Some(observer)),
                    )
                })
                .unwrap();
            inspect(evaluation, groups)
        }
    }

    impl Inputs {
        /// The view of the Evidence `at` makes in the declaration.
        fn at(
            &self,
            at: impl FnOnce(&DeclarationObserver<'_>) -> Result<EvidenceProjection, ObserverInvariant>,
        ) -> Value {
            Observed::declaration(&self.app).app(|observer| view(&at(observer).unwrap()))
        }
    }

    /// The view of the confirmed policy, which supplies every bound.
    fn policy() -> Value {
        view(ConfirmedPolicy::fixture_owned(fixture_limits(1)).evidence())
    }

    /// Each Phase 12 Finding candidate: its unit, and the views of its
    /// primary and related Evidence.
    fn observed(evaluation: &Evaluation) -> Vec<(Unit, Value, Vec<Value>)> {
        evaluation
            .occurrence_candidates()
            .iter()
            .filter(|candidate| candidate.finding.unit().phase == Phase::GroupResolution)
            .map(|candidate| {
                let related = candidate.related.iter().map(view).collect();
                (
                    candidate.finding.unit().clone(),
                    view(&candidate.primary),
                    related,
                )
            })
            .collect()
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
            units.push(member(group_id, id, Check::WithinBound));
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
            // Observed at its occurrence, beside the declared Target IDs.
            assert_eq!(
                observed(&evaluation),
                [(
                    unknown.clone(),
                    inputs.at(|observer| {
                        observer.element(&["deploymentGroups", "web", "members"], 2)
                    }),
                    vec![inputs.at(|observer| observer.value(&["targetProfiles"]))]
                )]
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
    fn a_member_naming_no_target_twice_is_observed_at_each_occurrence() {
        let mut inputs = Inputs::new();
        *inputs.members("web") = json!(["phone", "browser", "server", "phone"]);
        inputs.resolve(|evaluation, _| {
            let unknown = member("web", "phone", Check::KnownMember);
            let duplicate = member("web", "phone", Check::UniqueMember);
            assert_eq!(
                issues(&evaluation),
                [
                    (unknown.clone(), Code::GroupMemberUnknown),
                    (duplicate.clone(), Code::GroupMemberDuplicate),
                ]
            );
            let at = |index| {
                inputs
                    .at(|observer| observer.element(&["deploymentGroups", "web", "members"], index))
            };
            let targets = inputs.at(|observer| observer.value(&["targetProfiles"]));
            // One candidate for each occurrence of either result (D15:2388).
            assert_eq!(
                observed(&evaluation),
                [
                    (unknown.clone(), at(0), vec![targets.clone()]),
                    (unknown, at(3), vec![targets]),
                    (duplicate.clone(), at(0), vec![at(3)]),
                    (duplicate, at(3), vec![at(0)]),
                ]
            );
        });
    }

    #[test]
    fn a_member_named_twice_in_its_group_is_one_duplicate() {
        let mut inputs = Inputs::new();
        *inputs.members("web") = json!(["browser", "server", "browser"]);
        inputs.resolve(|evaluation, groups| {
            let duplicate = member("web", "browser", Check::UniqueMember);
            assert_eq!(
                issues(&evaluation),
                [(duplicate.clone(), Code::GroupMemberDuplicate)]
            );
            // Each occurrence, beside the other.
            let at = |index| {
                inputs
                    .at(|observer| observer.element(&["deploymentGroups", "web", "members"], index))
            };
            assert_eq!(
                observed(&evaluation),
                [
                    (duplicate.clone(), at(0), vec![at(2)]),
                    (duplicate, at(2), vec![at(0)]),
                ]
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
            let at = |id: &str, index| {
                inputs.at(|observer| observer.element(&["deploymentGroups", id, "members"], index))
            };
            assert_eq!(
                observed(&evaluation),
                [
                    // Each occurrence, beside the other group's.
                    (assignment("server"), at("jobs", 0), vec![at("web", 1)]),
                    (assignment("server"), at("web", 1), vec![at("jobs", 0)]),
                    // The target's key, beside the groups it must be in.
                    (
                        assignment("worker"),
                        inputs.at(|observer| observer.member_key(&["targetProfiles"], "worker")),
                        vec![inputs.at(|observer| observer.value(&["deploymentGroups"]))]
                    ),
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
    fn a_target_two_groups_name_is_observed_at_each_occurrence_beside_the_other_group() {
        // `server` twice in `web` and once in `jobs`: the groups conflict, and
        // `web` repeats it as well.
        let mut inputs = Inputs::new();
        *inputs.members("web") = json!(["browser", "server", "server"]);
        *inputs.members("jobs") = json!(["worker", "server"]);
        inputs.resolve(|evaluation, _| {
            let duplicate = member("web", "server", Check::UniqueMember);
            let server = assignment("server");
            assert_eq!(
                issues(&evaluation),
                [
                    (duplicate.clone(), Code::GroupMemberDuplicate),
                    (server.clone(), Code::TargetAssignedToMultipleGroups),
                ]
            );
            let at = |id: &str, index| {
                inputs.at(|observer| observer.element(&["deploymentGroups", id, "members"], index))
            };
            assert_eq!(
                observed(&evaluation),
                [
                    (duplicate.clone(), at("web", 1), vec![at("web", 2)]),
                    (duplicate, at("web", 2), vec![at("web", 1)]),
                    // The group counts once: an occurrence in the same group
                    // is no conflicting one.
                    (
                        server.clone(),
                        at("jobs", 1),
                        vec![at("web", 1), at("web", 2)]
                    ),
                    (server.clone(), at("web", 1), vec![at("jobs", 1)]),
                    (server, at("web", 2), vec![at("jobs", 1)]),
                ]
            );
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
                // Observed at the Group ID map, beside the policy.
                assert_eq!(
                    observed(&evaluation),
                    [(
                        map.clone(),
                        inputs.at(|observer| observer.value(&["deploymentGroups"])),
                        vec![policy()]
                    )]
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
                // Observed at the Group ID map, which holds every
                // membership, beside the policy.
                assert_eq!(
                    observed(&evaluation),
                    [(
                        total.clone(),
                        inputs.at(|observer| observer.value(&["deploymentGroups"])),
                        vec![policy()]
                    )]
                );
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
                // Observed at the Group ID's key or at the group's members,
                // beside the policy.
                let at = match &subject {
                    GroupSubject::Group(_) => {
                        inputs.at(|observer| observer.member_key(&["deploymentGroups"], "jobs"))
                    }
                    _ => inputs
                        .at(|observer| observer.value(&["deploymentGroups", "web", "members"])),
                };
                assert_eq!(observed(&evaluation), [(own.clone(), at, vec![policy()])]);
                let (blocked, other) = match subject {
                    GroupSubject::Group(_) => ("jobs", "web"),
                    _ => ("web", "jobs"),
                };
                if blocked == "jobs" {
                    // The group's member count waits on its ID's bytes.
                    assert_eq!(
                        waits_on(
                            &evaluation,
                            &group_unit(GroupSubject::Members(group("jobs")), Check::WithinBound)
                        ),
                        Some(vec![own.clone()])
                    );
                }
                let groups = groups.unwrap();
                assert!(groups.get(&group(blocked)).unwrap().is_err());
                assert!(groups.get(&group(other)).unwrap().is_ok());
                // Only the places the blocked group alone could give wait on
                // its bounds (D15:2185, 2366).
                let alone: &[&str] = match blocked {
                    "jobs" => &["worker"],
                    _ => &["browser", "server"],
                };
                for id in ["browser", "server", "worker"] {
                    let waits = alone.contains(&id).then(|| vec![own.clone()]);
                    assert_eq!(waits_on(&evaluation, &assignment(id)), waits, "{id}");
                }
            });
        }
    }

    #[test]
    fn a_member_waits_on_the_target_map_phase_7_bounds() {
        let mut inputs = Inputs::new();
        inputs.limits.target_grouping.max_target_profiles = bound(2);
        inputs.resolve(|evaluation, _| {
            let map = locales::unit(LocaleSubject::TargetProfiles, Check::WithinBound);
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
            // A member's bytes need no Target ID either.
            assert_eq!(
                waits_on(&evaluation, &member("web", "browser", Check::WithinBound)),
                None
            );
        });
    }

    #[test]
    fn a_member_holds_at_the_target_id_bound_and_one_byte_more_leaves_its_other_checks() {
        // `browser` is seven bytes and named twice: one subject, so one
        // bound result, and its repeat waits on it.
        for (limit, over) in [(7, false), (6, true)] {
            let mut inputs = Inputs::new();
            *inputs.members("web") = json!(["browser", "server", "browser"]);
            inputs.limits.target_grouping.max_target_id_bytes = bound(limit);
            inputs.resolve(|evaluation, groups| {
                let bytes = member("web", "browser", Check::WithinBound);
                // Phase 7 checks the Target ID key's own bytes as well.
                let found: Vec<(Unit, Code)> = issues(&evaluation)
                    .into_iter()
                    .filter(|(unit, _)| unit.phase == Phase::GroupResolution)
                    .collect();
                if !over {
                    assert_eq!(
                        found,
                        [(
                            member("web", "browser", Check::UniqueMember),
                            Code::GroupMemberDuplicate
                        )]
                    );
                    return;
                }
                assert_eq!(found, [(bytes.clone(), Code::ResourceLimitExceeded)]);
                // Each of the two occurrences, at its token, beside the
                // policy (D15:2388).
                let each = |index| {
                    (
                        bytes.clone(),
                        inputs.at(|observer| {
                            observer.element(&["deploymentGroups", "web", "members"], index)
                        }),
                        vec![policy()],
                    )
                };
                assert_eq!(observed(&evaluation), [each(0), each(2)]);
                let key = evaluation
                    .issues()
                    .find(|key| key.unit() == &bytes)
                    .unwrap()
                    .clone();
                assert_eq!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        bound: MAX_TARGET_ID_BYTES,
                        limit: bound(6),
                        actual: Actual::Exact(7),
                    })
                );
                for check in [Check::KnownMember, Check::UniqueMember] {
                    assert_eq!(
                        waits_on(&evaluation, &member("web", "browser", check)),
                        Some(vec![bytes.clone()]),
                        "{check:?}"
                    );
                }
                // The group's other member and the other group are checked.
                assert_eq!(
                    waits_on(&evaluation, &member("web", "server", Check::KnownMember)),
                    None
                );
                let groups = groups.unwrap();
                assert!(groups.get(&group("web")).unwrap().is_err());
                assert!(groups.get(&group("jobs")).unwrap().is_ok());
            });
        }
    }

    #[test]
    fn a_place_waits_on_its_own_target_id_alone() {
        let mut inputs = Inputs::new();
        // `browser` is seven bytes, the others six.
        inputs.limits.target_grouping.max_target_id_bytes = bound(6);
        inputs.resolve(|evaluation, groups| {
            let id = locales::unit(
                LocaleSubject::TargetId(target("browser")),
                Check::WithinBound,
            );
            assert_eq!(
                waits_on(&evaluation, &assignment("browser")),
                Some(vec![id.clone()])
            );
            for other in ["server", "worker"] {
                assert_eq!(waits_on(&evaluation, &assignment(other)), None);
            }
            // The member is checked on its own rather than waiting on the
            // key, and its group waits on it.
            let bytes = member("web", "browser", Check::WithinBound);
            assert_eq!(waits_on(&evaluation, &bytes), None);
            let groups = groups.unwrap();
            assert_eq!(
                groups
                    .partition()
                    .clone()
                    .unwrap_err()
                    .keys()
                    .map(|key| key.unit().clone())
                    .collect::<Vec<_>>(),
                [id, bytes]
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
    fn a_group_map_structural_admission_did_not_admit_records_its_bounds_and_each_place() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::DeliveryGraph);
        inputs.declared = Err(causes.clone());
        inputs.resolve(|evaluation, groups| {
            let waiting = |unit| (unit, Status::NotEvaluated(causes.clone()));
            assert_eq!(
                statuses(&evaluation),
                [
                    waiting(group_unit(GroupSubject::Groups, Check::WithinBound)),
                    waiting(group_unit(GroupSubject::Memberships, Check::WithinBound)),
                    waiting(assignment("browser")),
                    waiting(assignment("server")),
                    waiting(assignment("worker")),
                ]
            );
            assert_eq!(groups, Err(causes.clone()));
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
