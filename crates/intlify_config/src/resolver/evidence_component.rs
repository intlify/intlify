// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Resolver Input Component Path: where resolver-input Evidence
//! points inside the complete input its Safe Origin Kind names, with the order
//! and view of each path (D15:821-838, 2502; Decisions 015-186 and 015-188).
//!
//! Every origin permits only its own shapes, and the constructors build only
//! those: the empty root of any resolver input; one of the eight slots of the
//! Profile Resolution Specification Input, ranked `0` to `7`; the member role
//! of the vocabulary or of the artifact collection, ranked `0`, and one key;
//! `mode` or `pins` of the Resource Limit Policy Verification Input, ranked
//! `0` and `1`; and for the Admitted Implementation Capability one bound:
//! `policyComparable` or `bootstrapOnly`, ranked `0` and `1`, then its group,
//! `diagnostics` split into its `entry` and `shared` roles, and its name, each
//! ranked in the Capability Specification's order. The canonicalization data
//! permits only the root.
//!
//! A key is a `safe-key` holding `C` of the view of an admitted identity, a
//! vocabulary member's canonical vocabulary identity or an admitted envelope's
//! exact-reference identity, or else a `redacted-key`: the Redacted evidence
//! token of only the JSON type of the tuple or of the member that a
//! content-derived admission key was derived from.
//!
//! Segment rank is `fixed-role`, `safe-key`, then `redacted-key`. Roles
//! compare by their rank within the parent path, keys by exact bytes with a
//! shorter equal prefix first, and tokens by complete digest bytes and then
//! the collision ordinal, absence first. Paths compare segment by segment with
//! the root first; Evidence compares the Safe Origin Kind before them. The
//! derived orders here are those comparisons. Nothing builds Evidence yet.

use std::sync::Arc;

use intlify_shared_json::encoding::encode;
use serde_json::{json, Value};

use crate::artifacts::count;

use super::capability::{CapabilityBound, BOOTSTRAP_ONLY, POLICY_COMPARABLE};
use super::evidence_source::hex;
use super::evidence_token::{EvidenceTokenInvariant, EvidenceTokens, JsonType, Redacted};
use super::limits::Group;
use super::origin::SafeOriginKind;
use super::specification::Slot;
use super::token::{RedactedToken, Specification};

/// A resolver bug that leaves a component without a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ComponentInvariant {
    /// The origin is not one of the six resolver inputs.
    NotResolverInput(SafeOriginKind),
    /// The identity view has no canonical bytes.
    Key,
    /// The bound is not in the Capability Specification.
    UnlistedBound(CapabilityBound),
    Token(EvidenceTokenInvariant),
}

/// One fixed role: its rank within its parent path and Safe Origin Kind, and
/// its registry-owned ID. Ranks are unique there, so they alone decide order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Role {
    rank: u64,
    id: &'static str,
}

/// What a member key is made from: an admitted identity's view, or the JSON
/// type of an unsafe tuple or admission key's source.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Key {
    Identity(Value),
    Unsafe(JsonType),
}

/// A component of the Resource Limit Policy Verification Input that 018
/// lets Evidence name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationRole {
    Mode,
    Pins,
}

/// A path segment as Evidence projects it, before tokens are assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    FixedRole(Role),
    SafeKey(Arc<[u8]>),
    RedactedKey(Redacted),
}

/// A Resolver Input Component Path as Evidence projects it, before the
/// invocation's evidence tokens are assigned, with the Safe Origin Kind of
/// the input it points into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComponentPath {
    origin: SafeOriginKind,
    segments: Vec<Segment>,
}

const fn role(rank: u64, id: &'static str) -> Segment {
    Segment::FixedRole(Role { rank, id })
}

impl ComponentPath {
    /// The empty root of the input `origin` names.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an origin that is no resolver input.
    pub(crate) const fn root(origin: SafeOriginKind) -> Result<Self, ComponentInvariant> {
        use SafeOriginKind as O;
        match origin {
            O::ProfileResolutionSpecificationInput
            | O::IntentSurfaceClassVocabulary
            | O::LocaleCanonicalizationData
            | O::ProfileResolutionArtifactCollection
            | O::ResourceLimitPolicyVerification
            | O::ImplementationCapability => Ok(Self {
                origin,
                segments: Vec::new(),
            }),
            O::ConfigurationFile
            | O::ConfigurationProgrammatic
            | O::ProfileSelectorCli
            | O::ProfileSelectorProgrammatic => Err(ComponentInvariant::NotResolverInput(origin)),
        }
    }

