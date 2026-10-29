// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 13 (`hydration-validation`) over each group's explicit hydration
//! relations, `SSR Target ID -> Browser hydration-client Target ID`: every
//! relation occurrence under `maxHydrationRelations` and each endpoint under
//! `maxTargetIdBytes`; each relation's endpoints declared, distinct, and members
//! of its group; each relation once in its group; the server's Target Profile
//! rendering server-side and the client's hydrating in the browser; both
//! endpoints requesting the same locales with the same effective default; and
//! nothing statically known making their Target Profiles incompatible. Within a
//! group, no Target ID takes both roles. A relation is never inferred, and a
//! mismatch is never narrowed to the locales both endpoints share.
//!
//! The roles and the static compatibility are 024's to define, and 024 defines
//! none yet: the resolver asks `TargetCapabilities`, which only tests implement
//! (plan 023 ◆#1). The project's Locale Negotiation Profile is a dependency
//! every relation shares, never compared between endpoints. Not the render
//! equivalence of the two outputs (026, 030), the selection of a group (020),
//! or the Release both outputs join (025).

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{DeploymentGroup, DeploymentGroupId, HydrationRelation, NonEmptyMap, TargetId};

use super::artifact_resolution::AdmittedArtifact;
use super::evaluation::{
    and, both, within, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant,
    Phase, Reason, Subject, Unit,
};
use super::groups::ResolvedGroups;
use super::limits::{BoundId, ConfirmedPolicy, Group};
use super::targets::{ResolvedTargets, TargetFacts};

const MAX_HYDRATION_RELATIONS: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxHydrationRelations",
};

const MAX_TARGET_ID_BYTES: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxTargetIdBytes",
};

/// 024's answers about Target Profiles that 015 asks for: whether one renders
/// server-side or hydrates in the browser, and whether two are statically
/// known to be incompatible. 024 defines none of them yet, so only tests
/// implement this (plan 023 ◆#1).
pub(crate) trait TargetCapabilities {
    /// Whether the Target Profile exposes 024's checked SSR-renderer
    /// capability.
    fn renders_server_side(&self, profile: &AdmittedArtifact<'_>) -> bool;

    /// Whether it exposes 024's checked Browser hydration-client capability.
    fn hydrates_in_browser(&self, profile: &AdmittedArtifact<'_>) -> bool;

    /// Whether the server's and the client's Target Profiles are statically
    /// known to be incompatible, as Targets or as Locale Service profiles.
    fn statically_incompatible(
        &self,
        server: &AdmittedArtifact<'_>,
        client: &AdmittedArtifact<'_>,
    ) -> bool;
}

/// What one Phase 13 unit checks. The bound, which every relation waits on,
/// comes first, then each relation, then each Target ID's roles.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HydrationSubject {
    /// Every relation occurrence of every group, under
    /// `maxHydrationRelations`.
    Relations,
    /// One relation of one group, by its server and client Target IDs.
    Relation(DeploymentGroupId, TargetId, TargetId),
    /// A Target ID's roles among one group's relations.
    Roles(DeploymentGroupId, TargetId),
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

/// What Phase 13 established: each group's checked relations, and the Locale
/// Negotiation Profile every relation shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedHydration {
    relations: BTreeMap<DeploymentGroupId, Admitted<Relations>>,
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

    /// Whether every group's relations were checked and held, or the causes
    /// of each that did not.
    pub(crate) fn complete(&self) -> Admitted<()> {
        let mut holds = Ok(());
        for relations in self.relations.values() {
            holds = and(holds, relations.as_ref().map(|_| ()).map_err(Causes::clone));
        }
        holds
    }
}

/// Phase 13 under the confirmed policy, over the declared groups' relations,
/// the targets Phase 11 resolved, and the groups Phase 12 resolved. Every unit
/// waits on the relations' bound. A relation's endpoint checks come first;
/// its roles, locales, and compatibility are checked only between valid
/// endpoints, and each waits on just the facts it compares.
pub(crate) fn validate_hydration<C: TargetCapabilities>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    declared: Admitted<&NonEmptyMap<DeploymentGroupId, DeploymentGroup>>,
    targets: Result<&ResolvedTargets<'_>, &Causes>,
    groups: Result<&ResolvedGroups, &Causes>,
    negotiation: Admitted<()>,
    capabilities: &C,
) -> Result<Admitted<ResolvedHydration>, EvaluationInvariant> {
    let policy = policy.map_err(Causes::clone);
    let bounded = evaluation.check(
        unit(HydrationSubject::Relations, Check::WithinBound),
        both(policy.clone(), declared.clone()),
        |(policy, declared)| {
            within(
                MAX_HYDRATION_RELATIONS,
                policy.limits().target_grouping.max_hydration_relations,
                declared
                    .values()
                    .map(|group| relations_of(group).len())
                    .sum(),
            )
        },
    )?;
    let declared = match declared {
        Ok(declared) => declared,
        Err(causes) => return Ok(Err(causes)),
    };
    let shared = Shared {
        policy: &policy,
        bounded: &bounded,
        targets: targets.map_err(Causes::clone),
        groups: groups.map_err(Causes::clone),
        capabilities,
    };
    let mut relations = BTreeMap::new();
    for (id, group) in declared.iter() {
        relations.insert(id.clone(), shared.group(evaluation, id, group)?);
    }
    Ok(Ok(ResolvedHydration {
        relations,
        negotiation,
    }))
}

