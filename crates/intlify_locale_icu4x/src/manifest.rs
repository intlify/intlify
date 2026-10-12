// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's Locale Canonicalization Data Artifact manifest, and the
//! pre-invocation boundary that reads it.
//!
//! The manifest is small. It names the specification and the dataset, records
//! where the data came from, and points at the two payloads by address, byte
//! length, and file digest. Its integrity digest covers everything but itself.
//!
//! Decoding it is design 015's `locale-canonicalization-data-artifact`
//! boundary: the manifest's own byte and depth limits (017 step 1) and its
//! strict, closed decoding (step 2). A failure here has one of 015's closed
//! reasons, and nothing about the data has been looked at yet.

use intlify_shared_json::encoding::{hash_with_excluded_member, Domain, EncodingFailure};
use intlify_shared_json::json::decode_unique_json;
use intlify_shared_json::quantity::Quantity;
use intlify_shared_json::token::{IntegrityDigest, Token, VersionedIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::primitives::{FileDigest, LogicalAddress, NonemptyText, SemanticDigest};

/// The one manifest kind this reader implements.
pub const ARTIFACT_KIND: &str = "locale-canonicalization-data";

/// The one manifest schema revision this reader implements.
pub const ARTIFACT_SCHEMA_REVISION: &str = "0";

/// The digest domain 017 registers for the manifest's integrity.
pub const INTEGRITY_DOMAIN: &str = "locale-canonicalization-data-integrity";

// Single-value types: each serializes as one fixed spelling and refuses every
// other, which is what makes the committed schema as strict as the reader.
// They carry no doc comments, which would become schema descriptions.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ArtifactKind {
    #[serde(rename = "locale-canonicalization-data")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RevisionZero {
    #[serde(rename = "0")]
    Value,
}

/// The manifest of one Locale Canonicalization Data Artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocaleCanonicalizationDataArtifact {
    kind: ArtifactKind,
    schema_revision: RevisionZero,
    /// The Locale Canonicalization Specification this data realizes.
    specification: VersionedIdentity,
    dataset: DatasetDescriptor,
    /// Where the data came from. Integrity covers it; the semantic digest
    /// does not.
    generation: Generation,
    /// Exactly one `conformance` and one `markers` entry, sorted by role.
    payloads: Vec<Payload>,
    integrity_digest: IntegrityDigest,
}

/// The logical dataset the payloads realize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetDescriptor {
    identity: Token,
    schema: VersionedIdentity,
    semantic_digest: SemanticDigest,
}

/// The provenance design 015 requires the manifest to carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Generation {
    marker_data: MarkerData,
    cldr: Cldr,
    /// The marker set, sorted.
    markers: Vec<NonemptyText>,
}

/// The upstream marker-data package and the checksum of its published package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkerData {
    package: Token,
    version: Token,
    checksum: FileDigest,
}

/// The CLDR revision and every source file read during generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cldr {
    revision: Token,
    /// Addressed relative to the CLDR distribution root, sorted by address
    /// with unique addresses.
    sources: Vec<CldrSource>,
}

/// One CLDR source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CldrSource {
    address: LogicalAddress,
    digest: FileDigest,
}

/// One payload: its role, its format, and the transport integrity of its
/// exact bytes, which are admission facts and never profile semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Payload {
    role: Role,
    format: VersionedIdentity,
    /// Relative to the manifest's location.
    address: LogicalAddress,
    byte_length: Quantity,
    digest: FileDigest,
}

/// What a payload holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Conformance,
    Markers,
}

/// Why bytes are not a manifest this reader implements. Each has one of 015's
/// closed reasons at the boundary; [`EnvelopeFailure::reason`] names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeFailure {
    /// The manifest is over the caller's byte or depth limit.
    OverBound(EnvelopeLimit),
    /// The bytes are not one JSON value of the closed manifest.
    Malformed,
    /// The `kind` or `schemaRevision` is not the one this reader implements.
    Unsupported,
    /// An object repeats a member.
    DuplicateMember,
    /// An object has a member the closed manifest does not.
    UnknownMember,
}

