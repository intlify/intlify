// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Exact `intlify.shared-json.v0` framing from design 017. This operates on a
//! separately schema-checked value, never substitutes for body admission, and
//! performs no normalization of arrays, strings, nulls, or native owner data.

use serde_json::Value;
use sha2::{Digest as _, Sha256};

// Private harness capacity, not a new semantic JSON or project-policy limit.
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DEPTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum EncodingFailure {
    Number,
    Depth,
    Capacity,
    Overflow,
    Allocation,
    Serialization,
    RecordShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum Domain {
    RecordIntegrity,
    MeasurementCase,
}

impl Domain {
    const fn name(self) -> &'static str {
        match self {
            Self::RecordIntegrity => "verification-record-integrity",
            Self::MeasurementCase => "measurement-case-identity",
        }
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

pub(in crate::benchmark) fn encode(value: &Value) -> Result<Vec<u8>, EncodingFailure> {
    let bytes = usize::try_from(length(value, 1)?).map_err(|_| EncodingFailure::Overflow)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes)
        .map_err(|_| EncodingFailure::Allocation)?;
    write(value, &mut output)?;
    debug_assert_eq!(output.len(), bytes);
    Ok(output)
}

pub(in crate::benchmark) fn hash(
    domain: Domain,
    value: &Value,
) -> Result<[u8; 32], EncodingFailure> {
    let encoded = encode(value)?;
    let mut hasher = Sha256::new();
    hasher.update(b"intlify.shared-json.v0\0");
    // The domain is closed and selected by the caller's record kind, not input.
    hasher.update(encode(&Value::String(domain.name().into()))?);
    hasher.update(encoded);
    Ok(hasher.finalize().into())
}

pub(in crate::benchmark) fn record_hash(
    value: &impl serde::Serialize,
) -> Result<[u8; 32], EncodingFailure> {
    let mut value = serde_json::to_value(value).map_err(|_| EncodingFailure::Serialization)?;
    let envelope = value
        .get_mut("envelope")
        .and_then(Value::as_object_mut)
        .ok_or(EncodingFailure::RecordShape)?;
    if envelope.remove("integrityDigest").is_none() {
        return Err(EncodingFailure::RecordShape);
    }
    hash(Domain::RecordIntegrity, &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            hash(Domain::RecordIntegrity, &first),
            hash(Domain::RecordIntegrity, &second)
        );
        assert_ne!(
            hash(Domain::RecordIntegrity, &first),
            hash(Domain::MeasurementCase, &first)
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
                Domain::RecordIntegrity,
                Value::Null,
                "22df83382d3dd624514a94d41f08360df09553b44971ac2d645a973d676e839a",
            ),
            (
                Domain::MeasurementCase,
                Value::Null,
                "32483e47700ae1b656a73cc68328e948d764e0cb857a699492501ceffaa4ba3c",
            ),
            (
                Domain::RecordIntegrity,
                complex.clone(),
                "4e12da295b02e33e81665b680ad6845571ae8bcf21265aeebd7608e53bc16599",
            ),
            (
                Domain::MeasurementCase,
                complex,
                "49811a27f9c8edba886a661162e094f404307c43f95954e6a1811f80fe2656cb",
            ),
        ] {
            assert_eq!(hex(&hash(domain, &value).unwrap()), expected);
        }
    }

    #[test]
    fn only_the_top_envelope_integrity_member_is_excluded() {
        let mut record = json!({"envelope": {"integrityDigest": "old", "identity": "one"}, "body": {"integrityDigest": "nested"}});
        let initial = record_hash(&record).unwrap();
        record["envelope"]["integrityDigest"] = json!("replacement");
        assert_eq!(record_hash(&record).unwrap(), initial);
        record["body"]["integrityDigest"] = json!("changed");
        assert_ne!(record_hash(&record).unwrap(), initial);
        record["body"]["integrityDigest"] = json!("nested");
        record["envelope"]["identity"] = json!("two");
        assert_ne!(record_hash(&record).unwrap(), initial);
        assert_eq!(
            record_hash(&json!({"envelope": {}})),
            Err(EncodingFailure::RecordShape)
        );
    }
}