    /// One slot of the Profile Resolution Specification Input (D15:831),
    /// ranked in the slots' canonical order, in which `Slot` is declared.
    pub(crate) fn specification(slot: Slot) -> Self {
        Self {
            origin: SafeOriginKind::ProfileResolutionSpecificationInput,
            segments: vec![role(slot as u64, slot.name())],
        }
    }

    /// One member of the Intent Surface-Class Vocabulary Artifact (D15:832).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an identity view without canonical bytes.
    pub(crate) fn vocabulary_member(key: Key) -> Result<Self, ComponentInvariant> {
        Self::member(
            SafeOriginKind::IntentSurfaceClassVocabulary,
            "vocabulary-member",
            key,
        )
    }

    /// One member of the Submitted Profile Resolution Artifact Collection
    /// (D15:834).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an identity view without canonical bytes.
    pub(crate) fn artifact_member(key: Key) -> Result<Self, ComponentInvariant> {
        Self::member(
            SafeOriginKind::ProfileResolutionArtifactCollection,
            "artifact-member",
            key,
        )
    }

    fn member(
        origin: SafeOriginKind,
        id: &'static str,
        key: Key,
    ) -> Result<Self, ComponentInvariant> {
        let key = match key {
            Key::Identity(view) => {
                Segment::SafeKey(encode(&view).map_err(|_| ComponentInvariant::Key)?.into())
            }
            Key::Unsafe(source) => Segment::RedactedKey(
                Redacted::component_key(origin, source).map_err(ComponentInvariant::Token)?,
            ),
        };
        Ok(Self {
            origin,
            segments: vec![role(0, id), key],
        })
    }

    /// One component of the Resource Limit Policy Verification Input
    /// (D15:835).
    pub(crate) fn verification(component: VerificationRole) -> Self {
        Self {
            origin: SafeOriginKind::ResourceLimitPolicyVerification,
            segments: vec![match component {
                VerificationRole::Mode => role(0, "mode"),
                VerificationRole::Pins => role(1, "pins"),
            }],
        }
    }

    /// One bound of the Admitted Implementation Capability, under its
    /// namespace, group, and name, each ranked in the Capability
    /// Specification's order, in which the group types are declared and the
    /// inventory lists the bounds (D15:836).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a bound the inventory does not list.
    pub(crate) fn capability(bound: CapabilityBound) -> Result<Self, ComponentInvariant> {
        let name = CapabilityBound::inventory()
            .iter()
            .map(|(listed, _)| *listed)
            .filter(|listed| same_group(*listed, bound))
            .position(|listed| listed == bound)
            .ok_or(ComponentInvariant::UnlistedBound(bound))?;
        let name = count(name);
        let segments = match bound {
            CapabilityBound::PolicyComparable(id) => {
                let group = match id.group {
                    Group::DiagnosticsEntry => {
                        vec![
                            role(Group::DiagnosticsEntry as u64, "diagnostics"),
                            role(0, "entry"),
                        ]
                    }
                    Group::DiagnosticsShared => {
                        vec![
                            role(Group::DiagnosticsEntry as u64, "diagnostics"),
                            role(1, "shared"),
                        ]
                    }
                    group => vec![role(group as u64, group.path())],
                };
                [
                    vec![role(0, POLICY_COMPARABLE)],
                    group,
                    vec![role(name, id.name)],
                ]
                .concat()
            }
            CapabilityBound::BootstrapOnly(id) => vec![
                role(1, BOOTSTRAP_ONLY),
                role(id.group as u64, id.group.path()),
                role(name, id.name),
            ],
        };
        Ok(Self {
            origin: SafeOriginKind::ImplementationCapability,
            segments,
        })
    }

    /// The Safe Origin Kind of the input the path points into.
    pub(crate) const fn origin(&self) -> SafeOriginKind {
        self.origin
    }

    /// The components that tokens replace, one per redacted key.
    pub(crate) fn redacted(&self) -> impl Iterator<Item = &Redacted> {
        self.segments.iter().filter_map(|segment| match segment {
            Segment::RedactedKey(component) => Some(component),
            Segment::FixedRole(_) | Segment::SafeKey(_) => None,
        })
    }

