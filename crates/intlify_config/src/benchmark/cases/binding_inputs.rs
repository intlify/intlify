// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite Phase 9 exact-resolution inputs: the minimal configuration, whose
//! selected declaration names fixture-owned artifacts, and the members
//! submitted with it. A reference's semantic digest is computed from the
//! envelope it names, never written by hand. Every member is an admitted
//! envelope, and a blocked recipe breaks only the `trust` reference or the
//! collection, so each names the one decision it reaches.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::artifacts::content::sample;
use crate::artifacts::envelope::{envelope_of, ArtifactKind};
use crate::profile_fixtures::minimal_config;

use super::inputs::reverse_objects;
use super::policy_inputs::{capacity, reference_value, selected_policy, tuple_of};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum ResolvedBindingRecipe {
    RequiredOnly,
    EveryReference,
    EveryReferenceReordered,
    ManyTargets,
    SharedTargetProfile,
    UnreferencedArtifacts,
    RoleKindMismatch,
    SpecificationRevisionUnsupported,
    DigestMismatch,
    RevisionMismatch,
    KindMismatch,
    IdentityMismatch,
    NotFound,
    ManyNotFound,
    CollectionNotAdmitted,
}

/// A configuration and the members submitted with it, in submission order.
pub(in crate::benchmark) struct BindingFixture {
    pub(in crate::benchmark) config: Value,
    pub(in crate::benchmark) members: Vec<Value>,
}

/// The number of targets `ManyTargets`, `SharedTargetProfile`, and
/// `ManyNotFound` declare, and of Target Profiles `UnreferencedArtifacts`
/// submits that no reference names.
const SCALE: usize = 63;

/// Each required policy slot besides `resourceLimits`: its configuration
/// name, the kind its role takes, and the identity of the artifact it names.
pub(super) const REQUIRED: [(&str, ArtifactKind, &str); 4] = [
    ("trust", ArtifactKind::TrustPolicy, "storefront-trust"),
    (
        "sourceAdmission",
        ArtifactKind::SourceAdmissionPolicy,
        "storefront-source",
    ),
    (
        "approval",
        ArtifactKind::ApprovalPolicy,
        "storefront-approval",
    ),
    (
        "selection",
        ArtifactKind::SelectionPolicy,
        "storefront-selection",
    ),
];

/// The two slots a declaration may set to an explicit `null`.
const OPTIONAL: [(&str, ArtifactKind, &str); 2] = [
    (
        "providerRouting",
        ArtifactKind::ProviderRoutingPolicy,
        "storefront-routing",
    ),
    (
        "glossarySet",
        ArtifactKind::GlossarySet,
        "storefront-glossary",
    ),
];

impl ResolvedBindingRecipe {
    pub(super) const ALL: [Self; 15] = [
        Self::RequiredOnly,
        Self::EveryReference,
        Self::EveryReferenceReordered,
        Self::ManyTargets,
        Self::SharedTargetProfile,
        Self::UnreferencedArtifacts,
        Self::RoleKindMismatch,
        Self::SpecificationRevisionUnsupported,
        Self::DigestMismatch,
        Self::RevisionMismatch,
        Self::KindMismatch,
        Self::IdentityMismatch,
        Self::NotFound,
        Self::ManyNotFound,
        Self::CollectionNotAdmitted,
    ];

    /// Whether every present reference names its artifact, so the bindings
    /// are made.
    pub(in crate::benchmark) const fn binds(self) -> bool {
        matches!(
            self,
            Self::RequiredOnly
                | Self::EveryReference
                | Self::EveryReferenceReordered
                | Self::ManyTargets
                | Self::SharedTargetProfile
                | Self::UnreferencedArtifacts
        )
    }

    const fn every_slot(self) -> bool {
        matches!(self, Self::EveryReference | Self::EveryReferenceReordered)
    }

    /// Each declared target's ID and the identity of the Target Profile it
    /// names, in declaration order.
    fn targets(self) -> Vec<(String, String)> {
        let scaled = |profile: &dyn Fn(usize) -> String| -> Vec<(String, String)> {
            (0..SCALE)
                .map(|index| (format!("target-{index:02}"), profile(index)))
                .collect()
        };
        match self {
            _ if self.every_slot() => vec![
                ("browser".into(), "storefront-browser".into()),
                ("server".into(), "storefront-server".into()),
            ],
            Self::ManyTargets | Self::ManyNotFound => {
                scaled(&|index| format!("storefront-target-{index:02}"))
            }
            Self::SharedTargetProfile => scaled(&|_| "storefront-browser".into()),
            _ => vec![("browser".into(), "storefront-browser".into())],
        }
    }