/// A group's submitted relations; omission declares none.
fn relations_of(group: &DeploymentGroup) -> &[HydrationRelation] {
    group
        .hydration_relations
        .as_option()
        .map_or(&[], Vec::as_slice)
}

/// What every relation's checks share.
struct Shared<'a, 'p, 't, 's, C> {
    policy: &'a Admitted<&'p ConfirmedPolicy>,
    bounded: &'a Admitted<()>,
    targets: Admitted<&'t ResolvedTargets<'s>>,
    groups: Admitted<&'t ResolvedGroups>,
    capabilities: &'a C,
}

impl<'t, 's, C: TargetCapabilities> Shared<'_, '_, 't, 's, C> {
    /// One group's relations, each pair once however often submitted, and
    /// the roles of each Target ID among them.
    fn group(
        &self,
        evaluation: &mut Evaluation,
        id: &DeploymentGroupId,
        group: &DeploymentGroup,
    ) -> Result<Admitted<Relations>, EvaluationInvariant> {
        let mut occurrences: BTreeMap<(&TargetId, &TargetId), usize> = BTreeMap::new();
        for relation in relations_of(group) {
            *occurrences
                .entry((&relation.server, &relation.client))
                .or_default() += 1;
        }
        let members = self.groups.clone().and_then(|groups| {
            groups
                .get(id)
                .expect("Phase 12 resolves every declared group")
                .as_ref()
                .map_err(Causes::clone)
        });
        let mut holds = Ok(());
        let mut endpoint_bytes = BTreeMap::new();
        for (&(server, client), &count) in &occurrences {
            let relation = Relation { id, server, client };
            let (bytes, checked) = relation.check(evaluation, self, &members, count)?;
            for endpoint in [server, client] {
                let bound = endpoint_bytes.entry(endpoint).or_insert(Ok(()));
                *bound = and(bound.clone(), bytes.clone());
            }
            holds = and(holds, checked);
        }
        holds = and(
            holds,
            self.roles(evaluation, id, &occurrences, &endpoint_bytes)?,
        );
        Ok(holds.map(|()| {
            occurrences
                .into_keys()
                .map(|(server, client)| (server.clone(), client.clone()))
                .collect()
        }))
    }