    /// The path Evidence retains, `None` when `tokens` was not assigned a
    /// component of the path.
    pub(crate) fn retained(
        &self,
        specification: Specification<'_>,
        tokens: &EvidenceTokens,
    ) -> Option<ResolverInputComponentPath> {
        self.segments
            .iter()
            .map(|segment| match segment {
                Segment::FixedRole(role) => Some(PathComponent::FixedRole(*role)),
                Segment::SafeKey(bytes) => Some(PathComponent::SafeKey(Arc::clone(bytes))),
                Segment::RedactedKey(component) => tokens
                    .token(specification, component)
                    .map(PathComponent::RedactedKey),
            })
            .collect::<Option<_>>()
            .map(ResolverInputComponentPath)
    }
}

/// Whether two capability bounds share a namespace and group.
fn same_group(left: CapabilityBound, right: CapabilityBound) -> bool {
    match (left, right) {
        (CapabilityBound::PolicyComparable(left), CapabilityBound::PolicyComparable(right)) => {
            left.group == right.group
        }
        (CapabilityBound::BootstrapOnly(left), CapabilityBound::BootstrapOnly(right)) => {
            left.group == right.group
        }
        _ => false,
    }
}

/// A retained path segment, in 015's order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PathComponent {
    FixedRole(Role),
    SafeKey(Arc<[u8]>),
    RedactedKey(RedactedToken),
}

impl PathComponent {
    /// `{ "fixed-role": ID }`, `{ "safe-key": hex }`, or
    /// `{ "redacted-key": Token }` (D15:2502).
    pub(crate) fn view(&self) -> Value {
        match self {
            Self::FixedRole(role) => json!({ "fixed-role": role.id }),
            Self::SafeKey(bytes) => json!({ "safe-key": hex(bytes) }),
            Self::RedactedKey(token) => json!({ "redacted-key": token.view() }),
        }
    }
}

/// A retained Resolver Input Component Path, in 015's order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ResolverInputComponentPath(Vec<PathComponent>);

impl ResolverInputComponentPath {
    /// The array of its segments' views; the empty array is the root.
    pub(crate) fn view(&self) -> Value {
        Value::Array(self.0.iter().map(PathComponent::view).collect())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::resolver::capability::{BootstrapBoundId, BootstrapGroup, CapabilityBound};
    use crate::resolver::evidence_source::hex;
    use crate::resolver::evidence_token::{EvidenceTokens, JsonType};
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::specification::Slot;
    use crate::resolver::token::{Sha256, Specification};
    use intlify_shared_json::encoding::encode;

    use std::sync::Arc;

    use super::{
        ComponentInvariant, ComponentPath, Key, PathComponent, ResolverInputComponentPath, Role,
        VerificationRole,
    };

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };
    /// Computed apart from this crate by `.plans/drafts/025-pr5d5/vectors.py`:
    /// `C("checkout")`, and the token of a `resolver-input-component-key` of
    /// the vocabulary over `{ "type": "string" }`. The artifact key's token is
    /// PR 5d-1's vector.
    const VOCABULARY_KEY: &str = "040000000000000008636865636b6f7574";
    const VOCABULARY_TOKEN: &str =
        "sha256:344524bb2fdeb1b7f05757d45a90047330c072b814d7d8205d49019fc348a749";
    const ARTIFACT_TOKEN: &str =
        "sha256:e8b3e31460ec2c3bea9c5e56a1fd062e0699ed3daa04e758b76bdb44f6460c6a";

    /// The retained paths of `paths`, their tokens assigned together.
    fn retain(paths: &[ComponentPath]) -> Vec<ResolverInputComponentPath> {
        let tokens = EvidenceTokens::assign(
            PROFILE,
            paths.iter().flat_map(ComponentPath::redacted),
            &Sha256,
        );
        paths
            .iter()
            .map(|path| path.retained(PROFILE, &tokens).unwrap())
            .collect()
    }

    fn views(paths: &[ComponentPath]) -> Vec<serde_json::Value> {
        retain(paths)
            .iter()
            .map(ResolverInputComponentPath::view)
            .collect()
    }

    fn policy(group: Group, name: &'static str) -> CapabilityBound {
        CapabilityBound::PolicyComparable(BoundId { group, name })
    }

    fn bootstrap(group: BootstrapGroup, name: &'static str) -> CapabilityBound {
        CapabilityBound::BootstrapOnly(BootstrapBoundId { group, name })
    }

