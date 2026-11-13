// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Finding Registry's disclosure policies: one for each of design 015's
//! seven evidence kinds, in a table beside the registry's rows (D15:720,
//! 2233, 2249; Decision 015-192).
//!
//! A policy names the members every Evidence of its kind has and each
//! component its view can retain: a leaf variant of a member of D15:2492's
//! view, such as `source.redacted` or
//! `value.redacted-logical-path.redacted-member`. Each component has one of
//! the two disclosure policies. A `non-secret-literal` component whose
//! content is not fixed lists the classifications under which that content
//! counts as non-secret: the owner's mark on an exact identity (Decision
//! 015-185), the adapter's classification of a locator (D15:766), a
//! schema-defined name, an identifier key, or a canonical locale key
//! (D15:776), a declared Profile ID (D15:807), and a vocabulary member's
//! identity or an admitted envelope's exact reference (D15:824). A
//! `redacted-structure` component names the projection whose Redacted
//! evidence token replaces it (D15:722).
//!
//! A resolver-input kind's policy also lists the component path shapes that
//! some retained Evidence can carry, and no other (D15:838; Decision
//! 015-190): each with its Safe Origin Kind, its fixed roles and their
//! ranks, and the key that ends it, as the Resolver Input Component Path
//! builds them. No Finding reaches the verification `mode` role, the root of
//! the Specification Input, the verification input, or the capability, or a
//! `bootstrapOnly` group other than `rawInput` and `artifactBootstrap`.
//!
//! Construction reads the policies with the registry cross-references (row
//! 41): one for each evidence kind, in 015's order, whose members and
//! components are exactly those the kind's view has, each disclosed as 015
//! requires, and whose path shapes are ones the Resolver Input Component
//! Path builds for an origin the Safe Origin Kind Registry lets the kind
//! use, each once and in Evidence order. Which Finding Occurrence carries
//! each shape is the rows' to show.

use std::cmp::Ordering;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::resolver::capability::{BootstrapGroup, CapabilityBound};
use crate::resolver::evidence_component::{ComponentPath, Key, KeyShape, VerificationRole};
use crate::resolver::evidence_token::{JsonType, TokenKind};
use crate::resolver::origin::{EvidenceKind, SafeOriginKind};
use crate::resolver::specification::Slot;

use super::safe_origin::SafeOriginKindRegistry;
use super::RANK_PATTERN;

/// A member of an Evidence view (D15:2481).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Member {
    Source,
    Locator,
    Path,
    Value,
    Span,
    Selector,
    Component,
}

/// A component an Evidence view can retain: a leaf variant of one of its
/// members, in the order of the view's members and of each member's
/// variants (D15:2492).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub(crate) enum Component {
    #[serde(rename = "source.safe-opaque")]
    SourceSafeOpaque,
    #[serde(rename = "source.redacted")]
    SourceRedacted,
    #[serde(rename = "locator.relative-path")]
    LocatorRelativePath,
    #[serde(rename = "locator.safe-label")]
    LocatorSafeLabel,
    #[serde(rename = "locator.safe-uri")]
    LocatorSafeUri,
    #[serde(rename = "path.safe-member")]
    PathSafeMember,
    #[serde(rename = "path.array-index")]
    PathArrayIndex,
    #[serde(rename = "path.redacted-member")]
    PathRedactedMember,
    #[serde(rename = "value.exact-json-pointer")]
    ValueExactJsonPointer,
    #[serde(rename = "value.redacted-logical-path.safe-member")]
    ValueSafeMember,
    #[serde(rename = "value.redacted-logical-path.array-index")]
    ValueArrayIndex,
    #[serde(rename = "value.redacted-logical-path.redacted-member")]
    ValueRedactedMember,
    #[serde(rename = "span")]
    Span,
    #[serde(rename = "selector.absent")]
    SelectorAbsent,
    #[serde(rename = "selector.safe-profile-id")]
    SelectorSafeProfileId,
    #[serde(rename = "selector.redacted-value")]
    SelectorRedactedValue,
    #[serde(rename = "component.fixed-role")]
    FixedRole,
    #[serde(rename = "component.safe-key")]
    SafeKey,
    #[serde(rename = "component.redacted-key")]
    RedactedKey,
}

