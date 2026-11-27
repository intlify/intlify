// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 13 (`hydration-validation`) over each group's explicit hydration
//! relations, `SSR Target ID -> Browser hydration-client Target ID`: every
//! relation occurrence under `maxHydrationRelations`; each endpoint on its own
//! under `maxTargetIdBytes`, declared, a member of its relation's group, and
//! taking its field's role, the server's Target Profile rendering server-side
//! and the client's hydrating in the browser; each relation's endpoints
//! distinct, the relation once in its group, both endpoints requesting the
//! same locales with the same effective default, and nothing statically known
//! making their Target Profiles incompatible. Within a group, no Target ID
//! takes both roles. Every check of a group waits on the group's own bounds,
//! which Phase 12 checked, and otherwise only on the facts it needs, so one
//! failure never hides another that does not depend on it. A relation is
//! never inferred, and a mismatch is never narrowed to the locales both
//! endpoints share.
//!
//! The roles and the static compatibility are 024's: a Target Profile declares
//! `ssr-renderer` and `browser-hydration-client` in its closed body, and two
//! profiles are statically compatible when their `mf2Specification`,
//! `execution`, and `locale.services` are equal, whatever their physical
//! dimensions. A role is never inferred from a name, a mode, or a group.
//! The project's Locale Negotiation Profile is a dependency
//! every relation shares, never compared between endpoints. Not the render
//! equivalence of the two outputs (026, 030), the selection of a group (020),
//! or the Release both outputs join (025).
//!
//! A count past its bound is observed where the count is, beside the
//! confirmed policy, which supplies the bound (D15:2250): the Group ID map
//! for every group's relations, and each occurrence of an endpoint for its
//! bytes (D15:2388). An endpoint is observed at each of its occurrences: one
//! that names no declared Target ID beside the declared Target IDs, one that
//! names no member of its group beside the group's members, each the set it
//! must belong to, and one whose Target Profile lacks its field's role beside
//! that Target Profile. A relation is observed at each of its occurrences:
//! between equal endpoints alone, repeated beside its other occurrences, and
//! with endpoints of other locales, effective defaults, or statically
//! incompatible Target Profiles beside where it found both endpoints'. A
//! Target ID that takes both roles is observed at each of its occurrences in
//! one role beside those in the other.
//!
//! Each relation and each endpoint is read on its own (D15:2150, 2368, 3581).
//! The bound counts what was submitted, a relation that failed and one under
//! an entry whose key failed included. A relation with an endpoint that failed
//! has no identity and is keyed by its Subject content projection, so such
//! relations of one group share a subject, each checked as its own raw parent
//! (D15:2182, 2191, 2204): the endpoint it still holds is checked, and what
//! compares the two waits on the one that failed. An element that is no object
//! has no subject. What a later phase reads waits on all of it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use crate::artifacts::content::{Content, HydrationRoles, TargetProfile};
use crate::materialize::{MaterializedDocument, ObjectMember};
use crate::model::{DeploymentGroup, DeploymentGroupId, NonEmptyMap, TargetId};
use crate::structural::Edge;

use super::artifact_resolution::{self, AdmittedArtifact};
use super::coverage::object_identity;
use super::declaration_observer::{
    beside, each_beside, each_beside_all, DeclarationObserver, ObserverInvariant,
};
use super::evaluation::{
    and, both, within, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant,
    Found, Phase, Reason, Subject, Unit,
};
use super::evidence::EvidenceProjection;
use super::evidence_component::ComponentInvariant;
use super::groups::{NamedMembers, ResolvedGroups};
use super::limits::{BoundId, ConfirmedPolicy, Group};
use super::targets::{ResolvedTargets, TargetFacts};

/// A resolver bug Phase 13 refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HydrationInvariant {
    Evaluation(EvaluationInvariant),
    /// The Target Profile a check compared has no component path.
    Component(ComponentInvariant),
    /// What a check looked at in the declaration has no Evidence.
    Observer(ObserverInvariant),
}

impl From<ComponentInvariant> for HydrationInvariant {
    fn from(invariant: ComponentInvariant) -> Self {
        Self::Component(invariant)
    }
}

impl From<EvaluationInvariant> for HydrationInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

impl From<ObserverInvariant> for HydrationInvariant {
    fn from(invariant: ObserverInvariant) -> Self {
        Self::Observer(invariant)
    }
}

const MAX_HYDRATION_RELATIONS: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxHydrationRelations",
};

const MAX_TARGET_ID_BYTES: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxTargetIdBytes",
};

/// The 024 body of the Target Profile an endpoint's reference names. The Set
/// admits a Target Profile only with that body, and Phase 9 resolves a Target
/// Profile reference only to a Target Profile.
fn target_profile<'p>(profile: &'p AdmittedArtifact<'_>) -> Option<&'p TargetProfile> {
    match &profile.content {
        Content::TargetProfile(profile) => Some(profile),
        _ => None,
    }
}

/// The roles the Target Profile declares; none when it is no Target Profile.
fn roles(profile: &AdmittedArtifact<'_>) -> HydrationRoles {
    target_profile(profile).map_or_else(HydrationRoles::default, |profile| profile.hydration)
}

/// 024's closed static compatibility: equal `mf2Specification`, `execution`,
/// and `locale.services`. The reader admits only the one `execution` and the
/// one `locale.services` 017 fixes in revision `"0"`, so two admitted profiles
/// can differ only in their `mf2Specification`. Physical dimensions are never
/// compared.
fn statically_compatible(server: &AdmittedArtifact<'_>, client: &AdmittedArtifact<'_>) -> bool {
    match (target_profile(server), target_profile(client)) {
        (Some(server), Some(client)) => server.mf2_specification == client.mf2_specification,
        _ => false,
    }
}

/// What one Phase 13 unit checks. The bound, which every relation waits on,
/// comes first, then each relation's endpoints, which its checks wait on, then
/// each relation, then each Target ID's roles.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HydrationSubject {
    /// Every relation occurrence of every group, under
    /// `maxHydrationRelations`.
    Relations,
    /// One endpoint of one relation, by the relation's key and the field
    /// that names it, under `maxTargetIdBytes`.
    Endpoint(DeploymentGroupId, RelationKey, EndpointField),
    /// One relation of one group, by its key.
    Relation(DeploymentGroupId, RelationKey),
    /// A Target ID's roles among one group's relations.
    Roles(DeploymentGroupId, TargetId),
}

/// What names a relation: its identity once both endpoints were admitted,
/// which orders before every content key, and otherwise its content (D15:2182).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RelationKey {
    /// The relation's server and client Target IDs.
    Pair(TargetId, TargetId),
    /// The relation object as authored, with an endpoint that failed.
    Content(RelationContent),
}

impl RelationKey {
    /// The key with the content its projection drops replaced.
    pub(crate) fn logical(&self) -> Self {
        match self {
            Self::Pair(..) => self.clone(),
            Self::Content(_) => Self::Content(RelationContent::projected()),
        }
    }
}

/// Canonical content identity of a relation object with an endpoint that
/// failed: transient, it orders the raw parents inside their one subject and
/// is then discarded (D15:2187, 2191, 2204). Its Debug shows only its length:
/// rejected authoring text never reaches a log.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RelationContent(Arc<[u8]>);

impl fmt::Debug for RelationContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "RelationContent({} bytes)", self.0.len())
    }
}

impl RelationContent {
    /// The content that stands for every such relation in a logical subject:
    /// a relation is an object, and `redacted-structure` keeps only that type
    /// (D15:2189).
    pub(crate) fn projected() -> Self {
        Self(Arc::from([]))
    }

    /// The content identity of a relation object as authored, of `members`.
    pub(crate) fn read(
        document: &MaterializedDocument,
        members: &BTreeMap<String, ObjectMember>,
    ) -> Self {
        Self(object_identity(document, members))
    }
}

/// One object of a group's `hydrationRelations`: its key and each endpoint,
/// or why it has none (D15:2368).
#[derive(Debug, Clone)]
pub(crate) struct RelationField<'a> {
    pub(crate) key: RelationKey,
    pub(crate) server: Admitted<&'a TargetId>,
    pub(crate) client: Admitted<&'a TargetId>,
}

/// One entry of the Group ID map with an admitted key: its relations, each
/// one's element or why it is none, or why it has none to read.
#[derive(Debug, Clone)]
pub(crate) struct RelationGroup<'a> {
    pub(crate) id: &'a DeploymentGroupId,
    pub(crate) relations: Admitted<Vec<Admitted<RelationField<'a>>>>,
}

/// The Group ID map as Phase 13 reads it: how many relation occurrences its
/// arrays hold, whatever their entries' keys and values (D15:1937), each entry
/// with an admitted key, and whether everything at and below the map passed,
/// which what needs every relation waits on.
#[derive(Debug, Clone)]
pub(crate) struct HydrationFields<'a> {
    pub(crate) occurrences: Admitted<usize>,
    pub(crate) groups: Vec<RelationGroup<'a>>,
    pub(crate) whole: Admitted<()>,
}

impl<'a> HydrationFields<'a> {
    /// The fields of a map structural admission admitted whole.
    pub(crate) fn declared(groups: &'a NonEmptyMap<DeploymentGroupId, DeploymentGroup>) -> Self {
        let relations = |group: &'a DeploymentGroup| {
            group
                .hydration_relations
                .as_option()
                .map_or(&[][..], Vec::as_slice)
        };
        Self {
            occurrences: Ok(groups.values().map(|group| relations(group).len()).sum()),
            groups: groups
                .iter()
                .map(|(id, group)| RelationGroup {
                    id,
                    relations: Ok(relations(group)
                        .iter()
                        .map(|relation| {
                            Ok(RelationField {
                                key: RelationKey::Pair(
                                    relation.server.clone(),
                                    relation.client.clone(),
                                ),
                                server: Ok(&relation.server),
                                client: Ok(&relation.client),
                            })
                        })
                        .collect()),
                })
                .collect(),
            whole: Ok(()),
        }
    }
}

