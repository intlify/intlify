// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite Phase 4 inputs: the minimal configuration, whose `resourceLimits`
//! names a fixture-owned Resource Limit Policy, and the members submitted
//! with it. A reference's semantic digest is computed from the envelope it
//! names, never written by hand.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::artifacts::envelope::{envelope_of, sealed, ArtifactKind, ExactTuple, FormalMember};
use crate::input_limits::Bound;
use crate::profile_fixtures::minimal_config;
use crate::resolver::limits::{
    fixture_limits, ArtifactAdmissionLimits, ConfigurationInputLimits, ResolutionLimits, SECTION,
};

use super::prepare::fixture_capacity;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum ResourcePolicyRecipe {
    PolicyOnly,
    Mixed,
    MixedReordered,
    ManyArtifacts,
    RepeatedBodies,
    RoleKindMismatch,
    SpecificationRevisionUnsupported,
    ConflictingContent,
    DuplicateEnvelope,
    BodyIntegrityInvalid,
    BodySchemaInvalid,
    SectionMissing,
    SectionInvalid,
    DigestMismatch,
    SpecificationRevisionMismatch,
    RevisionMismatch,
    KindMismatch,
    IdentityMismatch,
    NotFound,
    CapabilityIncompatible,
}

/// A configuration and the members submitted with it, in submission order.
pub(in crate::benchmark) struct PolicyFixture {
    pub(in crate::benchmark) config: Value,
    pub(in crate::benchmark) members: Vec<Value>,
}

/// The selected policy's name. Every other member is named apart from it
/// unless a recipe shares its name on purpose.
const IDENTITY: &str = "storefront-limits";

/// The number of Target Profile members `ManyArtifacts` submits besides the
/// selected policy, and of copies `RepeatedBodies` submits of one.
const SCALE: usize = 63;
const REPEATS: usize = 32;

impl ResourcePolicyRecipe {
    pub(super) const ALL: [Self; 20] = [
        Self::PolicyOnly,
        Self::Mixed,
        Self::MixedReordered,
        Self::ManyArtifacts,
        Self::RepeatedBodies,
        Self::RoleKindMismatch,
        Self::SpecificationRevisionUnsupported,
        Self::ConflictingContent,
        Self::DuplicateEnvelope,
        Self::BodyIntegrityInvalid,
        Self::BodySchemaInvalid,
        Self::SectionMissing,
        Self::SectionInvalid,
        Self::DigestMismatch,
        Self::SpecificationRevisionMismatch,
        Self::RevisionMismatch,
        Self::KindMismatch,
        Self::IdentityMismatch,
        Self::NotFound,
        Self::CapabilityIncompatible,
    ];

    /// Whether Phase 4 confirms the policy under the default bounds.
    pub(in crate::benchmark) const fn confirms(self) -> bool {
        matches!(
            self,
            Self::PolicyOnly
                | Self::Mixed
                | Self::MixedReordered
                | Self::ManyArtifacts
                | Self::RepeatedBodies
        )
    }