/// The manifest's own limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeLimit {
    Bytes,
    Depth,
}

impl EnvelopeFailure {
    /// 015's closed reason at the `locale-canonicalization-data-artifact`
    /// boundary. A `kind` or `schemaRevision` this reader does not implement
    /// is no envelope it can read, so it is `malformed-envelope` there.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::OverBound(_) => "over-bound-envelope",
            Self::Malformed | Self::Unsupported => "malformed-envelope",
            Self::DuplicateMember => "duplicate-member",
            Self::UnknownMember => "unknown-member",
        }
    }
}

impl LocaleCanonicalizationDataArtifact {
    /// The manifest with its integrity digest computed over everything else.
    /// Whatever digest it held before is replaced.
    pub fn seal(mut self) -> Result<Self, EncodingFailure> {
        self.integrity_digest = self.integrity()?;
        Ok(self)
    }

    pub const fn specification(&self) -> &VersionedIdentity {
        &self.specification
    }

    pub const fn dataset(&self) -> &DatasetDescriptor {
        &self.dataset
    }

    pub fn payloads(&self) -> &[Payload] {
        &self.payloads
    }

    pub const fn integrity_digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }

    /// `H("locale-canonicalization-data-integrity", manifest)` with only the
    /// top-level `integrityDigest` left out.
    fn integrity(&self) -> Result<IntegrityDigest, EncodingFailure> {
        hash_with_excluded_member(
            Domain::literal(INTEGRITY_DOMAIN),
            self,
            &["integrityDigest"],
        )
        .map(IntegrityDigest::from_hash)
    }

    /// Whether the stored integrity digest is the digest of the rest.
    pub(crate) fn verify_integrity(&self) -> Result<bool, EncodingFailure> {
        Ok(self.integrity()? == self.integrity_digest)
    }

    /// The rules of the closed manifest that its types do not state: one
    /// payload per role in role order, and the sorted, unique lists.
    fn closed(&self) -> bool {
        self.payloads
            .iter()
            .map(|payload| payload.role)
            .eq([Role::Conformance, Role::Markers])
            && ascending(&self.generation.cldr.sources, |source| {
                source.address.as_str()
            })
            && ascending(&self.generation.markers, NonemptyText::as_str)
    }
}

impl DatasetDescriptor {
    pub const fn identity(&self) -> &Token {
        &self.identity
    }

    pub const fn schema(&self) -> &VersionedIdentity {
        &self.schema
    }

    pub const fn semantic_digest(&self) -> &SemanticDigest {
        &self.semantic_digest
    }
}

impl Payload {
    pub const fn role(&self) -> Role {
        self.role
    }

    pub const fn format(&self) -> &VersionedIdentity {
        &self.format
    }

    pub const fn address(&self) -> &LogicalAddress {
        &self.address
    }

    pub const fn byte_length(&self) -> u64 {
        self.byte_length.get()
    }

    pub const fn digest(&self) -> &FileDigest {
        &self.digest
    }
}

