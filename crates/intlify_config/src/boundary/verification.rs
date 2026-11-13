// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `resource-limit-policy-verification-input` boundary: 018's exact-pin
//! Verification Input, read from 017's representation and admitted against
//! the trusted setup path's assertion, or refused with one of the ten reasons
//! of its row (D15:3143, 3155; D18:226-239).
//!
//! The checks run in 015's order: an input is present; its bytes and depth,
//! before anything is decoded; its syntax; its pin count over the decoded
//! value; its `kind`, `schemaRevision`, and closed shape; a repeated member; a
//! member outside the shape that 018 does not reserve; a reserved
//! secret-bearing member; a reserved ambient-authority member; its integrity,
//! its mode, and its pin rules; and last its identity, revision, and semantic
//! digest against the assertion. The first check that fails decides, so a
//! reserved member is refused before the integrity hashes the input. A member
//! outside the shape is one wherever it is, in a pin as at the top level.
//!
//! The bounds are the Admitted Implementation Capability's
//! `bootstrapOnly.inputBoundary` bytes, depth, and pin count (018-017).
//! Admitting the input never consults the configuration or the collection,
//! so a Resource Limit Policy cannot pin itself.

use std::collections::BTreeSet;

use intlify_shared_json::encoding::encode;
use intlify_shared_json::json::decode_unique_json;
use intlify_shared_json::token::Token;
use serde_json::{Map, Value};

use crate::artifacts::envelope::{ArtifactKind, Envelope, ExactTuple};
use crate::artifacts::verification::{
    VerificationInputArtifact, AMBIENT_AUTHORITY, EXACT_PIN, MEMBERS, PIN_MEMBERS, SECRET_BEARING,
};
use crate::artifacts::{count, depth, SemanticDigest};
use crate::input_limits::Bound;
use crate::references::PolicyReference;

use super::capability::ImplementationCapability;

/// What the trusted setup path asserts about the input it supplies: the
/// identity, revision, and semantic digest the boundary compares last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerificationAssertion {
    pub(crate) identity: Token,
    pub(crate) revision: Token,
    pub(crate) semantic_digest: SemanticDigest,
}

/// The bounds on one input: the capability's, or a vector's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerificationLimits {
    pub(crate) bytes: Bound,
    pub(crate) depth: Bound,
    pub(crate) pins: Bound,
}

impl VerificationLimits {
    const fn of(capability: &ImplementationCapability) -> Self {
        let bounds = &capability.bootstrap_only().input_boundary;
        Self {
            bytes: bounds.max_verification_input_bytes,
            depth: bounds.max_depth,
            pins: bounds.max_verification_pins,
        }
    }
}

/// Which bound an input is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationBound {
    /// `inputBoundary.maxVerificationInputBytes`, on the raw bytes.
    InputBytes,
    /// `inputBoundary.maxDepth`, on the raw bytes.
    Depth,
    /// `inputBoundary.maxVerificationPins`, on the decoded pins.
    Pins,
}

/// Where an input is malformed. All are `malformed-input`; the boundary
/// reaches them at different checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Malformation {
    /// The bytes are not one JSON value in UTF-8.
    Syntax,
    /// The value is not 017's closed shape, its `kind` and `schemaRevision`
    /// included.
    Shape,
    /// The integrity digest is not the digest of the rest of the input.
    Integrity,
    /// A digest is beyond the shared encoding's capacity.
    Unframable,
    /// The mode is not `exact-pin`.
    Mode,
    /// The pins are empty, unsorted, repeated, or not all Resource Limit
    /// Policy references.
    Pins,
}

/// Why the boundary refuses an input: one of the ten reasons of its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationRefusal {
    Missing,
    OverBound(VerificationBound),
    Malformed(Malformation),
    DuplicateMember,
    UnknownMember,
    SecretBearing,
    AmbientAuthority,
    IdentityMismatch,
    RevisionMismatch,
    DigestMismatch,
}