impl Component {
    /// Every component, in view order.
    pub(crate) const ALL: [Self; 19] = [
        Self::SourceSafeOpaque,
        Self::SourceRedacted,
        Self::LocatorRelativePath,
        Self::LocatorSafeLabel,
        Self::LocatorSafeUri,
        Self::PathSafeMember,
        Self::PathArrayIndex,
        Self::PathRedactedMember,
        Self::ValueExactJsonPointer,
        Self::ValueSafeMember,
        Self::ValueArrayIndex,
        Self::ValueRedactedMember,
        Self::Span,
        Self::SelectorAbsent,
        Self::SelectorSafeProfileId,
        Self::SelectorRedactedValue,
        Self::FixedRole,
        Self::SafeKey,
        Self::RedactedKey,
    ];

    /// The member the component is a variant of.
    pub(crate) const fn member(self) -> Member {
        match self {
            Self::SourceSafeOpaque | Self::SourceRedacted => Member::Source,
            Self::LocatorRelativePath | Self::LocatorSafeLabel | Self::LocatorSafeUri => {
                Member::Locator
            }
            Self::PathSafeMember | Self::PathArrayIndex | Self::PathRedactedMember => Member::Path,
            Self::ValueExactJsonPointer
            | Self::ValueSafeMember
            | Self::ValueArrayIndex
            | Self::ValueRedactedMember => Member::Value,
            Self::Span => Member::Span,
            Self::SelectorAbsent | Self::SelectorSafeProfileId | Self::SelectorRedactedValue => {
                Member::Selector
            }
            Self::FixedRole | Self::SafeKey | Self::RedactedKey => Member::Component,
        }
    }

    /// The projection whose Redacted evidence token replaces the component,
    /// for a redacted one (D15:722).
    pub(crate) const fn projection(self) -> Option<TokenKind> {
        match self {
            Self::SourceRedacted => Some(TokenKind::SourceIdentity),
            Self::PathRedactedMember => Some(TokenKind::InputPathMember),
            Self::ValueRedactedMember => Some(TokenKind::ValuePathMember),
            Self::SelectorRedactedValue => Some(TokenKind::ProfileSelector),
            Self::RedactedKey => Some(TokenKind::ComponentKey),
            _ => None,
        }
    }
}

/// One of the two disclosure policies of a component (D15:720).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Disclosure {
    NonSecretLiteral,
    RedactedStructure,
}

/// Why a literal component's content counts as non-secret.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NonSecretClass {
    /// The adapter or frontend that owns the identity's domain marks it
    /// non-secret (Decision 015-185).
    OwnerMarkedIdentity,
    /// The supplying adapter classifies the canonical locator non-secret
    /// (D15:766).
    AdapterClassifiedLocator,
    /// The member name is defined by the configuration schema (D15:776).
    SchemaDefinedName,
    /// A `profiles`, `targetProfiles`, or `deploymentGroups` key that matches
    /// the identifier syntax within the capability's byte bound (D15:776).
    IdentifierKey,
    /// A `localeNegotiation.aliases` or `messageFallback` key for which the
    /// admitted canonicalization data returns a canonical identifier
    /// (D15:776).
    CanonicalLocaleKey,
    /// A selector that exactly matches a declared Profile ID (D15:807).
    DeclaredProfileId,
    /// A vocabulary member's canonical vocabulary identity (D15:824).
    VocabularyIdentity,
    /// An admitted envelope's canonical exact-reference identity (D15:824).
    AdmittedExactReference,
}

/// How a policy discloses one component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentPolicy {
    pub(crate) component: Component,
    pub(crate) disclosure: Disclosure,
    /// For a literal component whose content is not fixed, the
    /// classifications under which that content counts as non-secret.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(min = 1), extend("uniqueItems" = true))]
    pub(crate) non_secret_when: Vec<NonSecretClass>,
    /// For a redacted component, the projection whose token replaces it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "TokenKind")]
    pub(crate) token_projection: Option<TokenKind>,
}