    /// The fixture whose selected policy sets `policy`. The capability is
    /// separate: only `CapabilityIncompatible` writes a policy that needs more
    /// than `policy`, one more `maxArtifacts`.
    pub(in crate::benchmark) fn fixture(self, policy: &ResolutionLimits) -> PolicyFixture {
        let mut limits = policy.clone();
        if self == Self::CapabilityIncompatible {
            limits.artifact_admission.max_artifacts =
                Bound::new(policy.artifact_admission.max_artifacts.get() + 1)
                    .expect("a finite fixture bound");
        }
        let chosen = policy_member(IDENTITY, "1", json!({ SECTION: limits.section() }));
        let mut reference = reference_value(&tuple_of(&chosen));
        let members = match self {
            Self::PolicyOnly | Self::CapabilityIncompatible => vec![chosen],
            Self::Mixed => mixed(chosen),
            Self::MixedReordered => mixed(chosen).into_iter().rev().collect(),
            Self::ManyArtifacts => std::iter::once(chosen)
                .chain((0..SCALE).map(|index| target(&format!("storefront-target-{index:02}"))))
                .collect(),
            Self::RepeatedBodies => std::iter::once(chosen)
                .chain(std::iter::repeat_n(target("storefront-web"), REPEATS))
                .collect(),
            Self::RoleKindMismatch => {
                reference["kind"] = json!("trust-policy");
                vec![chosen]
            }
            Self::SpecificationRevisionUnsupported => {
                reference["specificationRevision"] = json!("1");
                vec![chosen]
            }
            Self::ConflictingContent => {
                let other =
                    policy_member(IDENTITY, "1", json!({ SECTION: other_section(&limits) }));
                vec![chosen, other]
            }
            Self::DuplicateEnvelope => vec![chosen.clone(), chosen],
            Self::BodyIntegrityInvalid => {
                let mut tampered = chosen;
                tampered["integrityDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
                vec![tampered]
            }
            Self::BodySchemaInvalid | Self::SectionMissing | Self::SectionInvalid => {
                let content = match self {
                    Self::BodySchemaInvalid => json!({ SECTION: "not-an-object" }),
                    Self::SectionMissing => json!({}),
                    _ => {
                        let mut section = limits.section();
                        section["artifactAdmission"]["maxArtifacts"] = json!("0");
                        json!({ SECTION: section })
                    }
                };
                let named = policy_member(IDENTITY, "1", content);
                reference = reference_value(&tuple_of(&named));
                vec![named]
            }
            Self::DigestMismatch => {
                vec![policy_member(
                    IDENTITY,
                    "1",
                    json!({ SECTION: other_section(&limits) }),
                )]
            }
            Self::SpecificationRevisionMismatch => {
                let mut member = chosen;
                member["specification"]["revision"] = json!("1");
                vec![sealed(member)]
            }
            Self::RevisionMismatch => vec![policy_member(
                IDENTITY,
                "2",
                json!({ SECTION: limits.section() }),
            )],
            Self::KindMismatch => vec![envelope_of(
                ArtifactKind::TrustPolicy,
                IDENTITY,
                "1",
                json!({}),
            )],
            Self::IdentityMismatch => vec![policy_member(
                "storefront-other-limits",
                "1",
                json!({ SECTION: limits.section() }),
            )],
            Self::NotFound => vec![target("storefront-web")],
        };
        let mut config = minimal_config();
        config["profiles"]["app"]["policies"]["resourceLimits"] = reference;
        PolicyFixture { config, members }
    }
}

/// The capability every case runs under unless a bound edge lowers one of
/// its values: the configuration bounds are the ones the fixture's entry and
/// structural stages run under, the artifact bounds are explicit owner
/// capacity, and every other bound is a distinct finite fixture value. Never
/// a product default.
pub(in crate::benchmark) fn capacity() -> ResolutionLimits {
    let (input, structural) = fixture_capacity();
    let bound = |value| Bound::new(value).expect("finite positive fixture capacity");
    let mut limits = fixture_limits(10_000);
    limits.configuration_input = ConfigurationInputLimits {
        value: input.value,
        structural,
    };
    limits.artifact_admission = ArtifactAdmissionLimits {
        max_artifacts: bound(1_000),
        max_policy_artifacts: bound(1_001),
        max_target_profile_artifacts: bound(1_002),
        max_artifact_references: bound(1_003),
        max_single_artifact_canonical_bytes: bound(100_000),
        max_total_artifact_canonical_bytes: bound(10_000_000),
        max_single_reference_canonical_bytes: bound(10_000),
        max_total_reference_canonical_bytes: bound(100_000),
    };
    limits
}

fn policy_member(identity: &str, revision: &str, content: Value) -> Value {
    envelope_of(
        ArtifactKind::ResourceLimitPolicy,
        identity,
        revision,
        content,
    )
}

fn target(identity: &str) -> Value {
    envelope_of(ArtifactKind::TargetProfile, identity, "1", json!({}))
}

/// The selected policy with a Target Profile and an Approval Policy.
fn mixed(chosen: Value) -> Vec<Value> {
    vec![
        chosen,
        target("storefront-web"),
        envelope_of(
            ArtifactKind::ApprovalPolicy,
            "storefront-approval",
            "1",
            json!({"localizedApproval": "required"}),
        ),
    ]
}

/// A section setting one bound apart from `limits`, so its content differs.
fn other_section(limits: &ResolutionLimits) -> Value {
    let mut section = limits.section();
    section["artifactAdmission"]["maxArtifacts"] =
        json!((limits.artifact_admission.max_artifacts.get() - 1).to_string());
    section
}

fn tuple_of(member: &Value) -> ExactTuple {
    FormalMember::new(member.clone())
        .ok()
        .and_then(|member| member.envelope().map(|envelope| envelope.tuple().clone()))
        .expect("a fixture member establishes its tuple")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tuple_named(fixture: &PolicyFixture) -> Value {
        fixture.config["profiles"]["app"]["policies"]["resourceLimits"].clone()
    }

    /// Each recipe differs from the selected reference in exactly the part
    /// its name says, whatever the resolver later decides.
    #[test]
    fn every_reference_names_its_member_by_a_computed_digest() {
        use ResourcePolicyRecipe as R;

        let limits = capacity();
        let first = |recipe: ResourcePolicyRecipe| {
            let fixture = recipe.fixture(&limits);
            let member = reference_value(&tuple_of(&fixture.members[0]));
            (tuple_named(&fixture), member)
        };
        for recipe in [
            R::PolicyOnly,
            R::Mixed,
            R::ManyArtifacts,
            R::RepeatedBodies,
            R::DuplicateEnvelope,
            R::BodyIntegrityInvalid,
            R::BodySchemaInvalid,
            R::SectionMissing,
            R::SectionInvalid,
            R::CapabilityIncompatible,
        ] {
            let (named, member) = first(recipe);
            assert_eq!(named, member, "{recipe:?}");
        }
        let differs = |recipe, parts: &[&str]| {
            let (named, member) = first(recipe);
            for part in [
                "kind",
                "identity",
                "revision",
                "specificationRevision",
                "semanticDigest",
            ] {
                assert_eq!(
                    named[part] != member[part],
                    parts.contains(&part),
                    "{recipe:?} {part}"
                );
            }
        };
        differs(R::RoleKindMismatch, &["kind"]);
        differs(
            R::SpecificationRevisionUnsupported,
            &["specificationRevision"],
        );
        differs(R::DigestMismatch, &["semanticDigest"]);
        // The semantic digest covers the specification revision, not the
        // name or revision.
        differs(
            R::SpecificationRevisionMismatch,
            &["specificationRevision", "semanticDigest"],
        );
        differs(R::RevisionMismatch, &["revision"]);
        differs(R::KindMismatch, &["kind", "semanticDigest"]);
        differs(R::IdentityMismatch, &["identity"]);
        differs(R::NotFound, &["kind", "identity", "semanticDigest"]);
        // The second member shares the selected policy's name, not its body.
        let conflict = R::ConflictingContent.fixture(&limits);
        let other = reference_value(&tuple_of(&conflict.members[1]));
        assert_eq!(other["identity"], tuple_named(&conflict)["identity"]);
        assert_ne!(
            other["semanticDigest"],
            tuple_named(&conflict)["semanticDigest"]
        );
        // The reordered recipe submits the same members backwards.
        let mixed = R::Mixed.fixture(&limits).members;
        let mut reordered = R::MixedReordered.fixture(&limits).members;
        reordered.reverse();
        assert_eq!(mixed, reordered);
    }
}
