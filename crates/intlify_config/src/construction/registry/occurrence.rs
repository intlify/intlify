// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Finding Registry rows' occurrence Evidence columns (D15:2242-2245,
//! 2249, 2251; Decisions 015-189 to 015-193).
//!
//! For each check that may report its code, in `allowedCheckIds` order, a
//! row names the primary evidence kind of the check's Finding Occurrences,
//! or `none` for the terminal reporting Finding, which has no occurrence,
//! and the related evidence kinds, without repeats and in the order of the
//! admitted evidence kinds (D15:2249). For each kind those name, in that
//! order, the row names the kind's disclosure policy with, for a
//! resolver-input kind, the component path shapes its occurrences can carry,
//! a subset of the shapes the policy pins, and the Safe Origin Kinds it
//! allows the kind, a non-empty subset of the Safe Origin Kind Registry's
//! compatibility table (D15:716, 2251).
//!
//! The table below gives each (check, code) pair of the Check Registry its
//! Evidence. Where 015 names an occurrence's Evidence, the table follows it:
//! a root-value bound applied before materialization is `entry-source`, an
//! over-limit selector `profile-selector`, and a bound over the materialized
//! configuration `configuration-value` (D15:2249); a selector Finding
//! relates the applicable declarations as one location (D15:812); the
//! bootstrap's mismatch candidates are related (D15:1734); and each
//! conflicting locale spelling is primary with the others related
//! (D15:2410). Elsewhere the primary Evidence is where the check observes
//! the value its Finding reports, and the related Evidence every other
//! location it compares with that value, a set counting as one (Decision
//! 015-191): the source of an exceeded bound, which is the capability until
//! the Resource Limit Policy is admitted and the admitted Policy after; the
//! configuration reference through which the check reaches a primary
//! resolver input (D15:714); the `pins` that do not authenticate a
//! candidate; and the collection a reference fails to resolve in. A related
//! record equal to the primary one is omitted (Decision 015-193), so a
//! member that only a redacted key names relates no other member. What 015
//! leaves open is read as plan 025 records it: a total derived from several
//! configuration locations is `configuration-value`, with the bound source
//! alone related; the locale canonicalization data, which validates a
//! spelling rather than being compared with it, is not related; a Target
//! Profile's declared directions and roles are a set its target must belong
//! to; and a hydration relation is primary, with the Target Profiles outside
//! it related.
//!
//! Construction reads the columns with the registry cross-references (row
//! 41): each per-check column lists the row's checks in order; only
//! `project-profile-reporting-limit-exceeded` has the primary kind `none`,
//! and then no related kind; related kinds are in the admitted order without
//! repeats; the policies and origins name exactly the kinds the per-check
//! columns name; a resolver-input kind's shapes are ones its policy pins, in
//! Evidence order, and no other kind has any; the origins are ones the
//! admitted Safe Origin Kind Registry permits the kind, in rank order, and
//! for a resolver-input kind exactly those of its shapes; and every shape a
//! policy pins is one some row, or Coverage Decision Evidence, can carry
//! (D15:838, 1609, 2274).

use std::cmp::Ordering;
use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::resolver::capability::CapabilityBound;
use crate::resolver::evaluation::Code;
use crate::resolver::evidence_component::{ComponentPath, VerificationRole};
use crate::resolver::origin::{EvidenceKind, SafeOriginKind};
use crate::resolver::specification::Slot;

use super::check::CHECK_ID_PATTERN;
use super::finding::FindingRow;
use super::policy::{self, DisclosurePolicy, KeyKind, PathShape};
use super::safe_origin::SafeOriginKindRegistry;

/// A check's primary evidence kind, or `none` for the terminal reporting
/// Finding (D15:2249).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PrimaryKind {
    EntrySource,
    ConfigurationValue,
    ProfileSelector,
    SpecificationInput,
    ArtifactInput,
    VerificationInput,
    ImplementationCapability,
    None,
}

impl PrimaryKind {
    /// The primary kind `kind` names, `none` for no kind.
    const fn of(kind: Option<EvidenceKind>) -> Self {
        match kind {
            Some(EvidenceKind::EntrySource) => Self::EntrySource,
            Some(EvidenceKind::ConfigurationValue) => Self::ConfigurationValue,
            Some(EvidenceKind::ProfileSelector) => Self::ProfileSelector,
            Some(EvidenceKind::SpecificationInput) => Self::SpecificationInput,
            Some(EvidenceKind::ArtifactInput) => Self::ArtifactInput,
            Some(EvidenceKind::VerificationInput) => Self::VerificationInput,
            Some(EvidenceKind::ImplementationCapability) => Self::ImplementationCapability,
            None => Self::None,
        }
    }

    /// The evidence kind, none for `none`.
    const fn kind(self) -> Option<EvidenceKind> {
        match self {
            Self::EntrySource => Some(EvidenceKind::EntrySource),
            Self::ConfigurationValue => Some(EvidenceKind::ConfigurationValue),
            Self::ProfileSelector => Some(EvidenceKind::ProfileSelector),
            Self::SpecificationInput => Some(EvidenceKind::SpecificationInput),
            Self::ArtifactInput => Some(EvidenceKind::ArtifactInput),
            Self::VerificationInput => Some(EvidenceKind::VerificationInput),
            Self::ImplementationCapability => Some(EvidenceKind::ImplementationCapability),
            Self::None => None,
        }
    }
}

/// One check's primary evidence kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrimaryEntry {
    #[schemars(pattern(CHECK_ID_PATTERN))]
    pub(crate) check_id: String,
    pub(crate) kind: PrimaryKind,
}

/// One check's related evidence kinds, in the order of the admitted
/// evidence kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RelatedEntry {
    #[schemars(pattern(CHECK_ID_PATTERN))]
    pub(crate) check_id: String,
    #[schemars(extend("uniqueItems" = true))]
    pub(crate) kinds: Vec<EvidenceKind>,
}

/// The disclosure policy of one evidence kind the row names, with, for a
/// resolver-input kind, the component path shapes its occurrences can
/// carry, in Evidence order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PolicyEntry {
    pub(crate) evidence_kind: EvidenceKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(min = 1))]
    pub(crate) paths: Vec<PathShape>,
}

/// The Safe Origin Kinds the row allows one evidence kind, in rank order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OriginEntry {
    pub(crate) evidence_kind: EvidenceKind,
    #[schemars(length(min = 1), extend("uniqueItems" = true))]
    pub(crate) origins: Vec<SafeOriginKind>,
}

/// A row's four occurrence Evidence columns.
pub(crate) struct Columns {
    pub(crate) primary: Vec<PrimaryEntry>,
    pub(crate) related: Vec<RelatedEntry>,
    pub(crate) policies: Vec<PolicyEntry>,
    pub(crate) origins: Vec<OriginEntry>,
}