    /// The configuration and the members, in submission order. The collection
    /// submits the policy the confirmed bounds come from, as Phase 4 found it.
    pub(in crate::benchmark) fn fixture(self) -> BindingFixture {
        let resource_limits = selected_policy(&capacity());
        let mut config = minimal_config();
        let declaration = &mut config["profiles"]["app"];
        declaration["policies"]["resourceLimits"] = reference_value(&tuple_of(&resource_limits));
        let mut members = vec![resource_limits];
        let optional: &[_] = if self.every_slot() { &OPTIONAL } else { &[] };
        for (slot, kind, identity) in REQUIRED.iter().chain(optional) {
            let member = artifact(*kind, identity);
            declaration["policies"][*slot] = reference_value(&tuple_of(&member));
            members.push(member);
        }
        let targets = self.targets();
        let mut declared = Map::new();
        for (id, profile) in &targets {
            let named = reference_value(&tuple_of(&artifact(ArtifactKind::TargetProfile, profile)));
            declared.insert(
                id.clone(),
                json!({"profile": named, "requestedLocales": ["en"]}),
            );
        }
        declaration["targetProfiles"] = Value::Object(declared);
        declaration["deploymentGroups"] = json!({"web": {
            "members": targets.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>()
        }});
        // Each named Target Profile is submitted once, unless no target's
        // profile is.
        if self != Self::ManyNotFound {
            let profiles = targets
                .iter()
                .map(|(_, profile)| profile.as_str())
                .collect::<BTreeSet<_>>();
            members.extend(
                profiles
                    .into_iter()
                    .map(|profile| artifact(ArtifactKind::TargetProfile, profile)),
            );
        }
        if self == Self::UnreferencedArtifacts {
            members.extend((0..SCALE).map(|index| {
                artifact(
                    ArtifactKind::TargetProfile,
                    &format!("storefront-unused-{index:02}"),
                )
            }));
        }
        let trust = artifact(ArtifactKind::TrustPolicy, "storefront-trust");
        let reference = &mut declaration["policies"]["trust"];
        match self {
            // Another role's artifact, named exactly.
            Self::RoleKindMismatch => {
                *reference = reference_value(&tuple_of(&artifact(
                    ArtifactKind::ApprovalPolicy,
                    "storefront-approval",
                )));
            }
            Self::SpecificationRevisionUnsupported => {
                reference["specificationRevision"] = json!("1");
            }
            Self::DigestMismatch => {
                reference["semanticDigest"] = json!(format!("sha256:{}", "1".repeat(64)));
            }
            Self::RevisionMismatch => reference["revision"] = json!("2"),
            // The identity of the source-admission policy.
            Self::KindMismatch => reference["identity"] = json!("storefront-source"),
            Self::IdentityMismatch => reference["identity"] = json!("storefront-vault"),
            Self::NotFound => members.retain(|member| *member != trust),
            // A second copy of the trust policy leaves the collection without
            // a Set.
            Self::CollectionNotAdmitted => members.push(trust),
            _ => {}
        }
        if self == Self::EveryReferenceReordered {
            reverse_objects(&mut config);
            members.reverse();
        }
        BindingFixture { config, members }
    }
}