    #[test]
    fn every_resolver_input_has_the_root_and_no_other_origin_does() {
        for origin in O::ALL {
            let root = ComponentPath::root(origin);
            if origin.rank() < O::ProfileResolutionSpecificationInput.rank() {
                assert_eq!(root, Err(ComponentInvariant::NotResolverInput(origin)));
            } else {
                assert_eq!(views(&[root.unwrap()]), [json!([])], "{origin:?}");
            }
        }
    }

    #[test]
    fn a_specification_slot_and_a_verification_component_are_one_role() {
        assert_eq!(
            views(&[
                ComponentPath::specification(Slot::ProjectProfile),
                ComponentPath::specification(Slot::DeliveryPlacement),
                ComponentPath::verification(VerificationRole::Mode),
                ComponentPath::verification(VerificationRole::Pins),
            ]),
            [
                json!([{ "fixed-role": "project-profile" }]),
                json!([{ "fixed-role": "delivery-placement" }]),
                json!([{ "fixed-role": "mode" }]),
                json!([{ "fixed-role": "pins" }]),
            ]
        );
    }

    /// An admitted identity is `C` of its view; anything else is the token
    /// of its JSON type under the member's origin.
    #[test]
    fn a_member_key_is_safe_only_for_an_admitted_identity() {
        let reference = json!({
            "kind": "trust-policy", "identity": "storefront-trust", "revision": "1",
            "specificationRevision": "0", "semanticDigest": format!("sha256:{}", "0".repeat(64)),
        });
        assert_eq!(
            views(&[
                ComponentPath::vocabulary_member(Key::Identity(json!("checkout"))).unwrap(),
                ComponentPath::vocabulary_member(Key::Unsafe(JsonType::String)).unwrap(),
                ComponentPath::artifact_member(Key::Identity(reference.clone())).unwrap(),
                ComponentPath::artifact_member(Key::Unsafe(JsonType::Object)).unwrap(),
            ]),
            [
                json!([{ "fixed-role": "vocabulary-member" }, { "safe-key": VOCABULARY_KEY }]),
                json!([
                    { "fixed-role": "vocabulary-member" },
                    { "redacted-key": { "token": VOCABULARY_TOKEN } },
                ]),
                json!([
                    { "fixed-role": "artifact-member" },
                    { "safe-key": hex(&encode(&reference).unwrap()) },
                ]),
                json!([
                    { "fixed-role": "artifact-member" },
                    { "redacted-key": { "token": ARTIFACT_TOKEN } },
                ]),
            ]
        );
        assert_eq!(
            ComponentPath::vocabulary_member(Key::Identity(json!(1))),
            Err(ComponentInvariant::Key),
            "a view with a number has no canonical bytes"
        );
    }

    /// A bound sits under its namespace, its group, with `diagnostics` split
    /// into `entry` and `shared`, and its name.
    #[test]
    fn a_capability_bound_is_its_namespace_group_and_name() {
        assert_eq!(
            views(&[
                ComponentPath::capability(policy(Group::ConfigurationInput, "maxNodes")).unwrap(),
                ComponentPath::capability(policy(Group::DiagnosticsEntry, "maxFindings")).unwrap(),
                ComponentPath::capability(policy(Group::DiagnosticsShared, "maxFindings")).unwrap(),
                ComponentPath::capability(bootstrap(BootstrapGroup::RawInput, "maxFileBytes"))
                    .unwrap(),
            ]),
            [
                json!([
                    { "fixed-role": "policyComparable" },
                    { "fixed-role": "configurationInput" },
                    { "fixed-role": "maxNodes" },
                ]),
                json!([
                    { "fixed-role": "policyComparable" },
                    { "fixed-role": "diagnostics" },
                    { "fixed-role": "entry" },
                    { "fixed-role": "maxFindings" },
                ]),
                json!([
                    { "fixed-role": "policyComparable" },
                    { "fixed-role": "diagnostics" },
                    { "fixed-role": "shared" },
                    { "fixed-role": "maxFindings" },
                ]),
                json!([
                    { "fixed-role": "bootstrapOnly" },
                    { "fixed-role": "rawInput" },
                    { "fixed-role": "maxFileBytes" },
                ]),
            ]
        );
    }

