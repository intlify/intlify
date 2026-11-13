// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The checks every construction component starts with.
//!
//! A component of the Resolver Construction Input Set is one declared tuple
//! and the exact bytes it declares. Construction reads every artifact once,
//! before any check, and checks each component in the order of design 015's
//! construction-failure precedence: that it is present; then, as one row, that
//! its declared tuple is valid against the envelope schema the Root Package
//! lists for it, that its bytes are strict UTF-8 JSON, and what the
//! component's row adds, such as a container's envelope schema; then that it
//! declares the identity the Root Package fixes for its role, a revision the
//! root admits, and the file digest of its bytes; and, for the Capability
//! Specification, that its body is valid against its own envelope schema. A
//! check refuses with the precedence row it belongs to, and a component's
//! checks stop at the first refusal.
//!
//! Construction applies the construction-root bounds to the whole input before
//! any of these checks, so reading an artifact applies no capacity of its own:
//! a reader capacity below those bounds would refuse input the bounds admit,
//! at a later row and with another reason. Reading also scans the bytes
//! without recursion ([`scan`](super::scan)), for what they count in those
//! bounds' units.

use serde::Deserialize;
use serde_json::Value;

use intlify_shared_json::decode;
use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::json::decode_unique_json;
use intlify_shared_json::token::IntegrityDigest;

use super::admission::{ComponentAdmission, ConstructionBoundId, DRAFT_7};
use super::failure::{Check, Component, ConstructionFailure, PRECEDENCE};
use super::root::ConstructionRoot;
use super::scan::{self, JsonCounts};
use super::validator::schema::Schema;

/// One artifact of the input: its exact bytes, what they count when they are
/// strict UTF-8 JSON, and their value when the shared decoder reads them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Artifact {
    bytes: Vec<u8>,
    counts: Option<JsonCounts>,
    value: Option<Value>,
}

impl Artifact {
    /// Read an artifact's bytes, once.
    ///
    /// The scan and the shared decoder read the same strict UTF-8 JSON, but
    /// the decoder recurses and stops at its recursion limit: a strict
    /// artifact deeper than that has counts and no value, and the depth bound
    /// refuses it before any check reads a value.
    pub(crate) fn read(bytes: Vec<u8>) -> Self {
        let counts = scan::scan(&bytes);
        let value = strict_json(&bytes);
        Self {
            bytes,
            counts,
            value,
        }
    }

    /// The artifact's exact bytes.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// What the bytes count in the construction-root bounds' units, when
    /// they are strict UTF-8 JSON.
    pub(crate) const fn counts(&self) -> Option<JsonCounts> {
        self.counts
    }

    /// The bytes' value, when they are strict UTF-8 JSON.
    pub(crate) const fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }
}

/// Read bytes as strict UTF-8 JSON, as design 017 has every decoder read them:
/// malformed UTF-8, a byte-order mark, trailing data, a duplicate object
/// member, and an unpaired surrogate escape are refused, so every string holds
/// Unicode scalar values only. Numbers are read exactly; whether a value may
/// hold one is its schema's to say.
pub(crate) fn strict_json(bytes: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(bytes).ok()?;
    decode_unique_json(text).ok()
}

/// A component of the input: its declared tuple, and the artifact it declares.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Submitted {
    pub(crate) tuple: Value,
    pub(crate) artifact: Artifact,
}

/// What a valid declared tuple declares.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Declared {
    pub(crate) identity: String,
    pub(crate) revision: String,
    pub(crate) digest: String,
    /// The semantic digest, which only the Profile Specification's tuple
    /// declares.
    pub(crate) semantic_digest: Option<String>,
}

/// The precedence row a check refused at, and, at the construction-root
/// bounds' row, the first bound the input exceeds: a refusal's only detail
/// (D15:3100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refusal {
    row: usize,
    bound: Option<ConstructionBoundId>,
}

impl Refusal {
    /// The row of one component's check.
    ///
    /// # Panics
    ///
    /// Panics when no row has this component and check. Construction builds
    /// its rows as constants, so such a pair fails to compile.
    pub(crate) const fn at(component: Component, check: Check) -> Self {
        let mut index = 0;
        while index < PRECEDENCE.len() {
            let row = PRECEDENCE[index];
            if row.component as u8 == component as u8 && row.check as u8 == check as u8 {
                return Self {
                    row: index,
                    bound: None,
                };
            }
            index += 1;
        }
        panic!("no precedence row has this component and check");
    }

