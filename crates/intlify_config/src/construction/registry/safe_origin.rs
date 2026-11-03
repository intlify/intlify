// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Safe Origin Kind Registry and its schema.
//!
//! The registry lists design 015's ten Safe Origin Kinds in rank order, each
//! with its ID, its rank as a shortest decimal string, and the evidence kinds
//! it permits in the order of 015's table. The table's boundary descriptions
//! are documentation, not registry fields. Revision `"0"` writes the registry
//! from this crate's vocabulary and its schema from the same types.
//!
//! The schema closes the container, every row, the vocabularies of IDs and
//! evidence kinds, and a rank's spelling. It does not say which rank or which
//! evidence kinds an ID has: the registry's file digest, compiled into the
//! resolver, fixes that. Construction reads the registry it admitted and
//! checks that no ID and no rank repeats.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use intlify_shared_json::decode;

use crate::resolver::origin::{EvidenceKind, SafeOriginKind};

use super::RANK_PATTERN;

/// The registry's identity.
pub(crate) const IDENTITY: &str = "intlify-project-profile-safe-origin-kind-registry";

/// The registry schema's identity.
pub(crate) const SCHEMA_IDENTITY: &str = "intlify-project-profile-safe-origin-kind-registry-schema";

/// The revision of the registry and of its schema.
pub(crate) const REVISION: &str = "0";

/// The registry's path below the conformance suite's root.
pub(crate) const PATH: &str = "registries/safe-origin-kind-v0.json";

/// The registry schema's path below the conformance suite's root.
pub(crate) const SCHEMA_PATH: &str = "registries/safe-origin-kind-v0.schema.json";

/// The registry's file digest, which the resolver is compiled from.
pub(crate) const DIGEST: &str =
    "sha256:5e38b2ad8d2bb7eeeb2e17e885817486d8e14aaaef5e7d6c4b262b185c10fba7";

/// The registry schema's file digest, which the resolver is compiled from.
pub(crate) const SCHEMA_DIGEST: &str =
    "sha256:3fec118838b775196e7c08834fab1779ef6cc596f17f6ab210d7605f1cceec89";

/// One Safe Origin Kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SafeOriginKindRow {
    pub(crate) id: SafeOriginKind,
    #[schemars(pattern(RANK_PATTERN))]
    pub(crate) rank: String,
    #[schemars(length(min = 1), extend("uniqueItems" = true))]
    pub(crate) permitted_evidence_kinds: Vec<EvidenceKind>,
}

/// The registry's body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SafeOriginKindRegistry {
    #[schemars(length(min = 1))]
    pub(crate) rows: Vec<SafeOriginKindRow>,
}

impl SafeOriginKindRegistry {
    /// The revision-`"0"` registry: every Safe Origin Kind in rank order.
    pub(crate) fn revision_zero() -> Self {
        Self {
            rows: SafeOriginKind::ALL
                .iter()
                .map(|&kind| SafeOriginKindRow {
                    id: kind,
                    rank: kind.rank().to_string(),
                    permitted_evidence_kinds: kind.permitted_evidence_kinds().to_vec(),
                })
                .collect(),
        }
    }

    /// Read a registry its admitted schema accepted, as the closed graph needs
    /// it: a registry this type cannot read, or one that repeats an ID or a
    /// rank, is not part of one.
    pub(crate) fn read(value: &Value) -> Option<Self> {
        let registry = decode::typed::<Self>(value.clone()).ok()?;
        let ids = registry
            .rows
            .iter()
            .map(|row| row.id)
            .collect::<BTreeSet<_>>();
        let ranks = registry
            .rows
            .iter()
            .map(|row| row.rank.as_str())
            .collect::<BTreeSet<_>>();
        (ids.len() == registry.rows.len() && ranks.len() == registry.rows.len()).then_some(registry)
    }
}