/// A fixed role of a component path, with its rank within its parent path
/// (D15:823).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoleShape {
    #[schemars(length(min = 1))]
    pub(crate) role: String,
    #[schemars(pattern(RANK_PATTERN))]
    pub(crate) rank: String,
}

/// The key that ends a component path shape.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum KeyKind {
    SafeKey,
    RedactedKey,
}

/// A component path shape: the origin, the fixed roles, and the key that
/// ends it, if one does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PathShape {
    pub(crate) origin: SafeOriginKind,
    pub(crate) roles: Vec<RoleShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "KeyKind")]
    pub(crate) key: Option<KeyKind>,
}

impl PathShape {
    /// The shape of `path`.
    fn of(path: &ComponentPath) -> Self {
        Self {
            origin: path.origin(),
            roles: path
                .roles()
                .into_iter()
                .map(|(role, rank)| RoleShape {
                    role: role.to_owned(),
                    rank: rank.to_string(),
                })
                .collect(),
            key: path.key_shape().map(|key| match key {
                KeyShape::Safe => KeyKind::SafeKey,
                KeyShape::Redacted => KeyKind::RedactedKey,
            }),
        }
    }

    /// Evidence order: the origin's rank, then the roles by rank with a
    /// shorter equal prefix first, then the key, `safe-key` first (D15:823,
    /// 827). `None` when a rank is not a shortest decimal string.
    fn order(&self, other: &Self) -> Option<Ordering> {
        let ranks = |shape: &Self| {
            shape
                .roles
                .iter()
                .map(|role| role.rank.parse::<u64>().ok())
                .collect::<Option<Vec<_>>>()
        };
        Some(
            self.origin
                .cmp(&other.origin)
                .then(ranks(self)?.cmp(&ranks(other)?))
                .then(self.key.cmp(&other.key)),
        )
    }
}

/// One evidence kind's disclosure policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DisclosurePolicy {
    pub(crate) evidence_kind: EvidenceKind,
    /// The members every Evidence of the kind has.
    #[schemars(length(min = 1), extend("uniqueItems" = true))]
    pub(crate) required: Vec<Member>,
    /// Each component the kind's view can retain, in view order.
    #[schemars(length(min = 1))]
    pub(crate) components: Vec<ComponentPolicy>,
    /// For a resolver-input kind, the component path shapes some retained
    /// Evidence can carry, in Evidence order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(min = 1))]
    pub(crate) paths: Vec<PathShape>,
}

/// The revision-`"0"` policies: one for each evidence kind, in 015's order.
pub(crate) fn revision_zero() -> Vec<DisclosurePolicy> {
    EvidenceKind::ALL
        .into_iter()
        .map(|kind| DisclosurePolicy {
            evidence_kind: kind,
            required: required(kind).to_vec(),
            components: components(kind),
            paths: reachable_paths(kind),
        })
        .collect()
}

/// Whether `policies` are one for each evidence kind, in 015's order, each
/// with exactly its kind's members and components, disclosed as 015
/// requires, and with path shapes the Resolver Input Component Path builds
/// for an origin `origins` lets the kind use, each once and in Evidence
/// order.
pub(crate) fn are_closed(policies: &[DisclosurePolicy], origins: &SafeOriginKindRegistry) -> bool {
    policies
        .iter()
        .map(|policy| policy.evidence_kind)
        .eq(EvidenceKind::ALL)
        && policies.iter().all(|policy| {
            let kind = policy.evidence_kind;
            policy.required == required(kind)
                && policy.components == components(kind)
                && paths_are_closed(kind, &policy.paths, origins)
        })
}