impl VerificationRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing-input",
            Self::OverBound(_) => "over-bound-input",
            Self::Malformed(_) => "malformed-input",
            Self::DuplicateMember => "duplicate-member",
            Self::UnknownMember => "unknown-member",
            Self::SecretBearing => "secret-bearing-input",
            Self::AmbientAuthority => "ambient-authority",
            Self::IdentityMismatch => "verification-identity-mismatch",
            Self::RevisionMismatch => "verification-revision-mismatch",
            Self::DigestMismatch => "verification-digest-mismatch",
        }
    }
}

/// 018's admitted exact-pin Verification Input: the input as read, its
/// semantic digest, and the exact references it pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerificationInput {
    input: VerificationInputArtifact,
    semantic_digest: SemanticDigest,
    pins: BTreeSet<ExactTuple>,
}

impl VerificationInput {
    pub(crate) const fn identity(&self) -> &Token {
        self.input.identity()
    }

    pub(crate) const fn revision(&self) -> &Token {
        self.input.revision()
    }

    pub(crate) const fn semantic_digest(&self) -> &SemanticDigest {
        &self.semantic_digest
    }

    /// Whether the candidate's computed exact reference is one of the pins.
    /// A partial match authenticates nothing.
    pub(crate) fn authenticates(&self, candidate: &Envelope<'_>) -> bool {
        self.pins.contains(candidate.tuple())
    }

    /// The normalized input: the input as admitted, which the Materialized
    /// Resolution Input Set carries (D15:205).
    pub(crate) fn normalized(&self) -> Value {
        serde_json::to_value(&self.input).expect("an admitted input serializes")
    }

    /// The input trusted setup would supply to pin `pins`, sealed and admitted
    /// through the boundary against its own assertion, with every bound at
    /// its largest. The pins are sorted by `C(pin)`, so their order does not
    /// matter.
    ///
    /// # Panics
    ///
    /// Panics when the boundary refuses the input: no pin, a repeated pin, or
    /// a pin of another kind.
    #[cfg(any(test, feature = "benchmark"))]
    pub(crate) fn fixture_owned(pins: impl IntoIterator<Item = ExactTuple>) -> Self {
        use crate::artifacts::verification::{sealed, unsealed};

        let mut pins: Vec<Value> = pins.into_iter().map(|pin| reference(&pin)).collect();
        pins.sort_by_cached_key(|pin| encode(pin).expect("a reference has a framing"));
        let input = sealed(unsealed("intlify-test-verification-input", "0", &pins))
            .expect("a fixture input has an integrity digest");
        let read: VerificationInputArtifact =
            serde_json::from_value(input.clone()).expect("a fixture input has the shape");
        let assertion = VerificationAssertion {
            identity: read.identity().clone(),
            revision: read.revision().clone(),
            semantic_digest: read
                .semantic_digest()
                .expect("a fixture input has a digest"),
        };
        let largest = Bound::new(u64::MAX - 1).expect("the largest bound");
        let limits = VerificationLimits {
            bytes: largest,
            depth: largest,
            pins: largest,
        };
        let bytes = serde_json::to_vec(&input).expect("a fixture input serializes");
        read_input(Some(&bytes), limits, &assertion).expect("trusted setup pins distinct policies")
    }
}

/// A Resource Limit Policy reference to `tuple`, as 017 writes a reference.
#[cfg(any(test, feature = "benchmark"))]
fn reference(tuple: &ExactTuple) -> Value {
    serde_json::json!({
        "kind": tuple.kind.spelling(),
        "identity": tuple.identity.as_str(),
        "revision": tuple.revision.as_str(),
        "specificationRevision": tuple.specification_revision.as_str(),
        "semanticDigest": tuple.semantic_digest.as_str(),
    })
}

/// Admit `bytes`, or `None` when no input was supplied, under the capability's
/// bounds and against the trusted setup path's `assertion`: the input, or the
/// boundary's first failing check.
pub(crate) fn admit(
    bytes: Option<&[u8]>,
    capability: &ImplementationCapability,
    assertion: &VerificationAssertion,
) -> Result<VerificationInput, VerificationRefusal> {
    read_input(bytes, VerificationLimits::of(capability), assertion)
}

