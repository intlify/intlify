// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's Resource Limit Policy Verification Input (D17:1124-1142).
//!
//! The input names itself, declares 018's `exact-pin` mode, and lists the
//! exact Resource Limit Policy references that trusted setup pins, sorted by
//! the unsigned bytes of each pin's canonical framing `C(pin)` and none twice.
//! Its integrity covers the input without its own `integrityDigest`; its
//! semantic digest covers `{ kind, schemaRevision, mode, pins }`, so it pins
//! the authority rather than its name. The input carries no signature, key,
//! credential, or locator: 018 reserves such members only to refuse them.
//!
//! This module is the representation. 015's
//! `resource-limit-policy-verification-input` boundary, in
//! `crate::boundary::verification`, reads it in 015's order.

use intlify_shared_json::encoding::{hash, hash_with_excluded_member, Domain, EncodingFailure};
use intlify_shared_json::token::{IntegrityDigest, Token};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::references::PolicyReference;

use super::SemanticDigest;

/// The one input kind this reader implements.
pub(crate) const INPUT_KIND: &str = "resource-limit-policy-verification-input";

/// The one input schema revision this reader implements.
pub(crate) const INPUT_SCHEMA_REVISION: &str = "0";

/// The one mode 018 defines in revision "0".
pub(crate) const EXACT_PIN: &str = "exact-pin";

/// The digest domain 017 registers for the input's integrity.
pub(crate) const INTEGRITY_DOMAIN: &str = "resource-limit-policy-verification-input-integrity";

/// The digest domain 017 registers for the input's semantic digest.
pub(crate) const SEMANTIC_DOMAIN: &str = "resource-limit-policy-verification-input-semantic-digest";

/// The closed input's members.
pub(crate) const MEMBERS: [&str; 7] = [
    "identity",
    "integrityDigest",
    "kind",
    "mode",
    "pins",
    "revision",
    "schemaRevision",
];

/// A pin's members: 017's five-member `PolicyReference`.
pub(crate) const PIN_MEMBERS: [&str; 5] = [
    "identity",
    "kind",
    "revision",
    "semanticDigest",
    "specificationRevision",
];

/// The members 018 reserves as secret-bearing, to be refused.
pub(crate) const SECRET_BEARING: [&str; 3] = ["privateKey", "credential", "secretLocator"];

/// The members 018 reserves as ambient authority, to be refused.
pub(crate) const AMBIENT_AUTHORITY: [&str; 3] = ["trustStore", "networkSource", "hostTrust"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum InputKind {
    #[serde(rename = "resource-limit-policy-verification-input")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum RevisionZero {
    #[serde(rename = "0")]
    Value,
}

/// One input in 017's closed shape. The shape admits any mode string and any
/// list of Policy references: whether the mode is `exact-pin` and the pins
/// follow 017's rules is checked after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VerificationInputArtifact {
    kind: InputKind,
    schema_revision: RevisionZero,
    identity: Token,
    revision: Token,
    mode: String,
    pins: Vec<PolicyReference>,
    integrity_digest: IntegrityDigest,
}

impl VerificationInputArtifact {
    pub(crate) const fn identity(&self) -> &Token {
        &self.identity
    }

    pub(crate) const fn revision(&self) -> &Token {
        &self.revision
    }

    pub(crate) fn mode(&self) -> &str {
        &self.mode
    }

    pub(crate) fn pins(&self) -> &[PolicyReference] {
        &self.pins
    }

    pub(crate) const fn integrity_digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }

    /// `H("resource-limit-policy-verification-input-integrity", input)` with
    /// only the top-level `integrityDigest` left out.
    pub(crate) fn integrity(&self) -> Result<IntegrityDigest, EncodingFailure> {
        hash_with_excluded_member(
            Domain::literal(INTEGRITY_DOMAIN),
            self,
            &["integrityDigest"],
        )
        .map(IntegrityDigest::from_hash)
    }

    /// `{ kind, schemaRevision, mode, pins }`: the input without the name and
    /// revision the semantic digest leaves out.
    pub(crate) fn semantic_preimage(&self) -> Value {
        json!({
            "kind": INPUT_KIND,
            "schemaRevision": INPUT_SCHEMA_REVISION,
            "mode": self.mode,
            "pins": self.pins,
        })
    }

    pub(crate) fn semantic_digest(&self) -> Result<SemanticDigest, EncodingFailure> {
        hash(Domain::literal(SEMANTIC_DOMAIN), &self.semantic_preimage())
            .map(SemanticDigest::from_hash)
    }
}