    /// The construction-root bounds' row, naming the first bound, in the
    /// canonical order, that the input exceeds.
    pub(crate) const fn exceeding(bound: ConstructionBoundId) -> Self {
        Self {
            bound: Some(bound),
            ..Self::at(Component::All, Check::ConstructionBounds)
        }
    }

    /// The row's number in design 015's table, counted from one.
    pub(crate) const fn number(self) -> usize {
        self.row + 1
    }

    /// The reason the row refuses with.
    pub(crate) const fn reason(self) -> ConstructionFailure {
        PRECEDENCE[self.row].reason
    }

    /// The bound a refusal at the construction-root bounds' row names; no
    /// other row has one.
    pub(crate) const fn bound(self) -> Option<ConstructionBoundId> {
        self.bound
    }
}

/// The rows of the checks every component starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rows {
    presence: Refusal,
    envelope: Refusal,
    identity: Refusal,
    revision: Refusal,
    file_digest: Refusal,
    body: Option<Refusal>,
}

impl Rows {
    /// One component's rows.
    pub(crate) const fn of(component: Component) -> Self {
        Self {
            presence: Refusal::at(component, Check::Presence),
            envelope: Refusal::at(component, Check::Envelope),
            identity: Refusal::at(component, Check::Identity),
            revision: Refusal::at(component, Check::Revision),
            file_digest: Refusal::at(component, Check::FileDigest),
            body: None,
        }
    }

    /// The same rows, with the artifact checked against its envelope schema
    /// in `row`, after the file digest, instead of in the envelope row: the
    /// Capability Specification's body has a row of its own.
    pub(crate) const fn with_body(self, row: Refusal) -> Self {
        Self {
            body: Some(row),
            ..self
        }
    }
}

/// A component that passed the checks every component starts with.
#[derive(Debug)]
pub(crate) struct Checked<'a, T> {
    /// What its tuple declares.
    pub(crate) declared: Declared,
    /// Its artifact's value.
    pub(crate) value: &'a Value,
    /// What the rest of its envelope row read from the value.
    pub(crate) read: T,
}

/// Check that a component is present, its envelope row, its identity, its
/// revision, and its file digest, in that order, and then, when its rows give
/// its body a row of its own, its body.
///
/// The root lists the envelope schemas a component is checked against: the
/// first for its declared tuple and, for a container or the Capability
/// Specification, a second for its artifact. A container's artifact is part
/// of its envelope row; the Capability Specification's body is checked in its
/// own row, after the file digest, since design 015 checks each authority's
/// envelope, identity, revision, and digest before its body. `rest` is the
/// rest of the envelope row: it reads what the component needs from the
/// artifact's value, or returns `None` to refuse.
pub(crate) fn check<'a, T>(
    root: &ConstructionRoot,
    rows: Rows,
    admission: &ComponentAdmission,
    submitted: Option<&'a Submitted>,
    rest: impl FnOnce(&'a Value) -> Option<T>,
) -> Result<Checked<'a, T>, Refusal> {
    let submitted = submitted.ok_or(rows.presence)?;
    let mut schemas = admission
        .envelope_schemas
        .iter()
        .map(|&envelope| root.envelope(envelope));
    let declared = schemas
        .next()
        .filter(|schema| schema.validate(&submitted.tuple).valid)
        .and_then(|_| decode::typed::<Declared>(submitted.tuple.clone()).ok())
        .ok_or(rows.envelope)?;
    let value = submitted.artifact.value().ok_or(rows.envelope)?;
    let artifact_schema = schemas.next();
    let refused = || artifact_schema.is_some_and(|schema| !schema.validate(value).valid);
    if rows.body.is_none() && refused() {
        return Err(rows.envelope);
    }
    let read = rest(value).ok_or(rows.envelope)?;
    if declared.identity != admission.identity {
        return Err(rows.identity);
    }
    if !admission.revisions.contains(&declared.revision) {
        return Err(rows.revision);
    }
    if file_digest(submitted.artifact.bytes()).as_str() != declared.digest {
        return Err(rows.file_digest);
    }
    if let Some(body) = rows.body.filter(|_| refused()) {
        return Err(body);
    }
    Ok(Checked {
        declared,
        value,
        read,
    })
}

