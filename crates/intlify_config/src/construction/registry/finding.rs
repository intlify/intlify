// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Finding Registry and its schema: the evidence kinds' disclosure
//! policies, and one row for each code with its checks, severity, blocking
//! state, subject kinds, reasons, occurrence Evidence, and suggestion kind.
//!
//! Design 015's Finding Registry has a table of one disclosure policy for each
//! evidence kind, which `policy` writes, and one row for each of its codes
//! (D15:2233, 2276). Revision `"0"` writes the rows from 015's codes, in table
//! order, and from the Check Registry: a code's checks are the rows that list
//! it, in row order; its subject kinds are those rows' kinds, in the order they
//! first appear; and its reasons are the ones 015's tables give it, in their
//! order, which are exactly those its checks list. Every blocking code is an
//! `error`, and `project-profile-locale-non-canonical`, the one code that
//! blocks nothing, is a `warning` (D15:2229) that suggests the exact canonical
//! replacement (D15:2308); no other code suggests anything. The four occurrence
//! Evidence columns come from `occurrence`.
//!
//! Construction reads the registry it admitted with the registry
//! cross-references (row 41), after the Check Registry, and refuses one this
//! type cannot read, whose policies are not 015's as `policy` checks them,
//! whose occurrence Evidence columns are not as `occurrence` checks them, or
//! whose rows do not relate to the Check Registry as 015 requires: every code
//! has exactly one row (D15:2276); a code's checks are exactly the rows that
//! list it, so that check-to-Finding and Finding-to-check relate symmetrically
//! (D15:2277); and its subject kinds and reasons are exactly those of its
//! checks. A row's severity, blocking state, and suggestion kind are the
//! registry's values, which the file digest compiled in fixes. Every evaluation
//! a test runs asserts that a blocking result's code blocks and a notice's does
//! not.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use intlify_shared_json::decode;

use crate::resolver::evaluation::{Code, Reason};
use crate::resolver::subject::SubjectKind;

use super::check::{CheckRegistry, CheckRow, CHECK_ID_PATTERN};
use super::occurrence::{self, OriginEntry, PolicyEntry, PrimaryEntry, RelatedEntry};
use super::policy::{self, DisclosurePolicy};
use super::safe_origin::SafeOriginKindRegistry;

/// The registry's identity.
pub(crate) const IDENTITY: &str = "intlify-project-profile-finding-registry";

/// The registry schema's identity.
pub(crate) const SCHEMA_IDENTITY: &str = "intlify-project-profile-finding-registry-schema";

/// The revision of the registry and of its schema.
pub(crate) const REVISION: &str = "0";

/// The registry's path below the conformance suite's root.
pub(crate) const PATH: &str = "registries/finding-v0.json";

/// The registry schema's path below the conformance suite's root.
pub(crate) const SCHEMA_PATH: &str = "registries/finding-v0.schema.json";

/// The registry's file digest, which the resolver is compiled from.
pub(crate) const DIGEST: &str =
    "sha256:2a669b7940f28b5a3bb971614fcde0bfb9f7a7a1699f1ef7472fd876834a2e6a";

/// The registry schema's file digest, which the resolver is compiled from.
pub(crate) const SCHEMA_DIGEST: &str =
    "sha256:42bc288412afc35c849bd1af7a85179740bbe8f91147f8993c7fa7bf0d1c43ec";

/// A Finding's severity: `error` for every blocking code, and `warning` for
/// the one code that blocks nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Severity {
    Error,
    Warning,
}

/// The shape of a Finding's safe suggestion: none, or the exact canonical
/// replacement of a locale spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SuggestionKind {
    None,
    CanonicalLocaleReplacement,
}

