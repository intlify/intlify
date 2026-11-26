// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Case Schema: the closed form of the suite's five manifest kinds.
//!
//! Every manifest the Suite Index lists passes the Case Schema (D15:3031).
//! Its root admits exactly one of the five kinds, each a closed object whose
//! `manifestKind` names it: a Resolver Construction Case (D15:3035-3060), a
//! Resolver Input Admission Case (D15:3109-3168), a Resolver Case
//! (D15:3174-3237), a Handoff Vector (D15:3309-3348), and a Conditional
//! Obligation (D15:3352-3382). The schema closes what those sections fix: the
//! ID grammars, the `traceability` member and its rule-ID grammars, the 38
//! construction failure reasons, each boundary kind's failure reasons and the
//! form of its admitted input (D15:3142-3156, 3256-3265), the slots of the two
//! admission steps' failures, the closed reasons of an unpaired entry, the
//! relations, the verification owners, and the eight rows of the obligation
//! table. Each vocabulary is written from this crate's own: the construction
//! reasons and bounds, the Safe Origin Kinds, the semantic groups, the
//! artifact kinds, and the Snapshot marker reasons.
//!
//! Beside the forms, the schema pins the failure precedence as its top-level
//! `failurePrecedence` (D15:3269): each boundary kind's ordered
//! `{ check, reason }` rows, and for the construction cases a reference to
//! the construction-admission body that holds the construction rows. Draft 7
//! ignores the keyword, so it changes nothing that the schema admits.
//!
//! What a manifest names besides itself, its fixtures, keeps the form the
//! cases that read it will give it: the forms of the input and
//! expected-outcome fixtures join this schema with those cases, as revision
//! `"0"` is written (plan 027 decision 32). A manifest refers to a fixture by
//! a suite-root-relative path and a file digest only.

use serde_json::{json, Map, Value};

use crate::artifacts::envelope::ArtifactKind;
use crate::boundary::locator::LocatorVariant;
use crate::construction::admission::{ConstructionBoundId, DRAFT_7};
use crate::construction::component::file_digest;
use crate::construction::envelope::{closed, digest, token};
use crate::construction::failure::ConstructionFailure;
use crate::construction::generate::{self, Artifact};
use crate::construction::profile::MARKER_REASONS;
use crate::construction::registry::check::{CHECK_ID_PATTERN, RULE_ID_PATTERN};
use crate::construction::root::RootRole;
use crate::resolver::dependency::SemanticGroup;
use crate::resolver::origin::SafeOriginKind;
use crate::snapshot::SnapshotBound;

use super::index::PATH_PATTERN;

/// The Case Schema's identity.
pub(crate) const IDENTITY: &str = "intlify-project-profile-conformance-case";

/// The Case Schema's revision.
pub(crate) const REVISION: &str = "0";

/// Where this crate writes the Case Schema below the suite root.
pub(crate) const PATH: &str = "schemas/case-v0.schema.json";

/// The Case Schema's file digest, which the runner pins.
pub(crate) const DIGEST: &str =
    "sha256:50d2a9d4dbd5faa9acb533e7538a82f6b9af577a9d5061b694eeda102281b3f7";

/// A case or Handoff Vector ID: `015-` and lowercase kebab-case.
pub(crate) const CASE_ID_PATTERN: &str = "^015-[a-z0-9]+(?:-[a-z0-9]+)*$";

/// A conditional obligation ID: `015-`, its class, `-`, and three digits.
pub(crate) const OBLIGATION_ID_PATTERN: &str = "^015-(?:XV|AD|HV)-[0-9]{3}$";

/// An Accepted Decision's ID.
const DECISION_ID_PATTERN: &str = "^015-[0-9]{3}$";

/// A construction rule ID: `015.construction.<component>.<check>`, after one
/// row of the construction-failure precedence (Decision 015-208).
const CONSTRUCTION_RULE_PATTERN: &str =
    "^015\\.construction\\.[a-z][A-Za-z0-9]*\\.[a-z][a-z0-9]*(?:-[a-z0-9]+)*$";

/// A pre-invocation rule ID: `015.input-admission.<boundary kind>.<reason>`.
const INPUT_ADMISSION_RULE_PATTERN: &str =
    "^015\\.input-admission\\.[a-z][a-z0-9]*(?:-[a-z0-9]+)*\\.[a-z][a-z0-9]*(?:-[a-z0-9]+)*$";

/// A Coverage inventory item's ID.
const COVERAGE_ITEM_PATTERN: &str = "^015-COV-[0-9]{3}$";

/// A Resolver Construction Identity's text: `rci0_` and the unpadded
/// base64url of 32 bytes, whose last character carries four bits.
const CONSTRUCTION_IDENTITY_PATTERN: &str = "^rci0_[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$";

/// A decimal number's text.
const DECIMAL_PATTERN: &str = "^-?(?:0|[1-9][0-9]*)(?:\\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$";

/// One manifest kind, in the order of the Suite Index's arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManifestKind {
    ResolverConstruction,
    ResolverInputAdmission,
    Resolver,
    HandoffVector,
    ConditionalObligation,
}

impl ManifestKind {
    /// Every kind, in the order of the Suite Index's arrays.
    pub(crate) const ALL: [Self; 5] = [
        Self::ResolverConstruction,
        Self::ResolverInputAdmission,
        Self::Resolver,
        Self::HandoffVector,
        Self::ConditionalObligation,
    ];

    /// The kind's `manifestKind`.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::ResolverConstruction => "resolver-construction",
            Self::ResolverInputAdmission => "resolver-input-admission",
            Self::Resolver => "resolver",
            Self::HandoffVector => "handoff-vector",
            Self::ConditionalObligation => "conditional-obligation",
        }
    }

    /// The name of the kind's definition in the Case Schema.
    const fn definition(self) -> &'static str {
        match self {
            Self::ResolverConstruction => "resolverConstructionCase",
            Self::ResolverInputAdmission => "resolverInputAdmissionCase",
            Self::Resolver => "resolverCase",
            Self::HandoffVector => "handoffVector",
            Self::ConditionalObligation => "conditionalObligation",
        }
    }
}

/// Design 015's boundary kinds, each with its closed failure reasons in the
/// order of its row (D15:3142-3156).
pub(crate) const BOUNDARIES: [(&str, &[&str]); 13] = [
    (
        "profile-resolution-specification-input",
        &[
            "missing-input",
            "over-bound-member",
            "malformed-input",
            "duplicate-member",
            "unknown-member",
        ],
    ),
    (
        "intent-surface-class-vocabulary-artifact",
        &[
            "missing-input",
            "over-bound-envelope",
            "malformed-envelope",
            "duplicate-member",
            "unknown-member",
        ],
    ),
    (
        "locale-canonicalization-data-artifact",
        &[
            "missing-input",
            "over-bound-envelope",
            "malformed-envelope",
            "duplicate-member",
            "unknown-member",
        ],
    ),
    (
        "submitted-profile-resolution-artifact-collection",
        &[
            "missing-input",
            "unsafe-host-inspection",
            "malformed-container",
            "formal-envelope-over-bound",
            "unrepresentable-member",
        ],
    ),
    (
        "resource-limit-policy-verification-input",
        &[
            "missing-input",
            "over-bound-input",
            "malformed-input",
            "duplicate-member",
            "unknown-member",
            "secret-bearing-input",
            "ambient-authority",
            "verification-identity-mismatch",
            "verification-revision-mismatch",
            "verification-digest-mismatch",
        ],
    ),
    (
        "safe-origin-kind",
        &[
            "missing-safe-origin-kind",
            "unknown-safe-origin-kind",
            "non-canonical-safe-origin-kind",
            "evidence-kind-incompatible",
        ],
    ),
    (
        "external-profile-selector-binding",
        &[
            "unsafe-host-inspection",
            "unrepresentable-selector",
            "malformed-normalized-selector",
        ],
    ),
    (
        "file-source-identity-revision",
        &[
            "missing-source-identity",
            "missing-source-revision",
            "over-bound-source-identity",
            "over-bound-source-revision",
            "unstable-source-identity",
            "unbound-source-revision",
            "source-snapshot-mismatch",
        ],
    ),
    (
        "programmatic-entry-identity-revision",
        &[
            "missing-entry-identity",
            "missing-entry-revision",
            "over-bound-entry-identity",
            "over-bound-entry-revision",
            "unstable-entry-identity",
            "unbound-entry-revision",
            "snapshot-identity-mismatch",
        ],
    ),
    (
        "programmatic-entry-snapshot",
        &[
            "missing-snapshot",
            "unsafe-host-inspection",
            "snapshot-envelope-over-bound",
            "malformed-snapshot",
            "snapshot-content-digest-mismatch",
        ],
    ),
    (
        "programmatic-call-site-evidence-input",
        &[
            "unsafe-host-inspection",
            "over-bound-input",
            "malformed-input",
            "invalid-utf8-source",
            "duplicate-mapping",
            "missing-source-identity",
            "missing-source-revision",
            "unstable-source-identity",
            "unbound-source-revision",
            "source-snapshot-mismatch",
        ],
    ),
    (
        "implementation-capability",
        &[
            "missing-capability",
            "malformed-capability",
            "capability-specification-identity-mismatch",
            "capability-specification-revision-mismatch",
            "incomplete-capability",
            "non-canonical-bound",
            "non-positive-bound",
            "unrepresentable-bound",
            "below-bootstrap-minimum",
        ],
    ),
    (
        "resolver-construction-identity-reference",
        &[
            "missing-construction-identity",
            "malformed-construction-identity",
            "construction-identity-mismatch",
        ],
    ),
];

