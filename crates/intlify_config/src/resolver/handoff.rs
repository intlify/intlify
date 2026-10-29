// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What 015 hands 020 for Group ID selection (D15:1861, D15:2415): every
//! Deployment Compatibility Group with each member's checked supported
//! requested-locale subset and effective default, and the Resolved Profile
//! Artifact Bindings that planning one group consumes, its members' Target
//! Profiles and the Resource Limit Policy. Neither selects a group. 020 admits
//! the selector, permitting omission only when the profile has one group, and
//! the selector and the group it selects never enter the profile's state
//! (D15:3085). The Handoff Vectors in `fixtures/handoff/` fix both projections
//! against the resolver; 020 verifies what each selector selects.
//!
//! A profile whose targets did not all resolve, or whose groups do not
//! partition them, hands over no group. Until 015's Implementation Phase 5
//! checks the profile itself, these read the state's targets, groups,
//! references, and bindings.

use std::collections::BTreeSet;

use crate::artifacts::envelope::ExactTuple;
use crate::locale::CanonicalLocale;
use crate::model::{DeploymentGroupId, TargetId};

use super::evaluation::{and, both, Admitted, Causes};
use super::groups::ResolvedGroups;
use super::locales::LocaleSet;
use super::profile_resolution::SemanticState;
use super::resource_policy::{PolicySlot, ReferenceField};
use super::targets::ResolvedTargets;

/// One member as planning reads it (D20:112).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberFacts<'a> {
    pub(crate) id: &'a TargetId,
    /// Its checked supported requested-locale subset.
    pub(crate) requested: &'a LocaleSet,
    pub(crate) effective_default: &'a CanonicalLocale,
}

/// One group with its members in Target ID order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupFacts<'a> {
    pub(crate) id: &'a DeploymentGroupId,
    pub(crate) members: Vec<MemberFacts<'a>>,
}

/// The targets and the groups of a profile that hands its groups over: every
/// target resolved, and the groups partitioning them. Otherwise not even a
/// group whose own members resolved is handed over.
fn partitioned<'a, 's, Id>(
    state: &'a SemanticState<'s, Id>,
) -> Admitted<(&'a ResolvedTargets<'s>, &'a ResolvedGroups)> {
    let (targets, groups) = both(
        state.targets.as_ref().map_err(Causes::clone),
        state.groups.as_ref().map_err(Causes::clone),
    )?;
    and(targets.complete(), groups.partition().clone())?;
    Ok((targets, groups))
}

/// Every group in Group ID order, or the causes of what keeps the profile
/// from handing any over.
pub(crate) fn group_facts<'a, Id>(
    state: &'a SemanticState<'_, Id>,
) -> Admitted<Vec<GroupFacts<'a>>> {
    let (targets, groups) = partitioned(state)?;
    groups
        .iter()
        .map(|(id, members)| {
            let members = members.as_ref().map_err(Causes::clone)?;
            // The partition holds, so every member is a resolved target.
            let members = targets
                .iter()
                .filter(|(target, _)| members.contains(*target))
                .map(|(target, facts)| {
                    let resolved = facts.resolved.as_ref().map_err(Causes::clone)?;
                    Ok(MemberFacts {
                        id: target,
                        requested: &resolved.requested,
                        effective_default: &resolved.effective_default,
                    })
                })
                .collect::<Admitted<Vec<_>>>()?;
            Ok(GroupFacts { id, members })
        })
        .collect()
}