/// Whether a schema document declares Draft 7: its root is an object whose
/// `$schema` is Draft 7's URI itself.
pub(crate) fn declares_draft_7(document: &Value) -> bool {
    document.get("$schema").and_then(Value::as_str) == Some(DRAFT_7)
}

/// Read a schema document construction admits: valid against the Draft 7
/// meta-schema, and read by the validator construction validates with.
///
/// The meta-schema admits schemas the validator cannot read: one that names
/// another document, nests a `$schema`, applies itself in place in a cycle,
/// or holds a pattern outside the validator's subset. Nothing could be
/// validated against such a schema, so construction refuses it with the
/// meta-schema's row.
pub(crate) fn draft_7_schema(root: &ConstructionRoot, document: &Value) -> Option<Schema> {
    if !root.meta_schema().validate(document).valid {
        return None;
    }
    Schema::compile(document).ok()
}

/// The file digest of exact bytes: their SHA-256, as `sha256:` and 64
/// lowercase hexadecimal digits.
pub(crate) fn file_digest(bytes: &[u8]) -> IntegrityDigest {
    IntegrityDigest::from_hash(digest_bytes(bytes))
}

/// Helpers for the components' tests.
#[cfg(test)]
pub(crate) mod testing {
    use serde_json::json;

    use super::{file_digest, Artifact, Refusal, Submitted};

    /// A component that declares `identity`, `revision`, and the file digest
    /// of `bytes`.
    pub(crate) fn submitted(identity: &str, revision: &str, bytes: &[u8]) -> Submitted {
        Submitted {
            tuple: json!({
                "identity": identity,
                "revision": revision,
                "digest": file_digest(bytes).as_str()
            }),
            artifact: Artifact::read(bytes.to_vec()),
        }
    }