/// The field of a relation that names an endpoint, in the byte order of the
/// field names: `client` before `server`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EndpointField {
    Client,
    Server,
}

impl EndpointField {
    /// The relation's member that names the endpoint.
    const fn name(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Server => "server",
        }
    }
}

/// The unit Phase 13 records for `check` over `subject`.
pub(crate) fn unit(subject: HydrationSubject, check: Check) -> Unit {
    Unit {
        phase: Phase::HydrationValidation,
        subject: Subject::Hydration(subject),
        check,
    }
}

/// One group's relations once every check over them held, as `(server,
/// client)` pairs in that order.
pub(crate) type Relations = BTreeSet<(TargetId, TargetId)>;

/// What Phase 13 established: each group's checked relations, whether
/// everything at and below the Group ID map passed, and the Locale Negotiation
/// Profile every relation shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedHydration {
    relations: BTreeMap<DeploymentGroupId, Admitted<Relations>>,
    whole: Admitted<()>,
    negotiation: Admitted<()>,
}

impl ResolvedHydration {
    /// One group's relations, or why they are unavailable.
    pub(crate) fn get(&self, id: &DeploymentGroupId) -> Option<&Admitted<Relations>> {
        self.relations.get(id)
    }

    /// The groups in Group ID order, each with its relations.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&DeploymentGroupId, &Admitted<Relations>)> {
        self.relations.iter()
    }

    /// The project's Locale Negotiation Profile as Phase 8 left it: a
    /// dependency of every relation, never compared between its endpoints.
    pub(crate) const fn negotiation(&self) -> &Admitted<()> {
        &self.negotiation
    }

    /// Whether the whole map was read and every group's relations were
    /// checked and held, or the causes of what was not and of each that did
    /// not.
    pub(crate) fn complete(&self) -> Admitted<()> {
        let mut holds = self.whole.clone();
        for relations in self.relations.values() {
            holds = and(holds, relations.as_ref().map(|_| ()).map_err(Causes::clone));
        }
        holds
    }
}

/// Phase 13 under the confirmed policy, over the declared groups' relations,
/// the targets Phase 11 resolved, and the groups Phase 12 resolved. Every unit
/// waits on the relations' bound, and every unit of a group on the group's
/// bounds. Each endpoint's checks wait on that endpoint alone, and each check
/// of a relation on just the facts it compares (D15:2366): a relation between
/// equal endpoints, or with an endpoint outside its group, still compares
/// their locales and Target Profiles, and one whose endpoint failed checks
/// the other. The checks observe through `observer`, the selected
/// declaration's, when there is one.
///
/// # Errors
///
/// Returns the resolver bug of a unit recorded against its phase's rules, of
/// a declared value without Evidence, or of a Target Profile without a
/// component path.
pub(crate) fn validate_hydration(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    declared: &Admitted<HydrationFields<'_>>,
    targets: Result<&ResolvedTargets<'_>, &Causes>,
    (groups, observer): (
        Result<&ResolvedGroups, &Causes>,
        Option<&DeclarationObserver<'_>>,
    ),
    negotiation: Admitted<()>,
) -> Result<Admitted<ResolvedHydration>, HydrationInvariant> {
    // The map is there to observe only once it can be read.
    let at_map = match declared {
        Ok(_) => beside(policy, observer, |observer| {
            observer.value(&["deploymentGroups"])
        })?,
        Err(_) => None,
    };
    let policy = policy.map_err(Causes::clone);
    let bounded = evaluation.check_found(
        unit(HydrationSubject::Relations, Check::WithinBound),
        both(
            policy.clone(),
            declared
                .as_ref()
                .map_err(Causes::clone)
                .and_then(|fields| fields.occurrences.clone()),
        ),
        |(policy, total)| {
            within(
                MAX_HYDRATION_RELATIONS,
                policy.limits().target_grouping.max_hydration_relations,
                total,
            )
            .map_err(|blocking| Found::through(blocking, at_map))
        },
    )?;
    let fields = match declared {
        Ok(fields) => fields,
        Err(causes) => return Ok(Err(causes.clone())),
    };
    // The declared Target IDs are there to observe only once Phase 11
    // resolved them.
    let at_targets = match (observer, &targets) {
        (Some(observer), Ok(_)) => Some(observer.value(&["targetProfiles"])?),
        _ => None,
    };
    let shared = Shared {
        policy: &policy,
        bounded: &bounded,
        targets: targets.map_err(Causes::clone),
        groups: groups.map_err(Causes::clone),
        observer,
        at_targets: at_targets.as_ref(),
    };
    let mut relations = BTreeMap::new();
    for group in &fields.groups {
        relations.insert(group.id.clone(), shared.group(evaluation, group)?);
    }
    Ok(Ok(ResolvedHydration {
        relations,
        whole: fields.whole.clone(),
        negotiation,
    }))
}

/// What every relation's checks share: what they wait on, the declaration's
/// observer, and the declared Target IDs once there to observe.
struct Shared<'a, 'p, 't, 's> {
    policy: &'a Admitted<&'p ConfirmedPolicy>,
    bounded: &'a Admitted<()>,
    targets: Admitted<&'t ResolvedTargets<'s>>,
    groups: Admitted<&'t ResolvedGroups>,
    observer: Option<&'a DeclarationObserver<'a>>,
    at_targets: Option<&'a EvidenceProjection>,
}

impl<'p, 't, 's> Shared<'_, 'p, 't, 's> {
    /// One group's relations, each key once however often submitted, and the
    /// roles of each Target ID the endpoints admitted name. An element that
    /// is no relation keeps its position and has no subject. The set exists
    /// only when every element holds and none failed.
    fn group(
        &self,
        evaluation: &mut Evaluation,
        group: &RelationGroup<'_>,
    ) -> Result<Admitted<Relations>, HydrationInvariant> {
        let id = group.id;
        // Each key's relation and occurrences, by their positions in the
        // group's relations.
        let mut occurrences: BTreeMap<&RelationKey, (&RelationField<'_>, Vec<usize>)> =
            BTreeMap::new();
        let mut holds = group.relations.as_ref().map(|_| ()).map_err(Causes::clone);
        for (index, relation) in group.relations.iter().flatten().enumerate() {
            match relation {
                Ok(relation) => {
                    occurrences
                        .entry(&relation.key)
                        .or_insert_with(|| (relation, Vec::new()))
                        .1
                        .push(index);
                }
                Err(causes) => holds = and(holds, Err(causes.clone())),
            }
        }
        let gate = self.gate(id)?;
        let mut taken: BTreeMap<&TargetId, Taken> = BTreeMap::new();
        for (&key, (field, positions)) in &occurrences {
            let relation = Relation {
                id,
                key,
                server: field.server.clone(),
                client: field.client.clone(),
                positions,
            };
            let checked = relation.check(evaluation, self, &gate)?;
            // An equal-endpoint relation is its own failure and takes neither
            // role; an endpoint that failed names no Target ID.
            if !matches!(key, RelationKey::Pair(server, client) if server == client) {
                if let Ok(server) = field.server {
                    let serving = taken.entry(server).or_default();
                    serving.serving.extend(positions);
                    serving.bytes = and(serving.bytes.clone(), checked.server);
                }
                if let Ok(client) = field.client {
                    let hydrating = taken.entry(client).or_default();
                    hydrating.hydrating.extend(positions);
                    hydrating.bytes = and(hydrating.bytes.clone(), checked.client);
                }
            }
            holds = and(holds, checked.holds);
        }
        holds = and(
            holds,
            single_roles(evaluation, (id, self.observer), &taken)?,
        );
        Ok(holds.map(|()| {
            occurrences
                .into_keys()
                .filter_map(|key| match key {
                    RelationKey::Pair(server, client) => Some((server.clone(), client.clone())),
                    RelationKey::Content(_) => None,
                })
                .collect()
        }))
    }

    /// What every endpoint of one group reads: the policy once the relations'
    /// bound and the group's own bounds hold, and the Target IDs the group's
    /// admitted members name; and where the members are, once there is an
    /// array to observe.
    fn gate(&self, id: &DeploymentGroupId) -> Result<Gate<'p, 't>, ObserverInvariant> {
        let bounds = self.groups.clone().and_then(|groups| {
            groups
                .bounds(id)
                .expect("Phase 12 resolves every declared group")
                .clone()
        });
        let named = self.groups.clone().and_then(|groups| {
            groups
                .named(id)
                .expect("Phase 12 resolves every declared group")
                .as_ref()
                .map_err(Causes::clone)
        });
        // The members are there to observe once Phase 12 read the array.
        let at_members = match (self.observer, &named) {
            (Some(observer), Ok(_)) => {
                Some(observer.value(&["deploymentGroups", id.as_str(), "members"])?)
            }
            _ => None,
        };
        Ok(Gate {
            policy: both(both(self.policy.clone(), self.bounded.clone()), bounds)
                .map(|((policy, ()), ())| policy),
            named,
            at_members,
        })
    }

    /// What Phase 11 established of a declared endpoint.
    fn facts(&self, id: &TargetId) -> Admitted<&'t TargetFacts<'s>> {
        let targets: &'t ResolvedTargets<'s> = self.targets.clone()?;
        Ok(targets
            .get(id)
            .expect("Phase 13 reads only the endpoints it found declared"))
    }
}