    /// Each constructor fixes the origin its shape belongs to.
    #[test]
    fn a_path_keeps_the_origin_it_points_into() {
        let origins = [
            ComponentPath::specification(Slot::Coverage).origin(),
            ComponentPath::vocabulary_member(Key::Unsafe(JsonType::String))
                .unwrap()
                .origin(),
            ComponentPath::root(O::LocaleCanonicalizationData)
                .unwrap()
                .origin(),
            ComponentPath::artifact_member(Key::Unsafe(JsonType::Object))
                .unwrap()
                .origin(),
            ComponentPath::verification(VerificationRole::Pins).origin(),
            ComponentPath::capability(policy(Group::ConfigurationInput, "maxNodes"))
                .unwrap()
                .origin(),
        ];
        assert_eq!(origins, O::ALL[4..]);
    }

    #[test]
    fn a_bound_the_inventory_does_not_list_is_a_resolver_bug() {
        let unlisted = policy(Group::ConfigurationInput, "maxSecrets");
        assert_eq!(
            ComponentPath::capability(unlisted),
            Err(ComponentInvariant::UnlistedBound(unlisted))
        );
    }

    /// Roles compare by rank, never by ID: every capability bound sorts into
    /// the Capability Specification's order, and `mode` precedes `pins`.
    #[test]
    fn roles_compare_by_their_rank() {
        let inventory = CapabilityBound::inventory().map(|(bound, _)| bound);
        let mut paths: Vec<_> = inventory
            .iter()
            .rev()
            .map(|bound| retain(&[ComponentPath::capability(*bound).unwrap()]).remove(0))
            .collect();
        paths.sort();
        let expected: Vec<_> = inventory
            .iter()
            .map(|bound| retain(&[ComponentPath::capability(*bound).unwrap()]).remove(0))
            .collect();
        assert_eq!(paths, expected);
        let mode = retain(&[ComponentPath::verification(VerificationRole::Mode)]);
        let pins = retain(&[ComponentPath::verification(VerificationRole::Pins)]);
        assert!(mode[0] < pins[0]);
        let slots: Vec<_> = Slot::ALL
            .iter()
            .map(|slot| retain(&[ComponentPath::specification(*slot)]).remove(0))
            .collect();
        assert!(slots.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// The root first, then fixed roles, safe keys by bytes, then tokens.
    #[test]
    fn segments_compare_by_kind_then_payload() {
        let mut retained = retain(&[
            ComponentPath::vocabulary_member(Key::Unsafe(JsonType::String)).unwrap(),
            ComponentPath::vocabulary_member(Key::Identity(json!("checkout-total"))).unwrap(),
            ComponentPath::root(O::IntentSurfaceClassVocabulary).unwrap(),
            ComponentPath::vocabulary_member(Key::Identity(json!("checkout"))).unwrap(),
        ]);
        retained.sort();
        let views: Vec<_> = retained
            .iter()
            .map(ResolverInputComponentPath::view)
            .collect();
        assert_eq!(views[0], json!([]));
        assert_eq!(views[1][1], json!({ "safe-key": VOCABULARY_KEY }));
        assert_eq!(
            views[2][1]["safe-key"].as_str().unwrap().len(),
            2 * (9 + 14)
        );
        assert_eq!(
            views[3][1],
            json!({ "redacted-key": { "token": VOCABULARY_TOKEN } })
        );
    }

    /// Valid shapes never put a role and a key at one position, so the
    /// segment kinds' rank shows only on the segments themselves: every role
    /// precedes every safe key, which precedes every token.
    #[test]
    fn segment_kinds_rank_fixed_role_then_safe_key_then_redacted_key() {
        let unsafe_key =
            retain(&[ComponentPath::artifact_member(Key::Unsafe(JsonType::Object)).unwrap()]);
        let token = unsafe_key[0].0[1].clone();
        let late_role = PathComponent::FixedRole(Role {
            rank: u64::MAX,
            id: "zzz",
        });
        let early_key = PathComponent::SafeKey(Arc::from(&[0_u8][..]));
        assert!(late_role < early_key && early_key < token);
    }

    #[test]
    fn a_path_without_its_token_is_not_retained() {
        let tokens = EvidenceTokens::assign(PROFILE, [], &Sha256);
        let unsafe_key = ComponentPath::artifact_member(Key::Unsafe(JsonType::Object)).unwrap();
        assert_eq!(unsafe_key.retained(PROFILE, &tokens), None);
        assert!(ComponentPath::specification(Slot::Coverage)
            .retained(PROFILE, &tokens)
            .is_some());
    }
}
