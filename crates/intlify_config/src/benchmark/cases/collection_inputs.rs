// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite Phase 9 inputs: collections that submit the policy the confirmed
//! bounds come from beside members no reference needs. Every member a recipe
//! does not break on purpose is an admitted envelope, so each blocked recipe
//! names the one failure it adds.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::artifacts::content::sample;
use crate::artifacts::envelope::{envelope_of, sealed, ArtifactKind};

use super::policy_inputs::{capacity, selected_policy};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum ArtifactCollectionRecipe {
    PolicyOnly,
    Mixed,
    MixedReordered,
    EveryKind,
    ManyArtifacts,
    UnreferencedDuplicate,
    RepeatedBodies,
    ArtifactConflict,
    ManyConflicts,
    EnvelopeInvalid,
    NoTuple,
    KindUnsupported,
}

/// The number of Target Profiles `ManyArtifacts` submits besides the policy,
/// of copies `RepeatedBodies` submits of one, and of conflicting pairs
/// `ManyConflicts` submits.
const SCALE: usize = 63;
const REPEATS: usize = 32;
const CONFLICTS: usize = 32;

impl ArtifactCollectionRecipe {
    pub(super) const ALL: [Self; 12] = [
        Self::PolicyOnly,
        Self::Mixed,
        Self::MixedReordered,
        Self::EveryKind,
        Self::ManyArtifacts,
        Self::UnreferencedDuplicate,
        Self::RepeatedBodies,
        Self::ArtifactConflict,
        Self::ManyConflicts,
        Self::EnvelopeInvalid,
        Self::NoTuple,
        Self::KindUnsupported,
    ];

    /// Whether Phase 9 admits the collection into a Set.
    pub(in crate::benchmark) const fn admits(self) -> bool {
        matches!(
            self,
            Self::PolicyOnly
                | Self::Mixed
                | Self::MixedReordered
                | Self::EveryKind
                | Self::ManyArtifacts
        )
    }

    /// The members, in submission order. Each collection submits the policy
    /// the confirmed bounds come from, as Phase 4 found it.
    pub(in crate::benchmark) fn members(self) -> Vec<Value> {
        let policy = selected_policy(&capacity());
        let mut members = match self {
            Self::PolicyOnly => vec![policy],
            Self::Mixed
            | Self::UnreferencedDuplicate
            | Self::ArtifactConflict
            | Self::EnvelopeInvalid
            | Self::NoTuple
            | Self::KindUnsupported => mixed(policy),
            Self::MixedReordered => mixed(policy).into_iter().rev().collect(),
            Self::EveryKind => std::iter::once(policy)
                .chain(
                    ArtifactKind::ALL
                        .into_iter()
                        .filter(|kind| *kind != ArtifactKind::ResourceLimitPolicy)
                        .map(|kind| {
                            envelope_of(
                                kind,
                                &format!("storefront-{}", kind.spelling()),
                                "1",
                                sample(kind),
                            )
                        }),
                )
                .collect(),
            Self::ManyArtifacts => std::iter::once(policy)
                .chain((0..SCALE).map(|index| target(&format!("storefront-target-{index:02}"))))
                .collect(),
            Self::RepeatedBodies => std::iter::once(policy)
                .chain(std::iter::repeat_n(target("storefront-web"), REPEATS))
                .collect(),
            Self::ManyConflicts => std::iter::once(policy)
                .chain((0..CONFLICTS).flat_map(|index| {
                    let identity = format!("storefront-approval-{index:02}");
                    [
                        approval(&identity, "required"),
                        approval(&identity, "not-required"),
                    ]
                }))
                .collect(),
        };
        match self {
            // A second copy of a member no reference reaches.
            Self::UnreferencedDuplicate => {
                members.push(approval("storefront-approval", "required"));
            }
            // The approval policy's name and revision with another content.
            Self::ArtifactConflict => {
                members.push(approval("storefront-approval", "not-required"));
            }
            // A tuple whose content 017 does not admit.
            Self::EnvelopeInvalid => members.push(envelope_of(
                ArtifactKind::TargetProfile,
                "storefront-server",
                "1",
                json!({}),
            )),
            // A body without its identity establishes no tuple.
            Self::NoTuple => {
                let mut member = target("storefront-server");
                member["body"]
                    .as_object_mut()
                    .expect("an envelope's body")
                    .remove("identity");
                members.push(sealed(member));
            }
            // A kind the collection does not admit.
            Self::KindUnsupported => {
                let mut member = target("storefront-server");
                member["kind"] = json!("unsupported-artifact");
                members.push(sealed(member));
            }
            _ => {}
        }
        members
    }
}

fn target(identity: &str) -> Value {
    envelope_of(
        ArtifactKind::TargetProfile,
        identity,
        "1",
        sample(ArtifactKind::TargetProfile),
    )
}

fn approval(identity: &str, mode: &str) -> Value {
    envelope_of(
        ArtifactKind::ApprovalPolicy,
        identity,
        "1",
        json!({ "localizedApproval": mode }),
    )
}