/// A component path shape, as the table names it.
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// A Specification Input slot.
    Slot(Slot),
    /// An artifact input's root.
    Root(SafeOriginKind),
    /// A vocabulary or collection member whose key is of this kind.
    Member(SafeOriginKind, KeyKind),
    /// The verification input's `pins`.
    Pins,
    /// One capability bound, by its path.
    Bound(&'static str),
    /// Every `policyComparable` bound.
    EveryPolicyComparable,
}

impl Shape {
    /// The path shapes the shape names.
    fn paths(self) -> Vec<PathShape> {
        let capability = |keep: &dyn Fn(CapabilityBound) -> bool| {
            CapabilityBound::inventory()
                .into_iter()
                .filter(|(bound, _)| keep(*bound))
                .filter_map(|(bound, _)| ComponentPath::capability(bound).ok())
                .map(|path| PathShape::of(&path))
                .collect()
        };
        match self {
            Self::Slot(slot) => vec![PathShape::of(&ComponentPath::specification(slot))],
            Self::Root(origin) => ComponentPath::root(origin)
                .iter()
                .map(PathShape::of)
                .collect(),
            Self::Member(origin, key) => policy::member_shape(origin, key).into_iter().collect(),
            Self::Pins => vec![PathShape::of(&ComponentPath::verification(
                VerificationRole::Pins,
            ))],
            Self::Bound(path) => capability(&|bound| bound.to_string() == path),
            Self::EveryPolicyComparable => {
                capability(&|bound| matches!(bound, CapabilityBound::PolicyComparable(_)))
            }
        }
    }
}

/// The Evidence one check's occurrences of one code carry: the primary
/// kind, none for `none`; the related kinds; the origins of its Entry
/// Source Evidence; and the shapes of its resolver-input Evidence.
struct Pair {
    check: &'static str,
    code: Code,
    primary: Option<EvidenceKind>,
    related: &'static [EvidenceKind],
    entry: &'static [SafeOriginKind],
    shapes: &'static [Shape],
}

impl Pair {
    /// The kinds the pair names, the primary one first.
    fn kinds(&self) -> impl Iterator<Item = EvidenceKind> + '_ {
        self.primary.into_iter().chain(self.related.iter().copied())
    }

    /// The pair's path shapes into an input of `kind`.
    fn paths(&self, kind: EvidenceKind) -> Vec<PathShape> {
        self.shapes
            .iter()
            .flat_map(|shape| shape.paths())
            .filter(|shape| shape.origin.permitted_evidence_kinds().contains(&kind))
            .collect()
    }

    /// The origins the pair's Evidence of `kind` can come through: the
    /// entry's for Entry Source Evidence, either configuration entry for a
    /// value, either selector boundary for a selector, and those of its
    /// shapes for a resolver input.
    fn origins(&self, kind: EvidenceKind) -> BTreeSet<SafeOriginKind> {
        match kind {
            EvidenceKind::EntrySource => self.entry.iter().copied().collect(),
            EvidenceKind::ConfigurationValue => CONFIGURATION.iter().copied().collect(),
            EvidenceKind::ProfileSelector => SELECTOR.iter().copied().collect(),
            _ => self.paths(kind).iter().map(|shape| shape.origin).collect(),
        }
    }
}

const ES: EvidenceKind = EvidenceKind::EntrySource;
const CV: EvidenceKind = EvidenceKind::ConfigurationValue;
const PS: EvidenceKind = EvidenceKind::ProfileSelector;
const SI: EvidenceKind = EvidenceKind::SpecificationInput;
const AI: EvidenceKind = EvidenceKind::ArtifactInput;
const VI: EvidenceKind = EvidenceKind::VerificationInput;
const IC: EvidenceKind = EvidenceKind::ImplementationCapability;

const CONFIGURATION: &[SafeOriginKind] = &[
    SafeOriginKind::ConfigurationFile,
    SafeOriginKind::ConfigurationProgrammatic,
];
const SELECTOR: &[SafeOriginKind] = &[
    SafeOriginKind::ProfileSelectorCli,
    SafeOriginKind::ProfileSelectorProgrammatic,
];
const VOCABULARY: SafeOriginKind = SafeOriginKind::IntentSurfaceClassVocabulary;
const LOCALE_DATA: SafeOriginKind = SafeOriginKind::LocaleCanonicalizationData;
const COLLECTION: SafeOriginKind = SafeOriginKind::ProfileResolutionArtifactCollection;

/// A member of the Profile Resolution Artifact Set, or the admitted Resource
/// Limit Policy, whose exact reference is its key (D15:824).
const ADMITTED: Shape = Shape::Member(COLLECTION, KeyKind::SafeKey);
/// A collection member not admitted, which only a redacted key names
/// (D15:834).
const CANDIDATE: Shape = Shape::Member(COLLECTION, KeyKind::RedactedKey);
/// A vocabulary member that establishes its identity.
const VOCABULARY_MEMBER: Shape = Shape::Member(VOCABULARY, KeyKind::SafeKey);
/// An invalid vocabulary member.
const REDACTED_VOCABULARY_MEMBER: Shape = Shape::Member(VOCABULARY, KeyKind::RedactedKey);

/// A pair whose Entry Source Evidence comes through either configuration
/// entry.
const fn pair(
    check: &'static str,
    code: Code,
    primary: EvidenceKind,
    related: &'static [EvidenceKind],
    shapes: &'static [Shape],
) -> Pair {
    Pair {
        check,
        code,
        primary: Some(primary),
        related,
        entry: CONFIGURATION,
        shapes,
    }
}

/// A pair of a raw-file check, whose Entry Source Evidence comes only
/// through a configuration file.
const fn raw(
    check: &'static str,
    code: Code,
    primary: EvidenceKind,
    related: &'static [EvidenceKind],
    shapes: &'static [Shape],
) -> Pair {
    Pair {
        check,
        code,
        primary: Some(primary),
        related,
        entry: &[SafeOriginKind::ConfigurationFile],
        shapes,
    }
}

/// The pair of the terminal reporting Finding, which has no occurrence.
const fn none(check: &'static str, code: Code) -> Pair {
    Pair {
        check,
        code,
        primary: None,
        related: &[],
        entry: CONFIGURATION,
        shapes: &[],
    }
}

