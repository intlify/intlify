// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What 015 hands 020 for Group ID selection (D15:2085, D15:2720): every
//! Deployment Compatibility Group with each member's checked supported
//! requested-locale subset and effective default, and the Resolved Profile
//! Artifact Bindings that planning one group consumes, its members' Target
//! Profiles and the Resource Limit Policy. Neither selects a group. 020 admits
//! the selector, permitting omission only when the profile has one group, and
//! the selector and the group it selects never enter the profile's state
//! (D15:3547). The conformance suite's Group ID selection Handoff Vectors
//! (`conformance/cases/handoff/`) hand 020 the same facts as the checked
//! profile view's `deploymentGroups` and `targets`, with the Target Profiles
//! `targets` references; 020 verifies what each selector selects.
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
/// D15:2720): each member's Target Profile and the selected Resource Limit
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
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;
    use crate::resolver::profile_resolution::tests::Inputs;

    fn group(id: &str) -> DeploymentGroupId {
        serde_json::from_value(json!(id)).unwrap()
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