/// Each boundary kind's failure precedence, in the order of [`BOUNDARIES`]:
/// its ordered `{ check, reason }` rows, the first failing row deciding
/// (D15:3160, 3269). Each row places one of the boundary's checks at the
/// position of the general order that the check takes, in the order the
/// boundary applies them, so a row lists its reasons in the order of the
/// first position each occupies. A reason can take several positions, such
/// as `malformed-input` for syntax, for the closed shape, and for the value
/// rules. The capability boundary compares its specification before its
/// inventory and bounds, so its `assertion-comparison` rows come before its
/// `value-rules` rows.
pub(crate) const BOUNDARY_PRECEDENCE: [(&str, &[(&str, &str)]); 13] = [
    (
        "profile-resolution-specification-input",
        &[
            ("missing-input", "missing-input"),
            ("raw-bounds", "over-bound-member"),
            ("strict-syntax", "malformed-input"),
            ("closed-shape", "malformed-input"),
            ("repeated-member", "duplicate-member"),
            ("unknown-member", "unknown-member"),
            ("value-rules", "malformed-input"),
        ],
    ),
    (
        "intent-surface-class-vocabulary-artifact",
        &[
            ("missing-input", "missing-input"),
            ("raw-bounds", "over-bound-envelope"),
            ("strict-syntax", "malformed-envelope"),
            ("count-bounds", "over-bound-envelope"),
            ("closed-shape", "malformed-envelope"),
            ("repeated-member", "duplicate-member"),
            ("unknown-member", "unknown-member"),
        ],
    ),
    (
        "locale-canonicalization-data-artifact",
        &[
            ("missing-input", "missing-input"),
            ("raw-bounds", "over-bound-envelope"),
            ("strict-syntax", "malformed-envelope"),
            ("closed-shape", "malformed-envelope"),
            ("repeated-member", "duplicate-member"),
            ("unknown-member", "unknown-member"),
        ],
    ),
    (
        "submitted-profile-resolution-artifact-collection",
        &[
            ("missing-input", "missing-input"),
            ("unsafe-host-inspection", "unsafe-host-inspection"),
            ("container-form", "malformed-container"),
            ("raw-bounds", "formal-envelope-over-bound"),
            ("strict-syntax", "unrepresentable-member"),
            ("value-rules", "unrepresentable-member"),
        ],
    ),
    (
        "resource-limit-policy-verification-input",
        &[
            ("missing-input", "missing-input"),
            ("raw-bounds", "over-bound-input"),
            ("strict-syntax", "malformed-input"),
            ("count-bounds", "over-bound-input"),
            ("closed-shape", "malformed-input"),
            ("repeated-member", "duplicate-member"),
            ("unknown-member", "unknown-member"),
            ("reserved-members", "secret-bearing-input"),
            ("reserved-members", "ambient-authority"),
            ("value-rules", "malformed-input"),
            ("assertion-comparison", "verification-identity-mismatch"),
            ("assertion-comparison", "verification-revision-mismatch"),
            ("assertion-comparison", "verification-digest-mismatch"),
        ],
    ),
    (
        "safe-origin-kind",
        &[
            ("missing-input", "missing-safe-origin-kind"),
            ("closed-shape", "unknown-safe-origin-kind"),
            ("value-rules", "unknown-safe-origin-kind"),
            ("value-rules", "non-canonical-safe-origin-kind"),
            ("assertion-comparison", "evidence-kind-incompatible"),
        ],
    ),
    (
        "external-profile-selector-binding",
        &[
            ("unsafe-host-inspection", "unsafe-host-inspection"),
            ("closed-shape", "unrepresentable-selector"),
            ("value-rules", "malformed-normalized-selector"),
        ],
    ),
    (
        "file-source-identity-revision",
        &[
            ("missing-input", "missing-source-identity"),
            ("missing-input", "missing-source-revision"),
            ("raw-bounds", "over-bound-source-identity"),
            ("raw-bounds", "over-bound-source-revision"),
            ("value-rules", "unstable-source-identity"),
            ("value-rules", "unbound-source-revision"),
            ("assertion-comparison", "source-snapshot-mismatch"),
        ],
    ),
    (
        "programmatic-entry-identity-revision",
        &[
            ("missing-input", "missing-entry-identity"),
            ("missing-input", "missing-entry-revision"),
            ("raw-bounds", "over-bound-entry-identity"),
            ("raw-bounds", "over-bound-entry-revision"),
            ("value-rules", "unstable-entry-identity"),
            ("value-rules", "unbound-entry-revision"),
            ("assertion-comparison", "snapshot-identity-mismatch"),
        ],
    ),
    (
        "programmatic-entry-snapshot",
        &[
            ("missing-input", "missing-snapshot"),
            ("unsafe-host-inspection", "unsafe-host-inspection"),
            ("raw-bounds", "snapshot-envelope-over-bound"),
            ("closed-shape", "malformed-snapshot"),
            ("value-rules", "snapshot-envelope-over-bound"),
            ("assertion-comparison", "snapshot-content-digest-mismatch"),
        ],
    ),
    (
        "programmatic-call-site-evidence-input",
        &[
            ("unsafe-host-inspection", "unsafe-host-inspection"),
            ("raw-bounds", "over-bound-input"),
            ("count-bounds", "over-bound-input"),
            ("closed-shape", "malformed-input"),
            ("closed-shape", "invalid-utf8-source"),
            ("repeated-member", "duplicate-mapping"),
            ("value-rules", "missing-source-identity"),
            ("value-rules", "missing-source-revision"),
            ("value-rules", "unstable-source-identity"),
            ("value-rules", "unbound-source-revision"),
            ("assertion-comparison", "source-snapshot-mismatch"),
        ],
    ),
    (
        "implementation-capability",
        &[
            ("missing-input", "missing-capability"),
            ("closed-shape", "malformed-capability"),
            (
                "assertion-comparison",
                "capability-specification-identity-mismatch",
            ),
            (
                "assertion-comparison",
                "capability-specification-revision-mismatch",
            ),
            ("value-rules", "malformed-capability"),
            ("value-rules", "incomplete-capability"),
            ("value-rules", "non-canonical-bound"),
            ("value-rules", "non-positive-bound"),
            ("value-rules", "unrepresentable-bound"),
            ("value-rules", "below-bootstrap-minimum"),
        ],
    ),
    (
        "resolver-construction-identity-reference",
        &[
            ("missing-input", "missing-construction-identity"),
            ("closed-shape", "malformed-construction-identity"),
            ("value-rules", "malformed-construction-identity"),
            ("assertion-comparison", "construction-identity-mismatch"),
        ],
    ),
];

/// The kind that admits a whole Common Invocation Input Set.
const COMMON_SET: &str = "common-invocation-input-set";

/// The kind that admits a whole file or programmatic Entry Admission Input
/// Set.
const ENTRY_SET: &str = "entry-admission-input-set";

/// The Common Invocation Input Set's slots, in rank order, each with the
/// boundary kind of its value.
const COMMON_SLOTS: [(SafeOriginKind, &str); 6] = [
    (
        SafeOriginKind::ProfileResolutionSpecificationInput,
        "profile-resolution-specification-input",
    ),
    (
        SafeOriginKind::IntentSurfaceClassVocabulary,
        "intent-surface-class-vocabulary-artifact",
    ),
    (
        SafeOriginKind::LocaleCanonicalizationData,
        "locale-canonicalization-data-artifact",
    ),
    (
        SafeOriginKind::ProfileResolutionArtifactCollection,
        "submitted-profile-resolution-artifact-collection",
    ),
    (
        SafeOriginKind::ResourceLimitPolicyVerification,
        "resource-limit-policy-verification-input",
    ),
    (
        SafeOriginKind::ImplementationCapability,
        "implementation-capability",
    ),
];

/// The value boundaries of a file entry's slots, each with the kinds of its
/// value. Each entry is admitted in a step of its own, which lists failures
/// of its own slots only (D15:3164).
const FILE_ENTRY_SLOTS: [(SafeOriginKind, &[&str]); 2] = [
    (
        SafeOriginKind::ConfigurationFile,
        &["file-source-identity-revision"],
    ),
    (
        SafeOriginKind::ProfileSelectorCli,
        &["external-profile-selector-binding"],
    ),
];

/// The value boundaries of a programmatic entry's slots.
const PROGRAMMATIC_ENTRY_SLOTS: [(SafeOriginKind, &[&str]); 2] = [
    (
        SafeOriginKind::ConfigurationProgrammatic,
        &[
            "programmatic-entry-identity-revision",
            "programmatic-entry-snapshot",
            "programmatic-call-site-evidence-input",
        ],
    ),
    (
        SafeOriginKind::ProfileSelectorProgrammatic,
        &["external-profile-selector-binding"],
    ),
];

/// The boundary kinds whose admitted input is `{ "json": <value> }`
/// (D15:3258).
const JSON_NORMALIZED: [&str; 6] = [
    "profile-resolution-specification-input",
    "intent-surface-class-vocabulary-artifact",
    "locale-canonicalization-data-artifact",
    "resource-limit-policy-verification-input",
    "programmatic-call-site-evidence-input",
    "implementation-capability",
];

/// The owners a traceability record or a manifest may name (D15:3420).
pub(crate) const VERIFICATION_OWNERS: [&str; 14] = [
    "intlify_config",
    "016",
    "019",
    "020",
    "021",
    "022",
    "023",
    "024",
    "025",
    "026",
    "027",
    "029",
    "030",
    "product-integration",
];

/// The owners of a Handoff Vector (D15:3334-3346).
const HANDOFF_OWNERS: [&str; 11] = [
    "016", "019", "020", "021", "022", "023", "024", "025", "026", "027", "030",
];

/// The reasons a programmatic entry may be left unpaired in a file-only case.
const FILE_ONLY_REASONS: [&str; 4] = [
    "raw-json-syntax",
    "duplicate-object-member",
    "malformed-encoding",
    "exact-token-span",
];

/// The reasons a file entry may be left unpaired in a programmatic-only case.
const PROGRAMMATIC_ONLY_REASONS: [&str; 4] = [
    "programmatic-source-label",
    "programmatic-call-site",
    "programmatic-entry-snapshot",
    "safe-host-inspection",
];

/// The kinds of a relation between two Resolver Cases (D15:3305).
const RELATION_KINDS: [&str; 6] = [
    "profile-equal",
    "profile-changed",
    "invocation-stale",
    "invocation-not-stale",
    "materialized-input-equal",
    "materialized-input-changed",
];

/// The conditional obligations of revision `"0"`: the ID, the activation,
/// the not-applicable reason, and the verification owner of each row
/// (D15:3373-3382).
pub(crate) const OBLIGATIONS: [(&str, &str, &str, &str); 8] = [
    (
        "015-XV-001",
        "second-configuration-schema-revision",
        "no-second-admitted-configuration-schema-revision",
        "intlify_config",
    ),
    (
        "015-XV-002",
        "extension-admitting-canonicalization-specification-revision",
        "no-extension-admitting-canonicalization-specification-revision",
        "intlify_config",
    ),
    (
        "015-AD-001",
        "trusted-local-editor-navigation-adapter",
        "no-trusted-local-editor-navigation-adapter",
        "product-integration",
    ),
    (
        "015-AD-002",
        "compatibility-conversion-adapter",
        "no-compatibility-conversion-adapter",
        "029",
    ),
    (
        "015-AD-003",
        "baked-canonicalization-provider",
        "no-baked-canonicalization-provider",
        "intlify_config",
    ),
    (
        "015-AD-004",
        "cli-adapter",
        "no-cli-adapter",
        "product-integration",
    ),
    (
        "015-HV-001",
        "negotiation-adapter-resource-section",
        "no-negotiation-adapter-resource-section",
        "027",
    ),
    (
        "015-HV-002",
        "expanded-fallback-probes",
        "no-expanded-fallback-probes",
        "020",
    ),
];

fn pattern(pattern: &str) -> Value {
    json!({ "type": "string", "pattern": pattern })
}

fn strings<'a>(values: impl IntoIterator<Item = &'a str>) -> Value {
    json!({ "type": "string", "enum": values.into_iter().collect::<Vec<_>>() })
}

fn constant(value: &str) -> Value {
    json!({ "const": value })
}

fn definition(name: &str) -> Value {
    json!({ "$ref": format!("#/definitions/{name}") })
}