/// Every (check, code) pair of the Check Registry, in its order.
#[rustfmt::skip]
const PAIRS: [Pair; 180] = {
    use Code as C;
    use Shape as S;
    [
        // resolver-bootstrap-admission
        pair("specification.project-profile.supported", C::SpecificationInputIncomplete, SI, &[], &[S::Slot(Slot::ProjectProfile)]),
        pair("specification.project-profile.supported", C::SpecificationNotAdmitted, SI, &[], &[S::Slot(Slot::ProjectProfile)]),
        // entry-admission
        raw("entry.input.within-file-bytes", C::BootstrapLimitExceeded, ES, &[IC], &[S::Bound("bootstrapOnly.rawInput.maxFileBytes")]),
        raw("entry.input.within-parser-tokens", C::BootstrapLimitExceeded, ES, &[IC], &[S::Bound("bootstrapOnly.rawInput.maxParserTokens")]),
        raw("entry.input.well-formed-json", C::JsonInvalid, ES, &[], &[]),
        raw("entry.input.unique-members", C::JsonMemberDuplicate, ES, &[ES], &[]),
        pair("entry.input.json-compatible", C::InputNotJsonCompatible, ES, &[], &[]),
        pair("entry.input.within-nodes", C::ResourceLimitExceeded, ES, &[IC], &[S::Bound("policyComparable.configurationInput.maxNodes")]),
        pair("entry.input.within-depth", C::ResourceLimitExceeded, ES, &[IC], &[S::Bound("policyComparable.configurationInput.maxDepth")]),
        pair("entry.input.within-collection-entries", C::ResourceLimitExceeded, ES, &[IC], &[S::Bound("policyComparable.configurationInput.maxCollectionEntries")]),
        pair("entry.input.within-total-string-bytes", C::ResourceLimitExceeded, ES, &[IC], &[S::Bound("policyComparable.configurationInput.maxTotalStringBytes")]),
        pair("entry.input.within-single-string-bytes", C::ResourceLimitExceeded, ES, &[IC], &[S::Bound("policyComparable.configurationInput.maxSingleStringBytes")]),
        // structural-admission
        pair("structural.schema-selection.object-root", C::FieldTypeInvalid, CV, &[], &[]),
        pair("structural.schema-selection.present-schema-version", C::RequiredFieldMissing, CV, &[], &[]),
        pair("structural.schema-selection.string-schema-version", C::FieldTypeInvalid, CV, &[], &[]),
        pair("structural.schema-selection.supported-schema-version", C::SchemaVersionUnsupported, CV, &[], &[]),
        pair("structural.analysis.within-capability", C::ResourceLimitExceeded, CV, &[IC], &[S::Bound("policyComparable.configurationInput.maxStructuralAnalysisUnits")]),
        pair("structural.root.required-field", C::RequiredFieldMissing, CV, &[], &[]),
        pair("structural.root.field-type", C::FieldTypeInvalid, CV, &[], &[]),
        pair("structural.root.field-value", C::FieldValueInvalid, CV, &[], &[]),
        pair("structural.root.known-field", C::UnknownField, CV, &[], &[]),
        pair("structural.profile-key.field-value", C::FieldValueInvalid, CV, &[], &[]),
        pair("structural.profile.required-field", C::RequiredFieldMissing, CV, &[], &[]),
        pair("structural.profile.field-type", C::FieldTypeInvalid, CV, &[], &[]),
        pair("structural.profile.field-value", C::FieldValueInvalid, CV, &[], &[]),
        pair("structural.profile.known-field", C::UnknownField, CV, &[], &[]),
        pair("structural.target-key.field-value", C::FieldValueInvalid, CV, &[], &[]),
        pair("structural.target.required-field", C::RequiredFieldMissing, CV, &[], &[]),
        pair("structural.target.field-type", C::FieldTypeInvalid, CV, &[], &[]),
        pair("structural.target.field-value", C::FieldValueInvalid, CV, &[], &[]),
        pair("structural.target.known-field", C::UnknownField, CV, &[], &[]),
        pair("structural.group-key.field-value", C::FieldValueInvalid, CV, &[], &[]),
        pair("structural.group.required-field", C::RequiredFieldMissing, CV, &[], &[]),
        pair("structural.group.field-type", C::FieldTypeInvalid, CV, &[], &[]),
        pair("structural.group.field-value", C::FieldValueInvalid, CV, &[], &[]),
        pair("structural.group.known-field", C::UnknownField, CV, &[], &[]),
        // profile-selection
        pair("selection.profiles.within-capability", C::ResourceLimitExceeded, CV, &[IC], &[S::Bound("policyComparable.configurationInput.maxProfiles")]),
        pair("selection.profile-id.within-capability", C::ResourceLimitExceeded, CV, &[IC], &[S::Bound("policyComparable.configurationInput.maxProfileIdBytes")]),
        pair("selection.selector.within-capability", C::ResourceLimitExceeded, PS, &[CV, IC], &[S::Bound("policyComparable.configurationInput.maxProfileIdBytes")]),
        pair("selection.selector.required", C::SelectorRequired, PS, &[CV], &[]),
        pair("selection.selector.valid", C::SelectorInvalid, PS, &[CV], &[]),
        pair("selection.selector.declared", C::SelectorUnknown, PS, &[CV], &[]),
        // resource-policy-admission
        pair("resource-policy.artifacts.within-capability", C::ResourceLimitExceeded, AI, &[IC], &[S::Root(COLLECTION), S::Bound("policyComparable.artifactAdmission.maxArtifacts")]),
        pair("resource-policy.policy-artifacts.within-capability", C::ResourceLimitExceeded, AI, &[IC], &[S::Root(COLLECTION), S::Bound("policyComparable.artifactAdmission.maxPolicyArtifacts")]),
        pair("resource-policy.target-profile-artifacts.within-capability", C::ResourceLimitExceeded, AI, &[IC], &[S::Root(COLLECTION), S::Bound("policyComparable.artifactAdmission.maxTargetProfileArtifacts")]),
        pair("resource-policy.references.within-capability", C::ResourceLimitExceeded, CV, &[IC], &[S::Bound("policyComparable.artifactAdmission.maxArtifactReferences")]),
        pair("resource-policy.artifact.within-capability", C::ResourceLimitExceeded, AI, &[IC], &[CANDIDATE, S::Bound("policyComparable.artifactAdmission.maxSingleArtifactCanonicalBytes")]),
        pair("resource-policy.artifact-bytes.within-capability", C::ResourceLimitExceeded, AI, &[IC], &[S::Root(COLLECTION), S::Bound("policyComparable.artifactAdmission.maxTotalArtifactCanonicalBytes")]),
        pair("resource-policy.policy-reference.within-capability", C::ResourceLimitExceeded, CV, &[IC], &[S::Bound("policyComparable.artifactAdmission.maxSingleReferenceCanonicalBytes")]),
        pair("resource-policy.target-profile-reference.within-capability", C::ResourceLimitExceeded, CV, &[IC], &[S::Bound("policyComparable.artifactAdmission.maxSingleReferenceCanonicalBytes")]),
        pair("resource-policy.reference-bytes.within-capability", C::ResourceLimitExceeded, CV, &[IC], &[S::Bound("policyComparable.artifactAdmission.maxTotalReferenceCanonicalBytes")]),
        pair("resource-policy.resource-limits.within-metadata-scan", C::BootstrapLimitExceeded, AI, &[IC], &[S::Root(COLLECTION), S::Bound("bootstrapOnly.artifactBootstrap.maxMetadataScanUnits")]),
        pair("resource-policy.resource-limits.bootstrap-candidate", C::ArtifactReferenceNotAdmitted, CV, &[AI], &[CANDIDATE, S::Root(COLLECTION)]),
        pair("resource-policy.resource-limits.bootstrap-candidate", C::ArtifactCollectionNotAdmitted, AI, &[CV], &[CANDIDATE]),
        pair("resource-policy.resource-limits.within-envelope-depth", C::BootstrapLimitExceeded, AI, &[CV, IC], &[CANDIDATE, S::Bound("bootstrapOnly.artifactBootstrap.maxEnvelopeDepth")]),
        pair("resource-policy.resource-limits.within-decoded-allocation", C::BootstrapLimitExceeded, AI, &[CV, IC], &[CANDIDATE, S::Bound("bootstrapOnly.artifactBootstrap.maxDecodedAllocationBytes")]),
        pair("resource-policy.resource-limits.within-validation-work", C::BootstrapLimitExceeded, AI, &[CV, IC], &[CANDIDATE, S::Bound("bootstrapOnly.artifactBootstrap.maxValidationWorkUnits")]),
        pair("resource-policy.resource-limits.candidate-body", C::ResourcePolicyNotAdmitted, AI, &[CV, VI], &[CANDIDATE, S::Pins]),
        pair("resource-policy.resource-limits.compatible", C::ResourcePolicyCapabilityIncompatible, AI, &[CV, IC], &[ADMITTED, S::EveryPolicyComparable]),
        pair("resource-policy.root-nodes.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.root-depth.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.root-collection-entries.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.root-string-bytes.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.root-longest-string.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.profiles.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.profile-id.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.selector.within-bound", C::ResourceLimitExceeded, PS, &[CV, AI], &[ADMITTED]),
        pair("resource-policy.structural-analysis.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.artifacts.within-bound", C::ResourceLimitExceeded, AI, &[AI], &[S::Root(COLLECTION), ADMITTED]),
        pair("resource-policy.policy-artifacts.within-bound", C::ResourceLimitExceeded, AI, &[AI], &[S::Root(COLLECTION), ADMITTED]),
        pair("resource-policy.target-profile-artifacts.within-bound", C::ResourceLimitExceeded, AI, &[AI], &[S::Root(COLLECTION), ADMITTED]),
        pair("resource-policy.references.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.artifact.within-bound", C::ResourceLimitExceeded, AI, &[AI], &[CANDIDATE, ADMITTED]),
        pair("resource-policy.artifact-bytes.within-bound", C::ResourceLimitExceeded, AI, &[AI], &[S::Root(COLLECTION), ADMITTED]),
        pair("resource-policy.policy-reference.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.target-profile-reference.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("resource-policy.reference-bytes.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        // specification-admission
        pair("specification.locale-canonicalization.supported", C::SpecificationInputIncomplete, SI, &[], &[S::Slot(Slot::LocaleCanonicalization)]),
        pair("specification.locale-canonicalization.supported", C::LocaleSpecificationNotAdmitted, SI, &[], &[S::Slot(Slot::LocaleCanonicalization)]),
        pair("specification.locale-negotiation.supported", C::SpecificationInputIncomplete, SI, &[], &[S::Slot(Slot::LocaleNegotiation)]),
        pair("specification.locale-negotiation.supported", C::NegotiationSpecificationNotAdmitted, SI, &[], &[S::Slot(Slot::LocaleNegotiation)]),
        pair("specification.message-locale-fallback.supported", C::SpecificationInputIncomplete, SI, &[], &[S::Slot(Slot::MessageLocaleFallback)]),
        pair("specification.message-locale-fallback.supported", C::MessageFallbackSpecificationNotAdmitted, SI, &[], &[S::Slot(Slot::MessageLocaleFallback)]),
        pair("specification.coverage.supported", C::SpecificationInputIncomplete, SI, &[], &[S::Slot(Slot::Coverage)]),
        pair("specification.coverage.supported", C::CoverageSpecificationNotAdmitted, SI, &[], &[S::Slot(Slot::Coverage)]),
        pair("specification.intent-surface-class-vocabulary.supported", C::SpecificationInputIncomplete, SI, &[], &[S::Slot(Slot::IntentSurfaceClassVocabulary)]),
        pair("specification.delivery-graph.supported", C::SpecificationInputIncomplete, SI, &[], &[S::Slot(Slot::DeliveryGraph)]),
        pair("specification.delivery-graph.supported", C::DeliverySpecificationNotAdmitted, SI, &[], &[S::Slot(Slot::DeliveryGraph)]),
        pair("specification.delivery-placement.supported", C::SpecificationInputIncomplete, SI, &[], &[S::Slot(Slot::DeliveryPlacement)]),
        pair("specification.delivery-placement.supported", C::DeliverySpecificationNotAdmitted, SI, &[], &[S::Slot(Slot::DeliveryPlacement)]),
        pair("specification.vocabulary.intact", C::IntentSurfaceClassVocabularyNotAdmitted, AI, &[], &[S::Root(VOCABULARY), VOCABULARY_MEMBER, REDACTED_VOCABULARY_MEMBER]),
        pair("specification.vocabulary.asserted", C::IntentSurfaceClassVocabularyNotAdmitted, AI, &[SI], &[S::Root(VOCABULARY), S::Slot(Slot::IntentSurfaceClassVocabulary)]),
        pair("specification.locale-data.matching", C::LocaleDataNotAdmitted, AI, &[SI], &[S::Root(LOCALE_DATA), S::Slot(Slot::LocaleCanonicalization)]),
        // locale-resolution
        pair("locale.occurrences.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.requested.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("locale.requested.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("locale.requested.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.requested.unique", C::LocaleDuplicate, CV, &[CV], &[]),
        pair("locale.requested-set.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.default-source.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("locale.default-source.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("locale.default-source.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.default-requested.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("locale.default-requested.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("locale.default-requested.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.default-requested.member", C::DefaultRequestedLocaleNotInProject, CV, &[CV], &[]),
        pair("locale.alias-key.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("locale.alias-key.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("locale.alias-key.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.alias-destination.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("locale.alias-destination.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("locale.alias-destination.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.fallback-key.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("locale.fallback-key.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("locale.fallback-key.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.target-profiles.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.target-id.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.target-requested.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("locale.target-requested.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("locale.target-requested.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("locale.target-requested.unique", C::LocaleDuplicate, CV, &[CV], &[]),
        pair("locale.target-default-requested.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("locale.target-default-requested.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("locale.target-default-requested.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        // locale-policy-resolution
        pair("negotiation.aliases.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("negotiation.alias-key.unique", C::NegotiationAliasDuplicate, CV, &[CV], &[]),
        pair("negotiation.destination.member", C::NegotiationAliasDestinationNotRequested, CV, &[CV], &[]),
        pair("fallback.sources.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("fallback.source.unique", C::LocaleDuplicate, CV, &[CV], &[]),
        pair("fallback.source.member", C::MessageFallbackSourceNotRequested, CV, &[CV], &[]),
        pair("fallback.sequence.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("fallback.candidate.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("fallback.candidate.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("fallback.candidate.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("fallback.candidate.distinct-from-source", C::MessageFallbackSequenceInvalid, CV, &[CV], &[]),
        pair("fallback.literal.unique", C::MessageFallbackSequenceInvalid, CV, &[CV], &[]),
        pair("fallback.intent-source.unique", C::MessageFallbackSequenceInvalid, CV, &[CV], &[]),
        pair("coverage.rules.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("coverage.locale-selectors.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("coverage.surface-selectors.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("coverage.locale.canonical", C::LocaleInvalid, CV, &[], &[]),
        pair("coverage.locale.canonical", C::LocaleNonCanonical, CV, &[], &[]),
        pair("coverage.locale.canonical", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("coverage.locale.unique", C::LocaleDuplicate, CV, &[CV], &[]),
        pair("coverage.locale.member", C::CoverageRuleInvalid, CV, &[CV], &[]),
        pair("coverage.surface-class.vocabulary-member", C::CoverageRuleInvalid, CV, &[AI], &[S::Root(VOCABULARY)]),
        pair("coverage.surface-class.unique", C::CoverageRuleInvalid, CV, &[CV], &[]),
        pair("coverage.domain.unique", C::CoverageRuleDuplicate, CV, &[CV], &[]),
        pair("coverage.cells.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("coverage.comparisons.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("coverage.cell.effective-mode", C::CoverageRuleConflict, CV, &[CV], &[]),
        // artifact-reference-resolution
        pair("artifact.member.admissible", C::ArtifactCollectionNotAdmitted, AI, &[], &[CANDIDATE]),
        pair("artifact.member.single-occurrence", C::ArtifactCollectionNotAdmitted, AI, &[], &[CANDIDATE]),
        pair("artifact.conflict-group.consistent-content", C::ArtifactCollectionNotAdmitted, AI, &[], &[CANDIDATE]),
        pair("artifact.policy-reference.resolved", C::ArtifactReferenceNotAdmitted, CV, &[AI], &[ADMITTED, S::Root(COLLECTION)]),
        pair("artifact.target-profile-reference.resolved", C::ArtifactReferenceNotAdmitted, CV, &[AI], &[ADMITTED, S::Root(COLLECTION)]),
        // target-resolution
        pair("target.requested-locales.project-subset", C::TargetLocalesNotSubset, CV, &[CV], &[]),
        pair("target.effective-default.supported", C::TargetDefaultLocaleNotSupported, CV, &[CV], &[]),
        pair("target.requested-locales.directed", C::TargetLocaleDirectionMissing, CV, &[AI], &[ADMITTED]),
        // group-resolution
        pair("group.groups.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("group.memberships.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("group.group-id.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("group.members.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("group.member.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("group.member.known", C::GroupMemberUnknown, CV, &[CV], &[]),
        pair("group.member.unique", C::GroupMemberDuplicate, CV, &[CV], &[]),
        pair("group.target.single-group", C::TargetUnassigned, CV, &[CV], &[]),
        pair("group.target.single-group", C::TargetAssignedToMultipleGroups, CV, &[CV], &[]),
        // hydration-validation
        pair("hydration.relations.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("hydration.endpoint.within-bound", C::ResourceLimitExceeded, CV, &[AI], &[ADMITTED]),
        pair("hydration.endpoint.known", C::HydrationRelationInvalid, CV, &[CV], &[]),
        pair("hydration.endpoint.same-group", C::HydrationRelationInvalid, CV, &[CV], &[]),
        pair("hydration.endpoint.role", C::HydrationRelationInvalid, CV, &[AI], &[ADMITTED]),
        pair("hydration.relation.distinct-endpoints", C::HydrationRelationInvalid, CV, &[], &[]),
        pair("hydration.relation.unique", C::HydrationRelationInvalid, CV, &[CV], &[]),
        pair("hydration.relation.same-requested-locales", C::HydrationLocalePolicyIncompatible, CV, &[CV], &[]),
        pair("hydration.relation.same-effective-default", C::HydrationLocalePolicyIncompatible, CV, &[CV], &[]),
        pair("hydration.relation.static-compatibility", C::HydrationCapabilityIncompatible, CV, &[AI], &[ADMITTED]),
        pair("hydration.target.single-role", C::HydrationRelationInvalid, CV, &[CV], &[]),
        // reporting-finalization
        none("reporting.diagnostic-domain.complete", C::ReportingLimitExceeded),
    ]
};

/// The pair of `check` and `code`.
fn find(check: &str, code: Code) -> Option<&'static Pair> {
    PAIRS
        .iter()
        .find(|pair| pair.check == check && pair.code == code)
}

/// Shapes sorted in Evidence order, each once.
fn ordered(mut shapes: Vec<PathShape>) -> Vec<PathShape> {
    shapes.sort_by(|left, right| left.order(right).unwrap_or(Ordering::Equal));
    shapes.dedup();
    shapes
}

/// The four columns of `code`'s row, whose checks are `check_ids`.
///
/// # Panics
///
/// When the table has no pair of a check and `code`, which the tests rule
/// out for every pair of the Check Registry.
pub(crate) fn columns(code: Code, check_ids: &[String]) -> Columns {
    let pairs: Vec<&Pair> = check_ids
        .iter()
        .map(|check| find(check, code).expect("the table has every pair of the Check Registry"))
        .collect();
    let kinds: BTreeSet<EvidenceKind> = pairs.iter().flat_map(|pair| pair.kinds()).collect();
    let naming = |kind: EvidenceKind| {
        pairs
            .iter()
            .filter(move |pair| pair.kinds().any(|named| named == kind))
    };
    Columns {
        primary: pairs
            .iter()
            .map(|pair| PrimaryEntry {
                check_id: pair.check.to_owned(),
                kind: PrimaryKind::of(pair.primary),
            })
            .collect(),
        related: pairs
            .iter()
            .map(|pair| RelatedEntry {
                check_id: pair.check.to_owned(),
                kinds: pair.related.to_vec(),
            })
            .collect(),
        policies: kinds
            .iter()
            .map(|&kind| PolicyEntry {
                evidence_kind: kind,
                paths: if policy::resolver_input(kind) {
                    ordered(naming(kind).flat_map(|pair| pair.paths(kind)).collect())
                } else {
                    Vec::new()
                },
            })
            .collect(),
        origins: kinds
            .iter()
            .map(|&kind| OriginEntry {
                evidence_kind: kind,
                origins: naming(kind)
                    .flat_map(|pair| pair.origins(kind))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            })
            .collect(),
    }
}

/// Whether `row`'s four columns are as 015 requires, under the admitted
/// `policies` and `origins`.
pub(crate) fn row_is_closed(
    row: &FindingRow,
    policies: &[DisclosurePolicy],
    origins: &SafeOriginKindRegistry,
) -> bool {
    let checks = row.allowed_check_ids.iter().map(String::as_str);
    let primary = &row.occurrence_primary_evidence_kind_by_check;
    let related = &row.occurrence_related_evidence_kinds_by_check;
    let terminal = row.code == Code::ReportingLimitExceeded;
    let kinds: BTreeSet<EvidenceKind> = primary
        .iter()
        .filter_map(|entry| entry.kind.kind())
        .chain(related.iter().flat_map(|entry| entry.kinds.iter().copied()))
        .collect();
    primary
        .iter()
        .map(|entry| entry.check_id.as_str())
        .eq(checks.clone())
        && related
            .iter()
            .map(|entry| entry.check_id.as_str())
            .eq(checks)
        && primary.iter().zip(related).all(|(primary, related)| {
            (primary.kind == PrimaryKind::None) == terminal
                && (primary.kind != PrimaryKind::None || related.kinds.is_empty())
                && related.kinds.windows(2).all(|pair| pair[0] < pair[1])
        })
        && row
            .evidence_disclosure_policies
            .iter()
            .map(|entry| entry.evidence_kind)
            .eq(kinds.iter().copied())
        && row
            .evidence_disclosure_policies
            .iter()
            .all(|entry| paths_are_closed(entry, policies))
        && row
            .allowed_safe_origin_kinds_by_evidence_kind
            .iter()
            .map(|entry| entry.evidence_kind)
            .eq(kinds.iter().copied())
        && row
            .allowed_safe_origin_kinds_by_evidence_kind
            .iter()
            .all(|entry| origins_are_closed(entry, row, origins))
}

/// Whether a policy entry's shapes are, for a resolver-input kind, a
/// non-empty list of shapes its policy pins, each once and in Evidence
/// order, and for any other kind empty.
fn paths_are_closed(entry: &PolicyEntry, policies: &[DisclosurePolicy]) -> bool {
    if !policy::resolver_input(entry.evidence_kind) {
        return entry.paths.is_empty();
    }
    let pinned = policies
        .iter()
        .find(|policy| policy.evidence_kind == entry.evidence_kind)
        .map_or(&[][..], |policy| &policy.paths[..]);
    !entry.paths.is_empty()
        && entry.paths.iter().all(|shape| pinned.contains(shape))
        && entry
            .paths
            .windows(2)
            .all(|pair| pair[0].order(&pair[1]) == Some(Ordering::Less))
}

/// Whether an origin entry is a non-empty list of origins `registry`
/// permits the kind, in rank order, which for a resolver-input kind are
/// exactly the origins of the row's shapes of that kind.
fn origins_are_closed(
    entry: &OriginEntry,
    row: &FindingRow,
    registry: &SafeOriginKindRegistry,
) -> bool {
    let permitted = |origin: SafeOriginKind| {
        registry.rows.iter().any(|row| {
            row.id == origin && row.permitted_evidence_kinds.contains(&entry.evidence_kind)
        })
    };
    let shaped: BTreeSet<SafeOriginKind> = row
        .evidence_disclosure_policies
        .iter()
        .filter(|policy| policy.evidence_kind == entry.evidence_kind)
        .flat_map(|policy| policy.paths.iter().map(|shape| shape.origin))
        .collect();
    !entry.origins.is_empty()
        && entry.origins.windows(2).all(|pair| pair[0] < pair[1])
        && entry.origins.iter().all(|&origin| permitted(origin))
        && (!policy::resolver_input(entry.evidence_kind)
            || entry.origins.iter().copied().collect::<BTreeSet<_>>() == shaped)
}

/// The shapes Coverage Decision Evidence can carry besides those of Finding
/// Occurrences: the applicable Coverage Specification input, when the safe
/// default is omitted (D15:1609).
fn coverage_decision_shapes() -> Vec<PathShape> {
    vec![PathShape::of(&ComponentPath::specification(Slot::Coverage))]
}

/// Whether every shape `policies` pin is one some row's occurrences, or
/// Coverage Decision Evidence, can carry (D15:838, 2274).
pub(crate) fn are_reachable(rows: &[FindingRow], policies: &[DisclosurePolicy]) -> bool {
    let carried: Vec<&PathShape> = rows
        .iter()
        .flat_map(|row| &row.evidence_disclosure_policies)
        .flat_map(|entry| &entry.paths)
        .collect();
    let decisions = coverage_decision_shapes();
    policies
        .iter()
        .flat_map(|policy| &policy.paths)
        .all(|shape| carried.contains(&shape) || decisions.contains(shape))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::registry::check::CheckRegistry;
    use crate::construction::registry::finding::FindingRegistry;

    fn registry() -> FindingRegistry {
        FindingRegistry::revision_zero()
    }

    fn origins() -> SafeOriginKindRegistry {
        SafeOriginKindRegistry::revision_zero()
    }

    fn row(code: Code) -> FindingRow {
        registry()
            .rows
            .into_iter()
            .find(|row| row.code == code)
            .unwrap()
    }

    fn pair_of(check: &str, code: Code) -> &'static Pair {
        find(check, code).unwrap()
    }

    #[test]
    fn the_table_has_every_pair_of_the_check_registry_in_its_order() {
        let checks = CheckRegistry::revision_zero();
        let pairs: Vec<(String, Code)> = checks
            .rows
            .iter()
            .flat_map(|row| {
                row.possible_finding_codes
                    .iter()
                    .map(|&code| (row.check_id.clone(), code))
            })
            .collect();
        let table: Vec<(String, Code)> = PAIRS
            .iter()
            .map(|pair| (pair.check.to_owned(), pair.code))
            .collect();
        assert_eq!(table, pairs);
    }

    /// A pair names its related kinds once each, in the admitted order, and
    /// shapes only of the kinds it names, each a path its kind's policy
    /// pins; every resolver-input kind it names has some shape.
    #[test]
    fn each_pair_names_shapes_of_its_kinds_that_the_policies_pin() {
        let policies = policy::revision_zero();
        let pinned = |kind: EvidenceKind| {
            policies
                .iter()
                .find(|policy| policy.evidence_kind == kind)
                .map(|policy| policy.paths.clone())
                .unwrap()
        };
        for pair in &PAIRS {
            let kinds: BTreeSet<EvidenceKind> = pair.kinds().collect();
            assert!(
                pair.related.windows(2).all(|two| two[0] < two[1]),
                "{}",
                pair.check
            );
            for shape in pair.shapes {
                let paths = shape.paths();
                assert!(!paths.is_empty(), "{} {shape:?}", pair.check);
                if let Shape::Bound(path) = shape {
                    assert_eq!(paths.len(), 1, "{path}");
                }
                for path in paths {
                    let kind = path.origin.permitted_evidence_kinds()[0];
                    assert!(kinds.contains(&kind), "{} {shape:?}", pair.check);
                    assert!(pinned(kind).contains(&path), "{} {shape:?}", pair.check);
                }
            }
            for kind in kinds {
                assert_eq!(
                    policy::resolver_input(kind),
                    !pair.paths(kind).is_empty(),
                    "{} {kind:?}",
                    pair.check
                );
            }
            if pair.primary.is_none() {
                assert!(pair.related.is_empty() && pair.shapes.is_empty());
            }
        }
    }

    /// The kinds 015 names: a root-value bound applied before
    /// materialization is `entry-source`, an over-limit selector
    /// `profile-selector`, and a bound over the materialized configuration
    /// `configuration-value` (D15:2249); a selector Finding relates the
    /// declarations (D15:812); the bootstrap's mismatch candidates are
    /// related (D15:1734); a conflicting spelling relates the others and a
    /// non-canonical one nothing (D15:2410); the pins that do not
    /// authenticate a candidate are related (D15:2249); and the terminal
    /// reporting Finding has `none` (D15:2249).
    #[test]
    fn the_kinds_015_names_are_the_tables() {
        let rle = Code::ResourceLimitExceeded;
        for check in [
            "entry.input.within-nodes",
            "entry.input.within-depth",
            "entry.input.within-collection-entries",
            "entry.input.within-total-string-bytes",
            "entry.input.within-single-string-bytes",
        ] {
            assert_eq!(pair_of(check, rle).primary, Some(ES), "{check}");
        }
        for check in [
            "selection.selector.within-capability",
            "resource-policy.selector.within-bound",
        ] {
            assert_eq!(pair_of(check, rle).primary, Some(PS), "{check}");
        }
        for check in [
            "resource-policy.root-nodes.within-bound",
            "locale.requested-set.within-bound",
            "group.members.within-bound",
        ] {
            assert_eq!(pair_of(check, rle).primary, Some(CV), "{check}");
        }
        for pair in PAIRS.iter().filter(|pair| pair.primary == Some(PS)) {
            assert!(pair.related.contains(&CV), "{}", pair.check);
        }
        let candidate = pair_of(
            "resource-policy.resource-limits.bootstrap-candidate",
            Code::ArtifactReferenceNotAdmitted,
        );
        assert_eq!(candidate.related, [AI]);
        assert!(candidate.paths(AI).contains(&CANDIDATE.paths()[0]));
        let duplicate = pair_of("locale.requested.unique", Code::LocaleDuplicate);
        assert_eq!(
            (duplicate.primary, duplicate.related),
            (Some(CV), &[CV][..])
        );
        let spelling = pair_of("locale.requested.canonical", Code::LocaleNonCanonical);
        assert_eq!((spelling.primary, spelling.related), (Some(CV), &[][..]));
        let body = pair_of(
            "resource-policy.resource-limits.candidate-body",
            Code::ResourcePolicyNotAdmitted,
        );
        assert_eq!(body.related, [CV, VI]);
        assert_eq!(body.paths(VI), Shape::Pins.paths());
        for pair in &PAIRS {
            assert_eq!(
                pair.primary.is_none(),
                pair.code == Code::ReportingLimitExceeded,
                "{}",
                pair.check
            );
        }
    }

    /// A row's columns follow its checks: the per-check columns list them in
    /// order, and the policies and origins name each kind those name once,
    /// in the admitted order, with the union of their shapes and origins.
    #[test]
    fn a_rows_columns_follow_its_checks() {
        let bootstrap = row(Code::BootstrapLimitExceeded);
        let checks = |entries: Vec<&str>| entries == bootstrap.allowed_check_ids;
        assert!(checks(
            bootstrap
                .occurrence_primary_evidence_kind_by_check
                .iter()
                .map(|entry| entry.check_id.as_str())
                .collect()
        ));
        assert!(checks(
            bootstrap
                .occurrence_related_evidence_kinds_by_check
                .iter()
                .map(|entry| entry.check_id.as_str())
                .collect()
        ));
        let kinds = |row: &FindingRow| -> Vec<EvidenceKind> {
            row.evidence_disclosure_policies
                .iter()
                .map(|entry| entry.evidence_kind)
                .collect()
        };
        assert_eq!(kinds(&bootstrap), [ES, CV, AI, IC]);
        let origins_of = |row: &FindingRow, kind: EvidenceKind| -> Vec<SafeOriginKind> {
            row.allowed_safe_origin_kinds_by_evidence_kind
                .iter()
                .find(|entry| entry.evidence_kind == kind)
                .unwrap()
                .origins
                .clone()
        };
        // Only raw-file checks report it with Entry Source Evidence.
        assert_eq!(
            origins_of(&bootstrap, ES),
            [SafeOriginKind::ConfigurationFile]
        );
        let paths_of = |row: &FindingRow, kind: EvidenceKind| -> Vec<PathShape> {
            row.evidence_disclosure_policies
                .iter()
                .find(|entry| entry.evidence_kind == kind)
                .unwrap()
                .paths
                .clone()
        };
        assert_eq!(paths_of(&bootstrap, IC).len(), 2 + 4);
        let limits = row(Code::ResourceLimitExceeded);
        assert_eq!(kinds(&limits), [ES, CV, PS, AI, IC]);
        assert_eq!(origins_of(&limits, ES), CONFIGURATION);
        assert_eq!(origins_of(&limits, PS), SELECTOR);
        assert_eq!(origins_of(&limits, AI), [COLLECTION]);
        let incompatible = row(Code::ResourcePolicyCapabilityIncompatible);
        assert_eq!(paths_of(&incompatible, IC).len(), 52);
        assert!(paths_of(&incompatible, IC)
            .iter()
            .all(|shape| shape.roles[0].role == "policyComparable"));
        let reporting = row(Code::ReportingLimitExceeded);
        assert_eq!(
            reporting.occurrence_primary_evidence_kind_by_check[0].kind,
            PrimaryKind::None
        );
        assert!(reporting.occurrence_related_evidence_kinds_by_check[0]
            .kinds
            .is_empty());
        assert!(reporting.evidence_disclosure_policies.is_empty());
        assert!(reporting
            .allowed_safe_origin_kinds_by_evidence_kind
            .is_empty());
        let registry = registry();
        for row in &registry.rows {
            assert!(
                row_is_closed(row, &registry.policies, &origins()),
                "{}",
                row.code.as_str()
            );
        }
    }

    /// The rows carry every shape the policies pin, so revision `"0"` needs
    /// no shape Coverage Decision Evidence alone carries.
    #[test]
    fn the_rows_carry_every_shape_the_policies_pin() {
        let registry = registry();
        let carried: BTreeSet<String> = registry
            .rows
            .iter()
            .flat_map(|row| &row.evidence_disclosure_policies)
            .flat_map(|entry| &entry.paths)
            .map(|shape| serde_json::to_string(shape).unwrap())
            .collect();
        let pinned: BTreeSet<String> = registry
            .policies
            .iter()
            .flat_map(|policy| &policy.paths)
            .map(|shape| serde_json::to_string(shape).unwrap())
            .collect();
        assert_eq!(carried, pinned);
        assert_eq!(pinned.len(), 74);
        assert!(are_reachable(&registry.rows, &registry.policies));
    }

    /// A change to a row.
    type Change<'a> = dyn Fn(&mut FindingRow) + 'a;

    /// Construction refuses a row whose columns depart from 015.
    #[test]
    fn rows_that_depart_from_015_are_not_closed() {
        let registry = registry();
        let closed = |code: Code, change: &dyn Fn(&mut FindingRow)| {
            let mut row = row(code);
            change(&mut row);
            row_is_closed(&row, &registry.policies, &origins())
        };
        let mode = PathShape::of(&ComponentPath::verification(VerificationRole::Mode));
        let rle = Code::ResourceLimitExceeded;
        let cases: [(&str, Code, &Change<'_>); 16] = [
            ("checks out of order", rle, &|row| {
                row.occurrence_primary_evidence_kind_by_check.swap(0, 1);
            }),
            ("a check missing", rle, &|row| {
                row.occurrence_related_evidence_kinds_by_check.pop();
            }),
            ("none on another code", rle, &|row| {
                row.occurrence_primary_evidence_kind_by_check[0].kind = PrimaryKind::None;
            }),
            (
                "a kind on the terminal code",
                Code::ReportingLimitExceeded,
                &|row| {
                    row.occurrence_primary_evidence_kind_by_check[0].kind =
                        PrimaryKind::ConfigurationValue;
                },
            ),
            (
                "a related kind after none",
                Code::ReportingLimitExceeded,
                &|row| {
                    row.occurrence_related_evidence_kinds_by_check[0].kinds = vec![CV];
                },
            ),
            ("related kinds out of order", rle, &|row| {
                row.occurrence_related_evidence_kinds_by_check[0].kinds = vec![IC, CV];
            }),
            ("a related kind twice", rle, &|row| {
                row.occurrence_related_evidence_kinds_by_check[0].kinds = vec![IC, IC];
            }),
            ("a policy missing", rle, &|row| {
                row.evidence_disclosure_policies.pop();
            }),
            ("a policy of no named kind", Code::JsonInvalid, &|row| {
                row.evidence_disclosure_policies.push(PolicyEntry {
                    evidence_kind: CV,
                    paths: Vec::new(),
                });
            }),
            ("shapes on a kind with none", rle, &|row| {
                row.evidence_disclosure_policies[0].paths = vec![mode.clone()];
            }),
            (
                "a shape no policy pins",
                Code::ResourcePolicyNotAdmitted,
                &|row| {
                    let entry = row
                        .evidence_disclosure_policies
                        .iter_mut()
                        .find(|entry| entry.evidence_kind == VI)
                        .unwrap();
                    entry.paths.insert(0, mode.clone());
                },
            ),
            (
                "shapes out of order",
                Code::BootstrapLimitExceeded,
                &|row| {
                    row.evidence_disclosure_policies
                        .last_mut()
                        .unwrap()
                        .paths
                        .swap(0, 1);
                },
            ),
            (
                "a resolver input without shapes",
                Code::ResourcePolicyNotAdmitted,
                &|row| {
                    row.evidence_disclosure_policies
                        .last_mut()
                        .unwrap()
                        .paths
                        .clear();
                },
            ),
            ("no origin", rle, &|row| {
                row.allowed_safe_origin_kinds_by_evidence_kind[0]
                    .origins
                    .clear();
            }),
            ("an origin the kind may not use", rle, &|row| {
                row.allowed_safe_origin_kinds_by_evidence_kind[1]
                    .origins
                    .push(SafeOriginKind::ProfileSelectorCli);
            }),
            ("an origin no shape uses", rle, &|row| {
                let entry = row
                    .allowed_safe_origin_kinds_by_evidence_kind
                    .iter_mut()
                    .find(|entry| entry.evidence_kind == AI)
                    .unwrap();
                entry.origins.insert(0, VOCABULARY);
            }),
        ];
        for (case, code, change) in cases {
            assert!(closed(code, &|_| {}), "{case}");
            assert!(!closed(code, change), "{case}");
        }
        // Origins out of order.
        assert!(!closed(rle, &|row| {
            row.allowed_safe_origin_kinds_by_evidence_kind[0]
                .origins
                .reverse();
        }));
    }

    /// A shape the policies pin and no Evidence carries breaks the graph;
    /// the Coverage Specification input is carried by Coverage Decision
    /// Evidence even when no row carries it.
    #[test]
    fn a_pinned_shape_no_evidence_carries_is_unreachable() {
        let registry = registry();
        let without = |shape: &PathShape| {
            let mut rows = registry.rows.clone();
            for row in &mut rows {
                for entry in &mut row.evidence_disclosure_policies {
                    entry.paths.retain(|path| path != shape);
                }
            }
            are_reachable(&rows, &registry.policies)
        };
        let pins = &Shape::Pins.paths()[0];
        assert!(!without(pins));
        let coverage = PathShape::of(&ComponentPath::specification(Slot::Coverage));
        assert!(without(&coverage));
    }
}