/// What every endpoint of one group reads.
struct Gate<'p, 't> {
    policy: Admitted<&'p ConfirmedPolicy>,
    named: Admitted<&'t NamedMembers>,
    /// The group's members, the set an endpoint must belong to, when there
    /// is a declaration to observe.
    at_members: Option<EvidenceProjection>,
}

/// The roles one Target ID takes among its group's relations between
/// distinct endpoints, by the positions of the relations that give it each,
/// and the bytes of each endpoint that names it there.
struct Taken {
    serving: Vec<usize>,
    hydrating: Vec<usize>,
    bytes: Admitted<()>,
}

impl Default for Taken {
    fn default() -> Self {
        Self {
            serving: Vec::new(),
            hydrating: Vec::new(),
            bytes: Ok(()),
        }
    }
}

/// Each Target ID that serves or hydrates between two distinct endpoints
/// takes one role in its group, waiting only on the bytes of the endpoints
/// that name it there. One that takes both is observed at each of its
/// occurrences in one role, beside its occurrences in the other (D15:2250,
/// 2388), through `observer` when there is one.
fn single_roles(
    evaluation: &mut Evaluation,
    (id, observer): (&DeploymentGroupId, Option<&DeclarationObserver<'_>>),
    taken: &BTreeMap<&TargetId, Taken>,
) -> Result<Admitted<()>, HydrationInvariant> {
    let mut holds = Ok(());
    for (&target, roles) in taken {
        let dual = !roles.serving.is_empty() && !roles.hydrating.is_empty();
        let observed = if dual {
            let serving = relations_at(observer, id, &roles.serving, Some(EndpointField::Server))?;
            let hydrating =
                relations_at(observer, id, &roles.hydrating, Some(EndpointField::Client))?;
            let mut observed = each_beside_all(&serving, all_of(&hydrating).as_deref());
            observed.extend(each_beside_all(&hydrating, all_of(&serving).as_deref()));
            observed
        } else {
            Vec::new()
        };
        let single = evaluation.check_found(
            unit(
                HydrationSubject::Roles(id.clone(), target.clone()),
                Check::SingleRole,
            ),
            roles.bytes.clone(),
            |()| {
                if dual {
                    Err(Found::each(invalid(Reason::DualRole), observed))
                } else {
                    Ok(())
                }
            },
        )?;
        holds = and(holds, single);
    }
    Ok(holds)
}

/// Each location at `at`, once every one was observed.
fn all_of(at: &[Option<EvidenceProjection>]) -> Option<Vec<EvidenceProjection>> {
    at.iter().cloned().collect()
}

/// Where the target `id` declares its effective default, as Phase 11 found
/// it: its own override, or else the project's default it inherits.
fn default_at(
    observer: &DeclarationObserver<'_>,
    id: &TargetId,
    facts: &TargetFacts<'_>,
) -> Result<EvidenceProjection, ObserverInvariant> {
    if facts.default_origin == Some(Reason::ExplicitOverride) {
        observer.value(&["targetProfiles", id.as_str(), "defaultRequestedLocale"])
    } else {
        observer.value(&["defaultRequestedLocale"])
    }
}

/// The relation at each of `positions` in the group `id`, or the endpoint
/// `field` names in it, at its token (D15:791), when there is a declaration to
/// observe.
fn relations_at(
    observer: Option<&DeclarationObserver<'_>>,
    id: &DeploymentGroupId,
    positions: &[usize],
    field: Option<EndpointField>,
) -> Result<Vec<Option<EvidenceProjection>>, ObserverInvariant> {
    positions
        .iter()
        .map(|&index| {
            observer
                .map(|observer| {
                    let index = u64::try_from(index).map_err(|_| ObserverInvariant::Missing)?;
                    let mut edges = vec![
                        Edge::Member("deploymentGroups"),
                        Edge::Member(id.as_str()),
                        Edge::Member("hydrationRelations"),
                        Edge::Index(index),
                    ];
                    edges.extend(field.map(|field| Edge::Member(field.name())));
                    observer.nested(&edges)
                })
                .transpose()
        })
        .collect()
}

/// One relation's checks, over the positions of its occurrences in the
/// group's relations: each endpoint, or why it has none.
struct Relation<'a> {
    id: &'a DeploymentGroupId,
    key: &'a RelationKey,
    server: Admitted<&'a TargetId>,
    client: Admitted<&'a TargetId>,
    positions: &'a [usize],
}

/// What one relation's checks leave its group: each endpoint's bytes, which
/// the roles wait on, and whether every check of the relation held.
struct Checked {
    server: Admitted<()>,
    client: Admitted<()>,
    holds: Admitted<()>,
}

/// What one endpoint's checks established: its bytes, what Phase 11 found of
/// it once it names a declared Target ID, and whether all of its checks held.
struct Endpoint<'t, 's> {
    bytes: Admitted<()>,
    facts: Admitted<&'t TargetFacts<'s>>,
    holds: Admitted<()>,
}

