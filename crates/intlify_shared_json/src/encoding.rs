// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Exact `intlify.shared-json.v0` framing and the domain-separated digest
//! function from design 017.
//!
//! This operates on a separately schema-checked value, never substitutes for
//! body admission, and performs no normalization of arrays, strings, nulls, or
//! registered owner fragments. The owning semantic schema must establish a
//! canonical sequence for a logical set before encoding; this function does not
//! sort arrays, deduplicate elements, infer defaults, or erase `null`.
//!
//! Digest domains are registered by the owning design. This module validates a
//! domain's spelling and frames it; it holds no registry and cannot decide
//! whether a domain is the right one for a value.

use serde_json::Value;
use sha2::{Digest as _, Sha256};

use crate::token::{valid_identity, IdentityFailure};

// Private reader capacity, not a new semantic JSON or project-policy limit.
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DEPTH: usize = 128;

/// Complete failure of a canonical encoding or digest operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingFailure {
    /// A JSON number was present; numbers are outside this encoding.
    Number,
    /// The value nests deeper than the reader capacity.
    Depth,
    /// The encoded form exceeds the reader capacity.
    Capacity,
    /// A length, count, or offset is not representable.
    Overflow,
    /// The exact output buffer could not be reserved.
    Allocation,
    /// The caller's value could not be serialized to JSON.
    Serialization,
    /// The excluded member's path is absent or is not an object.
    RecordShape,
}

/// One registered ASCII digest domain.
///
/// A body field can never select the algorithm or substitute another domain. A
/// common integrity digest must not be reused as semantic-result identity
/// merely because both are hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Domain(&'static str);

impl Domain {
    /// Validate and retain one registered domain spelling.
    pub fn new(name: &'static str) -> Result<Self, IdentityFailure> {
        valid_identity(name)
            .then_some(Self(name))
            .ok_or(IdentityFailure::InvalidToken)
    }

    /// Retain one domain that the caller registers as a literal.
    ///
    /// # Panics
    ///
    /// Panics when the literal is outside the exact token grammar. Registered
    /// domains are implementation constants, so an invalid one is a defect.
    #[must_use]
    pub fn literal(name: &'static str) -> Self {
        Self::new(name).expect("registered literal digest domain")
    }

    /// Borrow the exact registered spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

fn length(value: &Value, depth: usize) -> Result<u64, EncodingFailure> {
    if depth > MAX_DEPTH {
        return Err(EncodingFailure::Depth);
    }
    let count = |value: usize| u64::try_from(value).map_err(|_| EncodingFailure::Overflow);
    let add = |a: u64, b: u64| a.checked_add(b).ok_or(EncodingFailure::Overflow);
    let payload = match value {
        Value::Null | Value::Bool(_) => 0,
        Value::Number(_) => return Err(EncodingFailure::Number),
        Value::String(text) => count(text.len())?,
        Value::Array(items) => {
            count(items.len())?;
            let mut total = 8;
            for child in items {
                total = add(total, length(child, depth + 1)?)?;
            }
            total
        }
        Value::Object(members) => {
            count(members.len())?;
            let mut total = 8;
            for (key, child) in members {
                total = add(total, add(9, count(key.len())?)?)?;
                total = add(total, length(child, depth + 1)?)?;
            }
            total
        }
    };
    let total = add(9, payload)?;
    if total > MAX_BYTES {
        return Err(EncodingFailure::Capacity);
    }
    Ok(total)
}

fn string(text: &str, output: &mut Vec<u8>) {
    output.push(0x04);
    // The complete length preflight proved representability and reserved space.
    output.extend_from_slice(
        &u64::try_from(text.len())
            .expect("preflighted string")
            .to_be_bytes(),
    );
    output.extend_from_slice(text.as_bytes());
}

fn write(value: &Value, output: &mut Vec<u8>) -> Result<(), EncodingFailure> {
    if let Value::String(text) = value {
        string(text, output);
        return Ok(());
    }
    let tag = match value {
        Value::Null => 0x01,
        Value::Bool(false) => 0x02,
        Value::Bool(true) => 0x03,
        Value::Array(_) => 0x05,
        Value::Object(_) => 0x06,
        Value::Number(_) | Value::String(_) => unreachable!("preflight and string arm"),
    };
    output.push(tag);
    let header = output.len();
    output.extend_from_slice(&[0; 8]);
    let payload = output.len();
    match value {
        Value::Array(items) => {
            output.extend_from_slice(
                &u64::try_from(items.len())
                    .expect("preflighted count")
                    .to_be_bytes(),
            );
            for child in items {
                write(child, output)?;
            }
        }
        Value::Object(members) => {
            output.extend_from_slice(
                &u64::try_from(members.len())
                    .expect("preflighted count")
                    .to_be_bytes(),
            );
            let mut ordered = Vec::new();
            ordered
                .try_reserve_exact(members.len())
                .map_err(|_| EncodingFailure::Allocation)?;
            ordered.extend(members.iter());
            ordered.sort_unstable_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
            for (key, child) in ordered {
                string(key, output);
                write(child, output)?;
            }
        }
        _ => {}
    }
    let bytes = u64::try_from(output.len() - payload).expect("preflighted payload");
    output[header..header + 8].copy_from_slice(&bytes.to_be_bytes());
    Ok(())
}

/// Encode one admitted value under the complete canonical value function `C`.
///
/// Object keys are ordered by ascending unsigned UTF-8 bytes; array order is
/// preserved exactly. All lengths count bytes.
pub fn encode(value: &Value) -> Result<Vec<u8>, EncodingFailure> {
    let bytes = usize::try_from(length(value, 1)?).map_err(|_| EncodingFailure::Overflow)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes)
        .map_err(|_| EncodingFailure::Allocation)?;
    write(value, &mut output)?;
    debug_assert_eq!(output.len(), bytes);
    Ok(output)
}