/// One row: one Finding code, the checks that may report it, and how it
/// reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FindingRow {
    pub(crate) code: Code,
    /// The checks that may report the code, in the Check Registry's order.
    #[schemars(
        length(min = 1),
        extend("uniqueItems" = true),
        inner(pattern(CHECK_ID_PATTERN))
    )]
    pub(crate) allowed_check_ids: Vec<String>,
    pub(crate) severity: Severity,
    pub(crate) blocking: bool,
    /// The subject kinds of those checks, in the order they first appear.
    #[schemars(length(min = 1), extend("uniqueItems" = true))]
    pub(crate) allowed_subject_kinds: Vec<SubjectKind>,
    /// The code's stable reasons, in the order 015 lists them; none for a
    /// code without stable reasons.
    #[schemars(extend("uniqueItems" = true))]
    pub(crate) allowed_reasons: Vec<Reason>,
    /// For each check, in `allowedCheckIds` order, the primary evidence kind
    /// of its occurrences.
    #[schemars(length(min = 1))]
    pub(crate) occurrence_primary_evidence_kind_by_check: Vec<PrimaryEntry>,
    /// For each check, in `allowedCheckIds` order, the related evidence
    /// kinds of its occurrences.
    #[schemars(length(min = 1))]
    pub(crate) occurrence_related_evidence_kinds_by_check: Vec<RelatedEntry>,
    /// The disclosure policy of each evidence kind the two columns before
    /// name, with a resolver-input kind's shapes.
    pub(crate) evidence_disclosure_policies: Vec<PolicyEntry>,
    /// The Safe Origin Kinds the row allows each of those kinds.
    pub(crate) allowed_safe_origin_kinds_by_evidence_kind: Vec<OriginEntry>,
    pub(crate) suggestion_kind: SuggestionKind,
}

/// The registry's body: the disclosure policies, one for each evidence kind,
/// and the rows (D15:2233).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FindingRegistry {
    #[schemars(length(min = 1))]
    pub(crate) policies: Vec<DisclosurePolicy>,
    #[schemars(length(min = 1))]
    pub(crate) rows: Vec<FindingRow>,
}

impl FindingRegistry {
    /// The revision-`"0"` registry: the disclosure policies, and one row for
    /// each of 015's codes, in table order, related to the revision-`"0"`
    /// Check Registry.
    pub(crate) fn revision_zero() -> Self {
        Self::of(&CheckRegistry::revision_zero())
    }

    /// The policies, and the rows `checks` relates each code to.
    fn of(checks: &CheckRegistry) -> Self {
        let rows = Code::ALL
            .into_iter()
            .map(|code| {
                let listing = listing(checks, code);
                let mut kinds = Vec::new();
                for row in &listing {
                    if !kinds.contains(&row.subject_kind) {
                        kinds.push(row.subject_kind);
                    }
                }
                let ids: Vec<String> = listing.iter().map(|row| row.check_id.clone()).collect();
                let columns = occurrence::columns(code, &ids);
                FindingRow {
                    code,
                    allowed_check_ids: ids,
                    severity: severity(code),
                    blocking: blocking(code),
                    allowed_subject_kinds: kinds,
                    allowed_reasons: code.reasons().to_vec(),
                    occurrence_primary_evidence_kind_by_check: columns.primary,
                    occurrence_related_evidence_kinds_by_check: columns.related,
                    evidence_disclosure_policies: columns.policies,
                    allowed_safe_origin_kinds_by_evidence_kind: columns.origins,
                    suggestion_kind: suggestion_kind(code),
                }
            })
            .collect();
        Self {
            policies: policy::revision_zero(),
            rows,
        }
    }

    /// Read a registry its admitted schema accepted, as the closed graph
    /// needs it, against the Check Registry and the Safe Origin Kind
    /// Registry the package admitted: a registry this type cannot read, one
    /// whose rows do not relate to `checks` as 015 requires, one whose
    /// policies or occurrence Evidence columns 015 does not allow under
    /// `origins`, or one that pins a shape no Evidence can carry, is not
    /// part of one.
    pub(crate) fn read(
        value: &Value,
        checks: &CheckRegistry,
        origins: &SafeOriginKindRegistry,
    ) -> Option<Self> {
        let registry = decode::typed::<Self>(value.clone()).ok()?;
        (registry.is_closed(checks)
            && policy::are_closed(&registry.policies, origins)
            && registry
                .rows
                .iter()
                .all(|row| occurrence::row_is_closed(row, &registry.policies, origins))
            && occurrence::are_reachable(&registry.rows, &registry.policies))
        .then_some(registry)
    }