    /// Each Target ID that serves or hydrates between two distinct endpoints
    /// takes one role in its group; an equal-endpoint relation is its own
    /// failure and counts here for neither role.
    fn roles(
        &self,
        evaluation: &mut Evaluation,
        id: &DeploymentGroupId,
        occurrences: &BTreeMap<(&TargetId, &TargetId), usize>,
        endpoint_bytes: &BTreeMap<&TargetId, Admitted<()>>,
    ) -> Result<Admitted<()>, EvaluationInvariant> {
        let mut servers = BTreeSet::new();
        let mut clients = BTreeSet::new();
        for &(server, client) in occurrences.keys() {
            if server != client {
                servers.insert(server);
                clients.insert(client);
            }
        }
        let mut holds = Ok(());
        for &target in servers.union(&clients) {
            let single = evaluation.check(
                unit(
                    HydrationSubject::Roles(id.clone(), target.clone()),
                    Check::SingleRole,
                ),
                endpoint_bytes
                    .get(target)
                    .cloned()
                    .unwrap_or_else(|| self.bounded.clone()),
                |()| {
                    if servers.contains(target) && clients.contains(target) {
                        Err(invalid(Reason::DualRole))
                    } else {
                        Ok(())
                    }
                },
            )?;
            holds = and(holds, single);
        }
        Ok(holds)
    }

    /// What Phase 11 established of a declared endpoint.
    fn facts(&self, id: &TargetId) -> Admitted<&'t TargetFacts<'s>> {
        let targets: &'t ResolvedTargets<'s> = self.targets.clone()?;
        Ok(targets
            .get(id)
            .expect("Phase 13 reads only the endpoints it found declared"))
    }
}

/// One relation's checks.
struct Relation<'a> {
    id: &'a DeploymentGroupId,
    server: &'a TargetId,
    client: &'a TargetId,
}

