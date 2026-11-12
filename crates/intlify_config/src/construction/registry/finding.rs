// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Finding Registry and its schema, as far as each code's checks,
//! severity, blocking state, subject kinds, reasons, and suggestion kind go.
//!
//! Design 015's Finding Registry has one row for each of its codes
//! (D15:2227, 2270). Revision `"0"` writes the rows from 015's codes, in
//! table order, and from the Check Registry: a code's checks are the rows
//! that list it, in row order; its subject kinds are those rows' kinds, in
//! the order they first appear; and its reasons are the ones 015's tables
//! give it, in their order, which are exactly those its checks list. Every
//! blocking code is an `error`, and `project-profile-locale-non-canonical`,
//! the one code that blocks nothing, is a `warning` (D15:2223) that suggests
//! the exact canonical replacement (D15:2302); no other code suggests
//! anything. The rows' occurrence Evidence fields come with their own stage.
//!
//! Construction reads the registry it admitted with the registry
//! cross-references (row 41), after the Check Registry, and refuses one this
//! type cannot read or whose rows do not relate to the Check Registry as 015
//! requires: every code has exactly one row (D15:2270); a code's checks are
//! exactly the rows that list it, so that check-to-Finding and
//! Finding-to-check relate symmetrically (D15:2271); and its subject kinds
//! and reasons are exactly those of its checks. A row's severity, blocking
//! state, and suggestion kind are the registry's values, which the file
//! digest compiled in fixes. Every evaluation a test runs asserts that a
//! blocking result's code blocks and a notice's does not.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use intlify_shared_json::decode;

use crate::resolver::evaluation::{Code, Reason};
use crate::resolver::subject::SubjectKind;

use super::check::{CheckRegistry, CheckRow, CHECK_ID_PATTERN};

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
    "sha256:d5edabe903428bbd867bf363af683e29de6b50ab2c323cfeba516a9bf999fe0e";

/// The registry schema's file digest, which the resolver is compiled from.
pub(crate) const SCHEMA_DIGEST: &str =
    "sha256:7a54e536bf4938d207afbf96bad7669f25622ab59b4873829709bfe36e8f6e5e";

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
    pub(crate) suggestion_kind: SuggestionKind,
}

/// The registry's body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FindingRegistry {
    #[schemars(length(min = 1))]
    pub(crate) rows: Vec<FindingRow>,
}

impl FindingRegistry {
    /// The revision-`"0"` registry: one row for each of 015's codes, in
    /// table order, related to the revision-`"0"` Check Registry.
    pub(crate) fn revision_zero() -> Self {
        Self::of(&CheckRegistry::revision_zero())
    }

    /// The rows `checks` relates each code to.
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
                FindingRow {
                    code,
                    allowed_check_ids: listing.iter().map(|row| row.check_id.clone()).collect(),
                    severity: severity(code),
                    blocking: blocking(code),
                    allowed_subject_kinds: kinds,
                    allowed_reasons: code.reasons().to_vec(),
                    suggestion_kind: suggestion_kind(code),
                }
            })
            .collect();
        Self { rows }
    }

    /// Read a registry its admitted schema accepted, as the closed graph
    /// needs it, against the Check Registry the package admitted: a registry
    /// this type cannot read, or one whose rows do not relate to `checks` as
    /// 015 requires, is not part of one.
    pub(crate) fn read(value: &Value, checks: &CheckRegistry) -> Option<Self> {
        let registry = decode::typed::<Self>(value.clone()).ok()?;
        registry.is_closed(checks).then_some(registry)
    }

    /// Whether every code has exactly one row, whose checks are exactly the
    /// rows of `checks` that list the code, and whose subject kinds and
    /// reasons are exactly those of its checks (D15:2270, 2271).
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
/// only suggests its canonical form (D15:2223, 2302).
pub(crate) const fn blocking(code: Code) -> bool {
    !matches!(code, Code::LocaleNonCanonical)
}

/// A code's severity: an error when it blocks, and otherwise a warning
/// (D15:2223).
pub(crate) const fn severity(code: Code) -> Severity {
    if blocking(code) {
        Severity::Error
    } else {
        Severity::Warning
    }
}

/// A code's suggestion kind: the non-canonical spelling suggests its exact
/// canonical replacement (D15:2302), and no other code suggests anything.
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
    /// replacement (D15:2223, 2302).
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
        assert_eq!(FindingRegistry::read(&value, &checks()), Some(registry()));
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
            assert_eq!(FindingRegistry::read(&value, &checks()), None, "{case}");
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
            assert_eq!(FindingRegistry::read(&value, &checks()), None, "{case}");
        }
    }

    /// A code's checks are exactly the rows that list it (D15:2271).
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
            assert_eq!(FindingRegistry::read(&value, &checks()), None, "{case}");
        }
        // The order of a code's checks is the registry's to say.
        let reordered = changed(code, |row| {
            row["allowedCheckIds"].as_array_mut().unwrap().reverse();
        });
        assert!(FindingRegistry::read(&reordered, &checks()).is_some());
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
            assert_eq!(FindingRegistry::read(&value, &checks()), None, "{case}");
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
    fn the_schema_closes_the_container_every_row_and_the_vocabularies() {
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
        for (case, value) in [
            ("no rows", json!({ "rows": [] })),
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
        ] {
            assert!(!read.validate(&value).valid, "{case}");
            assert!(!oracle.is_valid(&value), "{case}");
        }
        // Whether the rows relate to the Check Registry is not the schema's
        // to say.
        let unrelated = changed(&|value| value["rows"][0]["allowedCheckIds"] = json!(["a.b.c"]));
        assert!(read.validate(&unrelated).valid);
        assert!(oracle.is_valid(&unrelated));
    }
}
