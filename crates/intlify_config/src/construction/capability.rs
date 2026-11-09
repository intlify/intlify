// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Project Profile Resolver Capability Specification.
//!
//! Design 015's Capability Specification is the construction authority that
//! closes the resolver's capability bounds. Its body lists every
//! `policyComparable` bound, the mirror of a Resource Limit Policy bound, with
//! the unit it counts in, and every `bootstrapOnly` bound with its unit and its
//! revision-`"0"` minimum as a shortest decimal string; both lists are in the
//! canonical order. A bound is listed by its path inside its namespace,
//! the form the Profile Specification lists its resource bounds in, and its
//! unit is its cell of 015's tables, word for word. A `policyComparable`
//! bound's minimum is one for every bound, so the body does not list it.
//! Revision `"0"` writes the body from this crate's capability inventory.
//!
//! The body's envelope schema closes its values: each list is exactly its
//! bounds in order, and each bound exactly its path, unit, and minimum. The
//! schema is written from the same inventory, and the Root Package holds it
//! in its authority-envelope schema package.
//!
//! Construction checks the Capability Specification it is given, rows 25 to 30
//! of the construction-failure precedence: the checks every component starts
//! with and then, after the file digest, the body against its envelope schema.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use intlify_shared_json::decode;

use crate::resolver::capability::CapabilityBound;

use super::component::{self, Declared, Refusal, Rows, Submitted};
use super::failure::{Check, Component};
use super::root::ConstructionRoot;

/// The Capability Specification's identity.
pub(crate) const IDENTITY: &str = "intlify-project-profile-resolver-capability";

/// The Capability Specification's revision.
pub(crate) const REVISION: &str = "0";

/// The Capability Specification's path below the conformance suite's root.
pub(crate) const PATH: &str = "authorities/resolver-capability-v0.json";

/// The Capability Specification's file digest, which the resolver is compiled
/// from.
pub(crate) const DIGEST: &str =
    "sha256:7ffcd14a9197e777182f4c67c971d864a3157e7b8d112cdbb000cead88a9bc6e";

/// One `policyComparable` bound: its path and the unit it counts in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PolicyComparableBound {
    pub(crate) path: String,
    pub(crate) unit: String,
}

/// One `bootstrapOnly` bound: its path, the unit it counts in, and its
/// minimum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BootstrapOnlyBound {
    pub(crate) path: String,
    pub(crate) unit: String,
    pub(crate) minimum: String,
}

/// The Capability Specification's body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapabilitySpecification {
    pub(crate) policy_comparable: Vec<PolicyComparableBound>,
    pub(crate) bootstrap_only: Vec<BootstrapOnlyBound>,
}

impl CapabilitySpecification {
    /// The revision-`"0"` body: every bound of this crate's capability
    /// inventory, in its canonical order.
    pub(crate) fn revision_zero() -> Self {
        let mut policy_comparable = Vec::new();
        let mut bootstrap_only = Vec::new();
        for (bound, minimum) in CapabilityBound::inventory() {
            let path = bound.path();
            let unit = bound.unit().to_owned();
            match bound {
                CapabilityBound::PolicyComparable(_) => {
                    policy_comparable.push(PolicyComparableBound { path, unit });
                }
                CapabilityBound::BootstrapOnly(_) => bootstrap_only.push(BootstrapOnlyBound {
                    path,
                    unit,
                    minimum: minimum.get().to_string(),
                }),
            }
        }
        Self {
            policy_comparable,
            bootstrap_only,
        }
    }
}

/// A list of exactly these bounds, in this order.
fn list<T: Serialize>(bounds: &[T]) -> Value {
    let items = bounds
        .iter()
        .map(|bound| json!({ "const": bound }))
        .collect::<Vec<_>>();
    json!({
        "type": "array",
        "minItems": items.len(),
        "additionalItems": false,
        "items": items,
    })
}