/// The boundary's checks under `limits`, which [`admit`] takes from the
/// capability and the vectors state outright.
pub(crate) fn read_input(
    bytes: Option<&[u8]>,
    limits: VerificationLimits,
    assertion: &VerificationAssertion,
) -> Result<VerificationInput, VerificationRefusal> {
    use VerificationRefusal::{
        AmbientAuthority, DigestMismatch, DuplicateMember, IdentityMismatch, Malformed, Missing,
        OverBound, RevisionMismatch, SecretBearing, UnknownMember,
    };

    let bytes = bytes.ok_or(Missing)?;
    if count(bytes.len()) > limits.bytes.get() {
        return Err(OverBound(VerificationBound::InputBytes));
    }
    if depth(bytes) > limits.depth.get() {
        return Err(OverBound(VerificationBound::Depth));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Malformed(Malformation::Syntax))?;
    let mut value: Value =
        serde_json::from_str(text).map_err(|_| Malformed(Malformation::Syntax))?;
    let pins = value
        .get("pins")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if count(pins) > limits.pins.get() {
        return Err(OverBound(VerificationBound::Pins));
    }
    // A member outside the shape is reported after the shape, so it is taken
    // out first and remembered.
    let outside = strip(&mut value);
    let input: VerificationInputArtifact =
        serde_json::from_value(value).map_err(|_| Malformed(Malformation::Shape))?;
    // The text parsed above, so a repeated member is all that can fail here.
    if decode_unique_json(text).is_err() {
        return Err(DuplicateMember);
    }
    if outside.unknown {
        return Err(UnknownMember);
    }
    if outside.secret_bearing {
        return Err(SecretBearing);
    }
    if outside.ambient_authority {
        return Err(AmbientAuthority);
    }
    let integrity = input
        .integrity()
        .map_err(|_| Malformed(Malformation::Unframable))?;
    if &integrity != input.integrity_digest() {
        return Err(Malformed(Malformation::Integrity));
    }
    if input.mode() != EXACT_PIN {
        return Err(Malformed(Malformation::Mode));
    }
    let pins = pins_of(&input)?;
    let semantic_digest = input
        .semantic_digest()
        .map_err(|_| Malformed(Malformation::Unframable))?;
    if input.identity() != &assertion.identity {
        return Err(IdentityMismatch);
    }
    if input.revision() != &assertion.revision {
        return Err(RevisionMismatch);
    }
    if semantic_digest != assertion.semantic_digest {
        return Err(DigestMismatch);
    }
    Ok(VerificationInput {
        input,
        semantic_digest,
        pins,
    })
}

/// The pins' exact references, when there is at least one, every one is a
/// Resource Limit Policy, and they ascend strictly by the unsigned bytes of
/// `C(pin)`, which also rules out a repeated pin.
fn pins_of(input: &VerificationInputArtifact) -> Result<BTreeSet<ExactTuple>, VerificationRefusal> {
    let malformed = VerificationRefusal::Malformed(Malformation::Pins);
    let tuples: Vec<ExactTuple> = input.pins().iter().map(PolicyReference::tuple).collect();
    if tuples.is_empty()
        || tuples
            .iter()
            .any(|tuple| tuple.kind != ArtifactKind::ResourceLimitPolicy)
    {
        return Err(malformed);
    }
    let framed = input
        .pins()
        .iter()
        .map(|pin| {
            serde_json::to_value(pin)
                .ok()
                .and_then(|value| encode(&value).ok())
                .ok_or(VerificationRefusal::Malformed(Malformation::Unframable))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if framed.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(malformed);
    }
    Ok(tuples.into_iter().collect())
}

/// What 017's shape leaves out, wherever it occurs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Outside {
    unknown: bool,
    secret_bearing: bool,
    ambient_authority: bool,
}