/// The input named `identity` at `revision` in the `exact-pin` mode, with
/// `pins` as given and no integrity digest yet.
#[cfg(any(test, feature = "vectors", feature = "benchmark"))]
pub(crate) fn unsealed(identity: &str, revision: &str, pins: &[Value]) -> Value {
    json!({
        "kind": INPUT_KIND,
        "schemaRevision": INPUT_SCHEMA_REVISION,
        "identity": identity,
        "revision": revision,
        "mode": EXACT_PIN,
        "pins": pins,
    })
}

/// `input` with its integrity digest computed over the rest of it, whatever
/// the rest is, so that a test or a vector can seal an input the reader then
/// refuses for something else.
///
/// # Panics
///
/// Panics when `input` is not an object.
#[cfg(any(test, feature = "vectors", feature = "benchmark"))]
pub(crate) fn sealed(mut input: Value) -> Result<Value, EncodingFailure> {
    let placeholder = IntegrityDigest::from_hash([0; 32]);
    input["integrityDigest"] = Value::from(placeholder.as_str());
    let digest = hash_with_excluded_member(
        Domain::literal(INTEGRITY_DOMAIN),
        &input,
        &["integrityDigest"],
    )?;
    input["integrityDigest"] = Value::from(IntegrityDigest::from_hash(digest).as_str());
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn artifact(input: &Value) -> VerificationInputArtifact {
        serde_json::from_value(input.clone()).unwrap()
    }

    /// The integrity covers the input without its own member, so a sealed
    /// input verifies and changing any member breaks it.
    #[test]
    fn the_integrity_covers_everything_but_its_own_member() {
        let input = sealed(unsealed("storefront-pins", "1", &[pin("limits", '1')])).unwrap();
        let read = artifact(&input);
        assert_eq!(&read.integrity().unwrap(), read.integrity_digest());
        let mut without = input.clone();
        without.as_object_mut().unwrap().remove("integrityDigest");
        let expected = hash(Domain::literal(INTEGRITY_DOMAIN), &without).unwrap();
        assert_eq!(
            read.integrity_digest(),
            &IntegrityDigest::from_hash(expected)
        );
        for (member, value) in [
            ("identity", json!("storefront-other")),
            ("revision", json!("2")),
            ("mode", json!("floating")),
            ("pins", json!([pin("limits", '2')])),
        ] {
            let mut changed = input.clone();
            changed[member] = value;
            let changed = artifact(&changed);
            assert_ne!(
                &changed.integrity().unwrap(),
                changed.integrity_digest(),
                "{member}"
            );
        }
    }

    /// The semantic digest is `H` over the kind, schema revision, mode, and
    /// pins: another name or revision keeps it, and another pin changes it.
    #[test]
    fn the_semantic_digest_names_the_pins_and_not_the_input() {
        let input =
            artifact(&sealed(unsealed("storefront-pins", "1", &[pin("limits", '1')])).unwrap());
        assert_eq!(
            input.semantic_preimage(),
            json!({
                "kind": INPUT_KIND,
                "schemaRevision": "0",
                "mode": "exact-pin",
                "pins": [pin("limits", '1')],
            })
        );
        let expected = hash(Domain::literal(SEMANTIC_DOMAIN), &input.semantic_preimage()).unwrap();
        let digest = input.semantic_digest().unwrap();
        assert_eq!(digest, SemanticDigest::from_hash(expected));
        let renamed =
            artifact(&sealed(unsealed("other-pins", "7", &[pin("limits", '1')])).unwrap());
        assert_eq!(renamed.semantic_digest().unwrap(), digest);
        let repinned =
            artifact(&sealed(unsealed("storefront-pins", "1", &[pin("limits", '2')])).unwrap());
        assert_ne!(repinned.semantic_digest().unwrap(), digest);
    }

    /// The closed members are the type's, and the reserved ones are none of
    /// them.
    #[test]
    fn the_closed_members_are_the_types_and_none_is_reserved() {
        let input = sealed(unsealed("storefront-pins", "1", &[pin("limits", '1')])).unwrap();
        let mut members: Vec<&str> = input
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        members.sort_unstable();
        assert_eq!(members, MEMBERS);
        let mut members: Vec<&str> = input["pins"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        members.sort_unstable();
        assert_eq!(members, PIN_MEMBERS);
        for reserved in SECRET_BEARING.into_iter().chain(AMBIENT_AUTHORITY) {
            assert!(
                !MEMBERS.contains(&reserved) && !PIN_MEMBERS.contains(&reserved),
                "{reserved}"
            );
        }
    }
}