    /// The number of the row a check refused at, or `None` when it passed.
    pub(crate) fn row<T>(result: Result<T, Refusal>) -> Option<usize> {
        result.err().map(Refusal::number)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::testing::{row, submitted};
    use super::*;

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    #[test]
    fn strict_json_reads_017s_json_with_its_numbers() {
        for (text, value) in [
            ("{}", json!({})),
            (" [ ]\n", json!([])),
            (
                r#"{"a":[0,-0.0025,true,null]}"#,
                json!({ "a": [0, -0.0025, true, null] }),
            ),
            (r#""\u00e9\uD83D\uDE00""#, json!("\u{e9}\u{1F600}")),
            (r#"{"b":1,"a":2}"#, json!({ "b": 1, "a": 2 })),
        ] {
            assert_eq!(strict_json(text.as_bytes()), Some(value), "{text}");
        }
        // Numbers keep their digits, beyond what a float holds; only an
        // exponent's sign is spelled out.
        let value = strict_json(b"[1e400, 0.10, -2.5e-3]").unwrap();
        assert_eq!(value[0].to_string(), "1e+400");
        assert_eq!(value[1].to_string(), "0.10");
        assert_eq!(value[2].to_string(), "-2.5e-3");
        // Member order is read as written.
        let keys = strict_json(br#"{"b":1,"a":2}"#).unwrap();
        assert_eq!(
            keys.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["b", "a"]
        );
    }

    #[test]
    fn strict_json_refuses_what_017_has_every_decoder_refuse() {
        for bytes in [
            b"\xff".as_slice(),
            b"\"\xc0\xaf\"",
            b"\"\xed\xa0\x80\"",
            b"\xef\xbb\xbf{}",
            b"{} {}",
            b"{}x",
            br#"{"a":1,"a":1}"#,
            br#"{"a":{"b":1,"b":2}}"#,
            br#""\uD800""#,
            br#""\uDC00""#,
            br#""\uD800\u0041""#,
            br#""\uD800A""#,
            br#""\uDBFFx""#,
            b"",
            b"{",
            b"[1,]",
            b"NaN",
            b"/*c*/{}",
            b"{'a':1}",
            b"\"\t\"",
        ] {
            assert_eq!(
                strict_json(bytes),
                None,
                "{}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn strict_json_applies_no_reader_capacity_of_its_own() {
        // A number longer than the shared reader's atom capacity: the
        // construction-root bounds count no number's length, so construction
        // must read it.
        let digits = format!("[1{}]", "0".repeat(1024 * 1024));
        assert_eq!(
            intlify_shared_json::decode::value(digits.as_bytes(), false),
            Err(intlify_shared_json::decode::DecodeFailure::Capacity)
        );
        assert!(strict_json(digits.as_bytes()).is_some());
        // As deep as `input.maxDepth` allows: 64 levels, the root at one.
        let deep = format!("{}0{}", "[".repeat(63), "]".repeat(63));
        assert!(strict_json(deep.as_bytes()).is_some());
    }

    #[test]
    fn an_artifact_keeps_its_bytes_and_their_value() {
        let artifact = Artifact::read(b" {\"a\": \"b\"} ".to_vec());
        assert_eq!(artifact.bytes(), b" {\"a\": \"b\"} ");
        assert_eq!(artifact.value(), Some(&json!({ "a": "b" })));
        assert_eq!(
            artifact.counts(),
            Some(JsonCounts {
                depth: 2,
                nodes: 2,
                entries: 1,
                string_bytes: 2,
                longest_string: 1,
            })
        );
        let unread = Artifact::read(b"{".to_vec());
        assert_eq!(unread.bytes(), b"{");
        assert_eq!(unread.value(), None);
        assert_eq!(unread.counts(), None);
    }

    /// A strict artifact deeper than the shared decoder's recursion limit
    /// keeps its counts, for the depth bound, and no value.
    #[test]
    fn an_artifact_too_deep_for_the_decoder_still_counts() {
        let levels = 200;
        let deep = format!("{}{}", "[".repeat(levels), "]".repeat(levels));
        let artifact = Artifact::read(deep.into_bytes());
        assert_eq!(artifact.value(), None);
        assert_eq!(artifact.counts().map(|counts| counts.depth), Some(200));
    }

    #[test]
    fn every_row_is_the_only_one_of_its_component_and_check() {
        for (index, precedence) in PRECEDENCE.iter().enumerate() {
            let refusal = Refusal::at(precedence.component, precedence.check);
            assert_eq!(refusal.number(), index + 1, "{precedence:?}");
            assert_eq!(refusal.reason(), precedence.reason);
        }
    }

    #[test]
    fn every_component_starts_with_its_own_five_rows() {
        let numbers = |rows: Rows| {
            [
                rows.presence,
                rows.envelope,
                rows.identity,
                rows.revision,
                rows.file_digest,
            ]
            .map(Refusal::number)
        };
        for (component, expected) in [
            (Component::ProfileSpecificationSchema, [2, 3, 4, 5, 6]),
            (Component::ProfileSpecification, [8, 9, 10, 11, 12]),
            (
                Component::ConfigurationSchemaAuthoritySet,
                [15, 16, 17, 18, 19],
            ),
            (
                Component::ResolverCapabilitySpecification,
                [25, 26, 27, 28, 29],
            ),
            (Component::RegistryPackage, [31, 32, 33, 34, 35]),
        ] {
            assert_eq!(numbers(Rows::of(component)), expected, "{component:?}");
            assert_eq!(Rows::of(component).body, None, "{component:?}");
        }
        // Giving the body a row of its own keeps the other five.
        let capability = Rows::of(Component::ResolverCapabilitySpecification);
        let body = Refusal::at(
            Component::ResolverCapabilitySpecification,
            Check::BodySchema,
        );
        let with_body = capability.with_body(body);
        assert_eq!(numbers(with_body), numbers(capability));
        assert_eq!(with_body.body.map(Refusal::number), Some(30));
    }

    /// Check a component under the Profile Specification Schema's rows, with
    /// the rest of the envelope row reading the artifact's value.
    fn check_schema_component(submitted: Option<&Submitted>) -> Result<Value, Refusal> {
        let admission = &root().admission().components.profile_specification_schema;
        check(
            root(),
            Rows::of(Component::ProfileSpecificationSchema),
            admission,
            submitted,
            |value| value.get("keep").cloned(),
        )
        .map(|checked| {
            assert_eq!(checked.value["keep"], checked.read);
            checked.read
        })
    }

    const SCHEMA_IDENTITY: &str = "intlify-project-profile-specification-schema";

    #[test]
    fn a_component_passes_with_what_its_tuple_declares() {
        let bytes = br#"{"keep": "this"}"#;
        let submitted = submitted(SCHEMA_IDENTITY, "0", bytes);
        let admission = &root().admission().components.profile_specification_schema;
        let checked = check(
            root(),
            Rows::of(Component::ProfileSpecificationSchema),
            admission,
            Some(&submitted),
            |_| Some(()),
        )
        .unwrap();
        assert_eq!(
            checked.declared,
            Declared {
                identity: SCHEMA_IDENTITY.to_owned(),
                revision: "0".to_owned(),
                digest: file_digest(bytes).as_str().to_owned(),
                semantic_digest: None,
            }
        );
        assert_eq!(checked.value, &json!({ "keep": "this" }));
        assert_eq!(check_schema_component(Some(&submitted)), Ok(json!("this")));
    }

    #[test]
    fn each_check_refuses_with_its_row() {
        let bytes = br#"{"keep": "this"}"#;
        let valid = submitted(SCHEMA_IDENTITY, "0", bytes);
        assert_eq!(row(check_schema_component(Some(&valid))), None);
        assert_eq!(row(check_schema_component(None)), Some(2));
        let digest = valid.tuple["digest"].clone();
        let mut cases = Vec::new();
        // Tuples the envelope schema refuses.
        for tuple in [
            json!(null),
            json!({ "identity": SCHEMA_IDENTITY, "revision": "0" }),
            json!({ "identity": "Upper", "revision": "0", "digest": digest }),
            json!({ "identity": SCHEMA_IDENTITY, "revision": "0", "digest": "sha256:00" }),
            json!({
                "identity": SCHEMA_IDENTITY,
                "revision": "0",
                "digest": digest,
                "semanticDigest": digest
            }),
        ] {
            cases.push((
                Submitted {
                    tuple,
                    ..valid.clone()
                },
                3,
            ));
        }
        // Bytes that are not strict UTF-8 JSON, and a value the rest of the
        // row refuses, each declared with its own digest.
        cases.push((submitted(SCHEMA_IDENTITY, "0", br#"{"keep": "this",}"#), 3));
        cases.push((submitted(SCHEMA_IDENTITY, "0", br#"{"other": "this"}"#), 3));
        cases.push((submitted("intlify-project-profile", "0", bytes), 4));
        cases.push((submitted(SCHEMA_IDENTITY, "1", bytes), 5));
        // The same value in other bytes is another file.
        let mut reformatted = valid.clone();
        reformatted.artifact = Artifact::read(br#"{"keep":"this"}"#.to_vec());
        cases.push((reformatted, 6));
        for (submitted, expected) in cases {
            assert_eq!(
                row(check_schema_component(Some(&submitted))),
                Some(expected),
                "{submitted:?}"
            );
        }
    }

    #[test]
    fn the_first_failing_row_is_the_one_reported() {
        let bytes = br#"{"keep": "this"}"#;
        // Every check fails at first; fixing them one at a time moves the
        // refusal to the next row.
        let mut submitted = submitted("another", "1", b"{}");
        submitted.artifact = Artifact::read(b"{".to_vec());
        assert_eq!(row(check_schema_component(Some(&submitted))), Some(3));
        submitted.artifact = Artifact::read(bytes.to_vec());
        assert_eq!(row(check_schema_component(Some(&submitted))), Some(4));
        submitted.tuple["identity"] = json!(SCHEMA_IDENTITY);
        assert_eq!(row(check_schema_component(Some(&submitted))), Some(5));
        submitted.tuple["revision"] = json!("0");
        assert_eq!(row(check_schema_component(Some(&submitted))), Some(6));
        submitted.tuple["digest"] = json!(file_digest(bytes).as_str());
        assert_eq!(row(check_schema_component(Some(&submitted))), None);
    }

    #[test]
    fn a_container_is_checked_against_its_second_envelope_schema() {
        let identity = "intlify-configuration-schema-authority-set";
        let admission = &root()
            .admission()
            .components
            .configuration_schema_authority_set;
        let check_set = |submitted: &Submitted| {
            row(check(
                root(),
                Rows::of(Component::ConfigurationSchemaAuthoritySet),
                admission,
                Some(submitted),
                |_| Some(()),
            ))
        };
        let digest = format!("sha256:{}", "0".repeat(64));
        let set = json!({
            "profileSpecification": {
                "identity": "intlify-project-profile",
                "revision": "0",
                "semanticDigest": digest
            },
            "members": {
                "0": {
                    "identity": "intlify-configuration-schema",
                    "revision": "0",
                    "dialect": DRAFT_7,
                    "digest": digest
                }
            }
        });
        let bytes = serde_json::to_vec(&set).unwrap();
        assert_eq!(check_set(&submitted(identity, "0", &bytes)), None);
        assert_eq!(
            check_set(&submitted(identity, "0", br#"{"members": {}}"#)),
            Some(16)
        );
    }

    #[test]
    fn a_body_with_a_row_of_its_own_is_checked_after_the_file_digest() {
        let identity = "intlify-project-profile-resolver-capability";
        let component = Component::ResolverCapabilitySpecification;
        let admission = &root()
            .admission()
            .components
            .resolver_capability_specification;
        let check_with = |rows: Rows, submitted: &Submitted| {
            row(check(
                root(),
                rows,
                admission,
                Some(submitted),
                |_| Some(()),
            ))
        };
        let own_row = Rows::of(component).with_body(Refusal::at(component, Check::BodySchema));
        let body = serde_json::to_vec(
            &crate::construction::capability::CapabilitySpecification::revision_zero(),
        )
        .unwrap();
        assert_eq!(check_with(own_row, &submitted(identity, "0", &body)), None);
        // A body its envelope schema refuses is refused in its own row, after
        // the identity, revision, and file digest pass.
        let refused = br#"{"policyComparable": [], "bootstrapOnly": []}"#;
        assert_eq!(
            check_with(own_row, &submitted(identity, "0", refused)),
            Some(30)
        );
        assert_eq!(
            check_with(own_row, &submitted("another", "0", refused)),
            Some(27)
        );
        let mut wrong_digest = submitted(identity, "0", refused);
        wrong_digest.artifact = Artifact::read(b"{}".to_vec());
        assert_eq!(check_with(own_row, &wrong_digest), Some(29));
        // Without a row of its own, the same schema is part of the envelope
        // row.
        assert_eq!(
            check_with(Rows::of(component), &submitted("another", "0", refused)),
            Some(26)
        );
    }

    #[test]
    fn a_schema_declares_draft_7_by_its_root_s_own_uri() {
        assert!(declares_draft_7(&json!({ "$schema": DRAFT_7 })));
        for document in [
            json!({}),
            json!(true),
            json!([{ "$schema": DRAFT_7 }]),
            json!({ "$schema": "http://json-schema.org/draft-07/schema" }),
            json!({ "$schema": "http://json-schema.org/draft-06/schema#" }),
            json!({ "$schema": ["http://json-schema.org/draft-07/schema#"] }),
            json!({ "definitions": { "a": { "$schema": DRAFT_7 } } }),
        ] {
            assert!(!declares_draft_7(&document), "{document}");
        }
    }

    #[test]
    fn an_admitted_schema_is_one_the_meta_schema_admits_and_the_validator_reads() {
        for document in [
            json!(true),
            json!({ "$schema": DRAFT_7, "type": "object", "required": ["a"] }),
        ] {
            assert!(draft_7_schema(root(), &document).is_some(), "{document}");
        }
        // The meta-schema refuses these, though the validator reads them.
        for document in [
            json!({ "$schema": DRAFT_7, "title": 1 }),
            json!({ "$schema": DRAFT_7, "$comment": ["a"] }),
        ] {
            assert!(Schema::compile(&document).is_ok(), "{document}");
            assert!(draft_7_schema(root(), &document).is_none(), "{document}");
        }
        // The validator refuses these, though the meta-schema admits them.
        for document in [
            json!({ "$schema": DRAFT_7, "$ref": "other.json" }),
            json!({ "$schema": DRAFT_7, "properties": { "a": { "$schema": DRAFT_7 } } }),
            json!({ "$schema": DRAFT_7, "allOf": [{ "$ref": "#" }] }),
            json!({ "$schema": DRAFT_7, "pattern": "(?<name>a)" }),
        ] {
            assert!(root().meta_schema().validate(&document).valid, "{document}");
            assert!(draft_7_schema(root(), &document).is_none(), "{document}");
        }
    }

    #[test]
    fn a_file_digest_is_the_sha_256_of_the_exact_bytes() {
        assert_eq!(
            file_digest(b"").as_str(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_ne!(file_digest(b"{}"), file_digest(b"{ }"));
    }
}