/// The body's envelope schema, without its `$schema`: a closed object whose
/// two lists are exactly the revision-`"0"` body's bounds, in order, each
/// bound exactly its path, unit, and minimum.
pub(crate) fn body_schema() -> Value {
    let body = CapabilitySpecification::revision_zero();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["policyComparable", "bootstrapOnly"],
        "properties": {
            "policyComparable": list(&body.policy_comparable),
            "bootstrapOnly": list(&body.bootstrap_only),
        },
    })
}

const BODY_SCHEMA_ROW: Refusal = Refusal::at(
    Component::ResolverCapabilitySpecification,
    Check::BodySchema,
);
const ROWS: Rows = Rows::of(Component::ResolverCapabilitySpecification).with_body(BODY_SCHEMA_ROW);

/// A Capability Specification construction admitted.
#[derive(Debug)]
pub(crate) struct AdmittedCapabilitySpecification {
    /// What its tuple declares.
    pub(crate) declared: Declared,
    /// Its body.
    pub(crate) body: CapabilitySpecification,
}

/// Check the Capability Specification construction is given: rows 25 to 30
/// of the construction-failure precedence.
///
/// Its envelope row checks only the declared tuple and that the bytes are
/// strict UTF-8 JSON; the body is checked against its envelope schema after
/// the file digest, in row 30.
pub(crate) fn admit(
    root: &ConstructionRoot,
    submitted: Option<&Submitted>,
) -> Result<AdmittedCapabilitySpecification, Refusal> {
    let admission = &root
        .admission()
        .components
        .resolver_capability_specification;
    let checked = component::check(root, ROWS, admission, submitted, |_| Some(()))?;
    // The schema closes every value, so a body it admits reads as the type.
    let body = decode::typed::<CapabilitySpecification>(checked.value.clone())
        .map_err(|_| BODY_SCHEMA_ROW)?;
    Ok(AdmittedCapabilitySpecification {
        declared: checked.declared,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::admission::{ConstructionAdmission, EnvelopeSchema, DRAFT_7};
    use crate::construction::component::testing::{row, submitted};
    use crate::construction::component::Artifact;
    use crate::construction::envelope;
    use crate::construction::validator::schema::Schema;

    /// The committed Capability Specification.
    const COMMITTED: &[u8] =
        include_bytes!("../../conformance/authorities/resolver-capability-v0.json");

    fn body() -> Value {
        serde_json::to_value(CapabilitySpecification::revision_zero()).unwrap()
    }

    #[test]
    fn the_body_lists_every_bound_in_the_canonical_order() {
        let body = CapabilitySpecification::revision_zero();
        assert_eq!(body.policy_comparable.len(), 52);
        assert_eq!(body.bootstrap_only.len(), 54);
        let paths = body
            .policy_comparable
            .iter()
            .map(|bound| bound.path.as_str())
            .chain(body.bootstrap_only.iter().map(|bound| bound.path.as_str()))
            .collect::<Vec<_>>();
        let inventory = CapabilityBound::inventory();
        assert_eq!(
            paths,
            inventory
                .iter()
                .map(|(bound, _)| bound.path())
                .collect::<Vec<_>>()
        );
        assert_eq!(paths[0], "configurationInput.maxNodes");
        assert_eq!(paths[51], "diagnostics.shared.maxTotalCanonicalBytes");
        assert_eq!(paths[52], "rawInput.maxFileBytes");
        assert_eq!(paths[105], "mandatoryReserve.maxCanonicalBytes");
        // Each bound counts in its own unit, and each `bootstrapOnly` bound
        // has its own minimum.
        for (bound, (id, _)) in body.policy_comparable.iter().zip(&inventory) {
            assert_eq!(bound.unit, id.unit(), "{id}");
        }
        for (bound, (id, minimum)) in body.bootstrap_only.iter().zip(&inventory[52..]) {
            assert_eq!(bound.unit, id.unit(), "{id}");
            assert_eq!(bound.minimum, minimum.get().to_string(), "{id}");
        }
        let larger = body
            .bootstrap_only
            .iter()
            .filter(|bound| bound.minimum != "1")
            .map(|bound| (bound.path.as_str(), bound.minimum.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            larger,
            [
                ("snapshot.maxEnvelopeCanonicalBytes", "4096"),
                ("snapshot.maxSingleMarkerCanonicalBytes", "1024"),
                ("mandatoryReserve.maxCanonicalBytes", "65536"),
            ]
        );
    }

    #[test]
    fn the_body_is_written_as_015_lists_it() {
        let body = body();
        assert_eq!(
            body.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["policyComparable", "bootstrapOnly"]
        );
        assert_eq!(
            serde_json::to_string(&body["policyComparable"][0]).unwrap(),
            r#"{"path":"configurationInput.maxNodes","unit":"One logical JSON value; the root counts as one and object keys are not nodes"}"#
        );
        assert_eq!(
            serde_json::to_string(&body["bootstrapOnly"][8]).unwrap(),
            r#"{"path":"snapshot.maxEnvelopeCanonicalBytes","unit":"One canonical byte in the complete formal Snapshot envelope","minimum":"4096"}"#
        );
    }

    #[test]
    fn the_body_reads_back_as_its_type() {
        let body = body();
        let read: CapabilitySpecification = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(read, CapabilitySpecification::revision_zero());
        // Each of its objects is closed.
        for pointer in ["", "/policyComparable/0", "/bootstrapOnly/0"] {
            let mut unknown = body.clone();
            unknown
                .pointer_mut(pointer)
                .and_then(Value::as_object_mut)
                .unwrap()
                .insert("note".to_owned(), json!("note"));
            assert!(
                serde_json::from_value::<CapabilitySpecification>(unknown).is_err(),
                "{pointer}"
            );
        }
    }

    #[test]
    fn the_body_schema_closes_the_body_s_values() {
        let document = envelope::schema(EnvelopeSchema::ResolverCapabilitySpecification);
        assert_eq!(document["$schema"], DRAFT_7);
        let schema = Schema::compile(&document).unwrap();
        let oracle = jsonschema::draft7::new(&document).unwrap();
        let body = body();
        assert!(schema.validate(&body).valid);
        assert!(oracle.is_valid(&body));
        let changed = |change: &dyn Fn(&mut Value)| {
            let mut value = body.clone();
            change(&mut value);
            value
        };
        for (case, value) in [
            (
                "another unit",
                changed(&|body| body["policyComparable"][3]["unit"] = json!("One byte")),
            ),
            (
                "another group",
                changed(&|body| {
                    body["policyComparable"][0]["path"] = json!("localeResolution.maxNodes");
                }),
            ),
            (
                "a lower minimum",
                changed(&|body| body["bootstrapOnly"][8]["minimum"] = json!("4095")),
            ),
            (
                "a higher minimum",
                changed(&|body| body["bootstrapOnly"][0]["minimum"] = json!("2")),
            ),
            (
                "a minimum as a number",
                changed(&|body| body["bootstrapOnly"][0]["minimum"] = json!(1)),
            ),
            (
                "a policyComparable minimum",
                changed(&|body| body["policyComparable"][0]["minimum"] = json!("1")),
            ),
            (
                "two bounds swapped",
                changed(&|body| body["bootstrapOnly"].as_array_mut().unwrap().swap(2, 3)),
            ),
            (
                "a bound missing",
                changed(&|body| {
                    body["policyComparable"].as_array_mut().unwrap().pop();
                }),
            ),
            (
                "a bound repeated",
                changed(&|body| {
                    let last = body["bootstrapOnly"][53].clone();
                    body["bootstrapOnly"].as_array_mut().unwrap().push(last);
                }),
            ),
            (
                "a namespace missing",
                changed(&|body| {
                    body.as_object_mut().unwrap().remove("bootstrapOnly");
                }),
            ),
            (
                "another member",
                changed(&|body| body["note"] = json!("note")),
            ),
            (
                "a list as an object",
                changed(&|body| body["policyComparable"] = json!({})),
            ),
        ] {
            assert!(!schema.validate(&value).valid, "{case}");
            assert!(!oracle.is_valid(&value), "{case}");
        }
    }

    #[test]
    fn the_specification_names_what_construction_admits() {
        let admission = ConstructionAdmission::revision_zero();
        let component = &admission.components.resolver_capability_specification;
        assert_eq!(component.identity, IDENTITY);
        assert_eq!(component.revisions, [REVISION]);
        assert_eq!(
            component.envelope_schemas,
            [
                EnvelopeSchema::ComponentTuple,
                EnvelopeSchema::ResolverCapabilitySpecification
            ]
        );
    }

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    /// The bytes of the body after `change`.
    fn changed_bytes(change: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value = body();
        change(&mut value);
        serde_json::to_vec_pretty(&value).unwrap()
    }

    #[test]
    fn construction_admits_the_committed_capability_specification() {
        let submitted = submitted(IDENTITY, REVISION, COMMITTED);
        let admitted = admit(root(), Some(&submitted)).unwrap();
        assert_eq!(admitted.body, CapabilitySpecification::revision_zero());
        assert_eq!(admitted.declared.identity, IDENTITY);
        assert_eq!(
            admitted.declared.digest,
            component::file_digest(COMMITTED).as_str()
        );
    }

    #[test]
    fn each_check_refuses_with_its_row() {
        let valid = submitted(IDENTITY, REVISION, COMMITTED);
        assert_eq!(row(admit(root(), Some(&valid))), None);
        assert_eq!(row(admit(root(), None)), Some(25));
        let mut tuple = valid.clone();
        tuple.tuple["semanticDigest"] = tuple.tuple["digest"].clone();
        let unit = changed_bytes(|body| body["bootstrapOnly"][0]["unit"] = json!("One byte"));
        for (submitted, expected) in [
            (tuple, 26),
            (
                submitted(IDENTITY, REVISION, b"{\"policyComparable\": []"),
                26,
            ),
            (
                submitted("intlify-project-profile", REVISION, COMMITTED),
                27,
            ),
            (submitted(IDENTITY, "1", COMMITTED), 28),
            (
                Submitted {
                    artifact: Artifact::read(unit.clone()),
                    ..valid.clone()
                },
                29,
            ),
            (submitted(IDENTITY, REVISION, &unit), 30),
            (submitted(IDENTITY, REVISION, b"{}"), 30),
            (submitted(IDENTITY, REVISION, b"[]"), 30),
        ] {
            assert_eq!(
                row(admit(root(), Some(&submitted))),
                Some(expected),
                "{submitted:?}"
            );
        }
    }

    #[test]
    fn the_body_is_checked_after_the_file_digest() {
        // A body its schema refuses, under every other check failing at first:
        // fixing them one at a time moves the refusal down to the body's row.
        let body = changed_bytes(|body| {
            body["policyComparable"].as_array_mut().unwrap().pop();
        });
        let mut submitted = submitted("another", "1", b"{}");
        submitted.artifact = Artifact::read(body.clone());
        assert_eq!(row(admit(root(), Some(&submitted))), Some(27));
        submitted.tuple["identity"] = json!(IDENTITY);
        assert_eq!(row(admit(root(), Some(&submitted))), Some(28));
        submitted.tuple["revision"] = json!(REVISION);
        assert_eq!(row(admit(root(), Some(&submitted))), Some(29));
        submitted.tuple["digest"] = json!(component::file_digest(&body).as_str());
        assert_eq!(row(admit(root(), Some(&submitted))), Some(30));
        submitted.artifact = Artifact::read(COMMITTED.to_vec());
        submitted.tuple["digest"] = json!(component::file_digest(COMMITTED).as_str());
        assert_eq!(row(admit(root(), Some(&submitted))), None);
    }

    #[test]
    fn the_body_s_form_is_not_its_bytes() {
        // The same body in other bytes passes every row once its tuple declares
        // those bytes: the file digest is the tuple's, and row 43 compares it
        // with the digest the resolver was compiled from.
        let compact = serde_json::to_vec(&body()).unwrap();
        assert_ne!(compact, COMMITTED);
        let admitted = admit(root(), Some(&submitted(IDENTITY, REVISION, &compact))).unwrap();
        assert_eq!(admitted.body, CapabilitySpecification::revision_zero());
    }
}