fn unique(items: Value) -> Value {
    let mut schema = json!({ "type": "array", "items": null, "uniqueItems": true });
    schema["items"] = items;
    schema
}

/// A closed object with these required members and these optional ones.
fn closed_with(required: &[(&str, Value)], optional: &[(&str, Value)]) -> Value {
    let mut schema = closed(required);
    for (name, member) in optional {
        schema["properties"][*name] = member.clone();
    }
    schema
}

/// The schema of a crate type's JSON value, without its document members.
fn of<T: schemars::JsonSchema>() -> Value {
    let mut schema = crate::schema::draft7_schema::<T>().expect("a derived schema serializes");
    if let Some(members) = schema.as_object_mut() {
        members.remove("$schema");
        members.remove("title");
        members.remove("description");
    }
    schema
}

/// What a manifest of `kind` starts with: the suite revision, its kind, and
/// its ID.
fn heading(kind: ManifestKind, id: Value) -> [(&'static str, Value); 3] {
    [
        ("suiteRevision", constant(REVISION)),
        ("manifestKind", constant(kind.name())),
        ("id", id),
    ]
}

/// The members shared by every manifest, and the reusable values they hold.
fn shared(definitions: &mut Map<String, Value>) {
    definitions.insert("caseId".into(), pattern(CASE_ID_PATTERN));
    definitions.insert("obligationId".into(), pattern(OBLIGATION_ID_PATTERN));
    definitions.insert(
        "reference".into(),
        closed(&[("path", pattern(PATH_PATTERN)), ("digest", digest())]),
    );
    definitions.insert(
        "traceability".into(),
        closed(&[
            ("decisionIds", unique(pattern(DECISION_ID_PATTERN))),
            (
                "ruleIds",
                unique(json!({
                    "anyOf": [
                        pattern(RULE_ID_PATTERN),
                        pattern(CONSTRUCTION_RULE_PATTERN),
                        pattern(INPUT_ADMISSION_RULE_PATTERN),
                    ]
                })),
            ),
            ("checkIds", unique(pattern(CHECK_ID_PATTERN))),
        ]),
    );
    definitions.insert("verificationOwner".into(), strings(VERIFICATION_OWNERS));
    definitions.insert(
        "constructionIdentity".into(),
        pattern(CONSTRUCTION_IDENTITY_PATTERN),
    );
    definitions.insert(
        "resolverConstruction".into(),
        closed(&[
            ("caseId", definition("caseId")),
            ("identityReference", definition("constructionIdentity")),
        ]),
    );
    definitions.insert("json".into(), closed(&[("json", json!({}))]));
    definitions.insert(
        "jsonObject".into(),
        closed(&[("json", json!({ "type": "object" }))]),
    );
    // A value the shared framing carries, which has no numbers: what the
    // collection boundary's strict decoder yields for a member.
    definitions.insert(
        "numberFree".into(),
        json!({
            "anyOf": [
                { "type": ["null", "boolean", "string"] },
                { "type": "array", "items": definition("numberFree") },
                { "type": "object", "additionalProperties": definition("numberFree") }
            ]
        }),
    );
}

/// A Resolver Construction Case: success with its identity, or one closed
/// failure reason, the input-limit reason alone with the bound it crossed
/// (D15:3060, 3105). Construction never invokes the resolver.
fn construction(definitions: &mut Map<String, Value>) {
    let reasons = of::<ConstructionFailure>();
    let limit = "construction-input-limit-exceeded";
    let others = json!({ "allOf": [reasons, { "not": constant(limit) }] });
    let expected = json!({
        "oneOf": [
            closed(&[
                ("constructed", json!({ "const": true })),
                ("resolverConstructionIdentity", definition("constructionIdentity")),
                ("resolverInvoked", json!({ "const": false })),
            ]),
            closed(&[
                ("constructed", json!({ "const": false })),
                ("resolverConstructionIdentity", json!({ "type": "null" })),
                ("failureReason", others),
                ("resolverInvoked", json!({ "const": false })),
            ]),
            closed(&[
                ("constructed", json!({ "const": false })),
                ("resolverConstructionIdentity", json!({ "type": "null" })),
                ("failureReason", constant(limit)),
                (
                    "failureDetails",
                    closed(&[("boundId", of::<ConstructionBoundId>())]),
                ),
                ("resolverInvoked", json!({ "const": false })),
            ]),
        ]
    });
    let kind = ManifestKind::ResolverConstruction;
    let mut members = heading(kind, definition("caseId")).to_vec();
    members.extend([
        ("constructionInputFixture", definition("reference")),
        ("expected", expected),
        ("traceability", definition("traceability")),
    ]);
    definitions.insert(kind.definition().into(), closed(&members));
}

/// A canonical Snapshot node (D15:3250).
fn snapshot_node(definitions: &mut Map<String, Value>) {
    let ordinary = MARKER_REASONS
        .iter()
        .copied()
        .filter(|reason| *reason != "bootstrap-limit-exceeded");
    let bounds = SnapshotBound::ALL.map(SnapshotBound::path);
    let marker = json!({
        "oneOf": [
            closed(&[("reason", strings(ordinary))]),
            closed(&[
                ("reason", constant("bootstrap-limit-exceeded")),
                ("boundPath", strings(bounds.iter().map(String::as_str))),
                ("witness", pattern("^[1-9][0-9]*$")),
            ]),
        ]
    });
    let node = definition("snapshotNode");
    definitions.insert(
        "snapshotNode".into(),
        json!({
            "oneOf": [
                closed(&[("null", json!({ "type": "null" }))]),
                closed(&[("boolean", json!({ "type": "boolean" }))]),
                closed(&[("number", pattern(DECIMAL_PATTERN))]),
                closed(&[("string", json!({ "type": "string" }))]),
                closed(&[("array", json!({ "type": "array", "items": node }))]),
                closed(&[(
                    "object",
                    json!({
                        "type": "array",
                        "items": {
                            "type": "array",
                            "items": [{ "type": "string" }, node],
                            "minItems": 2,
                            "additionalItems": false
                        }
                    }),
                )]),
                closed(&[("marker", marker)]),
            ]
        }),
    );
    definitions.insert(
        "snapshot".into(),
        closed(&[("snapshot", node), ("digest", digest())]),
    );
}

/// A selector as its boundary admits it (D15:3252, 3261).
fn selector(definitions: &mut Map<String, Value>) {
    definitions.insert(
        "admittedSelector".into(),
        json!({
            "oneOf": [
                closed(&[("report", constant("absent"))]),
                closed(&[("report", constant("string")), ("value", json!({ "type": "string" }))]),
                closed(&[("report", constant("over-limit-string"))]),
                closed(&[
                    ("report", constant("invalid-type")),
                    ("type", strings(["null", "boolean", "number", "array", "object"])),
                ]),
            ]
        }),
    );
}

/// A locator in its admitted canonical form, or its absence.
fn locator() -> Value {
    let variants = [
        LocatorVariant::RelativePath,
        LocatorVariant::SafeLabel,
        LocatorVariant::SafeUri,
    ];
    let mut forms = variants
        .map(|variant| closed(&[(variant.id(), json!({ "type": "string" }))]))
        .to_vec();
    forms.push(json!({ "type": "null" }));
    json!({ "oneOf": forms })
}

/// The admitted input of one boundary kind, as an admitted case states it
/// (D15:3256-3265).
fn normalized(kind: &str) -> Value {
    match kind {
        _ if JSON_NORMALIZED.contains(&kind) => definition("jsonObject"),
        "submitted-profile-resolution-artifact-collection" => closed(&[(
            "members",
            json!({ "type": "array", "items": closed(&[("json", definition("numberFree"))]) }),
        )]),
        "safe-origin-kind" => closed(&[(
            "safeOriginKind",
            strings(SafeOriginKind::ALL.map(SafeOriginKind::id)),
        )]),
        "external-profile-selector-binding" => {
            closed(&[("selector", definition("admittedSelector"))])
        }
        "file-source-identity-revision" | "programmatic-entry-identity-revision" => closed(&[
            ("identity", json!({ "type": "string", "minLength": 1 })),
            ("nonSecret", json!({ "type": "boolean" })),
            ("revision", digest()),
        ]),
        "programmatic-entry-snapshot" => definition("snapshot"),
        "resolver-construction-identity-reference" => closed(&[(
            "resolverConstructionIdentity",
            definition("constructionIdentity"),
        )]),
        _ => unreachable!("every boundary kind has an admitted form: {kind}"),
    }
}

/// One failure of an admission step: its slot, its boundary kind, and a
/// reason of that kind, the slot's origin or its value.
fn step_failures(slots: &[(SafeOriginKind, &[&str])]) -> Value {
    let reasons = |kind: &str| {
        let (_, reasons) = BOUNDARIES
            .iter()
            .find(|(name, _)| *name == kind)
            .expect("a step's kinds are boundary kinds");
        strings(reasons.iter().copied())
    };
    let mut items = Vec::new();
    items.push(closed(&[
        ("slot", strings(slots.iter().map(|(slot, _)| slot.id()))),
        ("boundaryKind", constant("safe-origin-kind")),
        ("failureReason", reasons("safe-origin-kind")),
    ]));
    for (slot, kinds) in slots {
        for kind in kinds.iter().copied() {
            items.push(closed(&[
                ("slot", constant(slot.id())),
                ("boundaryKind", constant(kind)),
                ("failureReason", reasons(kind)),
            ]));
        }
    }
    json!({ "type": "array", "items": { "oneOf": items }, "minItems": 0 })
}

/// A Resolver Input Admission Case: one boundary kind with its reasons and
/// admitted form, or one of the two admission steps (D15:3140-3168).
fn input_admission(definitions: &mut Map<String, Value>) {
    snapshot_node(definitions);
    selector(definitions);
    let refused = |failure: Value| {
        closed(&[
            ("admitted", json!({ "const": false })),
            ("normalizedInput", json!({ "type": "null" })),
            ("failureReason", failure),
            ("resolverInvoked", json!({ "const": false })),
        ])
    };
    let admitted = |input: Value| {
        closed(&[
            ("admitted", json!({ "const": true })),
            ("normalizedInput", input),
            ("resolverInvoked", json!({ "const": false })),
        ])
    };
    let mut kinds = BOUNDARIES
        .iter()
        .map(|(kind, reasons)| {
            json!({
                "properties": {
                    "boundaryKind": constant(kind),
                    "expected": {
                        "oneOf": [admitted(normalized(kind)), refused(strings(reasons.iter().copied()))]
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    let failures = |list: Value| {
        closed(&[
            ("admitted", json!({ "const": false })),
            ("normalizedInput", json!({ "type": "null" })),
            ("failures", list),
            ("resolverInvoked", json!({ "const": false })),
        ])
    };
    let common: Vec<(SafeOriginKind, &[&str])> = COMMON_SLOTS
        .iter()
        .map(|(slot, kind)| (*slot, std::slice::from_ref(kind)))
        .collect();
    let common_input = closed(
        &COMMON_SLOTS
            .iter()
            .map(|(slot, kind)| (slot.id(), normalized(kind)))
            .collect::<Vec<_>>(),
    );
    kinds.push(json!({
        "properties": {
            "boundaryKind": constant(COMMON_SET),
            "expected": { "oneOf": [admitted(common_input), failures(step_failures(&common))] }
        }
    }));
    let configuration_file = closed(&[
        (
            "bytes",
            json!({ "oneOf": [closed(&[("bytes", pattern("^(?:[0-9a-f]{2})*$"))]), definition("reference")] }),
        ),
        ("identity", json!({ "type": "string", "minLength": 1 })),
        ("nonSecret", json!({ "type": "boolean" })),
        ("revision", digest()),
        ("locator", locator()),
    ]);
    let configuration_programmatic = closed(&[
        ("snapshot", definition("snapshotNode")),
        ("digest", digest()),
        ("identity", json!({ "type": "string", "minLength": 1 })),
        ("nonSecret", json!({ "type": "boolean" })),
        ("revision", digest()),
        ("locator", locator()),
        (
            "callSite",
            json!({ "oneOf": [definition("jsonObject"), { "type": "null" }] }),
        ),
    ]);
    let selector = closed(&[("selector", definition("admittedSelector"))]);
    let entry_input = json!({
        "oneOf": [
            closed(&[
                (SafeOriginKind::ConfigurationFile.id(), configuration_file),
                (SafeOriginKind::ProfileSelectorCli.id(), selector.clone()),
            ]),
            closed(&[
                (SafeOriginKind::ConfigurationProgrammatic.id(), configuration_programmatic),
                (SafeOriginKind::ProfileSelectorProgrammatic.id(), selector),
            ]),
        ]
    });
    let entry_failures = json!({
        "anyOf": [
            step_failures(&FILE_ENTRY_SLOTS),
            step_failures(&PROGRAMMATIC_ENTRY_SLOTS),
        ]
    });
    kinds.push(json!({
        "properties": {
            "boundaryKind": constant(ENTRY_SET),
            "expected": { "oneOf": [admitted(entry_input), failures(entry_failures)] }
        }
    }));
    let kind = ManifestKind::ResolverInputAdmission;
    let mut members = heading(kind, definition("caseId")).to_vec();
    let mut names = BOUNDARIES.iter().map(|(kind, _)| *kind).collect::<Vec<_>>();
    names.extend([COMMON_SET, ENTRY_SET]);
    members.extend([
        ("resolverConstruction", definition("resolverConstruction")),
        ("boundaryKind", strings(names)),
        ("inputFixture", definition("reference")),
        ("expected", json!({ "type": "object" })),
        ("verificationOwner", definition("verificationOwner")),
        ("traceability", definition("traceability")),
    ]);
    let mut schema = closed(&members);
    schema["oneOf"] = Value::Array(kinds);
    definitions.insert(kind.definition().into(), schema);
}

/// A Resolver Case: its construction, its common input, both entries, each
/// with its fixtures or left unpaired with a closed reason, and its
/// relations (D15:3174-3237, 3305).
fn resolver(definitions: &mut Map<String, Value>) {
    let paired = closed(&[
        ("entryAdmissionInputFixture", definition("reference")),
        ("expectedOutcomeFixture", definition("reference")),
    ]);
    let unpaired = |reasons: &[&str]| {
        closed(&[(
            "unpaired",
            closed(&[
                ("reason", strings(reasons.iter().copied())),
                ("verificationOwner", definition("verificationOwner")),
            ]),
        )])
    };
    let entries = json!({
        "oneOf": [
            closed(&[("file", paired.clone()), ("programmatic", paired.clone())]),
            closed(&[("file", paired.clone()), ("programmatic", unpaired(&FILE_ONLY_REASONS))]),
            closed(&[("file", unpaired(&PROGRAMMATIC_ONLY_REASONS)), ("programmatic", paired)]),
        ]
    });
    let mutable = SemanticGroup::ALL
        .iter()
        .copied()
        .filter(|group| *group != SemanticGroup::ProfileSpecification)
        .map(SemanticGroup::id);
    // An equivalence class's change must leave the profile unchanged.
    let equivalence = RELATION_KINDS
        .into_iter()
        .filter(|kind| *kind != "profile-changed");
    let relation = json!({
        "oneOf": [
            closed(&[
                ("role", constant("equivalence")),
                ("kind", strings(equivalence)),
                ("case", definition("caseId")),
                ("class", pattern(COVERAGE_ITEM_PATTERN)),
            ]),
            closed(&[
                ("role", constant("mutation")),
                ("kind", strings(RELATION_KINDS)),
                ("case", definition("caseId")),
                ("class", strings(mutable)),
            ]),
        ]
    });
    let kind = ManifestKind::Resolver;
    let mut members = heading(kind, definition("caseId")).to_vec();
    members.extend([
        ("resolverConstruction", definition("resolverConstruction")),
        ("commonInvocationInputFixture", definition("reference")),
        ("entries", entries),
        ("traceability", definition("traceability")),
    ]);
    definitions.insert(
        kind.definition().into(),
        closed_with(
            &members,
            &[(
                "relations",
                json!({ "type": "array", "items": relation, "minItems": 1 }),
            )],
        ),
    );
}

/// A Handoff Vector: the producing case, the profile facts and bindings it
/// hands over, the consumer's input and relationship, and the owner that
/// verifies them (D15:3309-3348).
fn handoff(definitions: &mut Map<String, Value>) {
    definitions.insert(
        "exactReference".into(),
        closed(&[
            (
                "kind",
                strings(ArtifactKind::ALL.map(ArtifactKind::spelling)),
            ),
            ("identity", token()),
            ("revision", token()),
            ("specificationRevision", token()),
            ("semanticDigest", digest()),
        ]),
    );
    let kind = ManifestKind::HandoffVector;
    let mut members = heading(kind, definition("caseId")).to_vec();
    members.extend([
        ("producerCaseId", definition("caseId")),
        (
            "producerProfileFacts",
            json!({
                "type": "object",
                "minProperties": 1,
                "propertyNames": strings(SemanticGroup::ALL.map(SemanticGroup::id)),
            }),
        ),
        (
            "producerArtifactBindings",
            json!({
                "type": "array",
                "items": closed(&[("key", definition("exactReference"))]),
            }),
        ),
        ("consumerInputs", definition("json")),
        ("expectedRelationship", definition("json")),
        ("verificationOwner", strings(HANDOFF_OWNERS)),
        ("traceability", definition("traceability")),
    ]);
    definitions.insert(kind.definition().into(), closed(&members));
}

/// A Conditional Obligation: one row of the obligation table, inactive with
/// its not-applicable reason or active without one (D15:3352-3382).
fn obligation(definitions: &mut Map<String, Value>) {
    let kind = ManifestKind::ConditionalObligation;
    let rows = OBLIGATIONS
        .iter()
        .flat_map(|(id, activation, reason, owner)| {
            let row = |state: &str, reason: Option<&str>| {
                let mut members = heading(kind, constant(id)).to_vec();
                members.extend([
                    ("activation", constant(activation)),
                    ("activationState", constant(state)),
                ]);
                if let Some(reason) = reason {
                    members.push(("notApplicableReason", constant(reason)));
                }
                members.extend([
                    ("verificationOwner", constant(owner)),
                    ("traceability", definition("traceability")),
                ]);
                closed(&members)
            };
            [row("inactive", Some(*reason)), row("active", None)]
        })
        .collect::<Vec<_>>();
    definitions.insert(kind.definition().into(), json!({ "oneOf": rows }));
}

/// The top-level `failurePrecedence` (D15:3269): each boundary kind's rows,
/// and for the construction cases a reference to the construction-admission
/// body's rows, by the Root Package member that holds the body and the file
/// digest the Suite Index declares for it, instead of a copy. The two
/// admission steps' kinds order their failures as the steps do and have none
/// (D15:3166, 3168).
fn failure_precedence() -> Result<Value, serde_json::Error> {
    let role = RootRole::ConstructionAdmissionSpecification;
    let body = generate::artifacts()?
        .into_iter()
        .find(|artifact| artifact.path == role.path())
        .expect("the construction root writes each member");
    let mut precedence = Map::new();
    precedence.insert(
        ManifestKind::ResolverConstruction.name().into(),
        json!({
            "rootPackageMember": role.name(),
            "digest": file_digest(&body.bytes).as_str(),
        }),
    );
    for (kind, rows) in BOUNDARY_PRECEDENCE {
        let rows = rows
            .iter()
            .map(|(check, reason)| json!({ "check": check, "reason": reason }))
            .collect();
        precedence.insert(kind.into(), Value::Array(rows));
    }
    Ok(Value::Object(precedence))
}

/// The Case Schema's document. Draft 7 ignores the unknown keyword
/// `failurePrecedence`, so it changes nothing that the schema admits.
///
/// # Errors
///
/// Returns the serialization failure of the construction artifacts, which
/// their values rule out.
pub(crate) fn schema() -> Result<Value, serde_json::Error> {
    let mut definitions = Map::new();
    shared(&mut definitions);
    construction(&mut definitions);
    input_admission(&mut definitions);
    resolver(&mut definitions);
    handoff(&mut definitions);
    obligation(&mut definitions);
    Ok(json!({
        "$schema": DRAFT_7,
        "oneOf": ManifestKind::ALL.map(|kind| definition(kind.definition())),
        "definitions": definitions,
        "failurePrecedence": failure_precedence()?,
    }))
}

/// The Case Schema, as this crate writes it.
///
/// # Errors
///
/// Returns the serialization failure of the schema or of the construction
/// artifacts it references, which their values rule out.
pub fn artifact() -> Result<Artifact, serde_json::Error> {
    Ok(Artifact {
        path: PATH,
        bytes: generate::write(&schema()?)?,
    })
}

/// The Case Schema's file digest, which the runner pins.
///
/// # Errors
///
/// Returns the serialization failure of the schema, which its value rules
/// out.
pub fn schema_digest() -> Result<String, serde_json::Error> {
    Ok(file_digest(&artifact()?.bytes).as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;
    use crate::artifacts::collection::{CollectionFailure, CollectionLimit};
    use crate::artifacts::vocabulary::{EnvelopeFailure, EnvelopeLimit};
    use crate::boundary::call_site::CallSiteRefusal;
    use crate::boundary::capability::CapabilityRefusal;
    use crate::boundary::construction_identity::IdentityRefusal;
    use crate::boundary::origin::OriginRefusal;
    use crate::boundary::selector::SelectorRefusal;
    use crate::boundary::snapshot::SnapshotRefusal;
    use crate::boundary::source::SourceRefusal;
    use crate::boundary::specification::{self, SpecificationBound, SpecificationRefusal};
    use crate::boundary::step::Refusal;
    use crate::boundary::verification::{self, VerificationBound, VerificationRefusal};
    use crate::construction::failure::PRECEDENCE;
    use crate::construction::validator::schema::Schema;
    use crate::locale::EnvelopeReason;
    use crate::resolver::capability::CapabilityBound;

    const COMMITTED: &[u8] = include_bytes!("../../conformance/schemas/case-v0.schema.json");

    const ZERO: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    const IDENTITY_TEXT: &str = "rci0_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    /// This crate's validator and the independent oracle, over the schema.
    struct Validators {
        read: Schema,
        oracle: jsonschema::Validator,
    }

    impl Validators {
        fn new() -> Self {
            let schema = schema().unwrap();
            Self {
                read: Schema::compile(&schema).unwrap(),
                oracle: jsonschema::draft7::new(&schema).unwrap(),
            }
        }

        /// Whether the schema admits a manifest, as both decide.
        fn admits(&self, manifest: &Value) -> bool {
            let read = self.read.validate(manifest).valid;
            let oracle = self.oracle.is_valid(manifest);
            assert_eq!(read, oracle, "the validators disagree on {manifest}");
            read
        }
    }

    fn traceability() -> Value {
        json!({
            "decisionIds": ["015-086"],
            "ruleIds": ["015.construction.registryMember.file-digest"],
            "checkIds": []
        })
    }

    fn reference(path: &str) -> Value {
        json!({ "path": path, "digest": ZERO })
    }

    fn construction_case(expected: Value) -> Value {
        let mut manifest = json!({
            "suiteRevision": "0",
            "manifestKind": "resolver-construction",
            "id": "015-construction-revision-0",
            "constructionInputFixture": reference("fixtures/construction/a.json"),
            "expected": null,
            "traceability": traceability()
        });
        manifest["expected"] = expected;
        manifest
    }

    fn constructed() -> Value {
        json!({
            "constructed": true,
            "resolverConstructionIdentity": IDENTITY_TEXT,
            "resolverInvoked": false
        })
    }

    fn not_constructed(reason: &str) -> Value {
        json!({
            "constructed": false,
            "resolverConstructionIdentity": null,
            "failureReason": reason,
            "resolverInvoked": false
        })
    }

    fn admission_case(kind: &str, expected: Value) -> Value {
        let mut manifest = json!({
            "suiteRevision": "0",
            "manifestKind": "resolver-input-admission",
            "id": "015-admission-a",
            "resolverConstruction": {
                "caseId": "015-construction-revision-0",
                "identityReference": IDENTITY_TEXT
            },
            "boundaryKind": kind,
            "inputFixture": reference("fixtures/input-admission/a.json"),
            "expected": null,
            "verificationOwner": "intlify_config",
            "traceability": traceability()
        });
        manifest["expected"] = expected;
        manifest
    }

    fn refused(reason: &str) -> Value {
        json!({
            "admitted": false,
            "normalizedInput": null,
            "failureReason": reason,
            "resolverInvoked": false
        })
    }

    fn admitted(input: Value) -> Value {
        let mut expected =
            json!({ "admitted": true, "normalizedInput": null, "resolverInvoked": false });
        expected["normalizedInput"] = input;
        expected
    }

    fn failures(items: Value) -> Value {
        let mut expected = json!({
            "admitted": false,
            "normalizedInput": null,
            "failures": null,
            "resolverInvoked": false
        });
        expected["failures"] = items;
        expected
    }

    /// An admitted input of the kind's form.
    fn normalized_sample(kind: &str) -> Value {
        match kind {
            _ if JSON_NORMALIZED.contains(&kind) => json!({ "json": { "kind": "a" } }),
            "submitted-profile-resolution-artifact-collection" => {
                json!({ "members": [{ "json": {} }] })
            }
            "safe-origin-kind" => json!({ "safeOriginKind": "configuration-file" }),
            "external-profile-selector-binding" => {
                json!({ "selector": { "report": "string", "value": "web" } })
            }
            "file-source-identity-revision" | "programmatic-entry-identity-revision" => {
                json!({ "identity": "repo:intlify.config.json", "nonSecret": true, "revision": ZERO })
            }
            "programmatic-entry-snapshot" => {
                json!({ "snapshot": { "object": [["a", { "number": "1" }]] }, "digest": ZERO })
            }
            "resolver-construction-identity-reference" => {
                json!({ "resolverConstructionIdentity": IDENTITY_TEXT })
            }
            _ => unreachable!("{kind}"),
        }
    }

    fn entry() -> Value {
        json!({
            "entryAdmissionInputFixture": reference("fixtures/resolver/a.file-input.json"),
            "expectedOutcomeFixture": reference("fixtures/resolver/a.file-expected.json")
        })
    }

    fn unpaired(reason: &str) -> Value {
        json!({ "unpaired": { "reason": reason, "verificationOwner": "intlify_config" } })
    }

    fn resolver_case(file: Value, programmatic: Value) -> Value {
        let mut manifest = json!({
            "suiteRevision": "0",
            "manifestKind": "resolver",
            "id": "015-locale-default-not-in-project",
            "resolverConstruction": {
                "caseId": "015-construction-revision-0",
                "identityReference": IDENTITY_TEXT
            },
            "commonInvocationInputFixture": reference("fixtures/resolver/a.common.json"),
            "entries": { "file": null, "programmatic": null },
            "traceability": traceability()
        });
        manifest["entries"]["file"] = file;
        manifest["entries"]["programmatic"] = programmatic;
        manifest
    }

    fn handoff_vector() -> Value {
        json!({
            "suiteRevision": "0",
            "manifestKind": "handoff-vector",
            "id": "015-handoff-group-selection",
            "producerCaseId": "015-group-selection-one-group",
            "producerProfileFacts": { "deploymentGroups": {} },
            "producerArtifactBindings": [{
                "key": {
                    "kind": "target-profile",
                    "identity": "web",
                    "revision": "1",
                    "specificationRevision": "0",
                    "semanticDigest": ZERO
                }
            }],
            "consumerInputs": { "json": {} },
            "expectedRelationship": { "json": true },
            "verificationOwner": "020",
            "traceability": traceability()
        })
    }

    fn obligation(row: usize, state: &str) -> Value {
        let (id, activation, reason, owner) = OBLIGATIONS[row];
        let mut manifest = json!({
            "suiteRevision": "0",
            "manifestKind": "conditional-obligation",
            "id": id,
            "activation": activation,
            "activationState": state,
            "verificationOwner": owner,
            "traceability": traceability()
        });
        if state == "inactive" {
            manifest["notApplicableReason"] = json!(reason);
        }
        manifest
    }

    /// One manifest of each kind, each admitted.
    fn samples() -> [Value; 5] {
        [
            construction_case(constructed()),
            admission_case("safe-origin-kind", refused("unknown-safe-origin-kind")),
            resolver_case(entry(), entry()),
            handoff_vector(),
            obligation(0, "inactive"),
        ]
    }

    #[test]
    fn the_committed_case_schema_is_the_one_this_crate_writes() {
        let artifact = artifact().unwrap();
        assert_eq!(artifact.path, PATH);
        assert_eq!(
            artifact.bytes, COMMITTED,
            "{PATH} is stale; regenerate it with the generate_conformance_suite example"
        );
        assert!(artifact.bytes.ends_with(b"}\n"));
        assert!(!artifact.bytes.ends_with(b"\n\n"));
        assert!(!artifact.bytes.contains(&b'\r'));
        assert_eq!(
            schema_digest().unwrap(),
            DIGEST,
            "the pinned digest in conformance/case.rs is stale"
        );
    }

    #[test]
    fn the_case_schema_is_a_draft_7_schema_both_validators_read() {
        let schema = schema().unwrap();
        assert_eq!(schema["$schema"], DRAFT_7);
        assert!(jsonschema::draft7::meta::validator().is_valid(&schema));
        let validators = Validators::new();
        for sample in samples() {
            assert!(validators.admits(&sample), "{sample}");
        }
    }

    /// Each kind is a closed object: every member is required, nothing else
    /// is admitted, and a manifest of one kind does not pass as another.
    #[test]
    fn each_manifest_kind_is_closed() {
        let validators = Validators::new();
        for sample in samples() {
            for name in sample.as_object().unwrap().keys() {
                let mut fewer = sample.clone();
                fewer.as_object_mut().unwrap().remove(name);
                assert!(
                    !validators.admits(&fewer),
                    "{} without {name}",
                    sample["manifestKind"]
                );
            }
            let mut more = sample.clone();
            more["unknown"] = json!("unknown");
            assert!(!validators.admits(&more), "{}", sample["manifestKind"]);
            for kind in ManifestKind::ALL {
                let mut other = sample.clone();
                other["manifestKind"] = json!(kind.name());
                assert_eq!(
                    validators.admits(&other),
                    sample["manifestKind"] == kind.name()
                );
            }
            let mut later = sample;
            later["suiteRevision"] = json!("1");
            assert!(!validators.admits(&later));
        }
    }

    #[test]
    fn ids_and_traceability_have_their_grammar() {
        let validators = Validators::new();
        let with_id = |id: &str| {
            let mut manifest = construction_case(constructed());
            manifest["id"] = json!(id);
            manifest
        };
        for id in ["015-a", "015-construction-revision-0", "015-0"] {
            assert!(validators.admits(&with_id(id)), "{id}");
        }
        for id in [
            "015-",
            "015-Upper",
            "016-a",
            "015-a--b",
            "015-a-",
            "015-XV-001",
            "015_a",
        ] {
            assert!(!validators.admits(&with_id(id)), "{id}");
        }
        let with = |member: &str, ids: Value| {
            let mut manifest = construction_case(constructed());
            manifest["traceability"][member] = ids;
            manifest
        };
        for (member, good, bad) in [
            ("decisionIds", "015-213", "015-21"),
            (
                "ruleIds",
                "015.locale.default-requested.member",
                "015.locale.default",
            ),
            (
                "ruleIds",
                "015.construction.registryMember.file-digest",
                "015.construction.RegistryMember.file-digest",
            ),
            (
                "ruleIds",
                "015.construction.all.construction-bounds",
                "015.construction.all.Bounds",
            ),
            (
                "ruleIds",
                "015.input-admission.safe-origin-kind.unknown-safe-origin-kind",
                "015.input-admission.safe-origin-kind",
            ),
            (
                "checkIds",
                "locale.default-requested.member",
                "015.locale.default-requested.member",
            ),
        ] {
            assert!(
                validators.admits(&with(member, json!([good]))),
                "{member} {good}"
            );
            assert!(
                !validators.admits(&with(member, json!([bad]))),
                "{member} {bad}"
            );
            assert!(
                !validators.admits(&with(member, json!([good, good]))),
                "{member} twice"
            );
        }
    }

    /// A construction case expects an identity and no reason, or no identity
    /// and one closed reason, the input-limit reason alone with the bound it
    /// crossed; construction never invokes the resolver.
    #[test]
    fn a_construction_case_expects_an_identity_or_one_closed_reason() {
        let validators = Validators::new();
        let admits = |expected: Value| validators.admits(&construction_case(expected));
        let limit = || {
            let mut expected = not_constructed("construction-input-limit-exceeded");
            expected["failureDetails"] = json!({ "boundId": "input.maxDepth" });
            expected
        };
        assert!(admits(constructed()));
        assert!(admits(not_constructed("registry-member-digest-mismatch")));
        assert!(admits(limit()));
        let mut changed = Vec::new();
        let mut with_reason = constructed();
        with_reason["failureReason"] = json!("registry-member-digest-mismatch");
        changed.push(with_reason);
        let mut invoked = constructed();
        invoked["resolverInvoked"] = json!(true);
        changed.push(invoked);
        let mut identified = not_constructed("registry-member-digest-mismatch");
        identified["resolverConstructionIdentity"] = json!(IDENTITY_TEXT);
        changed.push(identified);
        changed.push(not_constructed("unknown"));
        changed.push(not_constructed("construction-input-limit-exceeded"));
        let mut detailed = not_constructed("registry-member-digest-mismatch");
        detailed["failureDetails"] = json!({ "boundId": "input.maxDepth" });
        changed.push(detailed);
        let mut unknown_bound = limit();
        unknown_bound["failureDetails"]["boundId"] = json!("input.maxEverything");
        changed.push(unknown_bound);
        let mut bad_identity = constructed();
        bad_identity["resolverConstructionIdentity"] =
            json!("rci0_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB");
        changed.push(bad_identity);
        for expected in changed {
            assert!(!admits(expected.clone()), "{expected}");
        }
        for row in PRECEDENCE {
            let reason = serde_json::to_value(row.reason).unwrap();
            let expected = if reason == "construction-input-limit-exceeded" {
                limit()
            } else {
                not_constructed(reason.as_str().unwrap())
            };
            assert!(admits(expected), "{reason}");
        }
    }

    /// An admission case pairs its boundary kind with that kind's reasons and
    /// admitted form, and no other kind's.
    #[test]
    fn an_admission_case_pairs_each_boundary_kind_with_its_own_reasons() {
        let validators = Validators::new();
        let all = BOUNDARIES
            .iter()
            .flat_map(|(_, reasons)| reasons.iter().copied())
            .collect::<BTreeSet<_>>();
        for (kind, reasons) in BOUNDARIES {
            for reason in &all {
                let manifest = admission_case(kind, refused(reason));
                assert_eq!(
                    validators.admits(&manifest),
                    reasons.contains(reason),
                    "{kind} {reason}"
                );
            }
            assert!(
                validators.admits(&admission_case(kind, admitted(normalized_sample(kind)))),
                "{kind}"
            );
            let mut invoked = refused(reasons[0]);
            invoked["resolverInvoked"] = json!(true);
            assert!(!validators.admits(&admission_case(kind, invoked)), "{kind}");
            let mut kept = refused(reasons[0]);
            kept["normalizedInput"] = normalized_sample(kind);
            assert!(!validators.admits(&admission_case(kind, kept)), "{kind}");
        }
        for (kind, other) in [
            ("safe-origin-kind", "external-profile-selector-binding"),
            (
                "programmatic-entry-snapshot",
                "file-source-identity-revision",
            ),
            (
                "resolver-construction-identity-reference",
                "safe-origin-kind",
            ),
            (
                "submitted-profile-resolution-artifact-collection",
                "implementation-capability",
            ),
        ] {
            let manifest = admission_case(kind, admitted(normalized_sample(other)));
            assert!(!validators.admits(&manifest), "{kind} with {other}'s form");
        }
        let manifest = admission_case(COMMON_SET, refused("missing-input"));
        assert!(!validators.admits(&manifest), "a set lists its failures");
        let unknown = json!({ "safeOriginKind": "unknown-kind" });
        let manifest = admission_case("safe-origin-kind", admitted(unknown));
        assert!(
            !validators.admits(&manifest),
            "an unregistered Safe Origin Kind"
        );
    }

    /// Each boundary kind admits only its own form of normalized input: a form
    /// that breaks one rule of the kind's row is refused (D15:3256-3265).
    #[test]
    fn an_admitted_input_has_its_kind_s_form() {
        let validators = Validators::new();
        let source = |identity: &str, non_secret: Value, revision: &str| json!({ "identity": identity, "nonSecret": non_secret, "revision": revision });
        let selector =
            |kind: &str| json!({ "selector": { "report": "invalid-type", "type": kind } });
        for (kind, input) in [
            (
                "implementation-capability",
                json!({ "safeOriginKind": "configuration-file" }),
            ),
            (
                "implementation-capability",
                json!({ "json": {}, "kind": "a" }),
            ),
            (
                "submitted-profile-resolution-artifact-collection",
                json!({ "members": [{ "bytes": "00" }] }),
            ),
            (
                "file-source-identity-revision",
                source("", json!(true), ZERO),
            ),
            (
                "file-source-identity-revision",
                source("repo:a", json!("yes"), ZERO),
            ),
            (
                "programmatic-entry-identity-revision",
                source("repo:a", json!(true), "abc"),
            ),
            (
                "resolver-construction-identity-reference",
                json!({ "resolverConstructionIdentity": "rci0_short" }),
            ),
            ("external-profile-selector-binding", selector("string")),
        ] {
            let manifest = admission_case(kind, admitted(input.clone()));
            assert!(!validators.admits(&manifest), "{kind} {input}");
        }
        for kind in ["null", "boolean", "number", "array", "object"] {
            let manifest = admission_case(
                "external-profile-selector-binding",
                admitted(selector(kind)),
            );
            assert!(validators.admits(&manifest), "{kind}");
        }
        // A collection keeps every member its strict decoder yields, which is
        // any value without numbers, not only an object.
        let collection = |members: Value| {
            admission_case(
                "submitted-profile-resolution-artifact-collection",
                admitted(json!({ "members": members })),
            )
        };
        let members = json!([{ "json": null }, { "json": "text" }, { "json": [true, false] }, { "json": { "a": [null] } }]);
        assert!(validators.admits(&collection(members)));
        for members in [json!([{ "json": 1 }]), json!([{ "json": { "a": [1] } }])] {
            assert!(
                !validators.admits(&collection(members.clone())),
                "{members}"
            );
        }
    }

    /// The two admission steps admit their slots' forms, or list failures,
    /// each naming a slot of the step and a boundary kind of that slot.
    #[test]
    fn an_admission_step_lists_failures_of_its_own_slots() {
        let validators = Validators::new();
        let failure = |slot: &str, kind: &str, reason: &str| json!({ "slot": slot, "boundaryKind": kind, "failureReason": reason });
        let common = COMMON_SLOTS
            .iter()
            .map(|(slot, kind)| (slot.id().to_owned(), normalized_sample(kind)))
            .collect::<Map<_, _>>();
        assert!(validators.admits(&admission_case(
            COMMON_SET,
            admitted(Value::Object(common.clone()))
        )));
        let mut short = common;
        short.remove("implementation-capability");
        assert!(!validators.admits(&admission_case(COMMON_SET, admitted(Value::Object(short)))));
        for (items, valid) in [
            (json!([]), true),
            (
                json!([failure(
                    "implementation-capability",
                    "implementation-capability",
                    "missing-capability"
                )]),
                true,
            ),
            (
                json!([failure(
                    "locale-canonicalization-data",
                    "safe-origin-kind",
                    "missing-safe-origin-kind"
                )]),
                true,
            ),
            (
                json!([failure(
                    "configuration-file",
                    "safe-origin-kind",
                    "missing-safe-origin-kind"
                )]),
                false,
            ),
            (
                json!([failure(
                    "implementation-capability",
                    "profile-resolution-specification-input",
                    "missing-input"
                )]),
                false,
            ),
            (
                json!([failure(
                    "implementation-capability",
                    "implementation-capability",
                    "missing-input"
                )]),
                false,
            ),
        ] {
            assert_eq!(
                validators.admits(&admission_case(COMMON_SET, failures(items.clone()))),
                valid,
                "{items}"
            );
        }
        let selector = json!({ "selector": { "report": "absent" } });
        let file = json!({
            "configuration-file": {
                "bytes": { "bytes": "7b7d" },
                "identity": "repo:intlify.config.json",
                "nonSecret": false,
                "revision": ZERO,
                "locator": { "relative-path": "intlify.config.json" }
            },
            "profile-selector-cli": selector
        });
        let programmatic = json!({
            "configuration-programmatic": {
                "snapshot": { "object": [] },
                "digest": ZERO,
                "identity": "repo:vite.config.ts",
                "nonSecret": true,
                "revision": ZERO,
                "locator": null,
                "callSite": null
            },
            "profile-selector-programmatic": selector
        });
        assert!(validators.admits(&admission_case(ENTRY_SET, admitted(file.clone()))));
        assert!(validators.admits(&admission_case(ENTRY_SET, admitted(programmatic))));
        let mut locator = file.clone();
        locator["configuration-file"]["locator"] = json!({ "unknown": "intlify.config.json" });
        assert!(!validators.admits(&admission_case(ENTRY_SET, admitted(locator))));
        let mut number = file.clone();
        number["configuration-file"]["locator"] = json!({ "relative-path": 1 });
        assert!(!validators.admits(&admission_case(ENTRY_SET, admitted(number))));
        let mut mixed = file;
        mixed["profile-selector-programmatic"] = mixed["profile-selector-cli"].take();
        mixed
            .as_object_mut()
            .unwrap()
            .remove("profile-selector-cli");
        assert!(!validators.admits(&admission_case(ENTRY_SET, admitted(mixed))));
        for (items, valid) in [
            (
                json!([failure(
                    "configuration-file",
                    "file-source-identity-revision",
                    "missing-source-identity"
                )]),
                true,
            ),
            (
                json!([failure(
                    "configuration-programmatic",
                    "programmatic-entry-snapshot",
                    "missing-snapshot"
                )]),
                true,
            ),
            (
                json!([failure(
                    "profile-selector-cli",
                    "external-profile-selector-binding",
                    "unrepresentable-selector"
                )]),
                true,
            ),
            (
                json!([failure(
                    "profile-selector-programmatic",
                    "external-profile-selector-binding",
                    "unrepresentable-selector"
                )]),
                true,
            ),
            (json!([]), true),
            (
                json!([
                    failure(
                        "configuration-file",
                        "file-source-identity-revision",
                        "missing-source-identity"
                    ),
                    failure(
                        "profile-selector-programmatic",
                        "external-profile-selector-binding",
                        "unrepresentable-selector"
                    )
                ]),
                false,
            ),
            (
                json!([failure(
                    "configuration-file",
                    "programmatic-entry-snapshot",
                    "missing-snapshot"
                )]),
                false,
            ),
            (
                json!([failure(
                    "implementation-capability",
                    "safe-origin-kind",
                    "missing-safe-origin-kind"
                )]),
                false,
            ),
        ] {
            assert_eq!(
                validators.admits(&admission_case(ENTRY_SET, failures(items.clone()))),
                valid,
                "{items}"
            );
        }
    }

    /// The Case Schema's boundary table holds exactly the kinds and reasons
    /// the boundaries refuse with.
    #[test]
    fn the_boundary_table_is_the_reasons_the_boundaries_give() {
        let bound = CapabilityBound::inventory()[0].0;
        let refusals = [
            Refusal::Specification(SpecificationRefusal::Missing),
            Refusal::Specification(SpecificationRefusal::OverBound(
                SpecificationBound::InputBytes,
            )),
            Refusal::Specification(SpecificationRefusal::OverBound(SpecificationBound::Depth)),
            Refusal::Specification(SpecificationRefusal::Malformed(
                specification::Malformation::Syntax,
            )),
            Refusal::Specification(SpecificationRefusal::Malformed(
                specification::Malformation::Shape,
            )),
            Refusal::Specification(SpecificationRefusal::Malformed(
                specification::Malformation::Grammar,
            )),
            Refusal::Specification(SpecificationRefusal::DuplicateMember),
            Refusal::Specification(SpecificationRefusal::UnknownMember),
            Refusal::Vocabulary(EnvelopeFailure::Missing),
            Refusal::Vocabulary(EnvelopeFailure::OverBound(EnvelopeLimit::ArtifactBytes)),
            Refusal::Vocabulary(EnvelopeFailure::OverBound(EnvelopeLimit::Depth)),
            Refusal::Vocabulary(EnvelopeFailure::OverBound(EnvelopeLimit::Members)),
            Refusal::Vocabulary(EnvelopeFailure::OverBound(EnvelopeLimit::MemberBytes)),
            Refusal::Vocabulary(EnvelopeFailure::OverBound(EnvelopeLimit::TotalMemberBytes)),
            Refusal::Vocabulary(EnvelopeFailure::Malformed),
            Refusal::Vocabulary(EnvelopeFailure::Unsupported),
            Refusal::Vocabulary(EnvelopeFailure::DuplicateMember),
            Refusal::Vocabulary(EnvelopeFailure::UnknownMember),
            Refusal::Data(EnvelopeReason::MissingInput),
            Refusal::Data(EnvelopeReason::MalformedEnvelope),
            Refusal::Data(EnvelopeReason::DuplicateMember),
            Refusal::Data(EnvelopeReason::UnknownMember),
            Refusal::Data(EnvelopeReason::OverBoundEnvelope),
            Refusal::Collection(CollectionFailure::Missing),
            Refusal::Collection(CollectionFailure::UnsafeHostInspection),
            Refusal::Collection(CollectionFailure::MalformedContainer),
            Refusal::Collection(CollectionFailure::OverBound(CollectionLimit::MemberBytes)),
            Refusal::Collection(CollectionFailure::OverBound(CollectionLimit::TotalBytes)),
            Refusal::Collection(CollectionFailure::OverBound(CollectionLimit::Depth)),
            Refusal::Collection(CollectionFailure::BeyondCapacity),
            Refusal::Collection(CollectionFailure::Unrepresentable),
            Refusal::Verification(VerificationRefusal::Missing),
            Refusal::Verification(VerificationRefusal::OverBound(
                VerificationBound::InputBytes,
            )),
            Refusal::Verification(VerificationRefusal::OverBound(VerificationBound::Depth)),
            Refusal::Verification(VerificationRefusal::OverBound(VerificationBound::Pins)),
            Refusal::Verification(VerificationRefusal::Malformed(
                verification::Malformation::Syntax,
            )),
            Refusal::Verification(VerificationRefusal::Malformed(
                verification::Malformation::Shape,
            )),
            Refusal::Verification(VerificationRefusal::Malformed(
                verification::Malformation::Integrity,
            )),
            Refusal::Verification(VerificationRefusal::Malformed(
                verification::Malformation::Unframable,
            )),
            Refusal::Verification(VerificationRefusal::Malformed(
                verification::Malformation::Mode,
            )),
            Refusal::Verification(VerificationRefusal::Malformed(
                verification::Malformation::Pins,
            )),
            Refusal::Verification(VerificationRefusal::DuplicateMember),
            Refusal::Verification(VerificationRefusal::UnknownMember),
            Refusal::Verification(VerificationRefusal::SecretBearing),
            Refusal::Verification(VerificationRefusal::AmbientAuthority),
            Refusal::Verification(VerificationRefusal::IdentityMismatch),
            Refusal::Verification(VerificationRefusal::RevisionMismatch),
            Refusal::Verification(VerificationRefusal::DigestMismatch),
            Refusal::Origin(OriginRefusal::Missing),
            Refusal::Origin(OriginRefusal::Unknown),
            Refusal::Origin(OriginRefusal::NonCanonical),
            Refusal::Origin(OriginRefusal::EvidenceKindIncompatible),
            Refusal::Selector(SelectorRefusal::UnsafeHostInspection),
            Refusal::Selector(SelectorRefusal::Unrepresentable),
            Refusal::Selector(SelectorRefusal::MalformedNormalized),
            Refusal::Snapshot(SnapshotRefusal::Missing),
            Refusal::Snapshot(SnapshotRefusal::UnsafeHostInspection),
            Refusal::Snapshot(SnapshotRefusal::EnvelopeOverBound),
            Refusal::Snapshot(SnapshotRefusal::Malformed),
            Refusal::Snapshot(SnapshotRefusal::ContentDigestMismatch),
            Refusal::CallSite(CallSiteRefusal::UnsafeHostInspection),
            Refusal::CallSite(CallSiteRefusal::OverBoundInput),
            Refusal::CallSite(CallSiteRefusal::MalformedInput),
            Refusal::CallSite(CallSiteRefusal::InvalidUtf8Source),
            Refusal::CallSite(CallSiteRefusal::DuplicateMapping),
            Refusal::CallSite(CallSiteRefusal::MissingIdentity),
            Refusal::CallSite(CallSiteRefusal::MissingRevision),
            Refusal::CallSite(CallSiteRefusal::UnstableIdentity),
            Refusal::CallSite(CallSiteRefusal::UnboundRevision),
            Refusal::CallSite(CallSiteRefusal::SnapshotMismatch),
            Refusal::Capability(CapabilityRefusal::Missing),
            Refusal::Capability(CapabilityRefusal::Malformed),
            Refusal::Capability(CapabilityRefusal::SpecificationIdentityMismatch),
            Refusal::Capability(CapabilityRefusal::SpecificationRevisionMismatch),
            Refusal::Capability(CapabilityRefusal::Incomplete),
            Refusal::Capability(CapabilityRefusal::NonCanonical(bound)),
            Refusal::Capability(CapabilityRefusal::NonPositive(bound)),
            Refusal::Capability(CapabilityRefusal::Unrepresentable(bound)),
            Refusal::Capability(CapabilityRefusal::BelowMinimum(bound)),
        ];
        let sources = [
            SourceRefusal::MissingIdentity,
            SourceRefusal::MissingRevision,
            SourceRefusal::OverBoundIdentity,
            SourceRefusal::OverBoundRevision,
            SourceRefusal::UnstableIdentity,
            SourceRefusal::UnboundRevision,
            SourceRefusal::SnapshotMismatch,
        ];
        let mut given = refusals
            .iter()
            .chain(&sources.map(Refusal::Source))
            .chain(&sources.map(Refusal::EntryIdentity))
            .map(|refusal| (refusal.boundary_kind(), refusal.reason()))
            .collect::<BTreeSet<_>>();
        for refusal in [
            IdentityRefusal::Missing,
            IdentityRefusal::Malformed,
            IdentityRefusal::Mismatch,
        ] {
            given.insert(("resolver-construction-identity-reference", refusal.reason()));
        }
        let table = BOUNDARIES
            .iter()
            .flat_map(|(kind, reasons)| reasons.iter().map(move |reason| (*kind, *reason)))
            .collect::<BTreeSet<_>>();
        assert_eq!(given, table);
    }

    /// A Resolver Case runs both entries, or leaves one unpaired with a
    /// closed reason of the other side: a file-only case's programmatic entry,
    /// a programmatic-only case's file entry, never both.
    #[test]
    fn a_resolver_case_pairs_its_entries_or_names_why_one_is_unpaired() {
        let validators = Validators::new();
        assert!(validators.admits(&resolver_case(entry(), entry())));
        for reason in FILE_ONLY_REASONS {
            assert!(
                validators.admits(&resolver_case(entry(), unpaired(reason))),
                "{reason}"
            );
            assert!(
                !validators.admits(&resolver_case(unpaired(reason), entry())),
                "{reason}"
            );
        }
        for reason in PROGRAMMATIC_ONLY_REASONS {
            assert!(
                validators.admits(&resolver_case(unpaired(reason), entry())),
                "{reason}"
            );
            assert!(
                !validators.admits(&resolver_case(entry(), unpaired(reason))),
                "{reason}"
            );
        }
        assert!(!validators.admits(&resolver_case(
            unpaired("safe-host-inspection"),
            unpaired("raw-json-syntax")
        )));
        assert!(!validators.admits(&resolver_case(entry(), unpaired("platform-specific"))));
        let mut owner = unpaired("raw-json-syntax");
        owner["unpaired"]["verificationOwner"] = json!("intlify");
        assert!(!validators.admits(&resolver_case(entry(), owner)));
        let mut missing = resolver_case(entry(), entry());
        missing["entries"]
            .as_object_mut()
            .unwrap()
            .remove("programmatic");
        assert!(!validators.admits(&missing));
    }

    /// A relation's class is a Coverage inventory item for an equivalence,
    /// and a semantic group other than the construction-only
    /// `profileSpecification` for a mutation.
    #[test]
    fn a_relation_names_a_class_of_its_role() {
        let validators = Validators::new();
        let with = |relations: Value| {
            let mut manifest = resolver_case(entry(), entry());
            manifest["relations"] = relations;
            manifest
        };
        let relation = |role: &str, kind: &str, class: &str| json!({ "role": role, "kind": kind, "case": "015-locale-default-other", "class": class });
        for kind in RELATION_KINDS {
            assert_eq!(
                validators.admits(&with(json!([relation("equivalence", kind, "015-COV-114")]))),
                kind != "profile-changed",
                "{kind}"
            );
            assert!(
                validators.admits(&with(json!([relation("mutation", kind, "projectLocales")]))),
                "{kind}"
            );
        }
        for relations in [
            json!([]),
            json!([relation("equivalence", "profile-equal", "projectLocales")]),
            json!([relation("mutation", "profile-changed", "015-COV-114")]),
            json!([relation(
                "mutation",
                "profile-changed",
                "profileSpecification"
            )]),
            json!([relation("mutation", "profile-unequal", "projectLocales")]),
            json!([relation("identity", "profile-equal", "015-COV-114")]),
        ] {
            assert!(!validators.admits(&with(relations.clone())), "{relations}");
        }
    }

    #[test]
    fn a_handoff_vector_names_its_producer_facts_and_owner() {
        let validators = Validators::new();
        assert!(validators.admits(&handoff_vector()));
        for owner in HANDOFF_OWNERS {
            let mut vector = handoff_vector();
            vector["verificationOwner"] = json!(owner);
            assert!(validators.admits(&vector), "{owner}");
        }
        let mut changes: Vec<(&str, Value)> = vec![
            ("/verificationOwner", json!("intlify_config")),
            ("/producerProfileFacts", json!({})),
            ("/producerProfileFacts", json!({ "groups": {} })),
            ("/producerArtifactBindings/0/key/kind", json!("glossary")),
            ("/consumerInputs", json!({ "value": {} })),
            ("/producerCaseId", json!("015-XV-001")),
        ];
        for (pointer, value) in changes.drain(..) {
            let mut vector = handoff_vector();
            *vector.pointer_mut(pointer).unwrap() = value;
            assert!(!validators.admits(&vector), "{pointer}");
        }
    }

    /// An obligation is one row of the table: its ID, activation, reason,
    /// and owner together, inactive with the reason or active without it.
    #[test]
    fn a_conditional_obligation_is_one_row_of_the_table() {
        let validators = Validators::new();
        for row in 0..OBLIGATIONS.len() {
            assert!(validators.admits(&obligation(row, "inactive")), "{row}");
            assert!(validators.admits(&obligation(row, "active")), "{row}");
            let mut reasoned = obligation(row, "active");
            reasoned["notApplicableReason"] = json!(OBLIGATIONS[row].2);
            assert!(!validators.admits(&reasoned), "{row}");
            let mut unreasoned = obligation(row, "inactive");
            unreasoned
                .as_object_mut()
                .unwrap()
                .remove("notApplicableReason");
            assert!(!validators.admits(&unreasoned), "{row}");
            let other = OBLIGATIONS[(row + 1) % OBLIGATIONS.len()];
            let mut activation = obligation(row, "inactive");
            activation["activation"] = json!(other.1);
            assert!(!validators.admits(&activation), "{row}");
            let mut owner = obligation(row, "inactive");
            owner["verificationOwner"] = json!(if other.3 == OBLIGATIONS[row].3 {
                "026"
            } else {
                other.3
            });
            assert!(!validators.admits(&owner), "{row}");
        }
        let mut unknown = obligation(0, "inactive");
        unknown["id"] = json!("015-XV-003");
        assert!(!validators.admits(&unknown));
        let mut pending = obligation(0, "pending");
        pending["notApplicableReason"] = json!(OBLIGATIONS[0].2);
        assert!(!validators.admits(&pending));
    }

    /// An admitted Snapshot is canonical in form: seven closed variants, a
    /// marker with a closed reason, and a terminal marker with its bound and
    /// witness.
    #[test]
    fn an_admitted_snapshot_node_is_closed() {
        let validators = Validators::new();
        let with = |node: Value| {
            admission_case(
                "programmatic-entry-snapshot",
                admitted(json!({ "snapshot": node, "digest": ZERO })),
            )
        };
        for node in [
            json!({ "null": null }),
            json!({ "boolean": true }),
            json!({ "number": "-1.5e3" }),
            json!({ "string": "a" }),
            json!({ "array": [{ "null": null }] }),
            json!({ "object": [["a", { "array": [] }]] }),
            json!({ "marker": { "reason": "cycle" } }),
            json!({ "marker": { "reason": "bootstrap-limit-exceeded", "boundPath": "bootstrapOnly.snapshot.maxNodes", "witness": "17" } }),
        ] {
            assert!(validators.admits(&with(node.clone())), "{node}");
        }
        for node in [
            json!({ "undefined": null }),
            json!({ "number": 1 }),
            json!({ "number": "01" }),
            json!({ "string": "a", "null": null }),
            json!({ "stringUtf16": [55296] }),
            json!({ "object": [["a"]] }),
            json!({ "object": [["a", { "null": null }, "b"]] }),
            json!({ "marker": { "reason": "unknown" } }),
            json!({ "marker": { "reason": "bootstrap-limit-exceeded" } }),
            json!({ "marker": { "reason": "cycle", "witness": "1" } }),
            json!({ "marker": { "reason": "bootstrap-limit-exceeded", "boundPath": "policyComparable.maxNodes", "witness": "1" } }),
            json!({ "marker": { "reason": "bootstrap-limit-exceeded", "boundPath": "bootstrapOnly.snapshot.maxNodes", "witness": "0" } }),
            json!({ "marker": { "reason": "bootstrap-limit-exceeded", "boundPath": "bootstrapOnly.snapshot.maxEverything", "witness": "1" } }),
        ] {
            assert!(!validators.admits(&with(node.clone())), "{node}");
        }
        for bound in SnapshotBound::ALL {
            let marker = json!({ "marker": { "reason": "bootstrap-limit-exceeded", "boundPath": bound.path(), "witness": "1" } });
            assert!(validators.admits(&with(marker.clone())), "{marker}");
        }
    }

    /// The closed vocabularies are this crate's: every construction reason
    /// and bound, every Safe Origin Kind, and every semantic group.
    #[test]
    fn the_vocabularies_are_this_crate_s() {
        let schema = schema().unwrap();
        let reasons = schema["definitions"]["resolverConstructionCase"]["properties"]["expected"]
            ["oneOf"][1]["properties"]["failureReason"]["allOf"][0]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|reason| reason.as_str().unwrap())
            .collect::<BTreeSet<_>>();
        let precedence = PRECEDENCE
            .iter()
            .map(|row| serde_json::to_value(row.reason).unwrap())
            .map(|reason| reason.as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(reasons.len(), 38);
        assert_eq!(
            reasons,
            precedence
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
        );
        let bounds = &schema["definitions"]["resolverConstructionCase"]["properties"]["expected"]
            ["oneOf"][2]["properties"]["failureDetails"]["properties"]["boundId"]["enum"];
        assert_eq!(
            bounds,
            &serde_json::to_value(ConstructionBoundId::ALL).unwrap()
        );
        assert_eq!(VERIFICATION_OWNERS.len(), 14);
        assert!(HANDOFF_OWNERS
            .iter()
            .all(|owner| VERIFICATION_OWNERS.contains(owner)));
        assert_eq!(OBLIGATIONS.len(), 8);
        let classes = OBLIGATIONS
            .iter()
            .map(|(id, ..)| &id[4..6])
            .collect::<BTreeSet<_>>();
        assert_eq!(classes, BTreeSet::from(["AD", "HV", "XV"]));
        assert_eq!(BOUNDARIES.len(), 13);
        assert_eq!(
            BOUNDARIES
                .iter()
                .map(|(_, reasons)| reasons.len())
                .sum::<usize>(),
            78
        );
    }

    /// The positions of the general order in which every boundary applies its
    /// checks, in that order (D15:3160, 3269).
    const CHECKS: [&str; 12] = [
        "missing-input",
        "unsafe-host-inspection",
        "container-form",
        "raw-bounds",
        "strict-syntax",
        "count-bounds",
        "closed-shape",
        "repeated-member",
        "unknown-member",
        "reserved-members",
        "value-rules",
        "assertion-comparison",
    ];

    /// The order of the positions a kind's rows take: the general order,
    /// except that the capability boundary compares its specification before
    /// its inventory and bounds (D15:3269).
    fn positions(kind: &str) -> Vec<&'static str> {
        let mut order = CHECKS.to_vec();
        if kind == "implementation-capability" {
            order.swap(10, 11);
        }
        order
    }

    /// Each kind's rows never go back in its order of positions, and list
    /// exactly the reasons of its row of the boundary table, in that row's
    /// order: the order of the first position each occupies (D15:3160).
    #[test]
    fn each_precedence_follows_the_general_order_and_its_row() {
        assert_eq!(
            BOUNDARY_PRECEDENCE.map(|(kind, _)| kind),
            BOUNDARIES.map(|(kind, _)| kind)
        );
        for ((kind, rows), (_, reasons)) in BOUNDARY_PRECEDENCE.iter().zip(BOUNDARIES) {
            let order = positions(kind);
            let taken = rows
                .iter()
                .map(|(check, _)| order.iter().position(|position| position == check))
                .collect::<Option<Vec<_>>>()
                .unwrap_or_else(|| panic!("{kind} names a check outside the order"));
            assert!(taken.is_sorted(), "{kind} goes back in its order");
            let mut first = Vec::new();
            for (_, reason) in *rows {
                if !first.contains(reason) {
                    first.push(*reason);
                }
            }
            assert_eq!(first, reasons.to_vec(), "{kind}");
        }
        assert_eq!(
            BOUNDARY_PRECEDENCE
                .iter()
                .map(|(_, rows)| rows.len())
                .sum::<usize>(),
            92
        );
    }

    /// The schema pins every boundary kind's rows as `{ check, reason }`
    /// objects and the construction cases' reference, and nothing for the
    /// two admission steps' kinds, which have no precedence of their own.
    #[test]
    fn the_schema_pins_the_failure_precedence() {
        let schema = schema().unwrap();
        let precedence = schema["failurePrecedence"].as_object().unwrap();
        let mut kinds = BOUNDARY_PRECEDENCE
            .map(|(kind, _)| kind)
            .into_iter()
            .collect::<BTreeSet<_>>();
        kinds.insert(ManifestKind::ResolverConstruction.name());
        assert_eq!(
            precedence
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            kinds
        );
        for (kind, rows) in BOUNDARY_PRECEDENCE {
            let rows = rows
                .iter()
                .map(|(check, reason)| json!({ "check": check, "reason": reason }))
                .collect::<Vec<_>>();
            assert_eq!(precedence[kind], json!(rows), "{kind}");
        }
        // Draft 7 ignores the keyword: the schema compiles with it.
        Schema::compile(&schema).unwrap();
    }

    /// The construction rows are referenced, not copied: by the Root Package
    /// member that holds them and the file digest the Suite Index declares
    /// for that member, whose body holds the 43 rows.
    #[test]
    fn the_construction_precedence_is_the_member_the_index_pins() {
        let schema = schema().unwrap();
        let reference = &schema["failurePrecedence"]["resolver-construction"];
        let artifacts = super::super::index::artifacts().unwrap();
        let index = artifacts
            .iter()
            .find(|artifact| artifact.path == super::super::index::PATH)
            .unwrap();
        let index: Value = serde_json::from_slice(&index.bytes).unwrap();
        let role = RootRole::ConstructionAdmissionSpecification;
        let member = &index["resolverConstructionRootPackage"]["members"][role.name()];
        assert_eq!(
            reference,
            &json!({ "rootPackageMember": role.name(), "digest": member["digest"] })
        );
        let body = generate::artifacts()
            .unwrap()
            .into_iter()
            .find(|artifact| artifact.path == role.path())
            .unwrap();
        let body: Value = serde_json::from_slice(&body.bytes).unwrap();
        assert_eq!(
            body["failurePrecedence"],
            serde_json::to_value(PRECEDENCE.as_slice()).unwrap()
        );
    }
}