/// The bindings planning `group` consumes (015's Requirement planner row,
/// D15:2415): each member's Target Profile and the selected Resource Limit
/// Policy, in the tuples' order, a tuple several references share once.
/// `None` when the profile has no such group.
pub(crate) fn planning_bindings<'s, Id>(
    state: &SemanticState<'s, Id>,
    group: &DeploymentGroupId,
) -> Admitted<Option<Vec<&'s ExactTuple>>> {
    let ((_, groups), (references, bindings)) = both(
        partitioned(state),
        both(
            state.references.as_ref().map_err(Causes::clone),
            state.bindings.as_ref().map_err(Causes::clone),
        ),
    )?;
    let Some(members) = groups.get(group) else {
        return Ok(None);
    };
    let members = members.as_ref().map_err(Causes::clone)?;
    let consumed: BTreeSet<&ExactTuple> = references
        .iter()
        .filter(|(field, _, _)| match field {
            ReferenceField::Policy(slot) => *slot == PolicySlot::ResourceLimits,
            ReferenceField::TargetProfile(target) => members.contains(target),
        })
        .map(|(_, tuple, _)| tuple)
        .collect();
    Ok(Some(
        bindings
            .iter()
            .map(|(tuple, _)| tuple)
            .filter(|tuple| consumed.contains(tuple))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use serde_json::{json, Value};

    use super::*;
    use crate::resolver::profile_resolution::tests::Inputs;

    /// The committed Handoff Vectors, relative to the crate.
    const VECTOR_FILE: &str = "fixtures/handoff/group-selection.json";

    /// The decisions every vector rests on: one complete group per
    /// transaction, 020's ownership of the selector, and the canonical order.
    const DECISIONS: [&str; 3] = ["015-034", "015-044", "015-049"];

    /// What 020 must conclude from a vector's selectors.
    #[derive(Debug, Clone, Copy)]
    enum Relationship {
        /// This group, with every member planned.
        Selected(&'static str),
        /// No group, by the rule 015 states (D15:1861).
        Rejected(&'static str),
    }

    /// One vector: the profile, what the transaction submits, and what 020
    /// must conclude.
    struct Vector {
        name: &'static str,
        two_groups: bool,
        group_ids: &'static [&'static str],
        target_ids: Option<&'static [&'static str]>,
        relationship: Relationship,
    }

    const fn vector(
        name: &'static str,
        two_groups: bool,
        group_ids: &'static [&'static str],
        target_ids: Option<&'static [&'static str]>,
        relationship: Relationship,
    ) -> Vector {
        Vector {
            name,
            two_groups,
            group_ids,
            target_ids,
            relationship,
        }
    }

    /// Each of the six selector forms over a profile with one group and over
    /// one with two (D15:1861, D20:110).
    const VECTORS: [Vector; 12] = {
        use Relationship::{Rejected, Selected};
        [
            vector("one-group/omitted", false, &[], None, Selected("web")),
            vector("one-group/group-id", false, &["web"], None, Selected("web")),
            vector(
                "one-group/unknown-group-id",
                false,
                &["edge"],
                None,
                Rejected("unknown-group-id"),
            ),
            vector(
                "one-group/repeated-group-id",
                false,
                &["web", "web"],
                None,
                Rejected("multiple-selectors"),
            ),
            vector(
                "one-group/target-id",
                false,
                &["browser"],
                None,
                Rejected("target-id-selector"),
            ),
            vector(
                "one-group/group-subset",
                false,
                &["web"],
                Some(&["browser"]),
                Rejected("group-subset"),
            ),
            vector(
                "two-groups/omitted",
                true,
                &[],
                None,
                Rejected("selector-required"),
            ),
            vector(
                "two-groups/group-id",
                true,
                &["jobs"],
                None,
                Selected("jobs"),
            ),
            vector(
                "two-groups/unknown-group-id",
                true,
                &["edge"],
                None,
                Rejected("unknown-group-id"),
            ),
            vector(
                "two-groups/two-group-ids",
                true,
                &["web", "jobs"],
                None,
                Rejected("multiple-selectors"),
            ),
            vector(
                "two-groups/target-id",
                true,
                &["worker"],
                None,
                Rejected("target-id-selector"),
            ),
            vector(
                "two-groups/group-subset",
                true,
                &["web"],
                Some(&["server"]),
                Rejected("group-subset"),
            ),
        ]
    };

    fn group(id: &str) -> DeploymentGroupId {
        serde_json::from_value(json!(id)).unwrap()
    }

    fn tuple(tuple: &ExactTuple) -> Value {
        json!({
            "kind": tuple.kind.spelling(),
            "identity": tuple.identity.as_str(),
            "revision": tuple.revision.as_str(),
            "specificationRevision": tuple.specification_revision.as_str(),
            "semanticDigest": tuple.semantic_digest.as_str(),
        })
    }

    /// The producer's side of one profile: its facts, each group's members,
    /// and each group's planning bindings.
    struct Projection {
        facts: Value,
        members: BTreeMap<String, Vec<String>>,
        bindings: BTreeMap<String, Value>,
    }

    impl Projection {
        fn of<Id>(state: &SemanticState<'_, Id>) -> Self {
            let groups = group_facts(state).unwrap();
            let facts = json!({
                "deploymentGroups": groups
                    .iter()
                    .map(|group| json!({
                        "groupId": group.id.as_str(),
                        "members": group
                            .members
                            .iter()
                            .map(|member| json!({
                                "targetId": member.id.as_str(),
                                "requestedLocales": member
                                    .requested
                                    .as_slice()
                                    .iter()
                                    .map(CanonicalLocale::as_str)
                                    .collect::<Vec<_>>(),
                                "effectiveDefaultRequestedLocale": member.effective_default.as_str(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            });
            let members = groups
                .iter()
                .map(|group| {
                    let ids = group
                        .members
                        .iter()
                        .map(|member| member.id.as_str().to_owned());
                    (group.id.as_str().to_owned(), ids.collect())
                })
                .collect();
            let bindings = groups
                .iter()
                .map(|group| {
                    let consumed = planning_bindings(state, group.id).unwrap().unwrap();
                    let consumed = consumed.into_iter().map(tuple).collect();
                    (group.id.as_str().to_owned(), Value::Array(consumed))
                })
                .collect();
            Self {
                facts,
                members,
                bindings,
            }
        }
    }

    /// Every vector, with the producer's fields taken from the resolver over
    /// `Inputs::complete` (one group) and `Inputs::two_groups`.
    fn document() -> Value {
        let one = Inputs::complete().resolve(|_, state| Projection::of(&state));
        let two = Inputs::two_groups().resolve(|_, state| Projection::of(&state));
        let vectors = VECTORS.iter().map(|vector| {
            let profile = if vector.two_groups { &two } else { &one };
            let (bindings, relationship) = match vector.relationship {
                Relationship::Selected(group) => (
                    profile.bindings[group].clone(),
                    json!({
                        "outcome": "selected-group",
                        "groupId": group,
                        "plannedTargetIds": profile.members[group],
                    }),
                ),
                // No group is selected, so nothing is projected for planning.
                Relationship::Rejected(rule) => {
                    (Value::Null, json!({"outcome": "rejected", "rule": rule}))
                }
            };
            json!({
                "suiteRevision": "0",
                "manifestKind": "handoff-vector",
                "vectorId": format!("group-selection/{}", vector.name),
                "producerProfileFacts": profile.facts,
                "producerArtifactBindings": bindings,
                "consumerInputs": {
                    "groupIdSelectors": vector.group_ids,
                    "targetIdSelectors": vector.target_ids,
                },
                "expectedRelationship": relationship,
                "verificationOwner": "020",
                "relatedDecisionIds": DECISIONS,
                "relatedRuleIds": [],
            })
        });
        Value::Array(vectors.collect())
    }

    fn committed() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTOR_FILE);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The identities of the bindings planning group `id` consumes, in order.
    fn consumed(state: &SemanticState<'_, &'static str>, id: &str) -> Vec<String> {
        planning_bindings(state, &group(id))
            .unwrap()
            .unwrap()
            .into_iter()
            .map(|tuple| tuple.identity.as_str().to_owned())
            .collect()
    }

    #[test]
    fn the_committed_vectors_are_the_resolvers() {
        // Equal decoded values are equal canonical bytes; the repository
        // formatter owns the file's layout.
        assert_eq!(committed(), document());
    }

    #[test]
    #[ignore = "prints the vectors to review before committing them"]
    fn print_vectors_for_review() {
        println!("{}", serde_json::to_string_pretty(&document()).unwrap());
    }

    #[test]
    fn every_vector_has_015s_fields_and_020_verifies_it() {
        let committed = committed();
        let vectors = committed.as_array().unwrap();
        assert_eq!(vectors.len(), VECTORS.len());
        let mut ids = BTreeSet::new();
        for vector in vectors {
            let fields: Vec<&str> = vector
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(
                fields,
                [
                    "suiteRevision",
                    "manifestKind",
                    "vectorId",
                    "producerProfileFacts",
                    "producerArtifactBindings",
                    "consumerInputs",
                    "expectedRelationship",
                    "verificationOwner",
                    "relatedDecisionIds",
                    "relatedRuleIds",
                ]
            );
            assert_eq!(vector["suiteRevision"], "0");
            assert_eq!(vector["manifestKind"], "handoff-vector");
            assert_eq!(vector["verificationOwner"], "020");
            assert!(ids.insert(vector["vectorId"].as_str().unwrap()));
            // Only a selected group has bindings to plan with.
            let selected = vector["expectedRelationship"]["outcome"] == "selected-group";
            assert_eq!(!vector["producerArtifactBindings"].is_null(), selected);
        }
    }

    #[test]
    fn no_selector_changes_what_the_profile_hands_over() {
        let document = document();
        let vectors = document.as_array().unwrap();
        for profile in ["one-group/", "two-groups/"] {
            let facts: BTreeSet<String> = vectors
                .iter()
                .filter(|vector| {
                    let id = vector["vectorId"].as_str().unwrap();
                    id.starts_with(&format!("group-selection/{profile}"))
                })
                .map(|vector| vector["producerProfileFacts"].to_string())
                .collect();
            assert_eq!(facts.len(), 1, "{profile}");
        }
        // Omitting the selector of a one-group profile and naming its group
        // hand over the same bindings.
        assert_eq!(
            vectors[0]["producerArtifactBindings"],
            vectors[1]["producerArtifactBindings"]
        );
    }

    /// A member's Target ID, requested locales, and effective default.
    type Member<'a> = (&'a str, Vec<&'a str>, &'a str);

    #[test]
    fn each_group_hands_over_its_members_checked_locale_facts() {
        Inputs::two_groups().resolve(|_, state| {
            let groups = group_facts(&state).unwrap();
            let handed: Vec<(&str, Vec<Member<'_>>)> = groups
                .iter()
                .map(|group| {
                    let members = group.members.iter().map(|member| {
                        let requested = member.requested.as_slice().iter();
                        (
                            member.id.as_str(),
                            requested.map(CanonicalLocale::as_str).collect(),
                            member.effective_default.as_str(),
                        )
                    });
                    (group.id.as_str(), members.collect())
                })
                .collect();
            assert_eq!(
                handed,
                [
                    ("jobs", vec![("worker", vec!["ja"], "ja")]),
                    (
                        "web",
                        vec![
                            ("browser", vec!["en", "ja"], "en"),
                            ("server", vec!["en", "ja"], "en"),
                        ]
                    ),
                ]
            );
        });
    }

    #[test]
    fn planning_a_group_consumes_its_members_target_profiles_and_the_resource_limits() {
        Inputs::two_groups().resolve(|_, state| {
            let web = consumed(&state, "web");
            let jobs = consumed(&state, "jobs");
            assert_eq!(
                web.iter().map(String::as_str).collect::<BTreeSet<_>>(),
                BTreeSet::from([
                    "storefront-browser",
                    "storefront-limits",
                    "storefront-server"
                ])
            );
            assert_eq!(
                jobs.iter().map(String::as_str).collect::<BTreeSet<_>>(),
                BTreeSet::from(["storefront-limits", "storefront-worker"])
            );
            // In the bindings' order, and no other policy.
            for planned in [web, jobs] {
                let order: Vec<_> = state
                    .bindings
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|(tuple, _)| tuple.identity.as_str().to_owned())
                    .filter(|identity| planned.contains(identity))
                    .collect();
                assert_eq!(order, planned);
            }
            assert_eq!(planning_bindings(&state, &group("edge")), Ok(None));
        });
    }

    #[test]
    fn a_profile_whose_targets_do_not_all_resolve_hands_over_no_group() {
        let mut inputs = Inputs::two_groups();
        // `worker` requests a locale the project does not.
        let worker = &mut inputs.declaration()["targetProfiles"]["worker"];
        worker["requestedLocales"] = json!(["de"]);
        worker["defaultRequestedLocale"] = json!("de");
        inputs.resolve(|_, state| {
            let causes = group_facts(&state).unwrap_err();
            // Not even `web`, whose own members resolved.
            assert_eq!(planning_bindings(&state, &group("web")), Err(causes));
        });
    }

    #[test]
    fn a_profile_whose_groups_do_not_partition_its_targets_hands_over_no_group() {
        let mut inputs = Inputs::two_groups();
        // `server` is in no group.
        inputs.declaration()["deploymentGroups"]["web"]["members"] = json!(["browser"]);
        inputs.declaration()["deploymentGroups"]["web"]["hydrationRelations"] = json!([]);
        inputs.resolve(|_, state| {
            let causes = group_facts(&state).unwrap_err();
            assert_eq!(planning_bindings(&state, &group("jobs")), Err(causes));
        });
    }

    #[test]
    fn reordering_the_profile_changes_nothing_it_hands_over() {
        let inputs = Inputs::two_groups();
        inputs.resolve(|_, first| {
            inputs.reordered().resolve(|_, second| {
                assert_eq!(group_facts(&first), group_facts(&second));
                for id in ["jobs", "web"] {
                    assert_eq!(consumed(&first, id), consumed(&second, id));
                }
            });
        });
    }
}