impl Relation<'_> {
    fn unit(&self, check: Check) -> Unit {
        unit(
            HydrationSubject::Relation(self.id.clone(), self.key.clone()),
            check,
        )
    }

    fn endpoint_unit(&self, field: EndpointField, check: Check) -> Unit {
        unit(
            HydrationSubject::Endpoint(self.id.clone(), self.key.clone(), field),
            check,
        )
    }

    /// One endpoint's checks, on its own: its bytes after the group's gate;
    /// then whether it names a declared Target ID, and whether the group's
    /// members name it, each after its bytes alone; and whether its Target
    /// Profile takes the role its field names, after what Phase 11 found of
    /// it. An endpoint that failed has no subject, so it records no unit
    /// (D15:1932, 2185). A check that fails observes each occurrence of the
    /// endpoint (D15:2388): past the bound beside the policy, with the
    /// first-over witness of a key that can stand for several values
    /// (D15:2204), naming no declared Target ID beside the declared Target
    /// IDs, naming no member beside the group's members, and without its role
    /// beside its Target Profile.
    fn endpoint<'t, 's>(
        &self,
        evaluation: &mut Evaluation,
        shared: &Shared<'_, '_, 't, 's>,
        gate: &Gate<'_, 't>,
        field: EndpointField,
    ) -> Result<Endpoint<'t, 's>, HydrationInvariant> {
        let endpoint = match field {
            EndpointField::Client => &self.client,
            EndpointField::Server => &self.server,
        };
        let endpoint = match endpoint {
            Ok(endpoint) => *endpoint,
            Err(causes) => {
                return Ok(Endpoint {
                    bytes: Err(causes.clone()),
                    facts: Err(causes.clone()),
                    holds: Err(causes.clone()),
                })
            }
        };
        let at = relations_at(shared.observer, self.id, self.positions, Some(field))?;
        let bytes = evaluation.check_found(
            self.endpoint_unit(field, Check::WithinBound),
            gate.policy.clone(),
            |policy| {
                within(
                    MAX_TARGET_ID_BYTES,
                    policy.limits().target_grouping.max_target_id_bytes,
                    endpoint.as_str().len(),
                )
                .map_err(|blocking| {
                    Found::each(
                        blocking.first_over(),
                        each_beside(&at, Some(policy.evidence())),
                    )
                })
            },
        )?;
        let declared = evaluation.check_found(
            self.endpoint_unit(field, Check::KnownEndpoint),
            both(
                bytes.clone(),
                shared
                    .targets
                    .clone()
                    .and_then(|targets| targets.declares(endpoint)),
            ),
            |((), declared)| {
                if declared {
                    Ok(())
                } else {
                    let unknown = invalid(Reason::UnknownEndpoint);
                    Err(Found::each(unknown, each_beside(&at, shared.at_targets)))
                }
            },
        )?;
        let member = evaluation.check_found(
            self.endpoint_unit(field, Check::SameGroupEndpoint),
            both(
                bytes.clone(),
                gate.named.clone().and_then(|named| named.holds(endpoint)),
            ),
            |((), named)| {
                if named {
                    Ok(())
                } else {
                    let outside = invalid(Reason::CrossGroup);
                    Err(Found::each(
                        outside,
                        each_beside(&at, gate.at_members.as_ref()),
                    ))
                }
            },
        )?;
        let facts = declared.clone().and_then(|()| shared.facts(endpoint));
        let named = facts.clone().and_then(|facts| facts.profile.clone());
        // The Target Profile the role is read from, once Phase 9 resolved it.
        let at_profile = match &named {
            Ok((tuple, _)) => Some(artifact_resolution::admitted(tuple)?),
            Err(_) => None,
        };
        let role = evaluation.check_found(
            self.endpoint_unit(field, Check::EndpointRole),
            named,
            |(_, profile)| {
                let taken = roles(profile);
                let missing = |reason| {
                    Err(Found::each(
                        invalid(reason),
                        each_beside(&at, at_profile.as_ref()),
                    ))
                };
                match field {
                    EndpointField::Client if taken.browser_hydration_client => Ok(()),
                    EndpointField::Client => missing(Reason::ClientCapabilityMissing),
                    EndpointField::Server if taken.ssr_renderer => Ok(()),
                    EndpointField::Server => missing(Reason::ServerCapabilityMissing),
                }
            },
        )?;
        Ok(Endpoint {
            bytes,
            facts,
            holds: and(and(declared, member), role),
        })
    }

    /// Each endpoint's checks, client then server as their fields order them,
    /// and then the relation's: its endpoints distinct and the relation once,
    /// after both endpoints' bytes; the same locales and effective default,
    /// after what Phase 11 found of both; and static compatibility, after both
    /// Target Profiles. No check of the relation waits on another's failure.
    /// A check of the relation that fails observes each of its occurrences
    /// (D15:2388): between equal endpoints alone, repeated beside the other
    /// occurrences, and otherwise beside where it found both endpoints'
    /// locales, effective defaults, or Target Profiles (D15:2250).
    fn check(
        &self,
        evaluation: &mut Evaluation,
        shared: &Shared<'_, '_, '_, '_>,
        gate: &Gate<'_, '_>,
    ) -> Result<Checked, HydrationInvariant> {
        let client = self.endpoint(evaluation, shared, gate, EndpointField::Client)?;
        let server = self.endpoint(evaluation, shared, gate, EndpointField::Server)?;
        let at = relations_at(shared.observer, self.id, self.positions, None)?;
        let bytes = and(server.bytes.clone(), client.bytes.clone());
        // Both endpoints, once each was admitted.
        let named = both(self.server.clone(), self.client.clone());
        let distinct = evaluation.check_found(
            self.unit(Check::DistinctEndpoints),
            both(bytes.clone(), named.clone()),
            |((), (server, client))| {
                if server == client {
                    // The relation compares its own two fields, no other
                    // location.
                    let equal = invalid(Reason::EqualEndpoint);
                    Err(Found::each(equal, each_beside_all(&at, Some(&[]))))
                } else {
                    Ok(())
                }
            },
        )?;
        let once = evaluation.check_found(self.unit(Check::UniqueRelation), bytes, |()| {
            if self.positions.len() == 1 {
                Ok(())
            } else {
                Err(Found::repeated(invalid(Reason::DuplicateRelation), &at))
            }
        })?;
        let facts = both(server.facts, client.facts);
        let requested_input = facts
            .clone()
            .and_then(|(server, client)| both(server.requested.clone(), client.requested.clone()));
        // Where each endpoint requests its locales, once both are resolved.
        let at_requested = match (shared.observer, &named, &requested_input) {
            (Some(observer), Ok((server, client)), Ok(_)) => Some(
                [*server, *client]
                    .map(|id| observer.value(&["targetProfiles", id.as_str(), "requestedLocales"]))
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            _ => None,
        };
        let requested = evaluation.check_found(
            self.unit(Check::SameRequestedLocales),
            requested_input,
            |(server, client)| {
                if server == client {
                    Ok(())
                } else {
                    let other = locale_policy(Reason::RequestedLocales);
                    Err(Found::each(
                        other,
                        each_beside_all(&at, at_requested.as_deref()),
                    ))
                }
            },
        )?;
        let default_input = facts.clone().and_then(|(server, client)| {
            both(
                server.effective_default.clone(),
                client.effective_default.clone(),
            )
        });
        // Where each endpoint's effective default is declared, once both are
        // resolved.
        let at_defaults = match (shared.observer, &named, &facts, &default_input) {
            (Some(observer), Ok((server, client)), Ok((of_server, of_client)), Ok(_)) => {
                Some(vec![
                    default_at(observer, server, of_server)?,
                    default_at(observer, client, of_client)?,
                ])
            }
            _ => None,
        };
        let default = evaluation.check_found(
            self.unit(Check::SameEffectiveDefault),
            default_input,
            |(server, client)| {
                if server == client {
                    Ok(())
                } else {
                    let other = locale_policy(Reason::EffectiveDefault);
                    Err(Found::each(
                        other,
                        each_beside_all(&at, at_defaults.as_deref()),
                    ))
                }
            },
        )?;
        let profiles =
            facts.and_then(|(server, client)| both(server.profile.clone(), client.profile.clone()));
        // Both Target Profiles, once Phase 9 resolved them.
        let at_profiles = match &profiles {
            Ok(((server, _), (client, _))) => Some(vec![
                artifact_resolution::admitted(server)?,
                artifact_resolution::admitted(client)?,
            ]),
            Err(_) => None,
        };
        let compatible = evaluation.check_found(
            self.unit(Check::StaticCompatibility),
            profiles,
            |((_, server), (_, client))| {
                if statically_compatible(server, client) {
                    Ok(())
                } else {
                    let incompatible = Blocking {
                        code: Code::HydrationCapabilityIncompatible,
                        reason: None,
                        details: None,
                    };
                    Err(Found::each(
                        incompatible,
                        each_beside_all(&at, at_profiles.as_deref()),
                    ))
                }
            },
        )?;
        let holds = and(
            and(and(server.holds, client.holds), and(distinct, once)),
            and(and(requested, default), compatible),
        );
        Ok(Checked {
            server: server.bytes,
            client: client.bytes,
            holds,
        })
    }
}

const fn invalid(reason: Reason) -> Blocking {
    Blocking {
        code: Code::HydrationRelationInvalid,
        reason: Some(reason),
        details: None,
    }
}