/// Whether each item's key is strictly after the one before it, by unsigned
/// UTF-8 bytes.
fn ascending<T>(items: &[T], key: impl Fn(&T) -> &str) -> bool {
    items.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

/// Read the manifest at the pre-invocation boundary.
///
/// 015 orders the boundary's reasons as malformed, duplicate member, unknown
/// member, over-bound, and exposes the first that applies. 017 checks limits
/// before anything is decoded, so the byte and depth limits come first; the
/// rest follow 015's order. Repeated members are found after the shape is
/// known to be right, which needs a reading that keeps one value per member;
/// unknown members are found by the closed structure and removed before the
/// shape is checked, so they are reported only when nothing else is wrong.
pub(crate) fn decode(
    bytes: &[u8],
    max_bytes: u64,
    max_depth: u64,
) -> Result<LocaleCanonicalizationDataArtifact, EnvelopeFailure> {
    if u64::try_from(bytes.len()).map_or(true, |length| length > max_bytes) {
        return Err(EnvelopeFailure::OverBound(EnvelopeLimit::Bytes));
    }
    if depth(bytes) > max_depth {
        return Err(EnvelopeFailure::OverBound(EnvelopeLimit::Depth));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| EnvelopeFailure::Malformed)?;
    let mut value: Value = serde_json::from_str(text).map_err(|_| EnvelopeFailure::Malformed)?;
    select(&value)?;
    let unknown = strip(&mut value, &MANIFEST);
    let manifest: LocaleCanonicalizationDataArtifact =
        serde_json::from_value(value).map_err(|_| EnvelopeFailure::Malformed)?;
    if !manifest.closed() {
        return Err(EnvelopeFailure::Malformed);
    }
    // The text parsed above, so a repeated member is all that can fail here.
    if decode_unique_json(text).is_err() {
        return Err(EnvelopeFailure::DuplicateMember);
    }
    if unknown {
        return Err(EnvelopeFailure::UnknownMember);
    }
    Ok(manifest)
}

/// The deepest nesting of objects and arrays, counted outside strings without
/// decoding anything.
fn depth(bytes: &[u8]) -> u64 {
    let (mut depth, mut deepest) = (0_u64, 0_u64);
    let (mut quoted, mut escaped) = (false, false);
    for &byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' | b'[' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// Select the exact `kind` and `schemaRevision` before the rest is decoded:
/// another kind or revision has another shape, so decoding it as this one
/// would report a malformed manifest for what is really one this reader does
/// not implement.
fn select(value: &Value) -> Result<(), EnvelopeFailure> {
    let text = |member: &str| {
        value
            .get(member)
            .and_then(Value::as_str)
            .ok_or(EnvelopeFailure::Malformed)
    };
    if text("kind")? != ARTIFACT_KIND || text("schemaRevision")? != ARTIFACT_SCHEMA_REVISION {
        return Err(EnvelopeFailure::Unsupported);
    }
    Ok(())
}

/// Where a value sits in the closed manifest: the members of each object, by
/// name, and how to walk each of them.
pub(crate) enum Shape {
    /// A value the closed types decide alone.
    Leaf,
    Object(&'static [(&'static str, &'static Shape)]),
    List(&'static Shape),
}

static LEAF: Shape = Shape::Leaf;
static VERSIONED: Shape = Shape::Object(&[("identity", &LEAF), ("revision", &LEAF)]);
static DATASET: Shape = Shape::Object(&[
    ("identity", &LEAF),
    ("schema", &VERSIONED),
    ("semanticDigest", &LEAF),
]);
static MARKER_DATA: Shape =
    Shape::Object(&[("checksum", &LEAF), ("package", &LEAF), ("version", &LEAF)]);
static SOURCE: Shape = Shape::Object(&[("address", &LEAF), ("digest", &LEAF)]);
static SOURCES: Shape = Shape::List(&SOURCE);
static CLDR: Shape = Shape::Object(&[("revision", &LEAF), ("sources", &SOURCES)]);
static MARKERS: Shape = Shape::List(&LEAF);
static GENERATION: Shape = Shape::Object(&[
    ("cldr", &CLDR),
    ("markerData", &MARKER_DATA),
    ("markers", &MARKERS),
]);
static PAYLOAD: Shape = Shape::Object(&[
    ("address", &LEAF),
    ("byteLength", &LEAF),
    ("digest", &LEAF),
    ("format", &VERSIONED),
    ("role", &LEAF),
]);
static PAYLOADS: Shape = Shape::List(&PAYLOAD);
pub(crate) static MANIFEST: Shape = Shape::Object(&[
    ("dataset", &DATASET),
    ("generation", &GENERATION),
    ("integrityDigest", &LEAF),
    ("kind", &LEAF),
    ("payloads", &PAYLOADS),
    ("schemaRevision", &LEAF),
    ("specification", &VERSIONED),
]);

/// Remove every member the closed manifest does not have, and say whether
/// there was one. A value of another type is left for the closed types to
/// refuse.
fn strip(value: &mut Value, shape: &Shape) -> bool {
    match (shape, value) {
        (Shape::Object(members), Value::Object(object)) => {
            let before = object.len();
            object.retain(|name, _| members.iter().any(|(known, _)| known == name));
            let mut found = object.len() != before;
            for (name, inner) in *members {
                if let Some(value) = object.get_mut(*name) {
                    found |= strip(value, inner);
                }
            }
            found
        }
        (Shape::List(item), Value::Array(items)) => {
            let mut found = false;
            for value in items {
                found |= strip(value, item);
            }
            found
        }
        _ => false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::markers::Marker;

    pub(crate) const NARROW: &[u8] = include_bytes!("../fixtures/artifact/manifest-narrow.json");

    /// The narrow fixture manifest's value.
    pub(crate) fn sample() -> Value {
        serde_json::from_slice(NARROW).unwrap()
    }

    fn decoded(bytes: &[u8]) -> Result<LocaleCanonicalizationDataArtifact, EnvelopeFailure> {
        decode(bytes, u64::MAX, u64::MAX)
    }

    /// The sample with the value at `pointer` replaced, as JSON bytes.
    fn edited(pointer: &str, value: Value) -> Vec<u8> {
        let mut sample = sample();
        *sample.pointer_mut(pointer).unwrap() = value;
        serde_json::to_vec(&sample).unwrap()
    }

    /// The sample with `member` added to, or removed from, the object at
    /// `pointer`, as JSON bytes.
    fn member(pointer: &str, member: &str, value: Option<Value>) -> Vec<u8> {
        let mut sample = sample();
        let object = sample
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap();
        match value {
            Some(value) => object.insert(member.into(), value),
            None => object.remove(member),
        };
        serde_json::to_vec(&sample).unwrap()
    }

    /// The canonical sample text with `from` replaced once by `to`.
    fn spliced(from: &str, to: &str) -> Vec<u8> {
        let text = std::str::from_utf8(NARROW).unwrap();
        assert_eq!(text.matches(from).count(), 1, "{from}");
        text.replacen(from, to, 1).into_bytes()
    }

    #[test]
    fn the_fixture_manifest_decodes_and_its_integrity_is_its_own() {
        let manifest = decoded(NARROW).unwrap();
        assert!(manifest.verify_integrity().unwrap());
        assert_eq!(manifest.clone().seal().unwrap(), manifest);
        assert_eq!(
            manifest.specification(),
            &VersionedIdentity::literal("intlify-locale-canonicalization", "0")
        );
        assert_eq!(
            manifest.dataset().identity().as_str(),
            "intlify-locale-canonicalization-fixture"
        );
        assert_eq!(
            manifest.dataset().schema(),
            &VersionedIdentity::literal("intlify-locale-canonicalization-dataset", "0")
        );
        let [conformance, markers] = manifest.payloads() else {
            panic!("two payloads");
        };
        assert_eq!(
            (conformance.role(), conformance.address().as_str()),
            (Role::Conformance, "conformance.json")
        );
        assert_eq!(
            (markers.role(), markers.address().as_str()),
            (Role::Markers, "markers-narrow.postcard")
        );
        assert_eq!(conformance.byte_length(), 1521);
        assert_eq!(
            markers.format(),
            &VersionedIdentity::literal("icu4x-blob", "2.2")
        );
    }

    #[test]
    fn integrity_covers_everything_but_its_own_member() {
        let manifest = decoded(NARROW).unwrap();
        let mut value = serde_json::to_value(&manifest).unwrap();
        value["integrityDigest"] = json!(format!("sha256:{}", "1".repeat(64)));
        let replaced: LocaleCanonicalizationDataArtifact = serde_json::from_value(value).unwrap();
        assert!(!replaced.verify_integrity().unwrap());
        assert_eq!(replaced.seal().unwrap(), manifest);
        for (pointer, changed) in [
            ("/generation/markerData/version", json!("2.2.1")),
            ("/payloads/1/byteLength", json!("567")),
            ("/dataset/identity", json!("another-dataset")),
        ] {
            let mut value = serde_json::to_value(&manifest).unwrap();
            *value.pointer_mut(pointer).unwrap() = changed;
            let changed: LocaleCanonicalizationDataArtifact =
                serde_json::from_value(value).unwrap();
            assert!(!changed.verify_integrity().unwrap(), "{pointer}");
        }
        assert_eq!(INTEGRITY_DOMAIN, "locale-canonicalization-data-integrity");
    }

    #[test]
    fn each_failure_has_its_015_reason() {
        for (failure, reason) in [
            (
                EnvelopeFailure::OverBound(EnvelopeLimit::Bytes),
                "over-bound-envelope",
            ),
            (
                EnvelopeFailure::OverBound(EnvelopeLimit::Depth),
                "over-bound-envelope",
            ),
            (EnvelopeFailure::Malformed, "malformed-envelope"),
            (EnvelopeFailure::Unsupported, "malformed-envelope"),
            (EnvelopeFailure::DuplicateMember, "duplicate-member"),
            (EnvelopeFailure::UnknownMember, "unknown-member"),
        ] {
            assert_eq!(failure.reason(), reason, "{failure:?}");
        }
    }

    #[test]
    fn the_byte_and_depth_limits_admit_their_exact_values() {
        let length = u64::try_from(NARROW.len()).unwrap();
        assert!(decode(NARROW, length, 5).is_ok());
        assert_eq!(
            decode(NARROW, length - 1, 5),
            Err(EnvelopeFailure::OverBound(EnvelopeLimit::Bytes))
        );
        // The deepest value is a CLDR source: the manifest, generation, cldr,
        // the source list, and the source.
        assert_eq!(
            decode(NARROW, length, 4),
            Err(EnvelopeFailure::OverBound(EnvelopeLimit::Depth))
        );
    }

    #[test]
    fn depth_is_counted_outside_strings() {
        assert_eq!(depth(b"{}"), 1);
        assert_eq!(depth(b"[[],[[]]]"), 3);
        assert_eq!(depth(br#"{"a":"[[[{"}"#), 1);
        assert_eq!(depth(br#"{"a":"\"[[[","b":[]}"#), 2);
        assert_eq!(depth(br#"["\\",[]]"#), 2);
        assert_eq!(depth(b"]]]"), 0);
    }

    #[test]
    fn limits_come_before_anything_is_decoded() {
        // Over both limits and not JSON at all: the limit is reported.
        assert_eq!(
            decode(b"[[[[[[ not json", 4, 64),
            Err(EnvelopeFailure::OverBound(EnvelopeLimit::Bytes))
        );
        assert_eq!(
            decode(b"[[[[[[ not json", 64, 4),
            Err(EnvelopeFailure::OverBound(EnvelopeLimit::Depth))
        );
    }

    #[test]
    fn bytes_that_are_not_one_closed_manifest_are_malformed() {
        let text = std::str::from_utf8(NARROW).unwrap();
        let three = {
            let mut sample = sample();
            let extra = sample["payloads"][1].clone();
            sample["payloads"].as_array_mut().unwrap().push(extra);
            serde_json::to_vec(&sample).unwrap()
        };
        let reversed = {
            let mut sample = sample();
            sample["payloads"].as_array_mut().unwrap().reverse();
            serde_json::to_vec(&sample).unwrap()
        };
        let markers = Marker::ALL.map(Marker::name);
        for (case, bytes) in [
            ("not UTF-8", b"\xff".to_vec()),
            (
                "byte-order mark",
                [b"\xef\xbb\xbf".as_slice(), NARROW].concat(),
            ),
            ("trailing data", format!("{text} {{}}").into_bytes()),
            ("not an object", b"[]".to_vec()),
            ("kind not a string", edited("/kind", json!(0))),
            ("no revision", member("", "schemaRevision", None)),
            ("missing member", member("", "dataset", None)),
            (
                "missing nested member",
                member("/generation/cldr", "sources", None),
            ),
            ("wrong type", edited("/payloads", json!({}))),
            ("number", edited("/payloads/0/byteLength", json!(1521))),
            (
                "leading zero",
                edited("/payloads/0/byteLength", json!("01521")),
            ),
            (
                "token grammar",
                edited("/specification/identity", json!("Intlify")),
            ),
            ("digest", edited("/payloads/0/digest", json!("sha256:00"))),
            (
                "address",
                edited("/payloads/0/address", json!("../conformance.json")),
            ),
            ("role", edited("/payloads/0/role", json!("dataset"))),
            ("empty marker", edited("/generation/markers/0", json!(""))),
            ("three payloads", three),
            ("payloads out of role order", reversed),
            (
                "a role twice",
                edited("/payloads/1/role", json!("conformance")),
            ),
            (
                "sources out of order",
                edited(
                    "/generation/cldr/sources",
                    json!([
                        sample()["generation"]["cldr"]["sources"][1],
                        sample()["generation"]["cldr"]["sources"][0]
                    ]),
                ),
            ),
            (
                "a source twice",
                edited(
                    "/generation/cldr/sources/1/address",
                    json!("common/bcp47/calendar.xml"),
                ),
            ),
            (
                "markers out of order",
                edited(
                    "/generation/markers",
                    json!([markers[1], markers[0], markers[2], markers[3]]),
                ),
            ),
            (
                "a marker twice",
                edited("/generation/markers/1", json!(markers[0])),
            ),
        ] {
            assert_eq!(decoded(&bytes), Err(EnvelopeFailure::Malformed), "{case}");
        }
    }

    #[test]
    fn another_kind_or_schema_revision_is_unsupported() {
        for bytes in [
            edited("/kind", json!("locale-canonicalization-dataset")),
            edited("/schemaRevision", json!("1")),
            // The kind is selected first, whatever follows it.
            {
                let mut sample = sample();
                sample["kind"] = json!("another-kind");
                sample.as_object_mut().unwrap().remove("schemaRevision");
                serde_json::to_vec(&sample).unwrap()
            },
        ] {
            assert_eq!(decoded(&bytes), Err(EnvelopeFailure::Unsupported));
        }
        assert_eq!(ARTIFACT_KIND, "locale-canonicalization-data");
        assert_eq!(ARTIFACT_SCHEMA_REVISION, "0");
    }

    #[test]
    fn a_repeated_member_is_found_at_any_depth() {
        for bytes in [
            spliced(
                "{\"dataset\"",
                "{\"kind\":\"locale-canonicalization-data\",\"dataset\"",
            ),
            spliced(
                "\"role\":\"conformance\"",
                "\"role\":\"conformance\",\"role\":\"conformance\"",
            ),
        ] {
            assert_eq!(decoded(&bytes), Err(EnvelopeFailure::DuplicateMember));
        }
    }

    #[test]
    fn an_unknown_member_is_found_at_any_depth() {
        for bytes in [
            member("", "comment", Some(json!("x"))),
            member("/generation/cldr/sources/0", "size", Some(json!("1"))),
            member("/payloads/1/format", "variant", Some(json!(null))),
            member("/dataset/schema", "extra", Some(json!([]))),
        ] {
            assert_eq!(decoded(&bytes), Err(EnvelopeFailure::UnknownMember));
        }
    }

    #[test]
    fn several_problems_report_the_first_of_015s_order() {
        let mut both = sample();
        both["comment"] = json!("x");
        both["kind"] = json!(0);
        let malformed_and_unknown = serde_json::to_vec(&both).unwrap();
        assert_eq!(
            decoded(&malformed_and_unknown),
            Err(EnvelopeFailure::Malformed)
        );

        let mut unknown = sample();
        unknown["comment"] = json!("x");
        let text = serde_json::to_string(&unknown).unwrap();
        let duplicate_and_unknown = text.replacen(
            "\"comment\":\"x\"",
            "\"comment\":\"x\",\"kind\":\"locale-canonicalization-data\"",
            1,
        );
        assert_eq!(
            decoded(duplicate_and_unknown.as_bytes()),
            Err(EnvelopeFailure::DuplicateMember)
        );

        // A repeated member whose last value is malformed: the shape is
        // checked on the value a plain reading keeps, the last one.
        let malformed_and_duplicate = spliced(
            "\"role\":\"conformance\"",
            "\"role\":\"conformance\",\"role\":0",
        );
        assert_eq!(
            decoded(&malformed_and_duplicate),
            Err(EnvelopeFailure::Malformed)
        );
    }

    #[test]
    fn stripping_removes_only_members_the_closed_manifest_lacks() {
        let mut value = sample();
        assert!(!strip(&mut value, &MANIFEST));
        assert_eq!(value, sample());

        value["generation"]["cldr"]["sources"][1]["extra"] = json!({"nested": "x"});
        value["payloads"][0]["format"]["extra"] = json!("x");
        assert!(strip(&mut value, &MANIFEST));
        assert_eq!(value, sample());

        // A value of another type is left for the closed types to refuse.
        let mut other = json!({"generation": "x", "payloads": [1, "x"], "dataset": []});
        assert!(!strip(&mut other, &MANIFEST));
        assert_eq!(
            other,
            json!({"generation": "x", "payloads": [1, "x"], "dataset": []})
        );
    }

    /// The member names of every object schema reached from `schema`, by path.
    fn schema_members(
        schema: &Value,
        root: &Value,
        path: &str,
        found: &mut Vec<(String, Vec<String>)>,
    ) {
        let resolved = match schema.get("$ref").and_then(Value::as_str) {
            Some(reference) => &root["definitions"][reference.trim_start_matches("#/definitions/")],
            None => schema,
        };
        if let Some(properties) = resolved.get("properties").and_then(Value::as_object) {
            let mut names: Vec<String> = properties.keys().cloned().collect();
            names.sort_unstable();
            found.push((path.to_owned(), names));
            for (name, property) in properties {
                schema_members(property, root, &format!("{path}/{name}"), found);
            }
        }
        if let Some(items) = resolved.get("items") {
            schema_members(items, root, &format!("{path}/*"), found);
        }
        // A property that only describes itself wraps its type in `allOf`.
        if let Some(all) = resolved.get("allOf").and_then(Value::as_array) {
            for part in all {
                schema_members(part, root, path, found);
            }
        }
    }

    fn table_members(shape: &Shape, path: &str, found: &mut Vec<(String, Vec<String>)>) {
        match shape {
            Shape::Leaf => {}
            Shape::Object(members) => {
                let mut names: Vec<String> =
                    members.iter().map(|(name, _)| (*name).to_owned()).collect();
                names.sort_unstable();
                found.push((path.to_owned(), names));
                for (name, inner) in *members {
                    table_members(inner, &format!("{path}/{name}"), found);
                }
            }
            Shape::List(item) => table_members(item, &format!("{path}/*"), found),
        }
    }

    #[test]
    fn the_closed_structure_table_is_the_committed_schema() {
        let schema = crate::schema::manifest_schema().unwrap();
        let mut from_schema = Vec::new();
        schema_members(&schema, &schema, "", &mut from_schema);
        let mut from_table = Vec::new();
        table_members(&MANIFEST, "", &mut from_table);
        from_schema.sort();
        from_table.sort();
        assert_eq!(from_table, from_schema);
        assert_eq!(from_table.len(), 10);
    }
}