impl Relation<'_> {
    fn unit(&self, check: Check) -> Unit {
        unit(
            HydrationSubject::Relation(self.id.clone(), self.server.clone(), self.client.clone()),
            check,
        )
    }

    /// The endpoints' bytes, which the roles also need, and whether every
    /// check of the relation held.
    fn check<C: TargetCapabilities>(
        &self,
        evaluation: &mut Evaluation,
        shared: &Shared<'_, '_, '_, '_, C>,
        members: &Admitted<&BTreeSet<TargetId>>,
        count: usize,
    ) -> Result<(Admitted<()>, Admitted<()>), EvaluationInvariant> {
        let (server, client) = (self.server, self.client);
        let bytes = evaluation.check(
            self.unit(Check::WithinBound),
            both(shared.policy.clone(), shared.bounded.clone()),
            |(policy, ())| {
                let limit = policy.limits().target_grouping.max_target_id_bytes;
                within(MAX_TARGET_ID_BYTES, limit, server.as_str().len())?;
                within(MAX_TARGET_ID_BYTES, limit, client.as_str().len())
            },
        )?;
        let known = shared
            .targets
            .clone()
            .and_then(|targets| targets.bounded().clone().map(|()| targets));
        let declared = evaluation.check(
            self.unit(Check::KnownEndpoints),
            both(bytes.clone(), known),
            |((), targets)| {
                if targets.get(server).is_some() && targets.get(client).is_some() {
                    Ok(())
                } else {
                    Err(invalid(Reason::UnknownEndpoint))
                }
            },
        )?;
        let distinct =
            evaluation.check(self.unit(Check::DistinctEndpoints), bytes.clone(), |()| {
                if server == client {
                    Err(invalid(Reason::EqualEndpoint))
                } else {
                    Ok(())
                }
            })?;
        let same_group = evaluation.check(
            self.unit(Check::SameGroupEndpoints),
            both(declared.clone(), members.clone()),
            |((), members)| {
                if members.contains(server) && members.contains(client) {
                    Ok(())
                } else {
                    Err(invalid(Reason::CrossGroup))
                }
            },
        )?;
        let once = evaluation.check(self.unit(Check::UniqueRelation), bytes.clone(), |()| {
            if count == 1 {
                Ok(())
            } else {
                Err(invalid(Reason::DuplicateRelation))
            }
        })?;
        let endpoints = and(and(declared, distinct), same_group);
        let facts = endpoints
            .clone()
            .and_then(|()| both(shared.facts(server), shared.facts(client)));
        let profiles = facts
            .clone()
            .and_then(|(server, client)| both(server.profile.clone(), client.profile.clone()));
        let capabilities = shared.capabilities;
        let serves = evaluation.check(
            self.unit(Check::ServerRole),
            profiles.clone().map(|(server, _)| server),
            |(_, profile)| {
                if capabilities.renders_server_side(profile) {
                    Ok(())
                } else {
                    Err(invalid(Reason::ServerCapabilityMissing))
                }
            },
        )?;
        let hydrates = evaluation.check(
            self.unit(Check::ClientRole),
            profiles.clone().map(|(_, client)| client),
            |(_, profile)| {
                if capabilities.hydrates_in_browser(profile) {
                    Ok(())
                } else {
                    Err(invalid(Reason::ClientCapabilityMissing))
                }
            },
        )?;
        let requested = evaluation.check(
            self.unit(Check::SameRequestedLocales),
            facts.clone().and_then(|(server, client)| {
                both(server.requested.clone(), client.requested.clone())
            }),
            |(server, client)| {
                if server == client {
                    Ok(())
                } else {
                    Err(locale_policy(Reason::RequestedLocales))
                }
            },
        )?;
        let default = evaluation.check(
            self.unit(Check::SameEffectiveDefault),
            facts.and_then(|(server, client)| {
                both(
                    server.effective_default.clone(),
                    client.effective_default.clone(),
                )
            }),
            |(server, client)| {
                if server == client {
                    Ok(())
                } else {
                    Err(locale_policy(Reason::EffectiveDefault))
                }
            },
        )?;
        let compatible = evaluation.check(
            self.unit(Check::StaticCompatibility),
            both(and(serves.clone(), hydrates.clone()), profiles),
            |((), ((_, server), (_, client)))| {
                if capabilities.statically_incompatible(server, client) {
                    Err(Blocking {
                        code: Code::HydrationCapabilityIncompatible,
                        reason: None,
                        details: None,
                    })
                } else {
                    Ok(())
                }
            },
        )?;
        let checked = and(
            and(and(endpoints, once), and(serves, hydrates)),
            and(and(requested, default), compatible),
        );
        Ok((bytes, checked))
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
    use std::collections::BTreeSet;

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
    use crate::resolver::groups::{self, resolve_groups, GroupSubject};
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

    /// The identity of the Target Profile the target `id` names.
    fn profile_identity(id: &str) -> String {
        format!("storefront-{id}")
    }

    fn profile_of(id: &str) -> Value {
        envelope_of(
            ArtifactKind::TargetProfile,
            &profile_identity(id),
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

    /// 024's answers as a test states them, by Target Profile identity.
    #[derive(Default)]
    struct Roles {
        renders: BTreeSet<String>,
        hydrates: BTreeSet<String>,
        incompatible: BTreeSet<(String, String)>,
    }

    impl Roles {
        fn identity(profile: &AdmittedArtifact<'_>) -> String {
            profile
                .member
                .envelope()
                .unwrap()
                .tuple()
                .identity
                .as_str()
                .to_owned()
        }
    }

    impl TargetCapabilities for Roles {
        fn renders_server_side(&self, profile: &AdmittedArtifact<'_>) -> bool {
            self.renders.contains(&Self::identity(profile))
        }

        fn hydrates_in_browser(&self, profile: &AdmittedArtifact<'_>) -> bool {
            self.hydrates.contains(&Self::identity(profile))
        }

        fn statically_incompatible(
            &self,
            server: &AdmittedArtifact<'_>,
            client: &AdmittedArtifact<'_>,
        ) -> bool {
            self.incompatible
                .contains(&(Self::identity(server), Self::identity(client)))
        }
    }

    /// What Phase 13 reads, which a test edits: the declaration, the Target
    /// Profiles submitted, 024's answers, the policy's bounds, and whether a
    /// policy was confirmed, structural admission admitted the groups, and
    /// Phase 8 admitted the Locale Negotiation Profile.
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
        /// Target Profile.
        fn declare(&mut self, id: &str, requested: &Value) {
            self.app["targetProfiles"][id] = json!({
                "profile": reference_value(&tuple_of(&profile_of(id))),
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
        fn resolve<R>(
            &self,
            inspect: impl FnOnce(Evaluation, Admitted<ResolvedHydration>) -> R,
        ) -> R {
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
                .filter(|(id, _)| !self.missing_profiles.contains(id.as_str()))
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
            let groups = resolve_groups(
                &mut evaluation,
                policy,
                Ok(&declaration.deployment_groups),
                targets.as_ref(),
            )
            .unwrap();
            let declared = self
                .declared
                .clone()
                .map(|()| &declaration.deployment_groups);
            let hydration = validate_hydration(
                &mut evaluation,
                policy,
                declared,
                targets.as_ref(),
                groups.as_ref(),
                self.negotiation.clone(),
                &self.roles,
            )
            .unwrap();
            inspect(evaluation, hydration)
        }
    }

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    fn relation(group_id: &str, server: &str, client: &str, check: Check) -> Unit {
        unit(
            HydrationSubject::Relation(group(group_id), target(server), target(client)),
            check,
        )
    }

    fn roles(group_id: &str, id: &str) -> Unit {
        unit(
            HydrationSubject::Roles(group(group_id), target(id)),
            Check::SingleRole,
        )
    }

    fn bound_unit() -> Unit {
        unit(HydrationSubject::Relations, Check::WithinBound)
    }

    /// Every check of one relation, in rank order.
    const CHECKS: [Check; 10] = [
        Check::WithinBound,
        Check::KnownEndpoints,
        Check::DistinctEndpoints,
        Check::SameGroupEndpoints,
        Check::UniqueRelation,
        Check::ServerRole,
        Check::ClientRole,
        Check::SameRequestedLocales,
        Check::SameEffectiveDefault,
        Check::StaticCompatibility,
    ];

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
    fn subjects_order_the_bound_then_relations_then_roles_after_every_phase_12_unit() {
        let mut ordered = vec![bound_unit()];
        ordered.extend(CHECKS.map(|check| relation("web", "ssr", "browser", check)));
        ordered.push(relation("web", "ssr", "worker", Check::WithinBound));
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
            expected.extend(CHECKS.map(|check| relation("web", "ssr", "browser", check)));
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
                equal,
                "project-profile-hydration-relation-invalid",
                Some("equal-endpoint")
            )));
            assert!(found.contains(&(
                repeated,
                "project-profile-hydration-relation-invalid",
                Some("duplicate-relation")
            )));
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
            let unknown = relation("web", "ssr", "phone", Check::KnownEndpoints);
            let cross = relation("web", "ssr", "worker", Check::SameGroupEndpoints);
            assert_eq!(
                issues(&evaluation),
                [
                    (
                        unknown.clone(),
                        "project-profile-hydration-relation-invalid",
                        Some("unknown-endpoint")
                    ),
                    (
                        cross.clone(),
                        "project-profile-hydration-relation-invalid",
                        Some("cross-group")
                    ),
                ]
            );
            // Between invalid endpoints, nothing else is compared.
            for (server, client, cause) in [("ssr", "phone", unknown), ("ssr", "worker", cross)] {
                for check in &CHECKS[5..] {
                    assert_eq!(
                        waits_on(&evaluation, &relation("web", server, client, *check)),
                        Some(vec![cause.clone()]),
                        "{server} {client} {check:?}"
                    );
                }
            }
            assert!(hydration.unwrap().complete().is_err());
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
    fn an_endpoint_naming_a_target_waits_on_the_target_map_phase_11_bounds() {
        let mut inputs = Inputs::new();
        // Three targets are declared.
        inputs.limits.target_grouping.max_target_profiles = bound(2);
        inputs.resolve(|evaluation, _| {
            let map = targets::unit(TargetSubject::Targets, Check::WithinBound);
            assert_eq!(
                waits_on(
                    &evaluation,
                    &relation("web", "ssr", "browser", Check::KnownEndpoints)
                ),
                Some(vec![map])
            );
            for check in [
                Check::WithinBound,
                Check::DistinctEndpoints,
                Check::UniqueRelation,
            ] {
                assert_eq!(
                    waits_on(&evaluation, &relation("web", "ssr", "browser", check)),
                    None
                );
            }
        });
    }

    #[test]
    fn the_roles_come_from_024_through_the_seam() {
        let mut inputs = Inputs::new();
        inputs.roles = Roles::default();
        inputs.resolve(|evaluation, _| {
            assert_eq!(
                issues(&evaluation),
                [
                    (
                        relation("web", "ssr", "browser", Check::ServerRole),
                        "project-profile-hydration-relation-invalid",
                        Some("server-capability-missing")
                    ),
                    (
                        relation("web", "ssr", "browser", Check::ClientRole),
                        "project-profile-hydration-relation-invalid",
                        Some("client-capability-missing")
                    ),
                ]
            );
            // Compatibility is a question only between the two roles.
            assert_eq!(
                waits_on(
                    &evaluation,
                    &relation("web", "ssr", "browser", Check::StaticCompatibility)
                ),
                Some(vec![
                    relation("web", "ssr", "browser", Check::ServerRole),
                    relation("web", "ssr", "browser", Check::ClientRole),
                ])
            );
        });
    }

    #[test]
    fn a_statically_known_incompatibility_blocks_the_relation() {
        let mut inputs = Inputs::new();
        inputs
            .roles
            .incompatible
            .insert((profile_identity("ssr"), profile_identity("browser")));
        inputs.resolve(|evaluation, hydration| {
            assert_eq!(
                issues(&evaluation),
                [(
                    relation("web", "ssr", "browser", Check::StaticCompatibility),
                    "project-profile-hydration-capability-incompatible",
                    None
                )]
            );
            assert!(hydration.unwrap().complete().is_err());
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
    fn each_endpoint_holds_at_the_target_id_bound_and_one_byte_more_blocks_its_relation() {
        // `browser` is seven bytes and `ssr` three.
        for (limit, over) in [(7, false), (6, true)] {
            let mut inputs = Inputs::new();
            inputs.limits.target_grouping.max_target_id_bytes = bound(limit);
            inputs.resolve(|evaluation, _| {
                let bytes = relation("web", "ssr", "browser", Check::WithinBound);
                let found: Vec<Unit> = issues(&evaluation)
                    .into_iter()
                    .map(|(unit, _, _)| unit)
                    .collect();
                if !over {
                    assert_eq!(found, Vec::<Unit>::new());
                    return;
                }
                assert_eq!(found, std::slice::from_ref(&bytes));
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
                for check in [Check::DistinctEndpoints, Check::UniqueRelation] {
                    assert_eq!(
                        waits_on(&evaluation, &relation("web", "ssr", "browser", check)),
                        Some(vec![bytes.clone()])
                    );
                }
                assert_eq!(
                    waits_on(&evaluation, &roles("web", "ssr")),
                    Some(vec![bytes.clone()])
                );
            });
        }
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
            for check in [
                Check::ServerRole,
                Check::ClientRole,
                Check::StaticCompatibility,
            ] {
                assert_eq!(
                    waits_on(&evaluation, &relation("web", "ssr", "browser", check)),
                    Some(vec![reference.clone()]),
                    "{check:?}"
                );
            }
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
            for check in &CHECKS[..7] {
                assert_eq!(
                    waits_on(&evaluation, &relation("web", "ssr", "browser", *check)),
                    None,
                    "{check:?}"
                );
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