    /// Whether every code has exactly one row, whose checks are exactly the
    /// rows of `checks` that list the code, and whose subject kinds and
    /// reasons are exactly those of its checks (D15:2276, 2277).
    fn is_closed(&self, checks: &CheckRegistry) -> bool {
        let codes: BTreeSet<&str> = self.rows.iter().map(|row| row.code.as_str()).collect();
        codes.len() == self.rows.len()
            && codes.len() == Code::ALL.len()
            && self.rows.iter().all(|row| row.relates_to(checks))
    }
}

impl FindingRow {
    /// Whether the row's checks are exactly the rows of `checks` that list
    /// its code, and its subject kinds and reasons exactly theirs.
    fn relates_to(&self, checks: &CheckRegistry) -> bool {
        let listing = listing(checks, self.code);
        let ids: BTreeSet<&str> = listing.iter().map(|row| row.check_id.as_str()).collect();
        let allowed: BTreeSet<&str> = self.allowed_check_ids.iter().map(String::as_str).collect();
        let kinds: BTreeSet<SubjectKind> = listing.iter().map(|row| row.subject_kind).collect();
        let allowed_kinds: BTreeSet<SubjectKind> =
            self.allowed_subject_kinds.iter().copied().collect();
        let reasons: BTreeSet<&str> = listing
            .iter()
            .flat_map(|row| &row.finding_payload_strategy.details)
            .filter(|details| details.code == self.code)
            .flat_map(|details| &details.reasons)
            .map(|reason| reason.as_str())
            .collect();
        let allowed_reasons: BTreeSet<&str> = self
            .allowed_reasons
            .iter()
            .map(|reason| reason.as_str())
            .collect();
        ids == allowed
            && allowed.len() == self.allowed_check_ids.len()
            && kinds == allowed_kinds
            && allowed_kinds.len() == self.allowed_subject_kinds.len()
            && reasons == allowed_reasons
            && allowed_reasons.len() == self.allowed_reasons.len()
    }
}

/// The rows of `checks` that list `code`, in row order.
fn listing(checks: &CheckRegistry, code: Code) -> Vec<&CheckRow> {
    checks
        .rows
        .iter()
        .filter(|row| row.possible_finding_codes.contains(&code))
        .collect()
}

/// The registry's schema.
///
/// # Errors
///
/// Returns the serialization failure of the generated schema, which the types
/// rule out.
pub(crate) fn schema() -> Result<Value, serde_json::Error> {
    crate::schema::draft7_schema::<FindingRegistry>()
}

/// Whether a code blocks: every code but the non-canonical spelling, which
/// only suggests its canonical form (D15:2229, 2308).
pub(crate) const fn blocking(code: Code) -> bool {
    !matches!(code, Code::LocaleNonCanonical)
}

/// A code's severity: an error when it blocks, and otherwise a warning
/// (D15:2229).
pub(crate) const fn severity(code: Code) -> Severity {
    if blocking(code) {
        Severity::Error
    } else {
        Severity::Warning
    }
}