/// Compute `H(D, V)` for one registered domain and admitted value.
pub fn hash(domain: Domain, value: &Value) -> Result<[u8; 32], EncodingFailure> {
    let encoded = encode(value)?;
    let mut hasher = Sha256::new();
    hasher.update(b"intlify.shared-json.v0\0");
    // The domain is closed and selected by the caller's record kind, not input.
    hasher.update(encode(&Value::String(domain.name().into()))?);
    hasher.update(encoded);
    Ok(hasher.finalize().into())
}

/// Digest exact bytes with SHA-256, outside the canonical value framing.
///
/// Design 017 defines a source unit's `utf8Digest` over the actual source
/// bytes rather than over `H` of a reserialized string, and the two are
/// deliberately different values. `H` frames its input and separates it by
/// domain, so it answers which admitted value this is; this answers whether
/// these are the exact bytes a snapshot names. Routing a snapshot digest
/// through `H` would make it depend on JSON string escaping, and no host could
/// reproduce it from a file it read.
///
/// There is no domain here for the same reason: the input is not a value in
/// the shared encoding, so there is nothing for a domain to separate it from.
#[must_use]
pub fn digest_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Compute `H(D, V)` over a value with exactly one member removed.
///
/// `path` names the object members to descend, ending with the member to
/// exclude. Only that member is removed; a same-named member elsewhere is
/// retained. A missing member, a missing intermediate object, or a
/// non-object along the path is a shape failure, because a stored integrity
/// member must be present before it can be excluded from its own preimage.
pub fn hash_with_excluded_member(
    domain: Domain,
    value: &impl serde::Serialize,
    path: &[&str],
) -> Result<[u8; 32], EncodingFailure> {
    let (last, parents) = path.split_last().ok_or(EncodingFailure::RecordShape)?;
    let mut value = serde_json::to_value(value).map_err(|_| EncodingFailure::Serialization)?;
    let mut cursor = &mut value;
    for member in parents {
        cursor = cursor.get_mut(member).ok_or(EncodingFailure::RecordShape)?;
    }
    let object = cursor.as_object_mut().ok_or(EncodingFailure::RecordShape)?;
    if object.remove(*last).is_none() {
        return Err(EncodingFailure::RecordShape);
    }
    hash(domain, &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const RECORD_INTEGRITY: Domain = Domain("verification-record-integrity");
    const MEASUREMENT_CASE: Domain = Domain("measurement-case-identity");

    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write;

        bytes.iter().fold(String::new(), |mut output, byte| {
            write!(&mut output, "{byte:02x}").unwrap();
            output
        })
    }

    #[test]
    fn exact_017_scalar_and_empty_collection_vectors() {
        for (value, expected) in [
            (Value::Null, "010000000000000000"),
            (json!(false), "020000000000000000"),
            (json!(true), "030000000000000000"),
            (json!(""), "040000000000000000"),
            (json!("a"), "04000000000000000161"),
            (json!([]), "0500000000000000080000000000000000"),
            (json!({}), "0600000000000000080000000000000000"),
        ] {
            assert_eq!(hex(&encode(&value).unwrap()), expected);
        }
        assert_eq!(hex(&encode(&json!("é")).unwrap()), "040000000000000002c3a9");
    }

    #[test]
    fn member_order_is_canonical_but_arrays_nulls_and_scalar_spellings_are_not_erased() {
        let first = json!({"z": ["1", null], "é": true, "a": {"b": false}});
        let second = json!({"a": {"b": false}, "é": true, "z": ["1", null]});
        assert_eq!(encode(&first), encode(&second));
        assert_eq!(
            hash(RECORD_INTEGRITY, &first),
            hash(RECORD_INTEGRITY, &second)
        );
        assert_ne!(
            hash(RECORD_INTEGRITY, &first),
            hash(MEASUREMENT_CASE, &first)
        );
        for (a, b) in [
            (json!(["a", "b"]), json!(["b", "a"])),
            (json!(["a"]), json!(["a", "a"])),
            (json!({}), json!({"value": null})),
            (json!("é"), json!("e\u{301}")),
            (json!("1"), json!("01")),
        ] {
            assert_ne!(encode(&a), encode(&b));
        }
        let encoded = encode(&json!({"𐀀": null, "\u{e000}": null})).unwrap();
        let first_key = 9 + 8 + 9;
        assert_eq!(&encoded[first_key..first_key + 3], "\u{e000}".as_bytes());
    }

    #[test]
    fn numbers_and_capacity_fail_before_returning_any_canonical_prefix() {
        for value in [json!(0), json!(1.5), json!({"value": [1]}), json!(-1)] {
            assert_eq!(encode(&value), Err(EncodingFailure::Number));
        }
        let mut value = Value::Null;
        for _ in 1..MAX_DEPTH {
            value = Value::Array(vec![value]);
        }
        assert!(encode(&value).is_ok());
        assert_eq!(
            encode(&Value::Array(vec![value])),
            Err(EncodingFailure::Depth)
        );
        assert_eq!(
            length(&json!("small"), MAX_DEPTH + 1),
            Err(EncodingFailure::Depth)
        );
        let mut string = "a".repeat(usize::try_from(MAX_BYTES - 9).unwrap());
        assert_eq!(length(&Value::String(string.clone()), 1), Ok(MAX_BYTES));
        string.push('a');
        assert_eq!(
            encode(&Value::String(string)),
            Err(EncodingFailure::Capacity)
        );
    }

    #[test]
    fn domain_hashes_match_independently_framed_node_crypto_vectors() {
        // Cross-checked by a separate Buffer/BigInt framing implementation and
        // Node's SHA-256, not copied from this Rust encoder's output.
        let complex = json!({"z": ["1", null], "é": true, "a": {"b": false}});
        for (domain, value, expected) in [
            (
                RECORD_INTEGRITY,
                Value::Null,
                "22df83382d3dd624514a94d41f08360df09553b44971ac2d645a973d676e839a",
            ),
            (
                MEASUREMENT_CASE,
                Value::Null,
                "32483e47700ae1b656a73cc68328e948d764e0cb857a699492501ceffaa4ba3c",
            ),
            (
                RECORD_INTEGRITY,
                complex.clone(),
                "4e12da295b02e33e81665b680ad6845571ae8bcf21265aeebd7608e53bc16599",
            ),
            (
                MEASUREMENT_CASE,
                complex,
                "49811a27f9c8edba886a661162e094f404307c43f95954e6a1811f80fe2656cb",
            ),
        ] {
            assert_eq!(hex(&hash(domain, &value).unwrap()), expected);
        }
    }

    #[test]
    fn only_the_named_member_is_excluded_and_it_must_be_present() {
        let mut record = json!({"envelope": {"integrityDigest": "old", "identity": "one"}, "body": {"integrityDigest": "nested"}});
        let path = ["envelope", "integrityDigest"];
        let initial = hash_with_excluded_member(RECORD_INTEGRITY, &record, &path).unwrap();
        record["envelope"]["integrityDigest"] = json!("replacement");
        assert_eq!(
            hash_with_excluded_member(RECORD_INTEGRITY, &record, &path).unwrap(),
            initial
        );
        record["body"]["integrityDigest"] = json!("changed");
        assert_ne!(
            hash_with_excluded_member(RECORD_INTEGRITY, &record, &path).unwrap(),
            initial
        );
        record["body"]["integrityDigest"] = json!("nested");
        record["envelope"]["identity"] = json!("two");
        assert_ne!(
            hash_with_excluded_member(RECORD_INTEGRITY, &record, &path).unwrap(),
            initial
        );
        for (value, path) in [
            (
                json!({"envelope": {}}),
                ["envelope", "integrityDigest"].as_slice(),
            ),
            (json!({}), ["envelope", "integrityDigest"].as_slice()),
            (
                json!({"envelope": []}),
                ["envelope", "integrityDigest"].as_slice(),
            ),
            (json!({"integrityDigest": "a"}), [].as_slice()),
        ] {
            assert_eq!(
                hash_with_excluded_member(RECORD_INTEGRITY, &value, path),
                Err(EncodingFailure::RecordShape)
            );
        }
        // A top-level exclusion is the authoring/localization artifact shape.
        let artifact = json!({"kind": "message-intent", "integrityDigest": "old"});
        assert!(
            hash_with_excluded_member(RECORD_INTEGRITY, &artifact, &["integrityDigest"]).is_ok()
        );
    }

    #[test]
    fn a_decoded_number_is_rejected_whatever_shape_serde_delivered_it_in() {
        // A number that arrives through serde_json's private single-entry map
        // must still reach this encoder as a number. If the decoder stored it
        // verbatim, the document would encode as an ordinary object and a
        // digest would be produced over a value this encoding forbids.
        for source in ["0", "-1", "1.5", "1e999", r#"{"total":1.5}"#, "[1e999]"] {
            let value = crate::json::decode_unique_json(source).unwrap();
            assert_eq!(
                encode(&value),
                Err(EncodingFailure::Number),
                "{source} encoded instead of being rejected"
            );
        }
        // A document that really uses the private key is an object, and an
        // object of strings is encodable.
        let literal =
            crate::json::decode_unique_json(r#"{"$serde_json::private::Number":"hello"}"#).unwrap();
        assert!(encode(&literal).is_ok());
    }

    #[test]
    fn a_byte_digest_is_plain_sha256_and_not_the_framed_value_digest() {
        // Published SHA-256 answers, so this fixes the function rather than
        // recording whatever this crate currently computes.
        assert_eq!(
            hex(&digest_bytes(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&digest_bytes(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        // A snapshot digest must stay reproducible from the bytes on disk, so
        // it must not pick up the shared framing or a domain separator.
        let framed = hash(
            Domain::literal("verification-record-integrity"),
            &json!("abc"),
        )
        .unwrap();
        assert_ne!(digest_bytes(b"abc"), framed);
        assert_ne!(
            digest_bytes(b"abc"),
            digest_bytes(&encode(&json!("abc")).unwrap())
        );

        // Bytes that are not valid UTF-8 still have a digest; whether a unit is
        // text is a separate question from which bytes it is.
        assert_eq!(digest_bytes(&[0xff, 0xfe]).len(), 32);
    }

    #[test]
    fn registered_domains_use_the_exact_token_grammar() {
        assert_eq!(
            Domain::literal("intent-semantic-revision").name(),
            "intent-semantic-revision"
        );
        for invalid in ["", "Record-Integrity", "domain/one", "-a", "a-"] {
            assert_eq!(Domain::new(invalid), Err(IdentityFailure::InvalidToken));
        }
    }
}