const fn locale_policy(reason: Reason) -> Blocking {
    Blocking {
        code: Code::HydrationLocalePolicyIncompatible,
        reason: Some(reason),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

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
    use crate::resolver::exact_resolution::resolve_fields;
    use crate::resolver::groups::{self, resolve_groups, GroupFields, GroupSubject};
    use crate::resolver::limits::{fixture_limits, ResolutionLimits};
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

    /// The identity of the Target Profile the target `id` names.
    fn profile_identity(id: &str) -> String {
        format!("storefront-{id}")
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

    /// What each Target Profile's 024 body declares, by its identity: the
    /// profiles that render server-side and hydrate in the browser, and other
    /// edits of the sample content, each a JSON pointer and its new value.
    #[derive(Default, Clone)]
    struct Roles {
        renders: BTreeSet<String>,
        hydrates: BTreeSet<String>,
        edits: BTreeMap<String, Vec<(&'static str, Value)>>,
    }

    impl Roles {
        /// The sealed Target Profile the target `id` names, with the roles
        /// and `mf2Specification` its identity is given here.
        fn profile_of(&self, id: &str) -> Value {
            let identity = profile_identity(id);
            let mut content = sample(ArtifactKind::TargetProfile);
            let mut declared = Vec::new();
            if self.hydrates.contains(&identity) {
                declared.push("browser-hydration-client");
            }
            if self.renders.contains(&identity) {
                declared.push("ssr-renderer");
            }
            content["hydration"]["roles"] = json!(declared);
            for (pointer, value) in self.edits.get(&identity).into_iter().flatten() {
                *content.pointer_mut(pointer).unwrap() = value.clone();
            }
            envelope_of(ArtifactKind::TargetProfile, &identity, "1", content)
        }
    }

    /// What Phase 13 reads, which a test edits: the declaration, the Target
    /// Profiles submitted with the roles and other contents they declare, the
    /// policy's bounds, and whether a policy was confirmed, structural
    /// admission admitted the groups, and Phase 8 admitted the Locale
    /// Negotiation Profile.
    struct Inputs {
        app: Value,
        missing_profiles: BTreeSet<String>,
        roles: Roles,
        limits: ResolutionLimits,
        policy: Admitted<()>,
        declared: Admitted<()>,
        negotiation: Admitted<()>,
    }

    impl Inputs {
        /// 015's example: targets `ssr` and `browser` requesting `en` and
        /// `ja`, in group `web` with the relation `ssr -> browser`, and
        /// target `worker` alone in group `jobs`. `ssr`'s Target Profile
        /// renders server-side and `browser`'s hydrates in the browser.
        fn new() -> Self {
            let mut inputs = Self {
                app: minimal_config()["profiles"]["app"].clone(),
                missing_profiles: BTreeSet::new(),
                roles: Roles::default(),
                limits: fixture_limits(100),
                policy: Ok(()),
                declared: Ok(()),
                negotiation: Ok(()),
            };
            inputs.app["requestedLocales"] = json!(["en", "ja"]);
            inputs.app["defaultRequestedLocale"] = json!("en");
            inputs.app["targetProfiles"] = Value::Object(Map::new());
            for id in ["ssr", "browser"] {
                inputs.declare(id, &json!(["en", "ja"]));
            }
            inputs.declare("worker", &json!(["en"]));
            inputs.app["deploymentGroups"] = json!({
                "web": {
                    "members": ["ssr", "browser"],
                    "hydrationRelations": [{"server": "ssr", "client": "browser"}]
                },
                "jobs": {"members": ["worker"]},
            });
            inputs.roles.renders.insert(profile_identity("ssr"));
            inputs.roles.hydrates.insert(profile_identity("browser"));
            inputs
        }

        /// Declares the target `id` requesting `requested` and naming its own
        /// Target Profile, whose reference `resolve` computes from the roles.
        fn declare(&mut self, id: &str, requested: &Value) {
            self.app["targetProfiles"][id] = json!({
                "profile": reference_value(&tuple_of(&self.roles.profile_of(id))),
                "requestedLocales": requested,
            });
        }

        fn group(&mut self, id: &str) -> &mut Value {
            &mut self.app["deploymentGroups"][id]
        }

        fn relate(&mut self, id: &str, relations: &[(&str, &str)]) {
            self.group(id)["hydrationRelations"] = Value::Array(
                relations
                    .iter()
                    .map(|(server, client)| json!({"server": server, "client": client}))
                    .collect(),
            );
        }

        /// Phases 7, 9, 11, 12, and 13 into one evaluation; `inspect` reads it
        /// and what Phase 13 returned.
        /// The declaration, each reference naming its Target Profile as the
        /// roles now declare it.
        fn declaration(&self) -> Declaration {
            let mut app = self.app.clone();
            for (id, entry) in app["targetProfiles"].as_object_mut().unwrap() {
                entry["profile"] = reference_value(&tuple_of(&self.roles.profile_of(id)));
            }
            serde_json::from_value(app).unwrap()
        }

        fn resolve<R>(
            &self,
            inspect: impl FnOnce(Evaluation, Admitted<ResolvedHydration>) -> R,
        ) -> R {
            let declaration = self.declaration();
            let confirmed = ConfirmedPolicy::fixture_owned(self.limits.clone());
            let unconfirmed = self.policy.clone().err();
            let policy = unconfirmed.as_ref().map_or(Ok(&confirmed), Err);
            let mut evaluation = Evaluation::default();
            let observed = Observed::declaration(&declaration);
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
                .filter(|(id, _)| !self.missing_profiles.contains(id.as_str()))
                .map(|(id, _)| self.roles.profile_of(id.as_str()).to_string())
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
            let groups = observed
                .app(|observer| {
                    resolve_groups(
                        &mut evaluation,
                        policy,
                        &Ok(GroupFields::declared(&declaration.deployment_groups)),
                        (targets.as_ref(), Some(observer)),
                    )
                })
                .unwrap();
            let declared = self
                .declared
                .clone()
                .map(|()| HydrationFields::declared(&declaration.deployment_groups));
            let hydration = observed
                .app(|observer| {
                    validate_hydration(
                        &mut evaluation,
                        policy,
                        &declared,
                        targets.as_ref(),
                        (groups.as_ref(), Some(observer)),
                        self.negotiation.clone(),
                    )
                })
                .unwrap();
            inspect(evaluation, hydration)
        }

        /// The view of the Evidence `at` makes in the declaration.
        fn at(
            &self,
            at: impl FnOnce(&DeclarationObserver<'_>) -> Result<EvidenceProjection, ObserverInvariant>,
        ) -> Value {
            Observed::declaration(&self.declaration()).app(|observer| view(&at(observer).unwrap()))
        }

        /// The view of the endpoint `field` names in the relation at `index`
        /// of `group_id`.
        fn endpoint_at(&self, group_id: &str, index: u64, field: &str) -> Value {
            self.at(|observer| {
                observer.nested(&[
                    Edge::Member("deploymentGroups"),
                    Edge::Member(group_id),
                    Edge::Member("hydrationRelations"),
                    Edge::Index(index),
                    Edge::Member(field),
                ])
            })
        }

        /// The view of the relation at `index` of `group_id`.
        fn relation_at(&self, group_id: &str, index: u64) -> Value {
            self.at(|observer| {
                observer.nested(&[
                    Edge::Member("deploymentGroups"),
                    Edge::Member(group_id),
                    Edge::Member("hydrationRelations"),
                    Edge::Index(index),
                ])
            })
        }

        /// The view of where the target `id` requests its locales.
        fn requested_at(&self, id: &str) -> Value {
            self.at(|observer| observer.value(&["targetProfiles", id, "requestedLocales"]))
        }

        /// The view of the Target Profile the target `id` names, as the
        /// roles now declare it.
        fn profile(&self, id: &str) -> Value {
            view(&artifact_resolution::admitted(&tuple_of(&self.roles.profile_of(id))).unwrap())
        }
    }

    /// The view of the confirmed policy, which supplies every bound.
    fn policy() -> Value {
        view(ConfirmedPolicy::fixture_owned(fixture_limits(1)).evidence())
    }

    /// Each Phase 13 Finding candidate: its unit, and the views of its
    /// primary and related Evidence.
    fn observed(evaluation: &Evaluation) -> Vec<(Unit, Value, Vec<Value>)> {
        evaluation
            .occurrence_candidates()
            .iter()
            .filter(|candidate| candidate.finding.unit().phase == Phase::HydrationValidation)
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

    fn relation(group_id: &str, server: &str, client: &str, check: Check) -> Unit {
        unit(
            HydrationSubject::Relation(
                group(group_id),
                RelationKey::Pair(target(server), target(client)),
            ),
            check,
        )
    }

    fn roles(group_id: &str, id: &str) -> Unit {
        unit(
            HydrationSubject::Roles(group(group_id), target(id)),
            Check::SingleRole,
        )
    }

    /// One check of the endpoint `field` names in a relation of `group_id`.
    fn on_endpoint(
        group_id: &str,
        server: &str,
        client: &str,
        field: EndpointField,
        check: Check,
    ) -> Unit {
        unit(
            HydrationSubject::Endpoint(
                group(group_id),
                RelationKey::Pair(target(server), target(client)),
                field,
            ),
            check,
        )
    }

    /// The bytes of the endpoint `field` names in a relation of `group_id`.
    fn endpoint(group_id: &str, server: &str, client: &str, field: EndpointField) -> Unit {
        on_endpoint(group_id, server, client, field, Check::WithinBound)
    }

    fn bound_unit() -> Unit {
        unit(HydrationSubject::Relations, Check::WithinBound)
    }

    /// Every check of one endpoint, in rank order.
    const ENDPOINT_CHECKS: [Check; 4] = [
        Check::WithinBound,
        Check::KnownEndpoint,
        Check::SameGroupEndpoint,
        Check::EndpointRole,
    ];

    /// Every check of one relation, in rank order; its endpoints' checks
    /// come before them.
    const CHECKS: [Check; 5] = [
        Check::DistinctEndpoints,
        Check::UniqueRelation,
        Check::SameRequestedLocales,
        Check::SameEffectiveDefault,
        Check::StaticCompatibility,
    ];

    /// Every unit of one relation of `web`, in 015's order: each endpoint's
    /// checks, the client's field first, then the relation's.
    fn relation_units(server: &str, client: &str) -> Vec<Unit> {
        let mut units = Vec::new();
        for field in [EndpointField::Client, EndpointField::Server] {
            units.extend(
                ENDPOINT_CHECKS.map(|check| on_endpoint("web", server, client, field, check)),
            );
        }
        units.extend(CHECKS.map(|check| relation("web", server, client, check)));
        units
    }

    /// Phase 13's units and their statuses, in 015's order.
    fn statuses(evaluation: &Evaluation) -> Vec<(Unit, Status)> {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.phase == Phase::HydrationValidation)
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    /// Phase 13's blocking results, as their unit, code, and reason.
    fn issues(evaluation: &Evaluation) -> Vec<(Unit, &'static str, Option<&'static str>)> {
        evaluation
            .issues()
            .filter(|key| key.unit().phase == Phase::HydrationValidation)
            .map(|key| {
                (
                    key.unit().clone(),
                    key.code().as_str(),
                    key.reason().map(Reason::as_str),
                )
            })
            .collect()
    }

    fn pairs(relations: &Relations) -> Vec<(&str, &str)> {
        relations
            .iter()
            .map(|(server, client)| (server.as_str(), client.as_str()))
            .collect()
    }

    #[test]
    fn subjects_order_the_bound_then_endpoints_relations_and_roles_after_every_phase_12_unit() {
        // Every relation's endpoints, the client's field first, come before
        // every relation's checks.
        let mut ordered = vec![bound_unit()];
        for field in [EndpointField::Client, EndpointField::Server] {
            ordered.extend(
                ENDPOINT_CHECKS.map(|check| on_endpoint("web", "ssr", "browser", field, check)),
            );
        }
        ordered.push(endpoint("web", "ssr", "worker", EndpointField::Client));
        ordered.extend(CHECKS.map(|check| relation("web", "ssr", "browser", check)));
        ordered.push(relation("web", "ssr", "worker", Check::DistinctEndpoints));
        ordered.push(roles("jobs", "browser"));
        ordered.push(roles("web", "browser"));
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        let last_of_phase_12 =
            groups::unit(GroupSubject::Assignment(target("zz")), Check::SingleGroup);
        assert!(last_of_phase_12 < bound_unit());
    }

    #[test]
    fn fifteens_example_relation_holds_and_keeps_its_pair() {
        Inputs::new().resolve(|evaluation, hydration| {
            assert_eq!(issues(&evaluation), []);
            let mut expected = vec![bound_unit()];
            expected.extend(relation_units("ssr", "browser"));
            expected.push(roles("web", "browser"));
            expected.push(roles("web", "ssr"));
            assert_eq!(
                statuses(&evaluation),
                expected
                    .into_iter()
                    .map(|unit| (unit, Status::Evaluated))
                    .collect::<Vec<_>>()
            );
            let hydration = hydration.unwrap();
            assert_eq!(hydration.complete(), Ok(()));
            assert_eq!(
                pairs(hydration.get(&group("web")).unwrap().as_ref().unwrap()),
                [("ssr", "browser")]
            );
            // A group without relations has none, and that holds.
            assert_eq!(hydration.get(&group("jobs")), Some(&Ok(BTreeSet::new())));
            assert_eq!(hydration.negotiation(), &Ok(()));
        });
    }

    #[test]
    fn fan_out_and_fan_in_are_valid() {
        let mut inputs = Inputs::new();
        for id in ["edge", "app"] {
            inputs.declare(id, &json!(["en", "ja"]));
        }
        inputs.group("web")["members"] = json!(["ssr", "edge", "browser", "app"]);
        inputs.relate(
            "web",
            &[("ssr", "browser"), ("ssr", "app"), ("edge", "browser")],
        );
        inputs.roles.renders.insert(profile_identity("edge"));
        inputs.roles.hydrates.insert(profile_identity("app"));
        inputs.resolve(|evaluation, hydration| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(
                pairs(
                    hydration
                        .unwrap()
                        .get(&group("web"))
                        .unwrap()
                        .as_ref()
                        .unwrap()
                ),
                [("edge", "browser"), ("ssr", "app"), ("ssr", "browser")]
            );
        });
    }

    #[test]
    fn equal_endpoints_and_a_repeated_pair_are_invalid_relations() {
        let mut inputs = Inputs::new();
        inputs.relate(
            "web",
            &[("ssr", "browser"), ("ssr", "ssr"), ("ssr", "browser")],
        );
        inputs.resolve(|evaluation, hydration| {
            let equal = relation("web", "ssr", "ssr", Check::DistinctEndpoints);
            let repeated = relation("web", "ssr", "browser", Check::UniqueRelation);
            let found = issues(&evaluation);
            assert!(found.contains(&(
                equal.clone(),
                "project-profile-hydration-relation-invalid",
                Some("equal-endpoint")
            )));
            assert!(found.contains(&(
                repeated.clone(),
                "project-profile-hydration-relation-invalid",
                Some("duplicate-relation")
            )));
            // The repeated pair at each occurrence beside the other; the equal
            // endpoints' relation on its own, and its client beside the Target
            // Profile without the role.
            let at = |index| inputs.relation_at("web", index);
            assert_eq!(
                observed(&evaluation),
                [
                    (repeated.clone(), at(0), vec![at(2)]),
                    (repeated, at(2), vec![at(0)]),
                    (
                        on_endpoint(
                            "web",
                            "ssr",
                            "ssr",
                            EndpointField::Client,
                            Check::EndpointRole
                        ),
                        inputs.endpoint_at("web", 1, "client"),
                        vec![inputs.profile("ssr")]
                    ),
                    (equal, at(1), vec![]),
                ]
            );
            // Between the equal endpoints the locales and the Target Profiles
            // are still compared, and each field's role is still checked:
            // `ssr`'s Target Profile does not hydrate.
            assert!(found.contains(&(
                on_endpoint(
                    "web",
                    "ssr",
                    "ssr",
                    EndpointField::Client,
                    Check::EndpointRole
                ),
                "project-profile-hydration-relation-invalid",
                Some("client-capability-missing")
            )));
            for check in &CHECKS[2..] {
                assert_eq!(
                    waits_on(&evaluation, &relation("web", "ssr", "ssr", *check)),
                    None,
                    "{check:?}"
                );
            }
            // The equal-endpoint relation is no second role for `ssr`.
            assert_eq!(waits_on(&evaluation, &roles("web", "ssr")), None);
            assert!(!found
                .iter()
                .any(|(_, _, reason)| *reason == Some("dual-role")));
            assert!(hydration.unwrap().get(&group("web")).unwrap().is_err());
        });
    }

    #[test]
    fn an_endpoint_naming_no_target_or_another_groups_member_is_invalid() {
        let mut inputs = Inputs::new();
        inputs.relate("web", &[("ssr", "phone"), ("ssr", "worker")]);
        inputs.resolve(|evaluation, hydration| {
            let phone = |check| on_endpoint("web", "ssr", "phone", EndpointField::Client, check);
            let worker = |check| on_endpoint("web", "ssr", "worker", EndpointField::Client, check);
            let invalid = "project-profile-hydration-relation-invalid";
            // `phone` names no Target ID and no member of `web`. `worker` is
            // `jobs`'s member, its Target Profile does not hydrate, and it
            // requests other locales than `ssr`. Each failure is its own.
            assert_eq!(
                issues(&evaluation),
                [
                    (
                        phone(Check::KnownEndpoint),
                        invalid,
                        Some("unknown-endpoint")
                    ),
                    (
                        phone(Check::SameGroupEndpoint),
                        invalid,
                        Some("cross-group")
                    ),
                    (
                        worker(Check::SameGroupEndpoint),
                        invalid,
                        Some("cross-group")
                    ),
                    (
                        worker(Check::EndpointRole),
                        invalid,
                        Some("client-capability-missing")
                    ),
                    (
                        relation("web", "ssr", "worker", Check::SameRequestedLocales),
                        "project-profile-hydration-locale-policy-incompatible",
                        Some("requested-locales")
                    ),
                ]
            );
            // Each endpoint at its occurrence: beside the declared Target IDs,
            // the group's members, or the Target Profile it compared; and the
            // relation beside where both endpoints request their locales.
            let targets = inputs.at(|observer| observer.value(&["targetProfiles"]));
            let members =
                inputs.at(|observer| observer.value(&["deploymentGroups", "web", "members"]));
            let (at_phone, at_worker) = (
                inputs.endpoint_at("web", 0, "client"),
                inputs.endpoint_at("web", 1, "client"),
            );
            assert_eq!(
                observed(&evaluation),
                [
                    (phone(Check::KnownEndpoint), at_phone.clone(), vec![targets]),
                    (
                        phone(Check::SameGroupEndpoint),
                        at_phone,
                        vec![members.clone()]
                    ),
                    (
                        worker(Check::SameGroupEndpoint),
                        at_worker.clone(),
                        vec![members]
                    ),
                    (
                        worker(Check::EndpointRole),
                        at_worker,
                        vec![inputs.profile("worker")]
                    ),
                    (
                        relation("web", "ssr", "worker", Check::SameRequestedLocales),
                        inputs.relation_at("web", 1),
                        vec![inputs.requested_at("ssr"), inputs.requested_at("worker")]
                    ),
                ]
            );
            // Only what needs `phone`'s facts waits on it, and the endpoints
            // outside the group are still compared.
            let unknown = vec![phone(Check::KnownEndpoint)];
            assert_eq!(
                waits_on(&evaluation, &phone(Check::EndpointRole)),
                Some(unknown.clone())
            );
            for check in &CHECKS[2..] {
                assert_eq!(
                    waits_on(&evaluation, &relation("web", "ssr", "phone", *check)),
                    Some(unknown.clone()),
                    "{check:?}"
                );
                assert_eq!(
                    waits_on(&evaluation, &relation("web", "ssr", "worker", *check)),
                    None,
                    "{check:?}"
                );
            }
            assert!(hydration.unwrap().complete().is_err());
        });
    }

    #[test]
    fn a_repeated_relation_observes_each_occurrence_of_its_endpoint() {
        // `ssr -> phone` twice: one subject, so one result for each failed
        // check, observed at both occurrences (D15:2388), and the repeat
        // itself at each occurrence beside the other.
        let mut inputs = Inputs::new();
        inputs.relate(
            "web",
            &[("ssr", "phone"), ("ssr", "browser"), ("ssr", "phone")],
        );
        inputs.resolve(|evaluation, _| {
            let phone = |check| on_endpoint("web", "ssr", "phone", EndpointField::Client, check);
            let targets = inputs.at(|observer| observer.value(&["targetProfiles"]));
            let members =
                inputs.at(|observer| observer.value(&["deploymentGroups", "web", "members"]));
            let at = |index| inputs.endpoint_at("web", index, "client");
            let repeated = relation("web", "ssr", "phone", Check::UniqueRelation);
            let relation_at = |index| inputs.relation_at("web", index);
            assert_eq!(
                observed(&evaluation),
                [
                    (phone(Check::KnownEndpoint), at(0), vec![targets.clone()]),
                    (phone(Check::KnownEndpoint), at(2), vec![targets]),
                    (
                        phone(Check::SameGroupEndpoint),
                        at(0),
                        vec![members.clone()]
                    ),
                    (phone(Check::SameGroupEndpoint), at(2), vec![members]),
                    (repeated.clone(), relation_at(0), vec![relation_at(2)]),
                    (repeated, relation_at(2), vec![relation_at(0)]),
                ]
            );
        });
    }

    #[test]
    fn a_target_serving_and_hydrating_in_one_group_takes_two_roles() {
        let mut inputs = Inputs::new();
        inputs.declare("app", &json!(["en", "ja"]));
        inputs.group("web")["members"] = json!(["ssr", "browser", "app"]);
        inputs.relate("web", &[("ssr", "browser"), ("browser", "app")]);
        inputs.roles.renders.insert(profile_identity("browser"));
        inputs.roles.hydrates.insert(profile_identity("app"));
        inputs.resolve(|evaluation, hydration| {
            assert_eq!(
                issues(&evaluation),
                [(
                    roles("web", "browser"),
                    "project-profile-hydration-relation-invalid",
                    Some("dual-role")
                )]
            );
            // `browser` serves in the relation at 1 and hydrates in the one at
            // 0: each occurrence beside those in the other role.
            let (serving, hydrating) = (
                inputs.endpoint_at("web", 1, "server"),
                inputs.endpoint_at("web", 0, "client"),
            );
            assert_eq!(
                observed(&evaluation),
                [
                    (
                        roles("web", "browser"),
                        serving.clone(),
                        vec![hydrating.clone()]
                    ),
                    (roles("web", "browser"), hydrating, vec![serving]),
                ]
            );
            // Every relation of the group held but the roles did not.
            let causes = hydration
                .unwrap()
                .get(&group("web"))
                .unwrap()
                .clone()
                .unwrap_err();
            assert_eq!(
                causes
                    .keys()
                    .map(|key| key.unit().clone())
                    .collect::<Vec<_>>(),
                [roles("web", "browser")]
            );
        });
    }

    #[test]
    fn an_endpoint_naming_a_target_waits_on_the_target_map_phase_7_bounds() {
        let mut inputs = Inputs::new();
        // Three targets are declared.
        inputs.limits.target_grouping.max_target_profiles = bound(2);
        inputs.resolve(|evaluation, _| {
            let map = locales::unit(LocaleSubject::TargetProfiles, Check::WithinBound);
            for field in [EndpointField::Client, EndpointField::Server] {
                let at = |check| on_endpoint("web", "ssr", "browser", field, check);
                assert_eq!(
                    waits_on(&evaluation, &at(Check::KnownEndpoint)),
                    Some(vec![map.clone()])
                );
                // Neither the bytes nor the membership needs a Target ID.
                for check in [Check::WithinBound, Check::SameGroupEndpoint] {
                    assert_eq!(waits_on(&evaluation, &at(check)), None, "{check:?}");
                }
            }
            for check in [Check::DistinctEndpoints, Check::UniqueRelation] {
                assert_eq!(
                    waits_on(&evaluation, &relation("web", "ssr", "browser", check)),
                    None
                );
            }
        });
    }

    #[test]
    fn the_roles_come_from_each_target_profiles_body() {
        let mut inputs = Inputs::new();
        inputs.roles = Roles::default();
        inputs.resolve(|evaluation, _| {
            assert_eq!(issues(&evaluation), both_roles_missing());
            // Each endpoint at its occurrence, beside the Target Profile its
            // role is read from.
            let missing = |field, name, id| {
                (
                    on_endpoint("web", "ssr", "browser", field, Check::EndpointRole),
                    inputs.endpoint_at("web", 0, name),
                    vec![inputs.profile(id)],
                )
            };
            assert_eq!(
                observed(&evaluation),
                [
                    missing(EndpointField::Client, "client", "browser"),
                    missing(EndpointField::Server, "server", "ssr"),
                ]
            );
            // Compatibility compares the two Target Profiles whatever roles
            // they declare.
            assert_eq!(
                waits_on(
                    &evaluation,
                    &relation("web", "ssr", "browser", Check::StaticCompatibility)
                ),
                None
            );
        });
    }

    #[test]
    fn a_statically_known_incompatibility_blocks_the_relation() {
        // The client's profile follows another MF2 specification revision.
        let mut inputs = Inputs::new();
        inputs.roles.edits.insert(
            profile_identity("browser"),
            vec![("/mf2Specification/revision", json!("47"))],
        );
        inputs.resolve(|evaluation, hydration| {
            assert_eq!(
                issues(&evaluation),
                [(
                    relation("web", "ssr", "browser", Check::StaticCompatibility),
                    "project-profile-hydration-capability-incompatible",
                    None
                )]
            );
            // The relation, beside both Target Profiles it compared.
            assert_eq!(
                observed(&evaluation),
                [(
                    relation("web", "ssr", "browser", Check::StaticCompatibility),
                    inputs.relation_at("web", 0),
                    vec![inputs.profile("ssr"), inputs.profile("browser")]
                )]
            );
            assert!(hydration.unwrap().complete().is_err());
        });
    }

    #[test]
    fn a_role_is_read_from_the_body_never_from_a_name() {
        // `ssr`'s profile declares only the client's role, and `browser`'s
        // only the server's.
        let mut inputs = Inputs::new();
        inputs.roles.renders = BTreeSet::from([profile_identity("browser")]);
        inputs.roles.hydrates = BTreeSet::from([profile_identity("ssr")]);
        inputs.resolve(|evaluation, _| {
            assert_eq!(issues(&evaluation), both_roles_missing());
        });
    }

    /// Each endpoint of `ssr -> browser` missing its role, the client's field
    /// first.
    fn both_roles_missing() -> Vec<(Unit, &'static str, Option<&'static str>)> {
        [
            (EndpointField::Client, "client-capability-missing"),
            (EndpointField::Server, "server-capability-missing"),
        ]
        .into_iter()
        .map(|(field, reason)| {
            (
                on_endpoint("web", "ssr", "browser", field, Check::EndpointRole),
                "project-profile-hydration-relation-invalid",
                Some(reason),
            )
        })
        .collect()
    }

    /// An artifact that is no Target Profile declares no role and is
    /// compatible with nothing. Phase 9 never resolves a Target Profile
    /// reference to one, so only these helpers can meet it.
    #[test]
    fn an_artifact_that_is_no_target_profile_has_no_role_and_no_compatibility() {
        let mut content = sample(ArtifactKind::TargetProfile);
        content["hydration"]["roles"] = json!(["browser-hydration-client", "ssr-renderer"]);
        let profile = FormalMember::new(envelope_of(
            ArtifactKind::TargetProfile,
            "storefront-app",
            "1",
            content,
        ))
        .unwrap();
        let trust = FormalMember::new(envelope_of(
            ArtifactKind::TrustPolicy,
            "storefront-trust",
            "1",
            sample(ArtifactKind::TrustPolicy),
        ))
        .unwrap();
        let admitted = |member| AdmittedArtifact {
            member,
            content: member.envelope().unwrap().admit().unwrap(),
        };
        let (profile, trust) = (admitted(&profile), admitted(&trust));
        assert_eq!(
            super::roles(&profile),
            HydrationRoles {
                ssr_renderer: true,
                browser_hydration_client: true,
            }
        );
        assert_eq!(super::roles(&trust), HydrationRoles::default());
        assert!(statically_compatible(&profile, &profile));
        for (server, client) in [(&profile, &trust), (&trust, &profile), (&trust, &trust)] {
            assert!(!statically_compatible(server, client));
        }
    }

    #[test]
    fn physical_dimensions_never_make_a_pair_incompatible() {
        // The client's profile is another mode, adapter, host grammar, codec,
        // and limit; its MF2 specification, execution, and services agree.
        let mut inputs = Inputs::new();
        inputs.roles.edits.insert(
            profile_identity("browser"),
            vec![
                ("/mode", json!("aot")),
                ("/adapter/revision", json!("1")),
                ("/host/grammar/revision", json!("2024")),
                ("/codecs/bindingTable/revision", json!("1")),
                ("/limits/maxFiles", json!("32")),
            ],
        );
        inputs.resolve(|evaluation, hydration| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(hydration.unwrap().complete(), Ok(()));
        });
    }

    #[test]
    fn endpoints_with_other_locales_or_defaults_are_incompatible_with_nothing_intersected() {
        let mut inputs = Inputs::new();
        // `browser` drops `ja`; `ssr` keeps both with its own default `ja`.
        inputs.app["targetProfiles"]["browser"]["requestedLocales"] = json!(["en"]);
        inputs.app["targetProfiles"]["ssr"]["defaultRequestedLocale"] = json!("ja");
        inputs.resolve(|evaluation, hydration| {
            assert_eq!(
                issues(&evaluation),
                [
                    (
                        relation("web", "ssr", "browser", Check::SameRequestedLocales),
                        "project-profile-hydration-locale-policy-incompatible",
                        Some("requested-locales")
                    ),
                    (
                        relation("web", "ssr", "browser", Check::SameEffectiveDefault),
                        "project-profile-hydration-locale-policy-incompatible",
                        Some("effective-default")
                    ),
                ]
            );
            // The relation beside where each endpoint declares what it
            // compared: `ssr` its own default, `browser` the project's.
            let at = inputs.relation_at("web", 0);
            assert_eq!(
                observed(&evaluation),
                [
                    (
                        relation("web", "ssr", "browser", Check::SameRequestedLocales),
                        at.clone(),
                        vec![inputs.requested_at("ssr"), inputs.requested_at("browser")]
                    ),
                    (
                        relation("web", "ssr", "browser", Check::SameEffectiveDefault),
                        at,
                        vec![
                            inputs.at(|observer| {
                                observer.value(&["targetProfiles", "ssr", "defaultRequestedLocale"])
                            }),
                            inputs.at(|observer| observer.value(&["defaultRequestedLocale"])),
                        ]
                    ),
                ]
            );
            assert!(hydration.unwrap().get(&group("web")).unwrap().is_err());
        });
    }

    #[test]
    fn the_relations_hold_at_their_bound_and_one_occurrence_more_leaves_every_relation_unchecked() {
        // Two occurrences, the repeated pair among them.
        for (limit, over) in [(2, false), (1, true)] {
            let mut inputs = Inputs::new();
            inputs.relate("web", &[("ssr", "browser"), ("ssr", "browser")]);
            inputs.limits.target_grouping.max_hydration_relations = bound(limit);
            inputs.resolve(|evaluation, _| {
                let found: Vec<Unit> = issues(&evaluation)
                    .into_iter()
                    .map(|(unit, _, _)| unit)
                    .collect();
                if !over {
                    assert_eq!(
                        found,
                        [relation("web", "ssr", "browser", Check::UniqueRelation)]
                    );
                    return;
                }
                assert_eq!(found, [bound_unit()]);
                // Observed at the Group ID map, beside the policy.
                assert_eq!(
                    observed(&evaluation),
                    [(
                        bound_unit(),
                        inputs.at(|observer| observer.value(&["deploymentGroups"])),
                        vec![policy()]
                    )]
                );
                let key = evaluation.issues().next().unwrap().clone();
                assert_eq!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        bound: MAX_HYDRATION_RELATIONS,
                        limit: bound(1),
                        actual: Actual::Exact(2),
                    })
                );
                for (unit, _) in &statuses(&evaluation)[1..] {
                    assert_eq!(
                        waits_on(&evaluation, unit),
                        Some(vec![bound_unit()]),
                        "{unit:?}"
                    );
                }
            });
        }
    }

    #[test]
    fn each_endpoint_holds_at_the_target_id_bound_on_its_own_and_one_byte_more_blocks_its_relation()
    {
        // `browser` is seven bytes and `ssr` three: at 6 only the client's
        // endpoint is over, at 2 both are, each its own result, with the
        // first-over witness of a key that can stand for several values
        // (D15:2204).
        let client = endpoint("web", "ssr", "browser", EndpointField::Client);
        let server = endpoint("web", "ssr", "browser", EndpointField::Server);
        for (limit, over) in [
            (7, vec![]),
            (6, vec![client.clone()]),
            (2, vec![client.clone(), server.clone()]),
        ] {
            let mut inputs = Inputs::new();
            inputs.limits.target_grouping.max_target_id_bytes = bound(limit);
            inputs.resolve(|evaluation, _| {
                let found: Vec<Unit> = issues(&evaluation)
                    .into_iter()
                    .map(|(unit, _, _)| unit)
                    .collect();
                assert_eq!(found, over, "{limit}");
                for unit in &over {
                    let key = evaluation
                        .issues()
                        .find(|key| key.unit() == unit)
                        .unwrap()
                        .clone();
                    assert_eq!(
                        evaluation.limit(&key),
                        Some(LimitExceeded {
                            bound: MAX_TARGET_ID_BYTES,
                            limit: bound(limit),
                            actual: Actual::AtLeast(limit + 1),
                        })
                    );
                }
                if over.is_empty() {
                    return;
                }
                // Each endpoint over the bound at its occurrence, beside the
                // policy.
                let at: Vec<_> = over
                    .iter()
                    .map(|unit| {
                        let field = if *unit == client { "client" } else { "server" };
                        (
                            unit.clone(),
                            inputs.endpoint_at("web", 0, field),
                            vec![policy()],
                        )
                    })
                    .collect();
                assert_eq!(observed(&evaluation), at, "{limit}");
                // The relation's checks wait on every endpoint over the
                // bound, and each Target ID's roles on its own endpoint.
                for check in [Check::DistinctEndpoints, Check::UniqueRelation] {
                    assert_eq!(
                        waits_on(&evaluation, &relation("web", "ssr", "browser", check)),
                        Some(over.clone()),
                        "{limit} {check:?}"
                    );
                }
                assert_eq!(
                    waits_on(&evaluation, &roles("web", "browser")),
                    Some(vec![client.clone()])
                );
                let serving = (over.len() == 2).then(|| vec![server.clone()]);
                assert_eq!(waits_on(&evaluation, &roles("web", "ssr")), serving);
            });
        }
    }

    #[test]
    fn every_check_of_a_group_waits_on_its_id_bounds_and_only_membership_on_its_member_count() {
        // `web` is three bytes and `jobs` four, and there are two groups;
        // only `web` has a relation.
        let id_bytes = groups::unit(GroupSubject::Group(group("web")), Check::WithinBound);
        let map = groups::unit(GroupSubject::Groups, Check::WithinBound);
        for (groups_bound, id_bound, cause) in [(100, 2, id_bytes), (1, 100, map)] {
            let mut inputs = Inputs::new();
            let grouping = &mut inputs.limits.target_grouping;
            grouping.max_deployment_groups = bound(groups_bound);
            grouping.max_group_id_bytes = bound(id_bound);
            inputs.resolve(|evaluation, hydration| {
                let recorded = statuses(&evaluation);
                // The relations' bound, two endpoints' four checks each, the
                // relation's five, and two Target IDs' roles.
                assert_eq!(recorded.len(), 16);
                assert_eq!(recorded[0], (bound_unit(), Status::Evaluated));
                for (unit, _) in &recorded[1..] {
                    assert_eq!(
                        waits_on(&evaluation, unit),
                        Some(vec![cause.clone()]),
                        "{unit:?}"
                    );
                }
                assert!(hydration.unwrap().get(&group("web")).unwrap().is_err());
            });
        }
        // A group's member count is no bound of its relations: only each
        // endpoint's same-group check, which needs the members, waits on it.
        let mut inputs = Inputs::new();
        inputs.limits.target_grouping.max_members_per_group = bound(1);
        inputs.resolve(|evaluation, _| {
            let count = groups::unit(GroupSubject::Members(group("web")), Check::WithinBound);
            for unit in relation_units("ssr", "browser") {
                let waits = (unit.check == Check::SameGroupEndpoint).then(|| vec![count.clone()]);
                assert_eq!(waits_on(&evaluation, &unit), waits, "{unit:?}");
            }
        });
    }

    #[test]
    fn each_check_waits_only_on_the_facts_it_compares() {
        let mut inputs = Inputs::new();
        // Phase 9 finds no Target Profile for `ssr`; its locales still hold.
        inputs.missing_profiles.insert("ssr".into());
        inputs.resolve(|evaluation, hydration| {
            let reference = crate::resolver::exact_resolution::unit(
                crate::resolver::resource_policy::ReferenceField::TargetProfile(target("ssr")),
            );
            for unit in [
                on_endpoint(
                    "web",
                    "ssr",
                    "browser",
                    EndpointField::Server,
                    Check::EndpointRole,
                ),
                relation("web", "ssr", "browser", Check::StaticCompatibility),
            ] {
                assert_eq!(
                    waits_on(&evaluation, &unit),
                    Some(vec![reference.clone()]),
                    "{unit:?}"
                );
            }
            // The client's role reads only the client's Target Profile.
            assert_eq!(
                waits_on(
                    &evaluation,
                    &on_endpoint(
                        "web",
                        "ssr",
                        "browser",
                        EndpointField::Client,
                        Check::EndpointRole
                    )
                ),
                None
            );
            for check in [Check::SameRequestedLocales, Check::SameEffectiveDefault] {
                assert_eq!(
                    waits_on(&evaluation, &relation("web", "ssr", "browser", check)),
                    None
                );
            }
            assert!(hydration.unwrap().complete().is_err());
        });
    }

    #[test]
    fn the_shared_negotiation_profile_is_kept_but_never_compared() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::LocaleNegotiation);
        inputs.negotiation = Err(causes.clone());
        inputs.resolve(|evaluation, hydration| {
            assert_eq!(issues(&evaluation), []);
            assert!(statuses(&evaluation)
                .iter()
                .all(|(_, status)| *status == Status::Evaluated));
            let hydration = hydration.unwrap();
            assert_eq!(hydration.complete(), Ok(()));
            assert_eq!(hydration.negotiation(), &Err(causes));
        });
    }

    #[test]
    fn without_a_confirmed_policy_no_relation_is_checked() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::Coverage);
        inputs.policy = Err(causes.clone());
        inputs.resolve(|evaluation, hydration| {
            let recorded = statuses(&evaluation);
            assert!(!recorded.is_empty());
            for (unit, status) in &recorded {
                assert!(matches!(status, Status::NotEvaluated(_)), "{unit:?}");
            }
            assert_eq!(recorded[0], (bound_unit(), Status::NotEvaluated(causes)));
            assert!(hydration.unwrap().complete().is_err());
        });
    }

    #[test]
    fn a_group_map_structural_admission_did_not_admit_records_only_the_bound() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::DeliveryGraph);
        inputs.declared = Err(causes.clone());
        inputs.resolve(|evaluation, hydration| {
            assert_eq!(
                statuses(&evaluation),
                [(bound_unit(), Status::NotEvaluated(causes.clone()))]
            );
            assert_eq!(hydration, Err(causes));
        });
    }

    #[test]
    fn reordering_relations_changes_nothing_phase_13_records() {
        let mut complete = Inputs::new();
        for id in ["edge", "app"] {
            complete.declare(id, &json!(["en", "ja"]));
        }
        complete.group("web")["members"] = json!(["ssr", "edge", "browser", "app"]);
        complete.roles.renders.insert(profile_identity("edge"));
        complete.roles.hydrates.insert(profile_identity("app"));
        let authored = [
            ("ssr", "browser"),
            ("ssr", "app"),
            ("edge", "browser"),
            ("ssr", "ssr"),
        ];
        complete.relate("web", &authored);
        let mut reversed = authored;
        reversed.reverse();
        let mut reordered = Inputs::new();
        reordered.app = complete.app.clone();
        reordered.roles.renders = complete.roles.renders.clone();
        reordered.roles.hydrates = complete.roles.hydrates.clone();
        reordered.relate("web", &reversed);
        complete.resolve(|first_evaluation, first| {
            reordered.resolve(|second_evaluation, second| {
                assert_eq!(statuses(&first_evaluation), statuses(&second_evaluation));
                assert_eq!(issues(&first_evaluation), issues(&second_evaluation));
                assert_eq!(first, second);
            });
        });
    }

    #[test]
    fn a_relation_with_an_unresolved_target_still_checks_its_endpoints() {
        let mut inputs = Inputs::new();
        // `browser` requests a locale outside the project.
        inputs.app["targetProfiles"]["browser"]["requestedLocales"] = json!(["en", "ja", "de"]);
        inputs.resolve(|evaluation, _| {
            let subset = targets::unit(
                TargetSubject::Target(target("browser")),
                Check::ProjectSubset,
            );
            for unit in &relation_units("ssr", "browser")[..10] {
                assert_eq!(waits_on(&evaluation, unit), None, "{unit:?}");
            }
            assert_eq!(
                waits_on(
                    &evaluation,
                    &relation("web", "ssr", "browser", Check::SameRequestedLocales)
                ),
                Some(vec![subset])
            );
        });
    }
}