/// Take every member outside the shape out of the input and its pins, and say
/// which kinds there were.
fn strip(value: &mut Value) -> Outside {
    let mut outside = Outside::default();
    let Some(input) = value.as_object_mut() else {
        return outside;
    };
    sort_out(input, &MEMBERS, &mut outside);
    if let Some(Value::Array(pins)) = input.get_mut("pins") {
        for pin in pins.iter_mut().filter_map(Value::as_object_mut) {
            sort_out(pin, &PIN_MEMBERS, &mut outside);
        }
    }
    outside
}

/// Remove the members of `object` that `shape` does not have, noting each as
/// 018 reserves it or as unknown.
fn sort_out(object: &mut Map<String, Value>, shape: &[&str], outside: &mut Outside) {
    object.retain(|name, _| {
        let name = name.as_str();
        if shape.contains(&name) {
            return true;
        }
        if SECRET_BEARING.contains(&name) {
            outside.secret_bearing = true;
        } else if AMBIENT_AUTHORITY.contains(&name) {
            outside.ambient_authority = true;
        } else {
            outside.unknown = true;
        }
        false
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::artifacts::verification::{sealed, unsealed};
    use crate::construction::capability::{IDENTITY, REVISION};
    use crate::locale::VersionedIdentity;
    use crate::resolver::limits::fixture_limits;

    /// A Resource Limit Policy reference with a digest of one repeated digit.
    fn pin(identity: &str, digit: char) -> Value {
        json!({
            "kind": "resource-limit-policy",
            "identity": identity,
            "revision": "1",
            "specificationRevision": "0",
            "semanticDigest": format!("sha256:{}", digit.to_string().repeat(64)),
        })
    }

    /// Two pins in the order of `C(pin)`.
    fn two_pins() -> Vec<Value> {
        let mut pins = vec![pin("limits-b", '2'), pin("limits-a", '1')];
        pins.sort_by_cached_key(|pin| encode(pin).unwrap());
        pins
    }

    /// The sealed input pinning `two_pins`.
    fn input() -> Value {
        sealed(unsealed("storefront-pins", "1", &two_pins())).unwrap()
    }

    /// `input` with `edit` made and then sealed again.
    fn resealed(edit: impl FnOnce(&mut Value)) -> Value {
        let mut value = input();
        edit(&mut value);
        sealed(value).unwrap()
    }

    /// `input` with `edit` made after sealing, so its integrity no longer holds
    /// unless the edit leaves the hashed members alone.
    fn edited(edit: impl FnOnce(&mut Value)) -> Value {
        let mut value = input();
        edit(&mut value);
        value
    }

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    /// The assertion that `input` itself makes.
    fn assertion() -> VerificationAssertion {
        let read: VerificationInputArtifact = serde_json::from_value(input()).unwrap();
        VerificationAssertion {
            identity: read.identity().clone(),
            revision: read.revision().clone(),
            semantic_digest: read.semantic_digest().unwrap(),
        }
    }

    fn largest() -> Bound {
        Bound::new(u64::MAX - 1).unwrap()
    }

    fn roomy() -> VerificationLimits {
        VerificationLimits {
            bytes: largest(),
            depth: largest(),
            pins: largest(),
        }
    }

    fn read_bytes(
        bytes: &[u8],
        limits: VerificationLimits,
    ) -> Result<VerificationInput, VerificationRefusal> {
        read_input(Some(bytes), limits, &assertion())
    }

    fn refused(value: &Value) -> VerificationRefusal {
        read_bytes(&bytes(value), roomy()).unwrap_err()
    }

    fn remove(value: &mut Value, pointer: &str, member: &str) {
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(member);
    }

    #[test]
    fn an_input_the_setup_path_asserts_is_admitted() {
        let admitted = read_bytes(&bytes(&input()), roomy()).unwrap();
        assert_eq!(admitted.identity().as_str(), "storefront-pins");
        assert_eq!(admitted.revision().as_str(), "1");
        assert_eq!(admitted.semantic_digest(), &assertion().semantic_digest);
        assert_eq!(admitted.normalized(), input());
    }

    #[test]
    fn an_absent_input_is_missing() {
        assert_eq!(
            read_input(None, roomy(), &assertion()),
            Err(VerificationRefusal::Missing)
        );
    }

    /// Each bound admits its exact value and refuses the next: the bytes and
    /// the depth of the raw input, and the number of pins.
    #[test]
    fn each_bound_admits_its_exact_value_and_refuses_the_next() {
        let raw = bytes(&input());
        let exact = |bytes: u64, depth_bound: u64, pins: u64| VerificationLimits {
            bytes: Bound::new(bytes).unwrap(),
            depth: Bound::new(depth_bound).unwrap(),
            pins: Bound::new(pins).unwrap(),
        };
        let (size, levels) = (count(raw.len()), depth(&raw));
        assert_eq!(levels, 3);
        assert!(read_bytes(&raw, exact(size, levels, 2)).is_ok());
        assert_eq!(
            read_bytes(&raw, exact(size - 1, levels, 2)),
            Err(VerificationRefusal::OverBound(
                VerificationBound::InputBytes
            ))
        );
        assert_eq!(
            read_bytes(&raw, exact(size, levels - 1, 2)),
            Err(VerificationRefusal::OverBound(VerificationBound::Depth))
        );
        assert_eq!(
            read_bytes(&raw, exact(size, levels, 1)),
            Err(VerificationRefusal::OverBound(VerificationBound::Pins))
        );
    }

    /// The bytes and the depth are bounded before anything is decoded, and
    /// the pins after the syntax but before the shape.
    #[test]
    fn the_bounds_come_where_015_puts_them() {
        let small = VerificationLimits {
            bytes: Bound::new(4).unwrap(),
            ..roomy()
        };
        assert_eq!(
            read_bytes(b"\xff\xff\xff\xff\xff", small),
            Err(VerificationRefusal::OverBound(
                VerificationBound::InputBytes
            ))
        );
        let shallow = VerificationLimits {
            depth: Bound::new(2).unwrap(),
            ..roomy()
        };
        assert_eq!(
            read_bytes(b"[[[ not json", shallow),
            Err(VerificationRefusal::OverBound(VerificationBound::Depth))
        );
        let both = VerificationLimits {
            bytes: Bound::new(4).unwrap(),
            depth: Bound::new(2).unwrap(),
            ..roomy()
        };
        assert_eq!(
            read_bytes(b"[[[[[", both),
            Err(VerificationRefusal::OverBound(
                VerificationBound::InputBytes
            ))
        );
        let one_pin = VerificationLimits {
            pins: Bound::new(1).unwrap(),
            ..roomy()
        };
        assert_eq!(
            read_bytes(b"{\"pins\": [1, 2", one_pin),
            Err(VerificationRefusal::Malformed(Malformation::Syntax))
        );
        let wrong_kind = edited(|value| value["kind"] = json!("trust-policy"));
        assert_eq!(
            read_bytes(&bytes(&wrong_kind), one_pin),
            Err(VerificationRefusal::OverBound(VerificationBound::Pins))
        );
    }

    /// What is not one JSON value, or not 017's closed shape, is malformed.
    #[test]
    fn what_is_not_the_closed_shape_is_malformed() {
        let syntax = [&b"{"[..], b"\"pins\": []", b"{} {}", b"\xff"];
        for text in syntax {
            assert_eq!(
                read_bytes(text, roomy()),
                Err(VerificationRefusal::Malformed(Malformation::Syntax)),
                "{text:?}"
            );
        }
        let cases = [
            ("an array", json!([])),
            (
                "another kind",
                edited(|value| value["kind"] = json!("trust-policy")),
            ),
            (
                "another schema revision",
                edited(|value| value["schemaRevision"] = json!("1")),
            ),
            ("no identity", edited(|value| remove(value, "", "identity"))),
            (
                "an identity that is no token",
                edited(|value| value["identity"] = json!("Storefront Pins")),
            ),
            (
                "a mode that is no string",
                edited(|value| value["mode"] = json!(["exact-pin"])),
            ),
            (
                "pins that are no array",
                edited(|value| value["pins"] = json!({})),
            ),
            (
                "a pin that is no object",
                edited(|value| value["pins"][0] = json!("limits")),
            ),
            (
                "a pin without its digest",
                edited(|value| remove(value, "/pins/0", "semanticDigest")),
            ),
            (
                "a pin of a target profile",
                edited(|value| value["pins"][0]["kind"] = json!("target-profile")),
            ),
            (
                "an integrity digest that is no digest",
                edited(|value| value["integrityDigest"] = json!("sha256:0")),
            ),
        ];
        for (name, value) in cases {
            assert_eq!(
                refused(&value),
                VerificationRefusal::Malformed(Malformation::Shape),
                "{name}"
            );
        }
    }

    /// A repeated member is refused after the shape and before an unknown
    /// member, at the top level and in a pin.
    #[test]
    fn a_repeated_member_comes_after_the_shape_and_before_an_unknown_one() {
        let text = String::from_utf8(bytes(&input())).unwrap();
        let repeated = text.replacen("{\"kind\"", "{\"mode\":\"exact-pin\",\"kind\"", 1);
        assert_eq!(
            read_bytes(repeated.as_bytes(), roomy()),
            Err(VerificationRefusal::DuplicateMember)
        );
        let in_pin = text.replacen(
            "\"specificationRevision\":\"0\"",
            "\"specificationRevision\":\"0\",\"revision\":\"1\"",
            1,
        );
        assert_ne!(in_pin, text);
        assert_eq!(
            read_bytes(in_pin.as_bytes(), roomy()),
            Err(VerificationRefusal::DuplicateMember)
        );
        let and_unknown = repeated.replacen("{\"mode\"", "{\"note\":\"x\",\"mode\"", 1);
        assert_eq!(
            read_bytes(and_unknown.as_bytes(), roomy()),
            Err(VerificationRefusal::DuplicateMember)
        );
        let and_shape =
            repeated.replacen("\"schemaRevision\":\"0\"", "\"schemaRevision\":\"1\"", 1);
        assert_eq!(
            read_bytes(and_shape.as_bytes(), roomy()),
            Err(VerificationRefusal::Malformed(Malformation::Shape))
        );
    }

    /// A member outside the shape that 018 does not reserve is unknown,
    /// whether it is at the top level or in a pin, and the shape comes first.
    #[test]
    fn a_member_outside_the_shape_is_unknown_wherever_it_is() {
        let cases = [
            (
                "at the top level",
                edited(|value| value["signature"] = json!("ed25519:x")),
            ),
            (
                "in a pin",
                edited(|value| value["pins"][1]["locator"] = json!("https://example.com/limits")),
            ),
        ];
        for (name, value) in cases {
            assert_eq!(
                refused(&value),
                VerificationRefusal::UnknownMember,
                "{name}"
            );
        }
        let and_shape = edited(|value| {
            value["signature"] = json!("ed25519:x");
            value["revision"] = json!("Not A Token");
        });
        assert_eq!(
            refused(&and_shape),
            VerificationRefusal::Malformed(Malformation::Shape)
        );
    }

    /// 018's reserved members are refused as what they are, at the top level
    /// and in a pin, after an unknown member: secret-bearing ones first.
    #[test]
    fn a_reserved_member_is_refused_after_an_unknown_one() {
        for name in SECRET_BEARING {
            let top = edited(|value| value[name] = json!("x"));
            assert_eq!(refused(&top), VerificationRefusal::SecretBearing, "{name}");
            let in_pin = edited(|value| value["pins"][0][name] = json!("x"));
            assert_eq!(
                refused(&in_pin),
                VerificationRefusal::SecretBearing,
                "{name}"
            );
        }
        for name in AMBIENT_AUTHORITY {
            let top = edited(|value| value[name] = json!("x"));
            assert_eq!(
                refused(&top),
                VerificationRefusal::AmbientAuthority,
                "{name}"
            );
            let in_pin = edited(|value| value["pins"][1][name] = json!("x"));
            assert_eq!(
                refused(&in_pin),
                VerificationRefusal::AmbientAuthority,
                "{name}"
            );
        }
        let both = edited(|value| {
            value["hostTrust"] = json!("x");
            value["pins"][1]["credential"] = json!("x");
        });
        assert_eq!(refused(&both), VerificationRefusal::SecretBearing);
        let and_unknown = edited(|value| {
            value["privateKey"] = json!("x");
            value["signature"] = json!("x");
        });
        assert_eq!(refused(&and_unknown), VerificationRefusal::UnknownMember);
    }

    /// A reserved member is refused before the integrity hashes the input, and
    /// the integrity, mode, and pins follow, each malformed.
    #[test]
    fn the_integrity_mode_and_pins_come_after_the_reserved_members() {
        let tampered = |value: &mut Value| {
            value["integrityDigest"] = json!(format!("sha256:{}", "e".repeat(64)));
        };
        assert_eq!(
            refused(&edited(|value| {
                tampered(value);
                value["trustStore"] = json!("x");
            })),
            VerificationRefusal::AmbientAuthority
        );
        assert_eq!(
            refused(&edited(tampered)),
            VerificationRefusal::Malformed(Malformation::Integrity)
        );
        assert_eq!(
            refused(&edited(|value| value["identity"] = json!("other-pins"))),
            VerificationRefusal::Malformed(Malformation::Integrity)
        );
        assert_eq!(
            refused(&resealed(|value| value["mode"] = json!("floating"))),
            VerificationRefusal::Malformed(Malformation::Mode)
        );
        let pins = [
            ("none", json!([])),
            ("unsorted", {
                let mut pins = two_pins();
                pins.reverse();
                json!(pins)
            }),
            (
                "repeated",
                json!([pin("limits-a", '1'), pin("limits-a", '1')]),
            ),
            ("a trust policy", {
                let mut trust = pin("trust", '3');
                trust["kind"] = json!("trust-policy");
                json!([trust])
            }),
        ];
        for (name, list) in pins {
            assert_eq!(
                refused(&resealed(|value| value["pins"] = list)),
                VerificationRefusal::Malformed(Malformation::Pins),
                "{name}"
            );
        }
    }

    /// The assertion is compared last: identity, then revision, then the
    /// semantic digest, which another name or revision keeps.
    #[test]
    fn the_assertion_is_compared_last() {
        let mismatched = |edit: fn(&mut VerificationAssertion)| {
            let mut asserted = assertion();
            edit(&mut asserted);
            read_input(Some(&bytes(&input())), roomy(), &asserted).unwrap_err()
        };
        assert_eq!(
            mismatched(|asserted| asserted.identity = Token::literal("other-pins")),
            VerificationRefusal::IdentityMismatch
        );
        assert_eq!(
            mismatched(|asserted| asserted.revision = Token::literal("2")),
            VerificationRefusal::RevisionMismatch
        );
        assert_eq!(
            mismatched(|asserted| {
                asserted.semantic_digest = SemanticDigest::from_hash([7; 32]);
            }),
            VerificationRefusal::DigestMismatch
        );
        assert_eq!(
            mismatched(|asserted| {
                asserted.revision = Token::literal("2");
                asserted.identity = Token::literal("other-pins");
            }),
            VerificationRefusal::IdentityMismatch
        );
        let renamed = resealed(|value| {
            value["identity"] = json!("other-pins");
            value["revision"] = json!("2");
        });
        assert_eq!(refused(&renamed), VerificationRefusal::IdentityMismatch);
        let asserted = VerificationAssertion {
            identity: Token::literal("other-pins"),
            revision: Token::literal("2"),
            ..assertion()
        };
        assert!(read_input(Some(&bytes(&renamed)), roomy(), &asserted).is_ok());
        let tampered = edited(|value| {
            value["integrityDigest"] = json!(format!("sha256:{}", "e".repeat(64)));
        });
        let other = VerificationAssertion {
            identity: Token::literal("other-pins"),
            ..assertion()
        };
        assert_eq!(
            read_input(Some(&bytes(&tampered)), roomy(), &other),
            Err(VerificationRefusal::Malformed(Malformation::Integrity))
        );
    }

    /// The capability whose `inputBoundary` sets these three bounds, and every
    /// other bound at its minimum, as the capability boundary admits it.
    fn capability(bytes: u64, depth: u64, pins: u64) -> ImplementationCapability {
        let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
        let bounds = &mut value["bootstrapOnly"]["inputBoundary"];
        bounds["maxVerificationInputBytes"] = json!(bytes.to_string());
        bounds["maxDepth"] = json!(depth.to_string());
        bounds["maxVerificationPins"] = json!(pins.to_string());
        let constructed = VersionedIdentity {
            identity: IDENTITY,
            revision: REVISION,
        };
        crate::boundary::capability::admit(Some(&value), &constructed).unwrap()
    }

    /// The boundary bounds the input by the capability's `inputBoundary`, each
    /// bound by its own: the bytes, the depth, and the pins.
    #[test]
    fn the_capability_bounds_the_input() {
        let raw = bytes(&input());
        let (size, levels) = (count(raw.len()), depth(&raw));
        let under =
            |bytes, depth, pins| admit(Some(&raw), &capability(bytes, depth, pins), &assertion());
        assert!(under(size, levels, 2).is_ok());
        assert_eq!(
            under(size - 1, levels, 2),
            Err(VerificationRefusal::OverBound(
                VerificationBound::InputBytes
            ))
        );
        assert_eq!(
            under(size, levels - 1, 2),
            Err(VerificationRefusal::OverBound(VerificationBound::Depth))
        );
        assert_eq!(
            under(size, levels, 1),
            Err(VerificationRefusal::OverBound(VerificationBound::Pins))
        );
        let least = ImplementationCapability::fixture_owned(fixture_limits(1));
        assert_eq!(
            admit(Some(&raw), &least, &assertion()),
            Err(VerificationRefusal::OverBound(
                VerificationBound::InputBytes
            ))
        );
    }

    /// Each refusal spells one of the ten reasons of 015's boundary row.
    #[test]
    fn the_refusals_are_015s_ten_reasons() {
        let refusals = [
            VerificationRefusal::Missing,
            VerificationRefusal::OverBound(VerificationBound::Pins),
            VerificationRefusal::Malformed(Malformation::Mode),
            VerificationRefusal::DuplicateMember,
            VerificationRefusal::UnknownMember,
            VerificationRefusal::SecretBearing,
            VerificationRefusal::AmbientAuthority,
            VerificationRefusal::IdentityMismatch,
            VerificationRefusal::RevisionMismatch,
            VerificationRefusal::DigestMismatch,
        ];
        assert_eq!(
            refusals.map(VerificationRefusal::reason),
            [
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
            ]
        );
    }

    /// A fixture input pins exactly its references, whatever order they come
    /// in, and authenticates a candidate only by an exact match.
    #[test]
    fn a_fixture_input_pins_exactly_its_references_in_any_order() {
        let tuple = |value: &Value| -> ExactTuple {
            serde_json::from_value::<PolicyReference>(value.clone())
                .unwrap()
                .tuple()
        };
        let [first, second] =
            [pin("limits-a", '1'), pin("limits-b", '2')].map(|value| tuple(&value));
        let fixture = VerificationInput::fixture_owned([second.clone(), first.clone()]);
        assert_eq!(
            fixture,
            VerificationInput::fixture_owned([first.clone(), second.clone()])
        );
        assert_eq!(fixture.pins, [first, second].into_iter().collect());
        let pins = fixture.normalized()["pins"].clone();
        assert_eq!(pins, json!(two_pins()));
    }
}