/// Whether `paths` are, for a resolver-input kind, a non-empty list of
/// shapes the grammar builds for origins the kind may use, each once and in
/// Evidence order, and for any other kind empty.
fn paths_are_closed(
    kind: EvidenceKind,
    paths: &[PathShape],
    origins: &SafeOriginKindRegistry,
) -> bool {
    if !resolver_input(kind) {
        return paths.is_empty();
    }
    let permitted = |origin: SafeOriginKind| {
        origins
            .rows
            .iter()
            .any(|row| row.id == origin && row.permitted_evidence_kinds.contains(&kind))
    };
    let grammar = grammar();
    !paths.is_empty()
        && paths
            .iter()
            .all(|shape| permitted(shape.origin) && grammar.contains(shape))
        && paths
            .windows(2)
            .all(|pair| pair[0].order(&pair[1]) == Some(Ordering::Less))
}

/// Whether Evidence of `kind` points into a resolver input.
const fn resolver_input(kind: EvidenceKind) -> bool {
    matches!(
        kind,
        EvidenceKind::SpecificationInput
            | EvidenceKind::ArtifactInput
            | EvidenceKind::VerificationInput
            | EvidenceKind::ImplementationCapability
    )
}

/// The members every Evidence of `kind` has (D15:2492).
const fn required(kind: EvidenceKind) -> &'static [Member] {
    match kind {
        EvidenceKind::EntrySource => &[Member::Source],
        EvidenceKind::ConfigurationValue => &[Member::Source, Member::Value],
        EvidenceKind::ProfileSelector => &[Member::Selector],
        EvidenceKind::SpecificationInput
        | EvidenceKind::ArtifactInput
        | EvidenceKind::VerificationInput
        | EvidenceKind::ImplementationCapability => &[Member::Component],
    }
}

/// The members `kind`'s view can have (D15:2492).
const fn members(kind: EvidenceKind) -> &'static [Member] {
    match kind {
        EvidenceKind::EntrySource => &[Member::Source, Member::Locator, Member::Path, Member::Span],
        EvidenceKind::ConfigurationValue => {
            &[Member::Source, Member::Locator, Member::Value, Member::Span]
        }
        EvidenceKind::ProfileSelector => &[Member::Selector],
        EvidenceKind::SpecificationInput
        | EvidenceKind::ArtifactInput
        | EvidenceKind::VerificationInput
        | EvidenceKind::ImplementationCapability => &[Member::Component],
    }
}

/// How `kind`'s view discloses each component it can retain, in view order.
/// Only the vocabulary and the artifact collection have keyed component
/// paths (D15:832, 834).
fn components(kind: EvidenceKind) -> Vec<ComponentPolicy> {
    Component::ALL
        .into_iter()
        .filter(|component| members(kind).contains(&component.member()))
        .filter(|component| {
            kind == EvidenceKind::ArtifactInput
                || !matches!(component, Component::SafeKey | Component::RedactedKey)
        })
        .map(|component| ComponentPolicy {
            component,
            disclosure: if component.projection().is_some() {
                Disclosure::RedactedStructure
            } else {
                Disclosure::NonSecretLiteral
            },
            non_secret_when: classes(component).to_vec(),
            token_projection: component.projection(),
        })
        .collect()
}

/// The classifications under which `component` counts as non-secret, none
/// when its content is fixed or redacted. Only Entry Source Evidence has
/// path members, and it precedes the admission of the canonicalization data,
/// so none of them is a canonical locale key (D15:776); only the artifact
/// input has keys.
const fn classes(component: Component) -> &'static [NonSecretClass] {
    use NonSecretClass as C;
    match component {
        Component::SourceSafeOpaque => &[C::OwnerMarkedIdentity],
        Component::LocatorRelativePath
        | Component::LocatorSafeLabel
        | Component::LocatorSafeUri => &[C::AdapterClassifiedLocator],
        Component::PathSafeMember => &[C::SchemaDefinedName, C::IdentifierKey],
        Component::ValueExactJsonPointer | Component::ValueSafeMember => &[
            C::SchemaDefinedName,
            C::IdentifierKey,
            C::CanonicalLocaleKey,
        ],
        Component::SelectorSafeProfileId => &[C::DeclaredProfileId],
        Component::SafeKey => &[C::VocabularyIdentity, C::AdmittedExactReference],
        _ => &[],
    }
}