pub(super) fn artifact(kind: ArtifactKind, identity: &str) -> Value {
    envelope_of(kind, identity, "1", sample(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policies(fixture: &BindingFixture) -> &Map<String, Value> {
        fixture.config["profiles"]["app"]["policies"]
            .as_object()
            .unwrap()
    }

    fn targets(fixture: &BindingFixture) -> &Map<String, Value> {
        fixture.config["profiles"]["app"]["targetProfiles"]
            .as_object()
            .unwrap()
    }

    /// Every reference of a recipe that binds names a submitted member by
    /// the digest computed from it.
    #[test]
    fn every_reference_names_a_member_by_a_computed_digest() {
        for recipe in ResolvedBindingRecipe::ALL
            .into_iter()
            .filter(|recipe| recipe.binds())
        {
            let fixture = recipe.fixture();
            let submitted = fixture
                .members
                .iter()
                .map(|member| reference_value(&tuple_of(member)))
                .collect::<Vec<_>>();
            let references = policies(&fixture)
                .values()
                .filter(|reference| !reference.is_null())
                .chain(targets(&fixture).values().map(|target| &target["profile"]));
            for reference in references {
                assert!(submitted.contains(reference), "{recipe:?}: {reference}");
            }
        }
    }

    /// Each blocked recipe changes only the `trust` reference or the
    /// collection of the required-only fixture.
    #[test]
    fn each_blocked_recipe_breaks_only_what_its_name_says() {
        use ResolvedBindingRecipe as R;

        let base = R::RequiredOnly.fixture();
        assert_eq!(
            policies(&base)
                .values()
                .filter(|reference| reference.is_null())
                .count(),
            2
        );
        for recipe in [
            R::RoleKindMismatch,
            R::SpecificationRevisionUnsupported,
            R::DigestMismatch,
            R::RevisionMismatch,
            R::KindMismatch,
            R::IdentityMismatch,
        ] {
            let fixture = recipe.fixture();
            assert_eq!(fixture.members, base.members, "{recipe:?}");
            let mut config = fixture.config.clone();
            assert_ne!(
                config["profiles"]["app"]["policies"]["trust"],
                base.config["profiles"]["app"]["policies"]["trust"],
                "{recipe:?}"
            );
            config["profiles"]["app"]["policies"]["trust"] =
                base.config["profiles"]["app"]["policies"]["trust"].clone();
            assert_eq!(config, base.config, "{recipe:?}");
        }
        let trust = artifact(ArtifactKind::TrustPolicy, "storefront-trust");
        let missing = R::NotFound.fixture();
        assert_eq!(missing.config, base.config);
        assert_eq!(missing.members.len(), base.members.len() - 1);
        assert!(!missing.members.contains(&trust));
        let duplicated = R::CollectionNotAdmitted.fixture();
        assert_eq!(duplicated.config, base.config);
        assert_eq!(duplicated.members[..base.members.len()], base.members[..]);
        assert_eq!(duplicated.members[base.members.len()..], [trust]);
    }

    #[test]
    fn the_scales_declare_what_their_names_say() {
        use ResolvedBindingRecipe as R;

        let profiles = |fixture: &BindingFixture| {
            fixture
                .members
                .iter()
                .filter(|member| member["kind"] == "target-profile")
                .count()
        };
        let every = R::EveryReference.fixture();
        assert!(policies(&every)
            .values()
            .all(|reference| !reference.is_null()));
        assert_eq!((targets(&every).len(), profiles(&every)), (2, 2));
        assert_eq!(every.members.len(), 9);
        // The same configuration, every object's members reversed: equal as
        // JSON, apart as bytes.
        let reordered = R::EveryReferenceReordered.fixture();
        assert_eq!(reordered.config, every.config);
        assert_ne!(
            serde_json::to_vec(&reordered.config).unwrap(),
            serde_json::to_vec(&every.config).unwrap()
        );
        assert_eq!(
            reordered.members.iter().rev().collect::<Vec<_>>(),
            every.members.iter().collect::<Vec<_>>()
        );
        for (recipe, declared, submitted) in [
            (R::RequiredOnly, 1, 1),
            (R::ManyTargets, SCALE, SCALE),
            (R::SharedTargetProfile, SCALE, 1),
            (R::UnreferencedArtifacts, 1, 1 + SCALE),
            (R::ManyNotFound, SCALE, 0),
        ] {
            let fixture = recipe.fixture();
            assert_eq!(
                (targets(&fixture).len(), profiles(&fixture)),
                (declared, submitted),
                "{recipe:?}"
            );
            // The five required policies are submitted beside them.
            assert_eq!(fixture.members.len(), 5 + submitted, "{recipe:?}");
            let members = &fixture.config["profiles"]["app"]["deploymentGroups"]["web"]["members"];
            assert_eq!(members.as_array().unwrap().len(), declared, "{recipe:?}");
        }
    }
}