/// The policy with a Target Profile and an Approval Policy.
fn mixed(policy: Value) -> Vec<Value> {
    vec![
        policy,
        target("storefront-web"),
        approval("storefront-approval", "required"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::envelope::{Declaration, FormalMember};

    fn formal(member: &Value) -> FormalMember {
        FormalMember::new(member.clone()).unwrap()
    }

    /// Each blocked recipe adds one member to the mixed collection, or
    /// submits the members it scales, and that member breaks only what the
    /// recipe's name says.
    #[test]
    fn each_recipe_adds_the_member_its_name_says() {
        use ArtifactCollectionRecipe as R;

        let mixed = R::Mixed.members();
        assert_eq!(mixed.len(), 3);
        for recipe in [
            R::UnreferencedDuplicate,
            R::ArtifactConflict,
            R::EnvelopeInvalid,
            R::NoTuple,
            R::KindUnsupported,
        ] {
            let members = recipe.members();
            assert_eq!(members[..3], mixed[..], "{recipe:?}");
            assert_eq!(members.len(), 4, "{recipe:?}");
        }
        // The duplicate is the approval policy itself; the conflict shares
        // its tuple but for the digest.
        assert_eq!(R::UnreferencedDuplicate.members()[3], mixed[2]);
        let conflict = formal(&R::ArtifactConflict.members()[3]);
        let original = formal(&mixed[2]);
        let (conflict, original) = (
            conflict.envelope().unwrap().tuple().clone(),
            original.envelope().unwrap().tuple().clone(),
        );
        assert_eq!(
            (conflict.kind, &conflict.identity, &conflict.revision),
            (original.kind, &original.identity, &original.revision)
        );
        assert_ne!(conflict.semantic_digest, original.semantic_digest);
        // Every added member keeps its tuple but for the two whose
        // declaration or body establishes none.
        let added = |recipe: R| formal(&recipe.members()[3]);
        assert!(added(R::EnvelopeInvalid).envelope().is_some());
        assert!(added(R::EnvelopeInvalid)
            .envelope()
            .unwrap()
            .admit()
            .is_err());
        assert_eq!(
            added(R::NoTuple).declaration(),
            Declaration::Admitted(ArtifactKind::TargetProfile)
        );
        assert!(added(R::NoTuple).envelope().is_none());
        assert_eq!(
            added(R::KindUnsupported).declaration(),
            Declaration::Unsupported
        );
        // Every other member is admitted.
        for recipe in R::ALL {
            let members = recipe.members();
            let breaks = usize::from(matches!(
                recipe,
                R::EnvelopeInvalid | R::NoTuple | R::KindUnsupported
            ));
            let admitted = members
                .iter()
                .filter(|member| {
                    formal(member)
                        .envelope()
                        .is_some_and(|envelope| envelope.admit().is_ok())
                })
                .count();
            assert_eq!(admitted, members.len() - breaks, "{recipe:?}");
        }
        // The reordered recipe submits the same members backwards.
        let mut reordered = R::MixedReordered.members();
        reordered.reverse();
        assert_eq!(reordered, mixed);
    }

    #[test]
    fn the_scales_count_what_their_names_say() {
        use ArtifactCollectionRecipe as R;

        let kinds = |recipe: R| {
            recipe
                .members()
                .iter()
                .map(|member| formal(member).envelope().unwrap().tuple().kind)
                .collect::<Vec<_>>()
        };
        // Each of the eight kinds once.
        let mut every = kinds(R::EveryKind);
        every.sort();
        assert_eq!(every, ArtifactKind::ALL);
        // The policy and 63 Target Profiles of their own.
        let many = R::ManyArtifacts.members();
        assert_eq!(many.len(), 64);
        let mut bodies = many
            .iter()
            .map(|member| formal(member).canonical_body().to_vec())
            .collect::<Vec<_>>();
        bodies.sort();
        bodies.dedup();
        assert_eq!(bodies.len(), 64);
        // The policy and 32 copies of one Target Profile.
        let repeated = R::RepeatedBodies.members();
        assert_eq!(repeated.len(), 33);
        assert!(repeated[1..].iter().all(|member| *member == repeated[1]));
        // The policy and 32 pairs, each sharing a name and revision.
        let conflicts = R::ManyConflicts.members();
        assert_eq!(conflicts.len(), 65);
        for pair in conflicts[1..].chunks(2) {
            let (left, right) = (formal(&pair[0]), formal(&pair[1]));
            let (left, right) = (
                left.envelope().unwrap().tuple().clone(),
                right.envelope().unwrap().tuple().clone(),
            );
            assert_eq!(left.identity, right.identity);
            assert_ne!(left.semantic_digest, right.semantic_digest);
        }
        // Every collection submits the policy the confirmed bounds come from.
        for recipe in R::ALL {
            assert!(
                recipe.members().contains(&selected_policy(&capacity())),
                "{recipe:?}"
            );
        }
    }
}