/// The shapes some retained Evidence of `kind` can carry, in Evidence order:
/// every Specification Input slot; the vocabulary's root and members, the
/// canonicalization data's root, and the artifact collection's root and
/// members; the verification input's `pins`; and every `policyComparable`
/// bound with the `bootstrapOnly` bounds of `rawInput` and
/// `artifactBootstrap`, which construction and invocation check.
fn reachable_paths(kind: EvidenceKind) -> Vec<PathShape> {
    match kind {
        EvidenceKind::SpecificationInput => Slot::ALL
            .into_iter()
            .map(|slot| PathShape::of(&ComponentPath::specification(slot)))
            .collect(),
        EvidenceKind::ArtifactInput => [
            SafeOriginKind::IntentSurfaceClassVocabulary,
            SafeOriginKind::LocaleCanonicalizationData,
            SafeOriginKind::ProfileResolutionArtifactCollection,
        ]
        .into_iter()
        .flat_map(origin_shapes)
        .collect(),
        EvidenceKind::VerificationInput => {
            vec![PathShape::of(&ComponentPath::verification(
                VerificationRole::Pins,
            ))]
        }
        EvidenceKind::ImplementationCapability => CapabilityBound::inventory()
            .into_iter()
            .filter(|(bound, _)| match bound {
                CapabilityBound::PolicyComparable(_) => true,
                CapabilityBound::BootstrapOnly(id) => matches!(
                    id.group,
                    BootstrapGroup::RawInput | BootstrapGroup::ArtifactBootstrap
                ),
            })
            .filter_map(|(bound, _)| ComponentPath::capability(bound).ok())
            .map(|path| PathShape::of(&path))
            .collect(),
        EvidenceKind::EntrySource
        | EvidenceKind::ConfigurationValue
        | EvidenceKind::ProfileSelector => Vec::new(),
    }
}

/// The root of the artifact input `origin` names, and its members.
fn origin_shapes(origin: SafeOriginKind) -> Vec<PathShape> {
    ComponentPath::root(origin)
        .iter()
        .map(PathShape::of)
        .chain(member_shapes(origin))
        .collect()
}

/// For the vocabulary and the artifact collection, a member with each key:
/// an admitted identity's, then a redacted one (D15:832, 834).
fn member_shapes(origin: SafeOriginKind) -> Vec<PathShape> {
    let member = |key: Key| match origin {
        SafeOriginKind::IntentSurfaceClassVocabulary => ComponentPath::vocabulary_member(key).ok(),
        SafeOriginKind::ProfileResolutionArtifactCollection => {
            ComponentPath::artifact_member(key).ok()
        }
        _ => None,
    };
    [Key::Identity(json!("")), Key::Unsafe(JsonType::Object)]
        .into_iter()
        .filter_map(member)
        .map(|path| PathShape::of(&path))
        .collect()
}