/// A code's suggestion kind: the non-canonical spelling suggests its exact
/// canonical replacement (D15:2308), and no other code suggests anything.
pub(crate) const fn suggestion_kind(code: Code) -> SuggestionKind {
    match code {
        Code::LocaleNonCanonical => SuggestionKind::CanonicalLocaleReplacement,
        _ => SuggestionKind::None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::admission::{ConstructionAdmission, DRAFT_7};
    use crate::construction::validator::schema::Schema;

    fn registry() -> FindingRegistry {
        FindingRegistry::revision_zero()
    }

    fn checks() -> CheckRegistry {
        CheckRegistry::revision_zero()
    }

    fn origins() -> SafeOriginKindRegistry {
        SafeOriginKindRegistry::revision_zero()
    }

    /// The row of `code`.
    fn row(code: Code) -> FindingRow {
        registry()
            .rows
            .into_iter()
            .find(|row| row.code == code)
            .unwrap()
    }

    #[test]
    fn the_registry_has_one_row_for_each_015_code_in_table_order() {
        let codes: Vec<Code> = registry().rows.iter().map(|row| row.code).collect();
        assert_eq!(codes, Code::ALL);
    }

    /// A code's checks are the rows that list it, in row order, its subject
    /// kinds theirs, and its reasons 015's, which its checks list together.
    #[test]
    fn a_codes_checks_are_the_rows_that_list_it_with_their_kinds_and_reasons() {
        let checks = checks();
        for row in registry().rows {
            let listing = listing(&checks, row.code);
            assert_eq!(
                row.allowed_check_ids,
                listing
                    .iter()
                    .map(|check| check.check_id.clone())
                    .collect::<Vec<_>>(),
                "{}",
                row.code.as_str()
            );
            assert!(!row.allowed_check_ids.is_empty(), "{}", row.code.as_str());
            let mut kinds = row.allowed_subject_kinds.clone();
            kinds.dedup();
            assert_eq!(kinds, row.allowed_subject_kinds, "{}", row.code.as_str());
            assert_eq!(
                row.allowed_reasons,
                row.code.reasons(),
                "{}",
                row.code.as_str()
            );
        }
        let invalid = row(Code::HydrationRelationInvalid);
        assert_eq!(
            invalid.allowed_check_ids,
            [
                "hydration.endpoint.known",
                "hydration.endpoint.same-group",
                "hydration.endpoint.role",
                "hydration.relation.distinct-endpoints",
                "hydration.relation.unique",
                "hydration.target.single-role"
            ]
        );
        assert_eq!(
            invalid.allowed_subject_kinds,
            [
                SubjectKind::RelationEndpoint,
                SubjectKind::Relation,
                SubjectKind::Roles
            ]
        );
        assert_eq!(row(Code::ResourceLimitExceeded).allowed_check_ids.len(), 65);
    }

    /// Every blocking code is an error, and the non-canonical spelling, the
    /// one code that blocks nothing, is a warning that suggests its canonical
    /// replacement (D15:2229, 2308).
    #[test]
    fn every_blocking_code_is_an_error_and_only_the_non_canonical_spelling_suggests() {
        for row in registry().rows {
            let non_canonical = row.code == Code::LocaleNonCanonical;
            assert_eq!(row.blocking, !non_canonical, "{}", row.code.as_str());
            assert_eq!(
                row.severity,
                if non_canonical {
                    Severity::Warning
                } else {
                    Severity::Error
                }
            );
            assert_eq!(
                row.suggestion_kind,
                if non_canonical {
                    SuggestionKind::CanonicalLocaleReplacement
                } else {
                    SuggestionKind::None
                }
            );
        }
    }

    #[test]
    fn a_row_reads_and_writes_in_015s_terms() {
        assert_eq!(
            serde_json::to_value(row(Code::LocaleNonCanonical)).unwrap(),
            json!({
                "code": "project-profile-locale-non-canonical",
                "allowedCheckIds": [
                    "locale.requested.canonical",
                    "locale.default-source.canonical",
                    "locale.default-requested.canonical",
                    "locale.alias-key.canonical",
                    "locale.alias-destination.canonical",
                    "locale.fallback-key.canonical",
                    "locale.target-requested.canonical",
                    "locale.target-default-requested.canonical",
                    "fallback.candidate.canonical",
                    "coverage.locale.canonical"
                ],
                "severity": "warning",
                "blocking": false,
                "allowedSubjectKinds": [
                    "requested-locale",
                    "default-source-locale",
                    "default-requested-locale",
                    "negotiation-alias-key",
                    "negotiation-alias-value",
                    "message-fallback-key",
                    "target-requested-locale",
                    "target-default-requested-locale",
                    "fallback-candidate",
                    "coverage-locale"
                ],
                "allowedReasons": [],
                "occurrencePrimaryEvidenceKindByCheck": [
                    { "checkId": "locale.requested.canonical", "kind": "configuration-value" },
                    { "checkId": "locale.default-source.canonical", "kind": "configuration-value" },
                    { "checkId": "locale.default-requested.canonical", "kind": "configuration-value" },
                    { "checkId": "locale.alias-key.canonical", "kind": "configuration-value" },
                    { "checkId": "locale.alias-destination.canonical", "kind": "configuration-value" },
                    { "checkId": "locale.fallback-key.canonical", "kind": "configuration-value" },
                    { "checkId": "locale.target-requested.canonical", "kind": "configuration-value" },
                    { "checkId": "locale.target-default-requested.canonical", "kind": "configuration-value" },
                    { "checkId": "fallback.candidate.canonical", "kind": "configuration-value" },
                    { "checkId": "coverage.locale.canonical", "kind": "configuration-value" }
                ],
                "occurrenceRelatedEvidenceKindsByCheck": [
                    { "checkId": "locale.requested.canonical", "kinds": [] },
                    { "checkId": "locale.default-source.canonical", "kinds": [] },
                    { "checkId": "locale.default-requested.canonical", "kinds": [] },
                    { "checkId": "locale.alias-key.canonical", "kinds": [] },
                    { "checkId": "locale.alias-destination.canonical", "kinds": [] },
                    { "checkId": "locale.fallback-key.canonical", "kinds": [] },
                    { "checkId": "locale.target-requested.canonical", "kinds": [] },
                    { "checkId": "locale.target-default-requested.canonical", "kinds": [] },
                    { "checkId": "fallback.candidate.canonical", "kinds": [] },
                    { "checkId": "coverage.locale.canonical", "kinds": [] }
                ],
                "evidenceDisclosurePolicies": [{ "evidenceKind": "configuration-value" }],
                "allowedSafeOriginKindsByEvidenceKind": [{
                    "evidenceKind": "configuration-value",
                    "origins": ["configuration-file", "configuration-programmatic"]
                }],
                "suggestionKind": "canonical-locale-replacement"
            })
        );
        let value = serde_json::to_value(registry()).unwrap();
        assert_eq!(
            serde_json::from_value::<FindingRegistry>(value).unwrap(),
            registry()
        );
    }

    /// The registry as JSON, with `change` made to the row of `code`.
    fn changed(code: Code, change: impl Fn(&mut Value)) -> Value {
        let mut value = serde_json::to_value(registry()).unwrap();
        for row in value["rows"].as_array_mut().unwrap() {
            if row["code"] == code.as_str() {
                change(row);
            }
        }
        value
    }

    #[test]
    fn construction_reads_the_registry_against_the_check_registry() {
        let value = serde_json::to_value(registry()).unwrap();
        assert_eq!(
            FindingRegistry::read(&value, &checks(), &origins()),
            Some(registry())
        );
        // A registry this type cannot read is not part of one.
        for (case, value) in [
            (
                "an unknown code",
                changed(Code::JsonInvalid, |row| {
                    row["code"] = json!("project-profile-unknown");
                }),
            ),
            (
                "another member",
                changed(Code::JsonInvalid, |row| row["note"] = json!("note")),
            ),
            (
                "an unknown severity",
                changed(Code::JsonInvalid, |row| row["severity"] = json!("fatal")),
            ),
        ] {
            assert_eq!(
                FindingRegistry::read(&value, &checks(), &origins()),
                None,
                "{case}"
            );
        }
    }

    #[test]
    fn every_code_has_exactly_one_row() {
        let mut missing = serde_json::to_value(registry()).unwrap();
        missing["rows"].as_array_mut().unwrap().pop();
        let mut repeated = serde_json::to_value(registry()).unwrap();
        let first = repeated["rows"][0].clone();
        repeated["rows"].as_array_mut().unwrap().push(first);
        for (case, value) in [
            ("a code without a row", missing),
            ("a code twice", repeated),
        ] {
            assert_eq!(
                FindingRegistry::read(&value, &checks(), &origins()),
                None,
                "{case}"
            );
        }
    }

    /// A code's checks are exactly the rows that list it (D15:2277).
    #[test]
    fn a_codes_checks_and_the_checks_codes_are_symmetric() {
        let code = Code::HydrationRelationInvalid;
        for (case, value) in [
            (
                "a check left out",
                changed(code, |row| {
                    row["allowedCheckIds"].as_array_mut().unwrap().pop();
                }),
            ),
            (
                "a check that does not list the code",
                changed(code, |row| {
                    row["allowedCheckIds"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("hydration.endpoint.within-bound"));
                }),
            ),
            (
                "a check of no row",
                changed(code, |row| {
                    row["allowedCheckIds"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("hydration.relation.unknown"));
                }),
            ),
            (
                "a check twice",
                changed(code, |row| {
                    row["allowedCheckIds"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("hydration.endpoint.known"));
                }),
            ),
        ] {
            assert_eq!(
                FindingRegistry::read(&value, &checks(), &origins()),
                None,
                "{case}"
            );
        }
        // The order of a code's checks is the registry's to say, and the
        // per-check columns follow it.
        let reordered = changed(code, |row| {
            for column in [
                "allowedCheckIds",
                "occurrencePrimaryEvidenceKindByCheck",
                "occurrenceRelatedEvidenceKindsByCheck",
            ] {
                row[column].as_array_mut().unwrap().reverse();
            }
        });
        assert!(FindingRegistry::read(&reordered, &checks(), &origins()).is_some());
    }

    #[test]
    fn a_codes_subject_kinds_and_reasons_are_its_checks() {
        let code = Code::HydrationRelationInvalid;
        for (case, value) in [
            (
                "a kind left out",
                changed(code, |row| {
                    row["allowedSubjectKinds"].as_array_mut().unwrap().pop();
                }),
            ),
            (
                "a kind none of its checks has",
                changed(code, |row| {
                    row["allowedSubjectKinds"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("relations"));
                }),
            ),
            (
                "a reason left out",
                changed(code, |row| {
                    row["allowedReasons"].as_array_mut().unwrap().pop();
                }),
            ),
            (
                "a reason none of its checks lists",
                changed(code, |row| {
                    row["allowedReasons"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("not-found"));
                }),
            ),
        ] {
            assert_eq!(
                FindingRegistry::read(&value, &checks(), &origins()),
                None,
                "{case}"
            );
        }
    }

    #[test]
    fn the_registry_names_what_construction_admits() {
        let admission = ConstructionAdmission::revision_zero();
        let members = &admission.registry_members;
        assert_eq!(members.finding_registry.identity, IDENTITY);
        assert_eq!(members.finding_registry.revisions, [REVISION]);
        assert_eq!(members.finding_registry_schema.identity, SCHEMA_IDENTITY);
        assert_eq!(members.finding_registry_schema.revisions, [REVISION]);
    }

    #[test]
    fn the_schema_closes_the_container_every_policy_and_row_and_the_vocabularies() {
        let document = schema().unwrap();
        assert_eq!(document["$schema"], DRAFT_7);
        let read = Schema::compile(&document).unwrap();
        let oracle = jsonschema::draft7::new(&document).unwrap();
        let registry = serde_json::to_value(registry()).unwrap();
        assert!(read.validate(&registry).valid);
        assert!(oracle.is_valid(&registry));
        let changed = |change: &dyn Fn(&mut Value)| {
            let mut value = registry.clone();
            change(&mut value);
            value
        };
        // The entry source's first component is literal and its second
        // redacted; the specification input pins slots, and the artifact
        // input's second shape is a member with an admitted key.
        let components = &registry["policies"][0]["components"];
        assert_eq!(components[0]["component"], "source.safe-opaque");
        assert_eq!(components[1]["component"], "source.redacted");
        assert_eq!(
            registry["policies"][3]["evidenceKind"],
            "specification-input"
        );
        assert_eq!(registry["policies"][4]["paths"][1]["key"], "safe-key");
        // The first row has no shape, and the twelfth row's first policy has
        // some.
        assert_eq!(registry["rows"][0]["code"], "project-profile-json-invalid");
        assert_eq!(
            registry["rows"][11]["evidenceDisclosurePolicies"][0]["evidenceKind"],
            "specification-input"
        );
        for (case, value) in [
            ("no rows", changed(&|value| value["rows"] = json!([]))),
            (
                "no policies member",
                changed(&|value| {
                    value.as_object_mut().unwrap().remove("policies");
                }),
            ),
            (
                "no policies",
                changed(&|value| value["policies"] = json!([])),
            ),
            (
                "a policy's other member",
                changed(&|value| value["policies"][0]["note"] = json!("note")),
            ),
            (
                "an unknown evidence kind",
                changed(&|value| value["policies"][0]["evidenceKind"] = json!("secret")),
            ),
            (
                "no required member",
                changed(&|value| value["policies"][0]["required"] = json!([])),
            ),
            (
                "a required member twice",
                changed(&|value| value["policies"][0]["required"] = json!(["source", "source"])),
            ),
            (
                "an unknown member",
                changed(&|value| value["policies"][0]["required"] = json!(["note"])),
            ),
            (
                "no component",
                changed(&|value| value["policies"][0]["components"] = json!([])),
            ),
            (
                "a component's other member",
                changed(&|value| value["policies"][0]["components"][0]["note"] = json!("note")),
            ),
            (
                "an unknown component",
                changed(&|value| {
                    value["policies"][0]["components"][0]["component"] = json!("source.raw");
                }),
            ),
            (
                "an unknown disclosure",
                changed(&|value| {
                    value["policies"][0]["components"][0]["disclosure"] = json!("hashed");
                }),
            ),
            (
                "no classification",
                changed(&|value| {
                    value["policies"][0]["components"][0]["nonSecretWhen"] = json!([]);
                }),
            ),
            (
                "a classification twice",
                changed(&|value| {
                    value["policies"][0]["components"][0]["nonSecretWhen"] =
                        json!(["owner-marked-identity", "owner-marked-identity"]);
                }),
            ),
            (
                "an unknown classification",
                changed(&|value| {
                    value["policies"][0]["components"][0]["nonSecretWhen"] =
                        json!(["caller-classified"]);
                }),
            ),
            (
                "a null projection",
                changed(&|value| {
                    value["policies"][0]["components"][1]["tokenProjection"] = Value::Null;
                }),
            ),
            (
                "an unknown projection",
                changed(&|value| {
                    value["policies"][0]["components"][1]["tokenProjection"] = json!("raw");
                }),
            ),
            (
                "no path",
                changed(&|value| value["policies"][3]["paths"] = json!([])),
            ),
            (
                "a path's other member",
                changed(&|value| value["policies"][3]["paths"][0]["note"] = json!("note")),
            ),
            (
                "an unknown origin",
                changed(&|value| value["policies"][3]["paths"][0]["origin"] = json!("cli")),
            ),
            (
                "a role without its rank",
                changed(&|value| {
                    value["policies"][3]["paths"][0]["roles"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("rank");
                }),
            ),
            (
                "a rank that is no rank",
                changed(&|value| {
                    value["policies"][3]["paths"][0]["roles"][0]["rank"] = json!("01");
                }),
            ),
            (
                "an empty role",
                changed(&|value| value["policies"][3]["paths"][0]["roles"][0]["role"] = json!("")),
            ),
            (
                "a null key",
                changed(&|value| value["policies"][4]["paths"][1]["key"] = Value::Null),
            ),
            (
                "an unknown key",
                changed(&|value| value["policies"][4]["paths"][1]["key"] = json!("hashed-key")),
            ),
            (
                "another member",
                changed(&|value| value["note"] = json!("note")),
            ),
            (
                "a row's other member",
                changed(&|value| value["rows"][0]["note"] = json!("note")),
            ),
            (
                "a row without its severity",
                changed(&|value| {
                    value["rows"][0].as_object_mut().unwrap().remove("severity");
                }),
            ),
            (
                "an unknown code",
                changed(&|value| value["rows"][0]["code"] = json!("project-profile-unknown")),
            ),
            (
                "no check",
                changed(&|value| value["rows"][0]["allowedCheckIds"] = json!([])),
            ),
            (
                "a check twice",
                changed(&|value| value["rows"][0]["allowedCheckIds"] = json!(["a.b.c", "a.b.c"])),
            ),
            (
                "a check that is no check ID",
                changed(&|value| value["rows"][0]["allowedCheckIds"] = json!(["a.b"])),
            ),
            (
                "an unknown severity",
                changed(&|value| value["rows"][0]["severity"] = json!("fatal")),
            ),
            (
                "a blocking state as a string",
                changed(&|value| value["rows"][0]["blocking"] = json!("yes")),
            ),
            (
                "no subject kind",
                changed(&|value| value["rows"][0]["allowedSubjectKinds"] = json!([])),
            ),
            (
                "an unknown subject kind",
                changed(&|value| value["rows"][0]["allowedSubjectKinds"] = json!(["no-such-kind"])),
            ),
            (
                "an unknown reason",
                changed(&|value| value["rows"][0]["allowedReasons"] = json!(["missing"])),
            ),
            (
                "a reason twice",
                changed(&|value| value["rows"][0]["allowedReasons"] = json!(["cycle", "cycle"])),
            ),
            (
                "an unknown suggestion kind",
                changed(&|value| value["rows"][0]["suggestionKind"] = json!("replacement")),
            ),
            (
                "a row without its primary kinds",
                changed(&|value| {
                    value["rows"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("occurrencePrimaryEvidenceKindByCheck");
                }),
            ),
            (
                "no primary entry",
                changed(&|value| {
                    value["rows"][0]["occurrencePrimaryEvidenceKindByCheck"] = json!([]);
                }),
            ),
            (
                "a primary entry's other member",
                changed(&|value| {
                    value["rows"][0]["occurrencePrimaryEvidenceKindByCheck"][0]["note"] =
                        json!("note");
                }),
            ),
            (
                "an unknown primary kind",
                changed(&|value| {
                    value["rows"][0]["occurrencePrimaryEvidenceKindByCheck"][0]["kind"] =
                        json!("secret");
                }),
            ),
            (
                "a primary check that is no check ID",
                changed(&|value| {
                    value["rows"][0]["occurrencePrimaryEvidenceKindByCheck"][0]["checkId"] =
                        json!("a.b");
                }),
            ),
            (
                "no related entry",
                changed(&|value| {
                    value["rows"][0]["occurrenceRelatedEvidenceKindsByCheck"] = json!([]);
                }),
            ),
            (
                "none as a related kind",
                changed(&|value| {
                    value["rows"][0]["occurrenceRelatedEvidenceKindsByCheck"][0]["kinds"] =
                        json!(["none"]);
                }),
            ),
            (
                "a related kind twice",
                changed(&|value| {
                    value["rows"][0]["occurrenceRelatedEvidenceKindsByCheck"][0]["kinds"] =
                        json!(["entry-source", "entry-source"]);
                }),
            ),
            (
                "a policy entry's other member",
                changed(&|value| {
                    value["rows"][0]["evidenceDisclosurePolicies"][0]["note"] = json!("note");
                }),
            ),
            (
                "no shape",
                changed(&|value| {
                    value["rows"][11]["evidenceDisclosurePolicies"][0]["paths"] = json!([]);
                }),
            ),
            (
                "a shape's other member",
                changed(&|value| {
                    value["rows"][11]["evidenceDisclosurePolicies"][0]["paths"][0]["note"] =
                        json!("note");
                }),
            ),
            (
                "an origin entry's other member",
                changed(&|value| {
                    value["rows"][0]["allowedSafeOriginKindsByEvidenceKind"][0]["note"] =
                        json!("note");
                }),
            ),
            (
                "no origin",
                changed(&|value| {
                    value["rows"][0]["allowedSafeOriginKindsByEvidenceKind"][0]["origins"] =
                        json!([]);
                }),
            ),
            (
                "an origin twice",
                changed(&|value| {
                    value["rows"][0]["allowedSafeOriginKindsByEvidenceKind"][0]["origins"] =
                        json!(["configuration-file", "configuration-file"]);
                }),
            ),
            (
                "an unknown origin",
                changed(&|value| {
                    value["rows"][0]["allowedSafeOriginKindsByEvidenceKind"][0]["origins"] =
                        json!(["cli"]);
                }),
            ),
        ] {
            assert!(!read.validate(&value).valid, "{case}");
            assert!(!oracle.is_valid(&value), "{case}");
        }
        // Whether the rows relate to the Check Registry, and whether the
        // policies and the occurrence Evidence columns are 015's, is not the
        // schema's to say.
        let unrelated = changed(&|value| value["rows"][0]["allowedCheckIds"] = json!(["a.b.c"]));
        let misaligned = changed(&|value| {
            value["rows"][0]["occurrencePrimaryEvidenceKindByCheck"][0]["checkId"] = json!("a.b.c");
        });
        let departing = changed(&|value| {
            value["policies"][0]["components"]
                .as_array_mut()
                .unwrap()
                .pop();
        });
        for value in [unrelated, misaligned, departing] {
            assert!(read.validate(&value).valid);
            assert!(oracle.is_valid(&value));
        }
    }
}