/// The registry's schema.
///
/// # Errors
///
/// Returns the serialization failure of the generated schema, which the types
/// rule out.
pub(crate) fn schema() -> Result<Value, serde_json::Error> {
    crate::schema::draft7_schema::<SafeOriginKindRegistry>()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::admission::{ConstructionAdmission, DRAFT_7};
    use crate::construction::validator::schema::Schema;

    fn registry() -> Value {
        serde_json::to_value(SafeOriginKindRegistry::revision_zero()).unwrap()
    }

    #[test]
    fn the_registry_lists_015s_ten_kinds_in_rank_order() {
        let row = |id: &str, rank: &str, kinds: &[&str]| json!({ "id": id, "rank": rank, "permittedEvidenceKinds": kinds });
        assert_eq!(
            registry(),
            json!({
                "rows": [
                    row("configuration-file", "0", &["entry-source", "configuration-value"]),
                    row("configuration-programmatic", "1", &["entry-source", "configuration-value"]),
                    row("profile-selector-cli", "2", &["profile-selector"]),
                    row("profile-selector-programmatic", "3", &["profile-selector"]),
                    row("profile-resolution-specification-input", "4", &["specification-input"]),
                    row("intent-surface-class-vocabulary", "5", &["artifact-input"]),
                    row("locale-canonicalization-data", "6", &["artifact-input"]),
                    row("profile-resolution-artifact-collection", "7", &["artifact-input"]),
                    row("resource-limit-policy-verification", "8", &["verification-input"]),
                    row("implementation-capability", "9", &["implementation-capability"]),
                ]
            })
        );
        // A row's members are written in the order the fields declare them.
        assert_eq!(
            serde_json::to_string(&registry()["rows"][0]).unwrap(),
            r#"{"id":"configuration-file","rank":"0","permittedEvidenceKinds":["entry-source","configuration-value"]}"#
        );
    }

    #[test]
    fn the_registry_names_what_construction_admits() {
        let admission = ConstructionAdmission::revision_zero();
        let members = &admission.registry_members;
        assert_eq!(members.safe_origin_kind_registry.identity, IDENTITY);
        assert_eq!(members.safe_origin_kind_registry.revisions, [REVISION]);
        assert_eq!(
            members.safe_origin_kind_registry_schema.identity,
            SCHEMA_IDENTITY
        );
        assert_eq!(
            members.safe_origin_kind_registry_schema.revisions,
            [REVISION]
        );
    }

    #[test]
    fn the_schema_closes_the_container_every_row_and_both_vocabularies() {
        let document = schema().unwrap();
        assert_eq!(document["$schema"], DRAFT_7);
        let read = Schema::compile(&document).unwrap();
        let oracle = jsonschema::draft7::new(&document).unwrap();
        let registry = registry();
        assert!(read.validate(&registry).valid);
        assert!(oracle.is_valid(&registry));
        let changed = |change: &dyn Fn(&mut Value)| {
            let mut value = registry.clone();
            change(&mut value);
            value
        };
        for (case, value) in [
            ("no rows", json!({ "rows": [] })),
            ("no container", json!([])),
            (
                "another member",
                changed(&|value| value["note"] = json!("note")),
            ),
            (
                "a row's other member",
                changed(&|value| value["rows"][0]["note"] = json!("note")),
            ),
            (
                "a row without its rank",
                changed(&|value| {
                    value["rows"][0].as_object_mut().unwrap().remove("rank");
                }),
            ),
            (
                "an unknown ID",
                changed(&|value| value["rows"][0]["id"] = json!("configuration-env")),
            ),
            (
                "an ID in another case",
                changed(&|value| value["rows"][0]["id"] = json!("Configuration-File")),
            ),
            (
                "a rank with a leading zero",
                changed(&|value| value["rows"][0]["rank"] = json!("00")),
            ),
            (
                "a rank as a number",
                changed(&|value| value["rows"][0]["rank"] = json!(0)),
            ),
            (
                "no evidence kind",
                changed(&|value| value["rows"][0]["permittedEvidenceKinds"] = json!([])),
            ),
            (
                "an evidence kind twice",
                changed(&|value| {
                    value["rows"][2]["permittedEvidenceKinds"] =
                        json!(["profile-selector", "profile-selector"]);
                }),
            ),
            (
                "an unknown evidence kind",
                changed(&|value| {
                    value["rows"][2]["permittedEvidenceKinds"] = json!(["call-site-source"]);
                }),
            ),
        ] {
            assert!(!read.validate(&value).valid, "{case}");
            assert!(!oracle.is_valid(&value), "{case}");
        }
        // Which rank and which kinds an ID has is not the schema's to say.
        let swapped = changed(&|value| {
            value["rows"][0]["rank"] = json!("1");
            value["rows"][1]["rank"] = json!("0");
            value["rows"][2]["permittedEvidenceKinds"] = json!(["artifact-input"]);
        });
        assert!(read.validate(&swapped).valid);
        assert!(oracle.is_valid(&swapped));
    }

    #[test]
    fn the_registry_reads_back_as_its_type() {
        let registry = registry();
        assert_eq!(
            SafeOriginKindRegistry::read(&registry),
            Some(SafeOriginKindRegistry::revision_zero())
        );
        // Each of its objects is closed.
        for pointer in ["", "/rows/0"] {
            let mut unknown = registry.clone();
            unknown
                .pointer_mut(pointer)
                .and_then(Value::as_object_mut)
                .unwrap()
                .insert("note".to_owned(), json!("note"));
            assert_eq!(SafeOriginKindRegistry::read(&unknown), None, "{pointer}");
        }
    }

    #[test]
    fn a_repeated_id_or_rank_is_not_part_of_the_closed_graph() {
        let registry = registry();
        let mut id = registry.clone();
        id["rows"][1]["id"] = json!("configuration-file");
        let mut rank = registry.clone();
        rank["rows"][1]["rank"] = json!("0");
        for value in [id, rank] {
            assert_eq!(SafeOriginKindRegistry::read(&value), None, "{value}");
        }
        // A registry of fewer kinds, each once, still reads.
        let mut fewer = registry;
        fewer["rows"].as_array_mut().unwrap().truncate(3);
        assert!(SafeOriginKindRegistry::read(&fewer).is_some());
    }
}