/// Every shape the Resolver Input Component Path builds: each resolver
/// input's root, each Specification Input slot, a vocabulary or artifact
/// member with either key, each verification component, and each capability
/// bound (D15:829-836).
fn grammar() -> Vec<PathShape> {
    let roots = SafeOriginKind::ALL
        .into_iter()
        .filter_map(|origin| ComponentPath::root(origin).ok());
    let slots = Slot::ALL.into_iter().map(ComponentPath::specification);
    let members = [
        SafeOriginKind::IntentSurfaceClassVocabulary,
        SafeOriginKind::ProfileResolutionArtifactCollection,
    ]
    .into_iter()
    .flat_map(member_shapes);
    let verification = [VerificationRole::Mode, VerificationRole::Pins]
        .into_iter()
        .map(ComponentPath::verification);
    let bounds = CapabilityBound::inventory()
        .into_iter()
        .filter_map(|(bound, _)| ComponentPath::capability(bound).ok());
    roots
        .chain(slots)
        .chain(verification)
        .chain(bounds)
        .map(|path| PathShape::of(&path))
        .chain(members)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policies() -> Vec<DisclosurePolicy> {
        revision_zero()
    }

    fn origins() -> SafeOriginKindRegistry {
        SafeOriginKindRegistry::revision_zero()
    }

    fn policy(kind: EvidenceKind) -> DisclosurePolicy {
        policies()
            .into_iter()
            .find(|policy| policy.evidence_kind == kind)
            .unwrap()
    }

    fn components_of(kind: EvidenceKind) -> Vec<Component> {
        policy(kind)
            .components
            .iter()
            .map(|component| component.component)
            .collect()
    }

    #[test]
    fn the_policies_are_one_for_each_evidence_kind_in_015s_order() {
        let kinds: Vec<EvidenceKind> = policies()
            .iter()
            .map(|policy| policy.evidence_kind)
            .collect();
        assert_eq!(kinds, EvidenceKind::ALL);
        assert!(are_closed(&policies(), &origins()));
    }

    /// Each kind discloses the members of its view, and only those
    /// (D15:2492).
    #[test]
    fn each_kind_discloses_the_components_of_its_view() {
        use Component as C;
        assert_eq!(
            components_of(EvidenceKind::EntrySource),
            [
                C::SourceSafeOpaque,
                C::SourceRedacted,
                C::LocatorRelativePath,
                C::LocatorSafeLabel,
                C::LocatorSafeUri,
                C::PathSafeMember,
                C::PathArrayIndex,
                C::PathRedactedMember,
                C::Span
            ]
        );
        assert_eq!(
            components_of(EvidenceKind::ConfigurationValue),
            [
                C::SourceSafeOpaque,
                C::SourceRedacted,
                C::LocatorRelativePath,
                C::LocatorSafeLabel,
                C::LocatorSafeUri,
                C::ValueExactJsonPointer,
                C::ValueSafeMember,
                C::ValueArrayIndex,
                C::ValueRedactedMember,
                C::Span
            ]
        );
        assert_eq!(
            components_of(EvidenceKind::ProfileSelector),
            [
                C::SelectorAbsent,
                C::SelectorSafeProfileId,
                C::SelectorRedactedValue
            ]
        );
        assert_eq!(
            components_of(EvidenceKind::ArtifactInput),
            [C::FixedRole, C::SafeKey, C::RedactedKey]
        );
        for kind in [
            EvidenceKind::SpecificationInput,
            EvidenceKind::VerificationInput,
            EvidenceKind::ImplementationCapability,
        ] {
            assert_eq!(components_of(kind), [C::FixedRole]);
        }
        assert_eq!(
            policy(EvidenceKind::ConfigurationValue).required,
            [Member::Source, Member::Value]
        );
        assert_eq!(policy(EvidenceKind::EntrySource).required, [Member::Source]);
    }

    /// A redacted component names the projection whose token replaces it,
    /// and a literal one the classifications that make its content
    /// non-secret (D15:722, 776, 807, 824).
    #[test]
    fn a_component_is_disclosed_as_015_requires() {
        let find = |kind: EvidenceKind, component: Component| {
            policy(kind)
                .components
                .into_iter()
                .find(|policy| policy.component == component)
                .unwrap()
        };
        let source = find(EvidenceKind::EntrySource, Component::SourceRedacted);
        assert_eq!(source.disclosure, Disclosure::RedactedStructure);
        assert_eq!(source.token_projection, Some(TokenKind::SourceIdentity));
        assert!(source.non_secret_when.is_empty());
        assert_eq!(
            find(EvidenceKind::EntrySource, Component::PathRedactedMember).token_projection,
            Some(TokenKind::InputPathMember)
        );
        assert_eq!(
            find(
                EvidenceKind::ConfigurationValue,
                Component::ValueRedactedMember
            )
            .token_projection,
            Some(TokenKind::ValuePathMember)
        );
        assert_eq!(
            find(
                EvidenceKind::ProfileSelector,
                Component::SelectorRedactedValue
            )
            .token_projection,
            Some(TokenKind::ProfileSelector)
        );
        assert_eq!(
            find(EvidenceKind::ArtifactInput, Component::RedactedKey).token_projection,
            Some(TokenKind::ComponentKey)
        );
        let pointer = find(
            EvidenceKind::ConfigurationValue,
            Component::ValueExactJsonPointer,
        );
        assert_eq!(pointer.disclosure, Disclosure::NonSecretLiteral);
        assert_eq!(pointer.token_projection, None);
        assert_eq!(
            pointer.non_secret_when,
            [
                NonSecretClass::SchemaDefinedName,
                NonSecretClass::IdentifierKey,
                NonSecretClass::CanonicalLocaleKey
            ]
        );
        // Entry Source Evidence precedes the canonicalization data.
        assert_eq!(
            find(EvidenceKind::EntrySource, Component::PathSafeMember).non_secret_when,
            [
                NonSecretClass::SchemaDefinedName,
                NonSecretClass::IdentifierKey
            ]
        );
        assert_eq!(
            find(EvidenceKind::ArtifactInput, Component::SafeKey).non_secret_when,
            [
                NonSecretClass::VocabularyIdentity,
                NonSecretClass::AdmittedExactReference
            ]
        );
        // Fixed content needs no classification.
        for (kind, component) in [
            (EvidenceKind::EntrySource, Component::PathArrayIndex),
            (EvidenceKind::EntrySource, Component::Span),
            (EvidenceKind::ProfileSelector, Component::SelectorAbsent),
            (EvidenceKind::SpecificationInput, Component::FixedRole),
        ] {
            let policy = find(kind, component);
            assert_eq!(policy.disclosure, Disclosure::NonSecretLiteral);
            assert!(policy.non_secret_when.is_empty(), "{component:?}");
        }
    }

    /// The pinned shapes are those some retained Evidence carries: no
    /// verification `mode`, no Specification Input, verification, or
    /// capability root, and only the `bootstrapOnly` groups construction and
    /// invocation check (D15:838, 2356; Decision 015-190).
    #[test]
    fn a_resolver_input_policy_pins_only_reachable_shapes() {
        fn roles(shape: &PathShape) -> Vec<(&str, &str)> {
            shape
                .roles
                .iter()
                .map(|role| (role.role.as_str(), role.rank.as_str()))
                .collect()
        }
        let shapes = |kind: EvidenceKind| policy(kind).paths;
        let slots = shapes(EvidenceKind::SpecificationInput);
        assert_eq!(slots.len(), 8);
        assert_eq!(roles(&slots[0]), [("project-profile", "0")]);
        assert_eq!(roles(&slots[7]), [("delivery-placement", "7")]);
        let artifacts = shapes(EvidenceKind::ArtifactInput);
        assert_eq!(
            artifacts
                .iter()
                .map(|shape| (shape.origin, roles(shape).len(), shape.key))
                .collect::<Vec<_>>(),
            [
                (SafeOriginKind::IntentSurfaceClassVocabulary, 0, None),
                (
                    SafeOriginKind::IntentSurfaceClassVocabulary,
                    1,
                    Some(KeyKind::SafeKey)
                ),
                (
                    SafeOriginKind::IntentSurfaceClassVocabulary,
                    1,
                    Some(KeyKind::RedactedKey)
                ),
                (SafeOriginKind::LocaleCanonicalizationData, 0, None),
                (SafeOriginKind::ProfileResolutionArtifactCollection, 0, None),
                (
                    SafeOriginKind::ProfileResolutionArtifactCollection,
                    1,
                    Some(KeyKind::SafeKey)
                ),
                (
                    SafeOriginKind::ProfileResolutionArtifactCollection,
                    1,
                    Some(KeyKind::RedactedKey)
                ),
            ]
        );
        let verification = shapes(EvidenceKind::VerificationInput);
        assert_eq!(verification.len(), 1);
        assert_eq!(roles(&verification[0]), [("pins", "1")]);
        let capability = shapes(EvidenceKind::ImplementationCapability);
        assert_eq!(capability.len(), 52 + 2 + 4);
        assert!(capability
            .iter()
            .all(|shape| !shape.roles.is_empty() && shape.key.is_none()));
        let groups: Vec<&str> = capability
            .iter()
            .filter(|shape| shape.roles[0].role == "bootstrapOnly")
            .map(|shape| shape.roles[1].role.as_str())
            .collect();
        assert_eq!(
            groups,
            [
                "rawInput",
                "rawInput",
                "artifactBootstrap",
                "artifactBootstrap",
                "artifactBootstrap",
                "artifactBootstrap"
            ]
        );
        for kind in [
            EvidenceKind::EntrySource,
            EvidenceKind::ConfigurationValue,
            EvidenceKind::ProfileSelector,
        ] {
            assert!(shapes(kind).is_empty());
        }
    }

    /// Construction refuses policies that are not one for each kind in
    /// order, that change a member or a component, or whose shapes leave the
    /// grammar, an origin the kind may use, or Evidence order.
    #[test]
    fn policies_that_depart_from_015_are_not_closed() {
        let origins = origins();
        let closed = |policies: &[DisclosurePolicy]| are_closed(policies, &origins);
        let with = |change: &dyn Fn(&mut Vec<DisclosurePolicy>)| {
            let mut policies = policies();
            change(&mut policies);
            policies
        };
        assert!(!closed(&with(&|policies| {
            policies.pop();
        })));
        assert!(!closed(&with(&|policies| policies.swap(0, 1))));
        assert!(!closed(&with(&|policies| {
            policies[1].required.pop();
        })));
        assert!(!closed(&with(&|policies| {
            policies[0].components.pop();
        })));
        assert!(!closed(&with(&|policies| {
            policies[0].components[1].disclosure = Disclosure::NonSecretLiteral;
        })));
        assert!(!closed(&with(&|policies| {
            policies[0].components[1].token_projection = Some(TokenKind::InputPathMember);
        })));
        assert!(!closed(&with(&|policies| {
            policies[1].components[5].non_secret_when.pop();
        })));
        // A path on a kind that has none, and none on a resolver-input kind.
        assert!(!closed(&with(&|policies| {
            let shape = policies[3].paths[0].clone();
            policies[1].paths.push(shape);
        })));
        assert!(!closed(&with(&|policies| policies[5].paths.clear())));
        // A shape the grammar does not build: a wrong rank, an unknown role,
        // or a key on a slot.
        assert!(!closed(&with(&|policies| {
            policies[3].paths[0].roles[0].rank = "1".to_owned();
        })));
        assert!(!closed(&with(&|policies| {
            policies[5].paths[0].roles[0].role = "secret".to_owned();
        })));
        assert!(!closed(&with(&|policies| {
            policies[3].paths[0].key = Some(KeyKind::SafeKey);
        })));
        // An origin the kind may not use, a repeated shape, and Evidence
        // order broken.
        assert!(!closed(&with(&|policies| {
            policies[4].paths[0].origin = SafeOriginKind::ProfileResolutionSpecificationInput;
        })));
        assert!(!closed(&with(&|policies| {
            let shape = policies[4].paths[0].clone();
            policies[4].paths.insert(0, shape);
        })));
        assert!(!closed(&with(&|policies| policies[4].paths.swap(0, 1))));
        // The verification `mode` and a capability root are in the grammar,
        // so their shapes alone do not break the closed graph; the rows show
        // which shapes are reachable.
        assert!(closed(&with(&|policies| {
            let mode = PathShape::of(&ComponentPath::verification(VerificationRole::Mode));
            policies[5].paths.insert(0, mode);
        })));
    }

    /// Every shape the policies pin is one the grammar builds.
    #[test]
    fn every_pinned_shape_is_one_the_grammar_builds() {
        let grammar = grammar();
        for policy in policies() {
            for shape in &policy.paths {
                assert!(grammar.contains(shape), "{shape:?}");
            }
        }
        // Six roots, eight slots, two verification components, every bound,
        // and the vocabulary's and the collection's keyed members.
        assert_eq!(grammar.len(), 6 + 8 + 2 + 106 + 4);
    }
}
